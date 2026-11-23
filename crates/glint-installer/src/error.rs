//! Typed installer errors with stable codes.

use std::fmt;
use std::io;
use std::path::PathBuf;

use glint_storage::{ErrorCategory, StorageError};

use crate::fault::FaultPoint;

/// Stable, machine-readable installer error codes (`GLINT_INSTALL_*`).
///
/// Codes are only ever added, never renamed or reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ErrorCode {
    /// The state database failed; see [`InstallError::storage_error`].
    Storage,
    /// A filesystem operation failed.
    Io,
    /// The disk is full (the plan's `filesystem-full`).
    FilesystemFull,
    /// Another installer holds the installer lock.
    Busy,
    /// The candidate was not staged under this installer's staging root, so
    /// it cannot be moved into the store with one rename.
    CandidateOutsideStaging,
    /// The provenance record does not describe the candidate (different ID,
    /// version or digest).
    OriginMismatch,
    /// No installed package has this digest.
    NotInstalled,
    /// The package is the active theme or pinned by a profile; removal is
    /// refused.
    InUse,
    /// Something unexpected already occupies the package's store entry.
    StoreConflict,
    /// A fault injector stopped the operation at a named point, simulating a
    /// crash: nothing was cleaned up.
    Interrupted,
}

impl ErrorCode {
    /// Every code, in declaration order.
    pub const ALL: &'static [ErrorCode] = &[
        ErrorCode::Storage,
        ErrorCode::Io,
        ErrorCode::FilesystemFull,
        ErrorCode::Busy,
        ErrorCode::CandidateOutsideStaging,
        ErrorCode::OriginMismatch,
        ErrorCode::NotInstalled,
        ErrorCode::InUse,
        ErrorCode::StoreConflict,
        ErrorCode::Interrupted,
    ];

    /// The stable string form, e.g. `GLINT_INSTALL_IN_USE`.
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Storage => "GLINT_INSTALL_STORAGE",
            ErrorCode::Io => "GLINT_INSTALL_IO",
            ErrorCode::FilesystemFull => "GLINT_INSTALL_FILESYSTEM_FULL",
            ErrorCode::Busy => "GLINT_INSTALL_BUSY",
            ErrorCode::CandidateOutsideStaging => "GLINT_INSTALL_CANDIDATE_OUTSIDE_STAGING",
            ErrorCode::OriginMismatch => "GLINT_INSTALL_ORIGIN_MISMATCH",
            ErrorCode::NotInstalled => "GLINT_INSTALL_NOT_INSTALLED",
            ErrorCode::InUse => "GLINT_INSTALL_IN_USE",
            ErrorCode::StoreConflict => "GLINT_INSTALL_STORE_CONFLICT",
            ErrorCode::Interrupted => "GLINT_INSTALL_INTERRUPTED",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Every way installing, removing or recovering can fail.
#[derive(Debug)]
#[non_exhaustive]
pub enum InstallError {
    /// The state database failed.
    Storage(StorageError),
    /// A filesystem operation failed.
    Io {
        /// The step that failed.
        context: &'static str,
        /// The path involved.
        path: PathBuf,
        /// The underlying error.
        source: io::Error,
    },
    /// Another installer holds the lock.
    Busy(PathBuf),
    /// The candidate is not in this installer's staging root.
    CandidateOutsideStaging(PathBuf),
    /// Provenance and candidate disagree.
    OriginMismatch(&'static str),
    /// No package has this digest.
    NotInstalled(String),
    /// Removal refused: the package is active or pinned.
    InUse {
        /// The package digest.
        digest: String,
        /// Profiles that reference it.
        profiles: Vec<String>,
    },
    /// The store entry is occupied unexpectedly.
    StoreConflict(PathBuf),
    /// Stopped by a fault injector (simulated crash).
    Interrupted(FaultPoint),
}

impl InstallError {
    /// The stable error code.
    pub fn code(&self) -> ErrorCode {
        match self {
            InstallError::Storage(_) => ErrorCode::Storage,
            InstallError::Io { source, .. } if source.kind() == io::ErrorKind::StorageFull => {
                ErrorCode::FilesystemFull
            }
            InstallError::Io { .. } => ErrorCode::Io,
            InstallError::Busy(_) => ErrorCode::Busy,
            InstallError::CandidateOutsideStaging(_) => ErrorCode::CandidateOutsideStaging,
            InstallError::OriginMismatch(_) => ErrorCode::OriginMismatch,
            InstallError::NotInstalled(_) => ErrorCode::NotInstalled,
            InstallError::InUse { .. } => ErrorCode::InUse,
            InstallError::StoreConflict(_) => ErrorCode::StoreConflict,
            InstallError::Interrupted(_) => ErrorCode::Interrupted,
        }
    }

    /// The broad category (unsupported, permission-denied, unavailable,
    /// timeout, failure).
    pub fn category(&self) -> ErrorCategory {
        match self {
            InstallError::Storage(error) => error.category(),
            InstallError::Io { source, .. } => match source.kind() {
                io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem => {
                    ErrorCategory::PermissionDenied
                }
                io::ErrorKind::NotFound => ErrorCategory::Unavailable,
                io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => ErrorCategory::Timeout,
                io::ErrorKind::Unsupported => ErrorCategory::Unsupported,
                _ => ErrorCategory::Failure,
            },
            InstallError::Busy(_) => ErrorCategory::Unavailable,
            _ => ErrorCategory::Failure,
        }
    }

    /// The storage error behind [`ErrorCode::Storage`].
    pub fn storage_error(&self) -> Option<&StorageError> {
        match self {
            InstallError::Storage(error) => Some(error),
            _ => None,
        }
    }

    pub(crate) fn io(
        context: &'static str,
        path: impl Into<PathBuf>,
    ) -> impl FnOnce(io::Error) -> InstallError {
        let path = path.into();
        move |source| InstallError::Io {
            context,
            path,
            source,
        }
    }

    pub(crate) fn db(context: &'static str) -> impl FnOnce(glint_storage::rusqlite::Error) -> Self {
        move |source| InstallError::Storage(StorageError::Database { context, source })
    }
}

impl From<StorageError> for InstallError {
    fn from(error: StorageError) -> Self {
        InstallError::Storage(error)
    }
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: ", self.code())?;
        match self {
            InstallError::Storage(error) => write!(f, "{error}"),
            InstallError::Io {
                context,
                path,
                source,
            } => write!(f, "{context} ({}): {source}", path.display()),
            InstallError::Busy(path) => write!(
                f,
                "another Glint installer holds {}; try again when it finishes",
                path.display()
            ),
            InstallError::CandidateOutsideStaging(path) => write!(
                f,
                "candidate staged at {} is not inside the installer's staging directory",
                path.display()
            ),
            InstallError::OriginMismatch(field) => {
                write!(f, "provenance does not match the package ({field})")
            }
            InstallError::NotInstalled(digest) => write!(f, "no installed package {digest}"),
            InstallError::InUse { digest, profiles } => write!(
                f,
                "package {digest} is used by profile(s) {}; switch or unpin first",
                profiles.join(", ")
            ),
            InstallError::StoreConflict(path) => {
                write!(
                    f,
                    "unexpected entry in the package store: {}",
                    path.display()
                )
            }
            InstallError::Interrupted(point) => {
                write!(f, "interrupted at {point:?} (simulated crash)")
            }
        }
    }
}

impl std::error::Error for InstallError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            InstallError::Storage(error) => Some(error),
            InstallError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
