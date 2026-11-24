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
    /// The release conflicts with an installed one under the identity
    /// policy (ADR 0006): the same package key from another origin, or the
    /// same origin and version with a different digest. Nothing changed;
    /// see [`InstallError::identity_outcome`].
    IdentityConflict,
    /// The source's origin cannot be expressed as an ADR 0006 origin.
    InvalidOrigin,
    /// A store entry no longer matches its content address or record
    /// (tampered with or damaged on disk). Reinstall the package.
    StoreCorrupt,
    /// Exporting failed: the package could not be re-packed or the exported
    /// archive did not pass the reader; see [`InstallError::package_error`].
    ExportFailed,
    /// The export destination is not a file path (a directory, or no file
    /// name).
    ExportDestinationInvalid,
    /// The bundled default theme archive embedded in the binary failed the
    /// reader (a build defect); see [`InstallError::source_error`].
    BundledInvalid,
    /// The package uses the reserved ID of the bundled default theme but
    /// is not the bundled release; nothing may shadow the fallback theme.
    ReservedId,
    /// A theme lookup matched several installed releases; name a version or
    /// digest.
    Ambiguous,
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
        ErrorCode::IdentityConflict,
        ErrorCode::InvalidOrigin,
        ErrorCode::StoreCorrupt,
        ErrorCode::ExportFailed,
        ErrorCode::ExportDestinationInvalid,
        ErrorCode::BundledInvalid,
        ErrorCode::ReservedId,
        ErrorCode::Ambiguous,
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
            ErrorCode::IdentityConflict => "GLINT_INSTALL_IDENTITY_CONFLICT",
            ErrorCode::InvalidOrigin => "GLINT_INSTALL_INVALID_ORIGIN",
            ErrorCode::StoreCorrupt => "GLINT_INSTALL_STORE_CORRUPT",
            ErrorCode::ExportFailed => "GLINT_INSTALL_EXPORT_FAILED",
            ErrorCode::ExportDestinationInvalid => "GLINT_INSTALL_EXPORT_DESTINATION_INVALID",
            ErrorCode::BundledInvalid => "GLINT_INSTALL_BUNDLED_INVALID",
            ErrorCode::ReservedId => "GLINT_INSTALL_RESERVED_ID",
            ErrorCode::Ambiguous => "GLINT_INSTALL_AMBIGUOUS",
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
    /// Refused by the identity policy.
    IdentityConflict {
        /// The `glint-schema` classification (`DigestConflict` or
        /// `OriginCollision`, with the installed origin or digest).
        outcome: Box<glint_schema::InstallOutcome>,
        /// The candidate's origin.
        candidate_origin: String,
        /// The candidate's digest (`sha256:...`).
        candidate_digest: String,
    },
    /// The origin cannot be mapped.
    InvalidOrigin(String),
    /// A store entry does not match its digest or record.
    StoreCorrupt {
        /// The package digest.
        digest: String,
        /// What did not match.
        reason: &'static str,
    },
    /// Re-packing or reading back an export failed.
    Export(glint_package::PackageError),
    /// The export destination is unusable.
    ExportDestinationInvalid(PathBuf),
    /// The embedded default theme archive could not be staged.
    Bundled(glint_sources::SourceError),
    /// A package claims the reserved default theme ID.
    ReservedId(String),
    /// A lookup matched several releases.
    Ambiguous {
        /// What was looked up.
        query: String,
        /// The matching releases as `id@version`.
        candidates: Vec<String>,
    },
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
            InstallError::IdentityConflict { .. } => ErrorCode::IdentityConflict,
            InstallError::InvalidOrigin(_) => ErrorCode::InvalidOrigin,
            InstallError::StoreCorrupt { .. } => ErrorCode::StoreCorrupt,
            InstallError::Export(_) => ErrorCode::ExportFailed,
            InstallError::ExportDestinationInvalid(_) => ErrorCode::ExportDestinationInvalid,
            InstallError::Bundled(_) => ErrorCode::BundledInvalid,
            InstallError::ReservedId(_) => ErrorCode::ReservedId,
            InstallError::Ambiguous { .. } => ErrorCode::Ambiguous,
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
            InstallError::Export(glint_package::PackageError::Io { source, .. })
                if source.kind() == io::ErrorKind::PermissionDenied =>
            {
                ErrorCategory::PermissionDenied
            }
            _ => ErrorCategory::Failure,
        }
    }

    /// The identity classification behind [`ErrorCode::IdentityConflict`];
    /// its `code()` is the `GLINT_IDENTITY_*` code.
    pub fn identity_outcome(&self) -> Option<&glint_schema::InstallOutcome> {
        match self {
            InstallError::IdentityConflict { outcome, .. } => Some(outcome),
            _ => None,
        }
    }

    /// The package error behind [`ErrorCode::ExportFailed`].
    pub fn package_error(&self) -> Option<&glint_package::PackageError> {
        match self {
            InstallError::Export(error) => Some(error),
            _ => None,
        }
    }

    /// The source error behind [`ErrorCode::BundledInvalid`].
    pub fn source_error(&self) -> Option<&glint_sources::SourceError> {
        match self {
            InstallError::Bundled(error) => Some(error),
            _ => None,
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
            InstallError::IdentityConflict {
                outcome,
                candidate_origin,
                candidate_digest,
            } => match outcome.as_ref() {
                glint_schema::InstallOutcome::OriginCollision { installed } => write!(
                    f,
                    "{}: already installed from {installed}; {candidate_origin} ({candidate_digest}) needs an explicit decision",
                    outcome.code()
                ),
                glint_schema::InstallOutcome::DigestConflict { installed } => write!(
                    f,
                    "{}: this version from {candidate_origin} is installed as {installed}, not {candidate_digest}",
                    outcome.code()
                ),
                other => write!(f, "{}", other.code()),
            },
            InstallError::InvalidOrigin(detail) => write!(f, "invalid origin: {detail}"),
            InstallError::StoreCorrupt { digest, reason } => write!(
                f,
                "store entry {digest} is damaged ({reason}); reinstall the package"
            ),
            InstallError::Export(error) => write!(f, "export failed: {error}"),
            InstallError::ExportDestinationInvalid(path) => write!(
                f,
                "export destination {} is not a file path",
                path.display()
            ),
            InstallError::Bundled(error) => {
                write!(f, "the bundled default theme is unusable: {error}")
            }
            InstallError::ReservedId(id) => write!(
                f,
                "{id} is reserved for the bundled default theme; this package is not it"
            ),
            InstallError::Ambiguous { query, candidates } => write!(
                f,
                "{query} matches several installed releases ({}); name one as ID@VERSION",
                candidates.join(", ")
            ),
        }
    }
}

impl std::error::Error for InstallError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            InstallError::Storage(error) => Some(error),
            InstallError::Io { source, .. } => Some(source),
            InstallError::Export(error) => Some(error),
            InstallError::Bundled(error) => Some(error),
            _ => None,
        }
    }
}
