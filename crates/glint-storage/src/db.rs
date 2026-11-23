//! The SQLite state database: versioned migrations, transactional apply and
//! corruption recovery (INS-002).
//!
//! # Migrations
//!
//! The schema version is SQLite's `PRAGMA user_version`. Each pending
//! [`Migration`] runs in its own `BEGIN IMMEDIATE` transaction together with
//! the version bump and a check that its tables exist, so a failing
//! migration rolls back completely and leaves the database readable at the
//! previous version ([`ErrorCode::MigrationFailed`]). A database newer than
//! this build is never touched ([`ErrorCode::SchemaTooNew`]).
//!
//! # Corruption
//!
//! A file that is not a database, fails `PRAGMA quick_check`, or claims a
//! schema version whose tables are missing is **quarantined** (renamed to
//! `glint.sqlite3.corrupt-<unix-time>`, never deleted) and a fresh database
//! is created. The active theme is restored from the active-theme record
//! (see [`crate::active`]), so corruption never loses it; the installer then
//! rebuilds the package index from the immutable store. The outcome is
//! reported through [`Storage::recovery`].
//!
//! [`ErrorCode::MigrationFailed`]: crate::ErrorCode::MigrationFailed
//! [`ErrorCode::SchemaTooNew`]: crate::ErrorCode::SchemaTooNew

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior};

use crate::active::{self, ActiveTheme};
use crate::error::StorageError;
use crate::paths::GlintPaths;

/// One numbered schema migration.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    /// Schema version after this migration; versions start at 1 and are
    /// contiguous.
    pub version: u32,
    /// Short name, for diagnostics.
    pub name: &'static str,
    /// SQL run in one transaction.
    pub sql: &'static str,
    /// Tables that must exist once this migration is applied; checked after
    /// applying it and on every open, to detect a damaged schema.
    pub tables: &'static [&'static str],
}

/// Every migration this build knows, in order.
pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "init",
    sql: include_str!("migrations/001_init.sql"),
    tables: &[
        "packages",
        "grants",
        "profiles",
        "install_locks",
        "active_profile",
    ],
}];

/// The schema version this build migrates to.
pub fn latest_schema_version() -> u32 {
    MIGRATIONS.last().map_or(0, |m| m.version)
}

/// What corruption recovery did when the database was opened.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DatabaseRecovery {
    /// Why the old database was rejected.
    pub reason: String,
    /// Where the old database was moved (inside the data directory).
    pub quarantined: PathBuf,
    /// The active theme restored from the record, if there was one.
    pub restored_active: Option<ActiveTheme>,
}

/// An open state database.
///
/// Only the core process opens it read-write ("daemon only"); the installer
/// is its only writer of package and journal rows.
#[derive(Debug)]
pub struct Storage {
    conn: Connection,
    paths: GlintPaths,
    schema_version: u32,
    recovery: Option<DatabaseRecovery>,
}

enum OpenFailure {
    /// Quarantine and recreate.
    Corrupt(String),
    /// Report to the caller; the database is left as it is.
    Fatal(StorageError),
}

impl From<StorageError> for OpenFailure {
    fn from(error: StorageError) -> Self {
        OpenFailure::Fatal(error)
    }
}

impl Storage {
    /// Create Glint's directories if needed, open (or create) the database,
    /// apply pending [`MIGRATIONS`] and recover from corruption.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::MigrationFailed`](crate::ErrorCode::MigrationFailed) (the
    /// database is unchanged and [`read_active_theme`] still works),
    /// [`ErrorCode::SchemaTooNew`](crate::ErrorCode::SchemaTooNew),
    /// [`ErrorCode::Database`](crate::ErrorCode::Database) (for example a
    /// busy database, category timeout), path and I/O errors, or
    /// [`ErrorCode::DatabaseCorrupt`](crate::ErrorCode::DatabaseCorrupt) when
    /// even a fresh database cannot be created.
    pub fn open(paths: &GlintPaths) -> Result<Self, StorageError> {
        Self::open_with_migrations(paths, MIGRATIONS)
    }

    /// [`Storage::open`] with an explicit migration list (tests inject
    /// failing migrations through this).
    ///
    /// # Errors
    ///
    /// As [`Storage::open`].
    pub fn open_with_migrations(
        paths: &GlintPaths,
        migrations: &[Migration],
    ) -> Result<Self, StorageError> {
        paths.ensure_dirs()?;
        let db = paths.database_file();
        let record = paths.active_record_file();
        match open_and_migrate(&db, migrations) {
            Ok((conn, schema_version)) => {
                let storage = Self {
                    conn,
                    paths: paths.clone(),
                    schema_version,
                    recovery: None,
                };
                storage.reconcile_record()?;
                Ok(storage)
            }
            Err(OpenFailure::Fatal(error)) => Err(error),
            Err(OpenFailure::Corrupt(reason)) => {
                let quarantined = quarantine(&db)?;
                let (conn, schema_version) =
                    open_and_migrate(&db, migrations).map_err(|failure| match failure {
                        OpenFailure::Fatal(error) => error,
                        OpenFailure::Corrupt(detail) => StorageError::DatabaseCorrupt(detail),
                    })?;
                let restored_active = active::read_record(&record);
                if let Some(theme) = &restored_active {
                    let tx = conn
                        .unchecked_transaction()
                        .map_err(StorageError::db("begin restore"))?;
                    active::restore_in(&tx, theme)?;
                    tx.commit().map_err(StorageError::db("commit restore"))?;
                }
                Ok(Self {
                    conn,
                    paths: paths.clone(),
                    schema_version,
                    recovery: Some(DatabaseRecovery {
                        reason,
                        quarantined,
                        restored_active,
                    }),
                })
            }
        }
    }

    /// The resolved paths this database belongs to.
    pub fn paths(&self) -> &GlintPaths {
        &self.paths
    }

    /// The schema version after opening.
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// What corruption recovery did on open, if anything.
    pub fn recovery(&self) -> Option<&DatabaseRecovery> {
        self.recovery.as_ref()
    }

    /// The underlying connection, for crates that own tables (the
    /// installer).
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// The underlying connection, mutable (for transactions).
    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// The active theme, if a profile is active.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::Database`](crate::ErrorCode::Database).
    pub fn active_theme(&self) -> Result<Option<ActiveTheme>, StorageError> {
        active::query_active(&self.conn).map_err(StorageError::db("read active profile"))
    }

    /// Make `profile_id` active with base release `digest`, in one
    /// transaction, then mirror it into the active-theme record.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::InvalidValue`](crate::ErrorCode::InvalidValue),
    /// [`ErrorCode::Database`](crate::ErrorCode::Database) or an I/O error
    /// writing the record (the database change is already committed then).
    pub fn set_active_theme(
        &mut self,
        profile_id: &str,
        digest: &str,
    ) -> Result<ActiveTheme, StorageError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StorageError::db("begin activation"))?;
        let theme = active::activate_in(&tx, profile_id, digest)?;
        tx.commit().map_err(StorageError::db("commit activation"))?;
        self.write_active_record()?;
        Ok(theme)
    }

    /// Mirror the database's active theme into the record file. Call after
    /// committing a transaction that used [`activate_in`](crate::activate_in).
    ///
    /// # Errors
    ///
    /// [`ErrorCode::Database`](crate::ErrorCode::Database) or
    /// [`ErrorCode::Io`](crate::ErrorCode::Io).
    pub fn write_active_record(&self) -> Result<(), StorageError> {
        let theme = self.active_theme()?;
        active::write_record(&self.paths.active_record_file(), theme.as_ref())
    }

    /// On a healthy open, bring a stale record (a crash between commit and
    /// record write) up to date. A database without an active profile keeps
    /// the record: it may be the only copy left.
    fn reconcile_record(&self) -> Result<(), StorageError> {
        let Some(theme) = self.active_theme()? else {
            return Ok(());
        };
        let path = self.paths.active_record_file();
        if active::read_record(&path).as_ref() != Some(&theme) {
            active::write_record(&path, Some(&theme))?;
        }
        Ok(())
    }
}

/// Read the active theme without migrating or repairing anything: from the
/// database if it is readable at any known schema version, otherwise from
/// the active-theme record.
///
/// This is what recovery UI and `glint safe-mode` use when
/// [`Storage::open`] failed (for example after a failed migration).
pub fn read_active_theme(paths: &GlintPaths) -> Option<ActiveTheme> {
    let db = paths.database_file();
    if db.is_file()
        && let Ok(conn) = Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        && let Ok(Some(theme)) = active::query_active(&conn)
    {
        return Some(theme);
    }
    active::read_record(&paths.active_record_file())
}

fn open_and_migrate(
    path: &Path,
    migrations: &[Migration],
) -> Result<(Connection, u32), OpenFailure> {
    let conn = Connection::open(path).map_err(StorageError::db("open database"))?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(StorageError::db("set busy timeout"))?;
    // Health first: a non-database file fails here with NOTADB.
    let check: String = match conn.query_row("PRAGMA quick_check", [], |row| row.get(0)) {
        Ok(value) => value,
        Err(error) if is_corruption(&error) => {
            return Err(OpenFailure::Corrupt(format!(
                "unreadable database: {error}"
            )));
        }
        Err(error) => return Err(StorageError::db("check database")(error).into()),
    };
    if check != "ok" {
        return Err(OpenFailure::Corrupt(format!("quick_check: {check}")));
    }
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = FULL;")
        .map_err(StorageError::db("configure database"))?;

    let latest = migrations.last().map_or(0, |m| m.version);
    let current = user_version(&conn)?;
    if current > latest {
        return Err(StorageError::SchemaTooNew {
            found: current,
            supported: latest,
        }
        .into());
    }
    // The applied part of the schema must be intact.
    for migration in migrations.iter().filter(|m| m.version <= current) {
        if let Some(missing) = missing_table(&conn, migration.tables)? {
            return Err(OpenFailure::Corrupt(format!(
                "schema version {current} but table {missing} (migration {:03}) is missing",
                migration.version
            )));
        }
    }
    let mut conn = conn;
    let mut version = current;
    for migration in migrations.iter().filter(|m| m.version > current) {
        apply(&mut conn, migration)?;
        version = migration.version;
    }
    Ok((conn, version))
}

fn apply(conn: &mut Connection, migration: &Migration) -> Result<(), StorageError> {
    let failed = |source| StorageError::MigrationFailed {
        version: migration.version,
        name: migration.name,
        source,
    };
    // Dropping the transaction on any error path rolls it back.
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(failed)?;
    tx.execute_batch(migration.sql).map_err(failed)?;
    tx.pragma_update(None, "user_version", migration.version)
        .map_err(failed)?;
    if let Some(missing) = missing_table(&tx, migration.tables)? {
        return Err(failed(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_SCHEMA),
            Some(format!("migration did not create table {missing}")),
        )));
    }
    tx.commit().map_err(failed)
}

fn user_version(conn: &Connection) -> Result<u32, StorageError> {
    let value: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(StorageError::db("read schema version"))?;
    Ok(u32::try_from(value).unwrap_or(u32::MAX))
}

fn missing_table(
    conn: &Connection,
    tables: &'static [&'static str],
) -> Result<Option<&'static str>, StorageError> {
    for table in tables {
        let found: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .optional()
            .map_err(StorageError::db("inspect schema"))?;
        if found.is_none() {
            return Ok(Some(table));
        }
    }
    Ok(None)
}

fn is_corruption(error: &rusqlite::Error) -> bool {
    matches!(
        error.sqlite_error_code(),
        Some(rusqlite::ErrorCode::NotADatabase | rusqlite::ErrorCode::DatabaseCorrupt)
    )
}

/// Move a damaged database (and its rollback journal) aside. Never deletes.
fn quarantine(db: &Path) -> Result<PathBuf, StorageError> {
    let stamp = crate::unix_now();
    let mut target = db.with_extension(format!("sqlite3.corrupt-{stamp}"));
    let mut n = 0u32;
    while target.exists() {
        n += 1;
        target = db.with_extension(format!("sqlite3.corrupt-{stamp}-{n}"));
    }
    fs::rename(db, &target).map_err(StorageError::io("quarantine corrupt database", db))?;
    let journal = db.with_extension("sqlite3-journal");
    if journal.exists() {
        let mut moved = target.clone().into_os_string();
        moved.push("-journal");
        fs::rename(&journal, PathBuf::from(moved))
            .map_err(StorageError::io("quarantine database journal", &journal))?;
    }
    Ok(target)
}
