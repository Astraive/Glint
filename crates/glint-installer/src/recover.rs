//! `Installer::recover_pending` (INS-003): finish or roll back interrupted
//! installs and removals, clean staging and trash, and rebuild the package
//! index from the store after database recovery.

use std::fs;
use std::path::PathBuf;

use glint_storage::rusqlite::OptionalExtension;
use glint_storage::validate_digest;

use crate::error::InstallError;
use crate::fsutil;
use crate::install::{ARCHIVE_NAME, Installer, ROOT_DIR, insert_package};
use crate::record::StoreRecord;
use crate::rollback::PendingApply;

/// What [`Installer::recover_pending`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct RecoveryReport {
    /// Interrupted installs rolled back (digests): the entry was removed
    /// from the store and the previous state kept.
    pub rolled_back_installs: Vec<String>,
    /// Installs whose commit had landed; only the journal row was stale.
    pub completed_installs: Vec<String>,
    /// Interrupted removals undone (the package is installed again).
    pub restored_removals: Vec<String>,
    /// Removals whose commit had landed; the trash entry was deleted.
    pub completed_removals: Vec<String>,
    /// Store entries re-added to the index after verifying their digest
    /// (after the database was recreated).
    pub reindexed: Vec<String>,
    /// Store entries that failed verification and were deleted.
    pub discarded: Vec<PathBuf>,
    /// Indexed packages whose store entry is missing (left in the index and
    /// reported; reinstall to repair).
    pub missing_entries: Vec<String>,
    /// Leftover staging directories removed.
    pub cleaned_staging: usize,
    /// Leftover trash entries removed.
    pub cleaned_trash: usize,
    /// Whether the database itself was recovered from corruption on open.
    pub database_recovered: bool,
    /// An interrupted apply found in the journal (INS-005). The previous
    /// theme is still active; call
    /// [`Installer::recover_apply`](crate::Installer::recover_apply) with the
    /// native adapters to restore the settings it may have changed.
    pub pending_apply: Option<PendingApply>,
}

impl RecoveryReport {
    /// True when nothing needed recovering.
    pub fn is_clean(&self) -> bool {
        *self == RecoveryReport::default()
    }
}

struct JournalRow {
    txn: i64,
    operation: String,
    digest: String,
    staging_name: Option<String>,
    trash_name: Option<String>,
}

impl Installer {
    /// Bring the store, staging and trash back to a state consistent with
    /// the database. Runs automatically in [`Installer::open`]; safe to run
    /// again at any time the installer is idle.
    ///
    /// - An `install` journal row whose package row is missing is rolled
    ///   back: the store entry it created and its staging directory are
    ///   deleted. If the package row exists, the commit landed and only the
    ///   row is cleared.
    /// - A `remove` journal row whose package row still exists is undone by
    ///   moving the entry back from the trash; otherwise the trash entry is
    ///   deleted.
    /// - Every remaining staging and trash entry is deleted.
    /// - Store entries without a package row (only possible after the
    ///   database was recreated) are re-indexed if their archive hashes to
    ///   their name and their record agrees; otherwise deleted.
    ///
    /// # Errors
    ///
    /// Storage or I/O errors; recovery stops at the first and can be re-run.
    pub fn recover_pending(&mut self) -> Result<RecoveryReport, InstallError> {
        let mut report = RecoveryReport {
            database_recovered: self.storage.recovery().is_some(),
            ..RecoveryReport::default()
        };
        for row in self.journal_rows()? {
            let installed = self.is_indexed(&row.digest)?;
            match row.operation.as_str() {
                "install" => {
                    if let Some(name) = &row.staging_name {
                        self.remove_owned(self.paths.staging_dir().join(name))?;
                    }
                    if installed {
                        report.completed_installs.push(row.digest.clone());
                    } else {
                        self.remove_owned(self.paths.store_dir().join(&row.digest))?;
                        report.rolled_back_installs.push(row.digest.clone());
                    }
                }
                _ => {
                    let trash = row
                        .trash_name
                        .as_ref()
                        .map(|name| self.paths.trash_dir().join(name));
                    let entry = self.paths.store_dir().join(&row.digest);
                    if installed {
                        if let Some(trash) = &trash
                            && trash.exists()
                            && !entry.exists()
                        {
                            fs::rename(trash, &entry)
                                .map_err(InstallError::io("restore removed package", &entry))?;
                        }
                        report.restored_removals.push(row.digest.clone());
                    } else {
                        if let Some(trash) = trash {
                            self.remove_owned(trash)?;
                        }
                        report.completed_removals.push(row.digest.clone());
                    }
                }
            }
            self.journal_delete(row.txn)?;
        }

        report.cleaned_staging = self.empty_dir(self.paths.staging_dir())?;
        report.cleaned_trash = self.empty_dir(self.paths.trash_dir())?;
        self.reconcile_store(&mut report)?;
        report.pending_apply = self.pending_apply()?;
        Ok(report)
    }

    fn journal_rows(&self) -> Result<Vec<JournalRow>, InstallError> {
        let conn = self.storage.connection();
        let mut stmt = conn
            .prepare(
                "SELECT txn_id, operation, digest, staging_name, trash_name
                   FROM install_journal ORDER BY txn_id",
            )
            .map_err(InstallError::db("read journal"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(JournalRow {
                    txn: row.get(0)?,
                    operation: row.get(1)?,
                    digest: row.get(2)?,
                    staging_name: row.get(3)?,
                    trash_name: row.get(4)?,
                })
            })
            .map_err(InstallError::db("read journal"))?;
        let rows = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(InstallError::db("read journal"))?;
        // Names come from our own database, but are still only ever joined
        // as single components under Glint-owned directories.
        Ok(rows
            .into_iter()
            .map(|mut row| {
                row.staging_name = row.staging_name.filter(|n| is_plain_name(n));
                row.trash_name = row.trash_name.filter(|n| is_plain_name(n));
                row
            })
            .collect())
    }

    fn is_indexed(&self, digest: &str) -> Result<bool, InstallError> {
        Ok(self
            .storage
            .connection()
            .query_row("SELECT 1 FROM packages WHERE digest = ?1", [digest], |_| {
                Ok(())
            })
            .optional()
            .map_err(InstallError::db("check package"))?
            .is_some())
    }

    /// Delete a Glint-owned path after checking it lies inside the roots.
    fn remove_owned(&self, path: PathBuf) -> Result<(), InstallError> {
        self.paths.check_within(&path)?;
        fsutil::remove_tree(&path).map_err(InstallError::io("remove leftover", &path))
    }

    /// Delete every entry in a Glint-owned directory; returns how many.
    fn empty_dir(&self, dir: PathBuf) -> Result<usize, InstallError> {
        let mut removed = 0;
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(source) => return Err(InstallError::io("list leftovers", &dir)(source)),
        };
        for entry in entries {
            let entry = entry.map_err(InstallError::io("list leftovers", &dir))?;
            self.remove_owned(entry.path())?;
            removed += 1;
        }
        Ok(removed)
    }

    fn reconcile_store(&mut self, report: &mut RecoveryReport) -> Result<(), InstallError> {
        let store = self.paths.store_dir();
        let mut on_disk = Vec::new();
        for entry in fs::read_dir(&store).map_err(InstallError::io("list store", &store))? {
            let entry = entry.map_err(InstallError::io("list store", &store))?;
            on_disk.push(entry.path());
        }
        on_disk.sort();
        for path in on_disk {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if validate_digest(name).is_ok() && self.is_indexed(name)? {
                continue;
            }
            match verify_entry(&path, name) {
                Some(record) => {
                    insert_package(self.storage.connection(), &record)?;
                    report.reindexed.push(record.digest);
                }
                None => {
                    self.remove_owned(path.clone())?;
                    report.discarded.push(path);
                }
            }
        }
        for package in self.installed()? {
            if !glint_storage::is_real_dir(&self.paths.store_dir().join(&package.digest)) {
                report.missing_entries.push(package.digest);
            }
        }
        Ok(())
    }
}

/// A store entry is trusted for re-indexing only if its name is a digest,
/// its archive hashes to that digest, its record names the same digest and
/// its extracted tree is a real directory.
fn verify_entry(path: &std::path::Path, name: &str) -> Option<StoreRecord> {
    validate_digest(name).ok()?;
    if !glint_storage::is_real_dir(path) || !glint_storage::is_real_dir(&path.join(ROOT_DIR)) {
        return None;
    }
    let record = StoreRecord::read(path)?;
    if record.digest != name {
        return None;
    }
    let actual = fsutil::sha256_file(&path.join(ARCHIVE_NAME)).ok()?;
    (actual == name).then_some(record)
}

fn is_plain_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':'])
}
