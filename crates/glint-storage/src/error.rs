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
}

impl ErrorCode {
    /// Every code, in declaration order.
    pub const ALL: &'static [ErrorCode] = &[
        ErrorCode::Io,
        ErrorCode::PathUnavailable,
        ErrorCode::PathNotAbsolute,
        ErrorCode::PathNotDirectory,
        ErrorCode::PathOutsideRoots,
    ];

    /// The stable string form, e.g. `GLINT_STORAGE_PATH_UNAVAILABLE`.
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Io => "GLINT_STORAGE_IO",
            ErrorCode::PathUnavailable => "GLINT_STORAGE_PATH_UNAVAILABLE",
            ErrorCode::PathNotAbsolute => "GLINT_STORAGE_PATH_NOT_ABSOLUTE",
            ErrorCode::PathNotDirectory => "GLINT_STORAGE_PATH_NOT_DIRECTORY",
            ErrorCode::PathOutsideRoots => "GLINT_STORAGE_PATH_OUTSIDE_ROOTS",
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
        }
    }

    /// The broad category of the failure.
    pub fn category(&self) -> ErrorCategory {
        match self {
            StorageError::Io { source, .. } => io_category(source),
            StorageError::PathUnavailable { .. } | StorageError::PathNotDirectory(_) => {
                ErrorCategory::Unavailable
            }
            StorageError::PathNotAbsolute { .. } | StorageError::PathOutsideRoots(_) => {
                ErrorCategory::Failure
            }
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
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StorageError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
