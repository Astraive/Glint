//! Typed storage errors with stable codes.

use std::fmt;
use std::io;
use std::path::PathBuf;

/// Stable, machine-readable storage error codes.
///
/// The string form returned by [`ErrorCode::as_str`] (`GLINT_STORAGE_*`) is
/// the stable contract. Codes are only ever added, never renamed or reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ErrorCode {
    /// An operating-system I/O operation failed.
    Io,
    /// The OS could not report a base directory (no home directory, no known
    /// folder).
    PathUnavailable,
    /// A root path (an override or an OS answer) is not absolute.
    PathNotAbsolute,
    /// A Glint-owned directory is a file, a symlink, a junction or another
    /// reparse point.
    PathNotDirectory,
    /// A path that Glint was asked to write lies outside the resolved roots.
    PathOutsideRoots,
    /// A SQLite operation failed.
    Database,
    /// A schema migration failed and was rolled back; the database is
    /// unchanged at its previous version.
    MigrationFailed,
    /// The database was written by a newer Glint (schema version above the
    /// newest migration this build knows). It is left untouched.
    SchemaTooNew,
    /// The database is corrupt and could not be recovered.
    DatabaseCorrupt,
    /// A value (profile ID, digest) is not acceptable for storage.
    InvalidValue,
}

impl ErrorCode {
    /// Every code, in declaration order.
    pub const ALL: &'static [ErrorCode] = &[
        ErrorCode::Io,
        ErrorCode::PathUnavailable,
        ErrorCode::PathNotAbsolute,
        ErrorCode::PathNotDirectory,
        ErrorCode::PathOutsideRoots,
        ErrorCode::Database,
        ErrorCode::MigrationFailed,
        ErrorCode::SchemaTooNew,
        ErrorCode::DatabaseCorrupt,
        ErrorCode::InvalidValue,
    ];

    /// The stable string form, e.g. `GLINT_STORAGE_PATH_UNAVAILABLE`.
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Io => "GLINT_STORAGE_IO",
            ErrorCode::PathUnavailable => "GLINT_STORAGE_PATH_UNAVAILABLE",
            ErrorCode::PathNotAbsolute => "GLINT_STORAGE_PATH_NOT_ABSOLUTE",
            ErrorCode::PathNotDirectory => "GLINT_STORAGE_PATH_NOT_DIRECTORY",
            ErrorCode::PathOutsideRoots => "GLINT_STORAGE_PATH_OUTSIDE_ROOTS",
            ErrorCode::Database => "GLINT_STORAGE_DATABASE",
            ErrorCode::MigrationFailed => "GLINT_STORAGE_MIGRATION_FAILED",
            ErrorCode::SchemaTooNew => "GLINT_STORAGE_SCHEMA_TOO_NEW",
            ErrorCode::DatabaseCorrupt => "GLINT_STORAGE_DATABASE_CORRUPT",
            ErrorCode::InvalidValue => "GLINT_STORAGE_INVALID_VALUE",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The broad class of a failure, so callers can tell "not possible here"
/// from "not allowed" from "try again" without matching every code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorCategory {
    /// The operation is not supported in this build or on this platform.
    Unsupported,
    /// The OS refused access.
    PermissionDenied,
    /// A resource (a directory, the database) is missing or unusable.
    Unavailable,
    /// The operation gave up waiting (a busy database).
    Timeout,
    /// Anything else.
    Failure,
}

/// Every way resolving paths or using the state database can fail.
#[derive(Debug)]
#[non_exhaustive]
pub enum StorageError {
    /// An I/O operation failed. `context` names the step.
    Io {
        /// The step that failed.
        context: &'static str,
        /// The path involved.
        path: PathBuf,
        /// The underlying error.
        source: io::Error,
    },
    /// The OS did not report a base directory.
    PathUnavailable {
        /// Which base directory (`config`, `data`, `cache`).
        which: &'static str,
    },
    /// A root is not absolute.
    PathNotAbsolute {
        /// What the path is (`GLINT_HOME override`, `config base directory`, ...).
        what: &'static str,
        /// The offending path.
        path: PathBuf,
    },
    /// A Glint-owned directory is not a real directory.
    PathNotDirectory(PathBuf),
    /// A path outside the resolved roots.
    PathOutsideRoots(PathBuf),
    /// A SQLite operation failed.
    Database {
        /// The step that failed.
        context: &'static str,
        /// The underlying error.
        source: rusqlite::Error,
    },
    /// A migration failed and was rolled back.
    MigrationFailed {
        /// The migration's version.
        version: u32,
        /// The migration's name.
        name: &'static str,
        /// The underlying error.
        source: rusqlite::Error,
    },
    /// The database schema is newer than this build supports.
    SchemaTooNew {
        /// The database's `user_version`.
        found: u32,
        /// The newest version this build knows.
        supported: u32,
    },
    /// The database is corrupt and recovery failed.
    DatabaseCorrupt(String),
    /// An unacceptable value.
    InvalidValue {
        /// What the value is.
        what: &'static str,
        /// Why it was refused.
        reason: &'static str,
    },
}

impl StorageError {
    /// The stable error code.
    pub fn code(&self) -> ErrorCode {
        match self {
            StorageError::Io { .. } => ErrorCode::Io,
            StorageError::PathUnavailable { .. } => ErrorCode::PathUnavailable,
            StorageError::PathNotAbsolute { .. } => ErrorCode::PathNotAbsolute,
            StorageError::PathNotDirectory(_) => ErrorCode::PathNotDirectory,
            StorageError::PathOutsideRoots(_) => ErrorCode::PathOutsideRoots,
            StorageError::Database { .. } => ErrorCode::Database,
            StorageError::MigrationFailed { .. } => ErrorCode::MigrationFailed,
            StorageError::SchemaTooNew { .. } => ErrorCode::SchemaTooNew,
            StorageError::DatabaseCorrupt(_) => ErrorCode::DatabaseCorrupt,
            StorageError::InvalidValue { .. } => ErrorCode::InvalidValue,
        }
    }

    /// The broad category of the failure.
    pub fn category(&self) -> ErrorCategory {
        match self {
            StorageError::Io { source, .. } => io_category(source),
            StorageError::PathUnavailable { .. }
            | StorageError::PathNotDirectory(_)
            | StorageError::DatabaseCorrupt(_) => ErrorCategory::Unavailable,
            StorageError::Database { source, .. }
            | StorageError::MigrationFailed { source, .. } => sqlite_category(source),
            StorageError::SchemaTooNew { .. } => ErrorCategory::Unsupported,
            StorageError::PathNotAbsolute { .. }
            | StorageError::PathOutsideRoots(_)
            | StorageError::InvalidValue { .. } => ErrorCategory::Failure,
        }
    }

    /// The SQLite primary error code behind a database or migration error.
    pub fn sqlite_code(&self) -> Option<rusqlite::ErrorCode> {
        match self {
            StorageError::Database { source, .. }
            | StorageError::MigrationFailed { source, .. } => source.sqlite_error_code(),
            _ => None,
        }
    }

    pub(crate) fn io(
        context: &'static str,
        path: impl Into<PathBuf>,
    ) -> impl FnOnce(io::Error) -> StorageError {
        let path = path.into();
        move |source| StorageError::Io {
            context,
            path,
            source,
        }
    }

    pub(crate) fn db(context: &'static str) -> impl FnOnce(rusqlite::Error) -> StorageError {
        move |source| StorageError::Database { context, source }
    }
}

/// Map a SQLite error to a category.
pub(crate) fn sqlite_category(error: &rusqlite::Error) -> ErrorCategory {
    use rusqlite::ErrorCode as Sql;
    match error.sqlite_error_code() {
        Some(Sql::DatabaseBusy | Sql::DatabaseLocked) => ErrorCategory::Timeout,
        Some(Sql::PermissionDenied | Sql::ReadOnly | Sql::AuthorizationForStatementDenied) => {
            ErrorCategory::PermissionDenied
        }
        Some(Sql::CannotOpen | Sql::NotADatabase | Sql::DatabaseCorrupt) => {
            ErrorCategory::Unavailable
        }
        _ => ErrorCategory::Failure,
    }
}

/// Map an I/O error to a category.
pub(crate) fn io_category(error: &io::Error) -> ErrorCategory {
    match error.kind() {
        io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem => {
            ErrorCategory::PermissionDenied
        }
        io::ErrorKind::NotFound => ErrorCategory::Unavailable,
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => ErrorCategory::Timeout,
        io::ErrorKind::Unsupported => ErrorCategory::Unsupported,
        _ => ErrorCategory::Failure,
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: ", self.code())?;
        match self {
            StorageError::Io {
                context,
                path,
                source,
            } => write!(f, "{context} ({}): {source}", path.display()),
            StorageError::PathUnavailable { which } => {
                write!(f, "the operating system reports no {which} directory")
            }
            StorageError::PathNotAbsolute { what, path } => {
                write!(f, "{what} {} is not an absolute path", path.display())
            }
            StorageError::PathNotDirectory(path) => write!(
                f,
                "{} is not a real directory (file, link or reparse point)",
                path.display()
            ),
            StorageError::PathOutsideRoots(path) => {
                write!(f, "{} is outside Glint's directories", path.display())
            }
            StorageError::Database { context, source } => write!(f, "{context}: {source}"),
            StorageError::MigrationFailed {
                version,
                name,
                source,
            } => write!(
                f,
                "migration {version:03} ({name}) failed and was rolled back: {source}"
            ),
            StorageError::SchemaTooNew { found, supported } => write!(
                f,
                "database schema version {found} is newer than this build supports ({supported})"
            ),
            StorageError::DatabaseCorrupt(detail) => {
                write!(f, "database is corrupt and recovery failed: {detail}")
            }
            StorageError::InvalidValue { what, reason } => write!(f, "invalid {what}: {reason}"),
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StorageError::Io { source, .. } => Some(source),
            StorageError::Database { source, .. }
            | StorageError::MigrationFailed { source, .. } => Some(source),
            _ => None,
        }
    }
}
