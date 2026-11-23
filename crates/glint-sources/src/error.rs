//! Typed source errors with stable codes.

use std::fmt;
use std::io;
use std::path::PathBuf;

use glint_package::PackageError;

use crate::SourceKind;

/// Stable, machine-readable source error codes.
///
/// The string form returned by [`SourceErrorCode::as_str`]
/// (`GLINT_SOURCE_*`) is the stable contract. Codes are only ever added,
/// never renamed or reused for a different meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum SourceErrorCode {
    /// The local path does not exist.
    NotFound,
    /// A local archive source is not a regular file.
    NotAFile,
    /// A local directory source is not a directory.
    NotADirectory,
    /// The operating system refused access to the source.
    PermissionDenied,
    /// This source kind is not implemented by the resolver (remote kinds
    /// until M4). Never a fake success.
    Unsupported,
    /// The package layer refused or failed on the source content; see
    /// [`SourceError::package_error`] for the `GLINT_PACKAGE_*` code.
    PackageRejected,
    /// The caller's staging root is missing, not a directory or a link.
    StagingRootInvalid,
    /// Another I/O failure that is not a verdict about the source.
    Io,
}

impl SourceErrorCode {
    /// Every code, in declaration order.
    pub const ALL: &'static [SourceErrorCode] = &[
        SourceErrorCode::NotFound,
        SourceErrorCode::NotAFile,
        SourceErrorCode::NotADirectory,
        SourceErrorCode::PermissionDenied,
        SourceErrorCode::Unsupported,
        SourceErrorCode::PackageRejected,
        SourceErrorCode::StagingRootInvalid,
        SourceErrorCode::Io,
    ];

    /// The stable string form, e.g. `GLINT_SOURCE_NOT_FOUND`.
    pub const fn as_str(self) -> &'static str {
        match self {
            SourceErrorCode::NotFound => "GLINT_SOURCE_NOT_FOUND",
            SourceErrorCode::NotAFile => "GLINT_SOURCE_NOT_A_FILE",
            SourceErrorCode::NotADirectory => "GLINT_SOURCE_NOT_A_DIRECTORY",
            SourceErrorCode::PermissionDenied => "GLINT_SOURCE_PERMISSION_DENIED",
            SourceErrorCode::Unsupported => "GLINT_SOURCE_UNSUPPORTED",
            SourceErrorCode::PackageRejected => "GLINT_SOURCE_PACKAGE_REJECTED",
            SourceErrorCode::StagingRootInvalid => "GLINT_SOURCE_STAGING_ROOT_INVALID",
            SourceErrorCode::Io => "GLINT_SOURCE_IO",
        }
    }
}

impl fmt::Display for SourceErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Every way staging a source can fail.
///
/// Each variant maps to exactly one [`SourceErrorCode`] through
/// [`SourceError::code`]. Match on the code, never on the display text.
#[derive(Debug)]
#[non_exhaustive]
pub enum SourceError {
    /// The local path does not exist.
    NotFound(PathBuf),
    /// A local archive source is not a regular file.
    NotAFile(PathBuf),
    /// A local directory source is not a directory.
    NotADirectory(PathBuf),
    /// Access to the source was denied.
    PermissionDenied(PathBuf),
    /// The resolver does not implement this source kind yet.
    Unsupported {
        /// The requested kind.
        kind: SourceKind,
        /// Where the backend is planned, for the message.
        planned: &'static str,
    },
    /// The package layer refused or failed.
    PackageRejected(PackageError),
    /// The staging root is unusable.
    StagingRootInvalid(PathBuf),
    /// Another I/O failure.
    Io {
        /// The step that failed.
        context: &'static str,
        /// The underlying error.
        source: io::Error,
    },
}

impl SourceError {
    /// The stable error code for this error.
    pub fn code(&self) -> SourceErrorCode {
        match self {
            SourceError::NotFound(_) => SourceErrorCode::NotFound,
            SourceError::NotAFile(_) => SourceErrorCode::NotAFile,
            SourceError::NotADirectory(_) => SourceErrorCode::NotADirectory,
            SourceError::PermissionDenied(_) => SourceErrorCode::PermissionDenied,
            SourceError::Unsupported { .. } => SourceErrorCode::Unsupported,
            SourceError::PackageRejected(_) => SourceErrorCode::PackageRejected,
            SourceError::StagingRootInvalid(_) => SourceErrorCode::StagingRootInvalid,
            SourceError::Io { .. } => SourceErrorCode::Io,
        }
    }

    /// The `glint-package` error behind [`SourceErrorCode::PackageRejected`].
    pub fn package_error(&self) -> Option<&PackageError> {
        match self {
            SourceError::PackageRejected(error) => Some(error),
            _ => None,
        }
    }

    /// The kind behind [`SourceErrorCode::Unsupported`].
    pub fn unsupported_kind(&self) -> Option<SourceKind> {
        match self {
            SourceError::Unsupported { kind, .. } => Some(*kind),
            _ => None,
        }
    }

    /// Map a metadata lookup failure on `path` to a typed error.
    pub(crate) fn from_lookup(path: &std::path::Path, error: io::Error) -> SourceError {
        match error.kind() {
            io::ErrorKind::NotFound => SourceError::NotFound(path.to_path_buf()),
            io::ErrorKind::PermissionDenied => SourceError::PermissionDenied(path.to_path_buf()),
            _ => SourceError::Io {
                context: "inspect source path",
                source: error,
            },
        }
    }
}

impl From<PackageError> for SourceError {
    fn from(error: PackageError) -> Self {
        match error {
            PackageError::StagingRootInvalid(path) => SourceError::StagingRootInvalid(path),
            other => SourceError::PackageRejected(other),
        }
    }
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: ", self.code())?;
        match self {
            SourceError::NotFound(path) => write!(f, "{} does not exist", path.display()),
            SourceError::NotAFile(path) => {
                write!(f, "{} is not a regular file", path.display())
            }
            SourceError::NotADirectory(path) => {
                write!(f, "{} is not a directory", path.display())
            }
            SourceError::PermissionDenied(path) => {
                write!(f, "access to {} was denied", path.display())
            }
            SourceError::Unsupported { kind, planned } => write!(
                f,
                "{} sources are not implemented yet (planned: {planned})",
                kind.as_str()
            ),
            SourceError::PackageRejected(error) => write!(f, "{error}"),
            SourceError::StagingRootInvalid(path) => {
                write!(f, "staging root {} is unusable", path.display())
            }
            SourceError::Io { context, source } => write!(f, "{context}: {source}"),
        }
    }
}

impl std::error::Error for SourceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SourceError::PackageRejected(error) => Some(error),
            SourceError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
