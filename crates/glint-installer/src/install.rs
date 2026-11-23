//! `Installer::install` and `Installer::remove` (INS-003).
//!
//! # Store layout
//!
//! ```text
//! <data>/store/<sha256>/          one immutable entry per release
//!     source.glint                the validated archive (read-only)
//!     root/                       the extracted tree (read-only files)
//!     install.record              metadata for rebuilding the index
//! <data>/staging/<pkg-...>/       private staging (same volume as store)
//! <data>/trash/<sha256>-<n>/      removed entries awaiting deletion
//! ```
//!
//! # Install transaction
//!
//! 1. Refuse candidates not staged under [`Installer::staging_root`] and
//!    provenance that does not describe the candidate.
//! 2. If the digest is installed, return it (idempotent re-install); the
//!    new staging copy is discarded.
//! 3. Write `install.record` into the staging directory.
//! 4. Commit a journal row (`install`, digest, staging name).
//! 5. Rename the staging directory to `store/<sha256>` (one atomic rename on
//!    one volume; the entry appears complete or not at all).
//! 6. In one transaction: insert the package row, delete the journal row
//!    and, only if requested, activate it. Commit.
//!
//! A crash before step 6 commits leaves the previous state: the journal row
//! survives and [`Installer::recover_pending`] removes the moved entry and
//! the staging directory. Removal mirrors this with a rename into the trash.

use std::fs::{self, File, TryLockError};
use std::path::{Path, PathBuf};

use glint_package::PackageCandidate;
use glint_sources::SourceOrigin;
use glint_storage::rusqlite::{OptionalExtension, Row, TransactionBehavior, params};
use glint_storage::{ActiveTheme, GlintPaths, Storage, activate_in, unix_now};

use crate::error::InstallError;
use crate::fault::{FaultInjector, FaultPoint};
use crate::fsutil;
use crate::record::StoreRecord;
use crate::recover::RecoveryReport;

/// Name of the extracted tree inside a store entry (as in staging).
pub(crate) const ROOT_DIR: &str = "root";
/// Name of the archive copy inside a store entry (as in staging).
pub(crate) const ARCHIVE_NAME: &str = "source.glint";

/// An installed, immutable release.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct InstalledPackage {
    /// Content address: lowercase hex SHA-256 of the archive.
    pub digest: String,
    /// Manifest `kind` (`theme`).
    pub kind: String,
    /// Manifest `id`.
    pub id: String,
    /// Manifest `version`.
    pub version: String,
    /// Source kind recorded at install (`local-archive`, ...).
    pub source_kind: String,
    /// Canonical source location recorded at install.
    pub source_locator: String,
    /// Archive size in bytes.
    pub archive_size: u64,
    /// The extracted, read-only package tree.
    pub path: PathBuf,
    /// The validated archive copy.
    pub archive_path: PathBuf,
    /// Unix time of installation.
    pub installed_at: i64,
}

/// Options for [`Installer::install_with`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct InstallOptions {
    /// Make the package the base theme of this profile and activate it, in
    /// the same transaction as the install. `None` (the default, and what
    /// CLI import uses) installs without activating.
    pub activate_profile: Option<String>,
}

impl InstallOptions {
    /// Install and activate in `profile_id`.
    pub fn activate(profile_id: impl Into<String>) -> Self {
        Self {
            activate_profile: Some(profile_id.into()),
        }
    }
}

/// The transactional package installer. Holds the exclusive installer lock
/// for its lifetime.
pub struct Installer {
    pub(crate) storage: Storage,
    pub(crate) paths: GlintPaths,
    _lock: File,
    pub(crate) faults: Option<Box<dyn FaultInjector>>,
    pub(crate) startup: RecoveryReport,
}

impl std::fmt::Debug for Installer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Installer")
            .field("paths", &self.paths)
            .field("fault_injector", &self.faults.is_some())
            .finish_non_exhaustive()
    }
}

impl Installer {
    /// Open the state database (migrating and recovering it), take the
    /// installer lock and run [`recover_pending`](Self::recover_pending).
    /// The startup report is kept in
    /// [`startup_recovery`](Self::startup_recovery).
    ///
    /// # Errors
    ///
    /// [`ErrorCode::Busy`](crate::ErrorCode::Busy) when another installer
    /// holds the lock, storage errors, or recovery I/O errors.
    pub fn open(paths: &GlintPaths) -> Result<Self, InstallError> {
        let storage = Storage::open(paths)?;
        let lock_path = paths.installer_lock_file();
        let lock = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(InstallError::io("open installer lock", &lock_path))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(InstallError::Busy(lock_path)),
            Err(TryLockError::Error(source)) => {
                return Err(InstallError::Io {
                    context: "take installer lock",
                    path: lock_path,
                    source,
                });
            }
        }
        let mut installer = Self {
            storage,
            paths: paths.clone(),
            _lock: lock,
            faults: None,
            startup: RecoveryReport::default(),
        };
        installer.startup = installer.recover_pending()?;
        Ok(installer)
    }

    /// What [`open`](Self::open) recovered at startup.
    pub fn startup_recovery(&self) -> &RecoveryReport {
        &self.startup
    }

    /// The resolved paths.
    pub fn paths(&self) -> &GlintPaths {
        &self.paths
    }

    /// The state database.
    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    /// Where sources must stage candidates for this installer
    /// (`PackageSource::stage(source, installer.staging_root())`).
    pub fn staging_root(&self) -> PathBuf {
        self.paths.staging_dir()
    }

    /// Install a fault injector that simulates crashes (tests only).
    pub fn set_fault_injector(&mut self, injector: impl FaultInjector + 'static) {
        self.faults = Some(Box::new(injector));
    }

    /// Remove the fault injector.
    pub fn clear_fault_injector(&mut self) {
        self.faults = None;
    }

    pub(crate) fn fault(&mut self, point: FaultPoint) -> Result<(), InstallError> {
        if let Some(injector) = &mut self.faults
            && injector.crash_at(point)
        {
            return Err(InstallError::Interrupted(point));
        }
        Ok(())
    }

    /// Install without activating. See [`install_with`](Self::install_with).
    ///
    /// # Errors
    ///
    /// As [`install_with`](Self::install_with).
    pub fn install(
        &mut self,
        candidate: PackageCandidate,
        origin: SourceOrigin,
    ) -> Result<InstalledPackage, InstallError> {
        self.install_with(candidate, origin, &InstallOptions::default())
    }

    /// Move a validated candidate into the content-addressed store and
    /// record it, transactionally. Installing an already installed digest
    /// returns the existing package (activating it if requested).
    ///
    /// # Errors
    ///
    /// [`ErrorCode::CandidateOutsideStaging`](crate::ErrorCode::CandidateOutsideStaging),
    /// [`ErrorCode::OriginMismatch`](crate::ErrorCode::OriginMismatch),
    /// [`ErrorCode::StoreConflict`](crate::ErrorCode::StoreConflict), I/O and
    /// storage errors. On every error the previous state is kept.
    pub fn install_with(
        &mut self,
        mut candidate: PackageCandidate,
        origin: SourceOrigin,
        options: &InstallOptions,
    ) -> Result<InstalledPackage, InstallError> {
        let digest = candidate.sha256_hex();
        let staging_dir = candidate.staging_dir().to_path_buf();
        self.check_candidate(&candidate, &origin, &staging_dir)?;

        if let Some(existing) = self.package(&digest)? {
            self.check_identity(&candidate, &origin, Some(&existing))?;
            // `candidate` drops here and removes its staging copy.
            drop(candidate);
            if let Some(profile) = &options.activate_profile {
                self.storage.set_active_theme(profile, &digest)?;
            }
            return Ok(existing);
        }
        self.check_identity(&candidate, &origin, None)?;

        let store_entry = self.paths.store_dir().join(&digest);
        if fs::symlink_metadata(&store_entry).is_ok() {
            return Err(InstallError::StoreConflict(store_entry));
        }
        let record = StoreRecord {
            digest: digest.clone(),
            kind: candidate.manifest.kind.as_str().to_owned(),
            package_id: candidate.manifest.id.clone(),
            version: candidate.manifest.version.to_string(),
            archive_size: candidate.archive_size,
            source_kind: origin.kind.as_str().to_owned(),
            source_locator: origin.locator.clone(),
            installed_at: unix_now(),
        };
        record.write_new(&staging_dir)?;

        // From here on the staging directory belongs to the journal.
        candidate.keep_staging();
        drop(candidate);
        let staging_name = dir_name(&staging_dir)?;
        let txn = self.journal_insert("install", &digest, Some(&staging_name), None)?;
        self.fault(FaultPoint::InstallAfterJournal)?;

        if let Err(source) = fs::rename(&staging_dir, &store_entry) {
            let _ = fsutil::remove_tree(&staging_dir);
            self.journal_delete(txn)?;
            return Err(InstallError::Io {
                context: "move package into store",
                path: store_entry,
                source,
            });
        }
        self.fault(FaultPoint::InstallAfterMove)?;

        let activated = {
            let tx = self
                .storage
                .connection_mut()
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(InstallError::db("begin install commit"))?;
            insert_package(&tx, &record)?;
            tx.execute("DELETE FROM install_journal WHERE txn_id = ?1", [txn])
                .map_err(InstallError::db("close install journal"))?;
            let activated = match &options.activate_profile {
                Some(profile) => Some(activate_in(&tx, profile, &digest)?),
                None => None,
            };
            // Dropping `tx` here (simulated crash) rolls it back.
            if let Some(injector) = &mut self.faults
                && injector.crash_at(FaultPoint::InstallBeforeCommit)
            {
                return Err(InstallError::Interrupted(FaultPoint::InstallBeforeCommit));
            }
            tx.commit().map_err(InstallError::db("commit install"))?;
            activated
        };
        if activated.is_some() {
            self.storage.write_active_record()?;
        }
        self.fault(FaultPoint::InstallAfterCommit)?;
        Ok(self.installed_from(&record))
    }

    /// Remove an installed release by digest.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::NotInstalled`](crate::ErrorCode::NotInstalled),
    /// [`ErrorCode::InUse`](crate::ErrorCode::InUse) while the active profile
    /// or any profile (base theme or install lock) references it, I/O and
    /// storage errors. On every error the package stays installed.
    pub fn remove(&mut self, digest: &str) -> Result<(), InstallError> {
        if self.package(digest)?.is_none() {
            return Err(InstallError::NotInstalled(digest.to_owned()));
        }
        let mut profiles = self.referencing_profiles(digest)?;
        if let Some(pending) = self.pending_apply()?
            && pending.target_digest == digest
            && !profiles.contains(&pending.target_profile_id)
        {
            profiles.push(pending.target_profile_id);
        }
        if !profiles.is_empty() {
            return Err(InstallError::InUse {
                digest: digest.to_owned(),
                profiles,
            });
        }
        let store_entry = self.paths.store_dir().join(digest);
        let trash_name = unique_name(&self.paths.trash_dir(), digest);
        let trash_entry = self.paths.trash_dir().join(&trash_name);
        let txn = self.journal_insert("remove", digest, None, Some(&trash_name))?;
        self.fault(FaultPoint::RemoveAfterJournal)?;

        match fs::rename(&store_entry, &trash_entry) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                self.journal_delete(txn)?;
                return Err(InstallError::Io {
                    context: "move package to trash",
                    path: store_entry,
                    source,
                });
            }
        }
        self.fault(FaultPoint::RemoveAfterMove)?;

        {
            let tx = self
                .storage
                .connection_mut()
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(InstallError::db("begin remove commit"))?;
            tx.execute("DELETE FROM packages WHERE digest = ?1", [digest])
                .map_err(InstallError::db("delete package"))?;
            tx.execute("DELETE FROM install_journal WHERE txn_id = ?1", [txn])
                .map_err(InstallError::db("close remove journal"))?;
            if let Some(injector) = &mut self.faults
                && injector.crash_at(FaultPoint::RemoveBeforeCommit)
            {
                return Err(InstallError::Interrupted(FaultPoint::RemoveBeforeCommit));
            }
            tx.commit().map_err(InstallError::db("commit remove"))?;
        }
        self.fault(FaultPoint::RemoveAfterCommit)?;
        // Committed; a failure here only leaves trash for the next recovery.
        let _ = fsutil::remove_tree(&trash_entry);
        Ok(())
    }

    /// One installed package by digest.
    ///
    /// # Errors
    ///
    /// Storage errors.
    pub fn package(&self, digest: &str) -> Result<Option<InstalledPackage>, InstallError> {
        self.storage
            .connection()
            .query_row(
                &format!("SELECT {PACKAGE_COLUMNS} FROM packages WHERE digest = ?1"),
                [digest],
                |row| self.row_to_package(row),
            )
            .optional()
            .map_err(InstallError::db("read package"))
    }

    /// Every installed package, ordered by ID, version and digest.
    ///
    /// # Errors
    ///
    /// Storage errors.
    pub fn installed(&self) -> Result<Vec<InstalledPackage>, InstallError> {
        let conn = self.storage.connection();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {PACKAGE_COLUMNS} FROM packages ORDER BY package_id, version, digest"
            ))
            .map_err(InstallError::db("list packages"))?;
        let rows = stmt
            .query_map([], |row| self.row_to_package(row))
            .map_err(InstallError::db("list packages"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(InstallError::db("list packages"))
    }

    /// The active theme.
    ///
    /// # Errors
    ///
    /// Storage errors.
    pub fn active_theme(&self) -> Result<Option<ActiveTheme>, InstallError> {
        Ok(self.storage.active_theme()?)
    }

    /// Activate an installed release as `profile_id`'s base theme (Glint
    /// state only, no surfaces or native settings).
    ///
    /// # Errors
    ///
    /// [`ErrorCode::NotInstalled`](crate::ErrorCode::NotInstalled) or storage
    /// errors.
    pub fn activate(
        &mut self,
        profile_id: &str,
        digest: &str,
    ) -> Result<ActiveTheme, InstallError> {
        if self.package(digest)?.is_none() {
            return Err(InstallError::NotInstalled(digest.to_owned()));
        }
        Ok(self.storage.set_active_theme(profile_id, digest)?)
    }

    /// Pin an installed release in a profile's install locks (creating the
    /// profile, inactive, if needed). A pinned release cannot be removed.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::NotInstalled`](crate::ErrorCode::NotInstalled) or storage
    /// errors.
    pub fn pin(&mut self, profile_id: &str, digest: &str) -> Result<(), InstallError> {
        glint_storage::validate_profile_id(profile_id)?;
        if self.package(digest)?.is_none() {
            return Err(InstallError::NotInstalled(digest.to_owned()));
        }
        let tx = self
            .storage
            .connection_mut()
            .transaction()
            .map_err(InstallError::db("begin pin"))?;
        tx.execute(
            "INSERT OR IGNORE INTO profiles (profile_id, display_name, base_digest, revision, updated_at)
                  VALUES (?1, ?1, ?2, 0, ?3)",
            params![profile_id, digest, unix_now()],
        )
        .map_err(InstallError::db("create profile"))?;
        tx.execute(
            "INSERT OR IGNORE INTO install_locks (profile_id, digest) VALUES (?1, ?2)",
            params![profile_id, digest],
        )
        .map_err(InstallError::db("pin package"))?;
        tx.commit().map_err(InstallError::db("commit pin"))
    }

    /// Remove a pin.
    ///
    /// # Errors
    ///
    /// Storage errors.
    pub fn unpin(&mut self, profile_id: &str, digest: &str) -> Result<(), InstallError> {
        self.storage
            .connection()
            .execute(
                "DELETE FROM install_locks WHERE profile_id = ?1 AND digest = ?2",
                params![profile_id, digest],
            )
            .map_err(InstallError::db("unpin package"))?;
        Ok(())
    }

    /// Profiles that reference `digest` as base theme or through an install
    /// lock, sorted.
    fn referencing_profiles(&self, digest: &str) -> Result<Vec<String>, InstallError> {
        let conn = self.storage.connection();
        let mut stmt = conn
            .prepare(
                "SELECT profile_id FROM profiles WHERE base_digest = ?1
                 UNION SELECT profile_id FROM install_locks WHERE digest = ?1
                 ORDER BY 1",
            )
            .map_err(InstallError::db("find package references"))?;
        let rows = stmt
            .query_map([digest], |row| row.get(0))
            .map_err(InstallError::db("find package references"))?;
        rows.collect::<Result<Vec<String>, _>>()
            .map_err(InstallError::db("find package references"))
    }

    fn check_candidate(
        &self,
        candidate: &PackageCandidate,
        origin: &SourceOrigin,
        staging_dir: &Path,
    ) -> Result<(), InstallError> {
        let root = self.paths.staging_dir();
        let inside = staging_dir.parent() == Some(root.as_path())
            || match (
                staging_dir.parent().map(fs::canonicalize),
                fs::canonicalize(&root),
            ) {
                (Some(Ok(parent)), Ok(root)) => parent == root,
                _ => false,
            };
        if !inside {
            return Err(InstallError::CandidateOutsideStaging(
                staging_dir.to_path_buf(),
            ));
        }
        if origin.archive_sha256 != candidate.sha256 {
            return Err(InstallError::OriginMismatch("archive digest"));
        }
        if origin.package_id != candidate.manifest.id {
            return Err(InstallError::OriginMismatch("package id"));
        }
        if origin.package_version != candidate.manifest.version.to_string() {
            return Err(InstallError::OriginMismatch("package version"));
        }
        Ok(())
    }

    /// Identity policy hook (INS-004): decides whether a release with this
    /// ID from this origin may be installed next to what is installed.
    /// Until the identity policy lands it never refuses; the store never
    /// overwrites anything either way, because entries are addressed by
    /// digest and an existing digest is returned unchanged.
    fn check_identity(
        &self,
        _candidate: &PackageCandidate,
        _origin: &SourceOrigin,
        _existing: Option<&InstalledPackage>,
    ) -> Result<(), InstallError> {
        Ok(())
    }

    pub(crate) fn journal_insert(
        &self,
        operation: &str,
        digest: &str,
        staging_name: Option<&str>,
        trash_name: Option<&str>,
    ) -> Result<i64, InstallError> {
        let conn = self.storage.connection();
        conn.execute(
            "INSERT INTO install_journal (operation, digest, staging_name, trash_name, created_at)
                  VALUES (?1, ?2, ?3, ?4, ?5)",
            params![operation, digest, staging_name, trash_name, unix_now()],
        )
        .map_err(InstallError::db("open journal"))?;
        Ok(conn.last_insert_rowid())
    }

    pub(crate) fn journal_delete(&self, txn: i64) -> Result<(), InstallError> {
        self.storage
            .connection()
            .execute("DELETE FROM install_journal WHERE txn_id = ?1", [txn])
            .map_err(InstallError::db("close journal"))?;
        Ok(())
    }

    pub(crate) fn installed_from(&self, record: &StoreRecord) -> InstalledPackage {
        let entry = self.paths.store_dir().join(&record.digest);
        InstalledPackage {
            digest: record.digest.clone(),
            kind: record.kind.clone(),
            id: record.package_id.clone(),
            version: record.version.clone(),
            source_kind: record.source_kind.clone(),
            source_locator: record.source_locator.clone(),
            archive_size: record.archive_size,
            path: entry.join(ROOT_DIR),
            archive_path: entry.join(ARCHIVE_NAME),
            installed_at: record.installed_at,
        }
    }

    fn row_to_package(&self, row: &Row<'_>) -> glint_storage::rusqlite::Result<InstalledPackage> {
        let record = StoreRecord {
            digest: row.get(0)?,
            kind: row.get(1)?,
            package_id: row.get(2)?,
            version: row.get(3)?,
            archive_size: row.get::<_, i64>(4)?.try_into().unwrap_or(0),
            source_kind: row.get(5)?,
            source_locator: row.get(6)?,
            installed_at: row.get(7)?,
        };
        Ok(self.installed_from(&record))
    }
}

const PACKAGE_COLUMNS: &str =
    "digest, kind, package_id, version, archive_size, source_kind, source_locator, installed_at";

pub(crate) fn insert_package(
    conn: &glint_storage::rusqlite::Connection,
    record: &StoreRecord,
) -> Result<(), InstallError> {
    conn.execute(
        "INSERT INTO packages (digest, kind, package_id, version, store_name, archive_size,
                               source_kind, source_locator, installed_at)
              VALUES (?1, ?2, ?3, ?4, ?1, ?5, ?6, ?7, ?8)",
        params![
            record.digest,
            record.kind,
            record.package_id,
            record.version,
            i64::try_from(record.archive_size).unwrap_or(i64::MAX),
            record.source_kind,
            record.source_locator,
            record.installed_at,
        ],
    )
    .map_err(InstallError::db("insert package"))?;
    Ok(())
}

fn dir_name(path: &Path) -> Result<String, InstallError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| InstallError::CandidateOutsideStaging(path.to_path_buf()))
}

/// `<base>-<n>` that does not exist yet in `dir`.
fn unique_name(dir: &Path, base: &str) -> String {
    (0u32..)
        .map(|n| format!("{base}-{n}"))
        .find(|name| fs::symlink_metadata(dir.join(name)).is_err())
        .unwrap_or_else(|| base.to_owned())
}
