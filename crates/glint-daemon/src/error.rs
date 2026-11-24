//! Typed daemon errors with stable codes.

use std::fmt;

use glint_installer::InstallError;
use glint_sources::SourceError;
use glint_storage::{ErrorCategory, StorageError};

use crate::service::FallbackReason;

/// Stable, machine-readable daemon error codes.
///
/// The string form returned by [`ErrorCode::as_str`] (`GLINT_DAEMON_*`) is
/// the stable contract. Codes are only ever added, never renamed or reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ErrorCode {
    /// Opening the state database, the installer or startup recovery failed.
    StateUnavailable,
    /// A step was called before the step it depends on (for example
    /// selecting a theme before the state was opened).
    InvalidState,
    /// The active theme cannot be used and `shell.use_default_fallback` is
    /// off, so no theme is presented.
    FallbackDisabled,
    /// The bundled default theme failed validation (a build defect).
    DefaultThemeInvalid,
    /// Flushing state at shutdown failed.
    FlushFailed,
    /// A theme operation failed in the installer or while staging the
    /// package (CLI-006); the wrapped `GLINT_INSTALL_*` / `GLINT_SOURCE_*`
    /// error says why. Nothing was changed.
    OperationFailed,
    /// A theme operation's arguments are unusable (a relative path).
    InvalidRequest,
    /// This daemon does not perform theme operations.
    OperationUnsupported,
}

impl ErrorCode {
    /// Every code, in declaration order.
    pub const ALL: &'static [ErrorCode] = &[
        ErrorCode::StateUnavailable,
        ErrorCode::InvalidState,
        ErrorCode::FallbackDisabled,
        ErrorCode::DefaultThemeInvalid,
        ErrorCode::FlushFailed,
        ErrorCode::OperationFailed,
        ErrorCode::InvalidRequest,
        ErrorCode::OperationUnsupported,
    ];

    /// The stable string form, e.g. `GLINT_DAEMON_STATE_UNAVAILABLE`.
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorCode::StateUnavailable => "GLINT_DAEMON_STATE_UNAVAILABLE",
            ErrorCode::InvalidState => "GLINT_DAEMON_INVALID_STATE",
            ErrorCode::FallbackDisabled => "GLINT_DAEMON_FALLBACK_DISABLED",
            ErrorCode::DefaultThemeInvalid => "GLINT_DAEMON_DEFAULT_THEME_INVALID",
            ErrorCode::FlushFailed => "GLINT_DAEMON_FLUSH_FAILED",
            ErrorCode::OperationFailed => "GLINT_DAEMON_OPERATION_FAILED",
            ErrorCode::InvalidRequest => "GLINT_DAEMON_INVALID_REQUEST",
            ErrorCode::OperationUnsupported => "GLINT_DAEMON_OPERATION_UNSUPPORTED",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Every way a daemon step can fail.
#[derive(Debug)]
#[non_exhaustive]
pub enum DaemonError {
    /// Opening storage, taking the installer lock or recovery failed.
    State(InstallError),
    /// A step ran out of order.
    InvalidState {
        /// The step that was called.
        step: &'static str,
        /// What it needs first.
        requires: &'static str,
    },
    /// The active theme is unusable and the fallback is disabled.
    FallbackDisabled(FallbackReason),
    /// The bundled default theme is invalid.
    DefaultThemeInvalid(String),
    /// Flushing state at shutdown failed.
    Flush(StorageError),
    /// A theme operation failed in the installer.
    Operation(InstallError),
    /// Staging a package for a theme operation failed.
    Source(SourceError),
    /// A theme operation's arguments are unusable.
    InvalidRequest(String),
    /// This daemon does not perform the named theme operation.
    OperationUnsupported(&'static str),
}

impl DaemonError {
    /// The stable error code.
    pub fn code(&self) -> ErrorCode {
        match self {
            DaemonError::State(_) => ErrorCode::StateUnavailable,
            DaemonError::InvalidState { .. } => ErrorCode::InvalidState,
            DaemonError::FallbackDisabled(_) => ErrorCode::FallbackDisabled,
            DaemonError::DefaultThemeInvalid(_) => ErrorCode::DefaultThemeInvalid,
            DaemonError::Flush(_) => ErrorCode::FlushFailed,
            DaemonError::Operation(_) | DaemonError::Source(_) => ErrorCode::OperationFailed,
            DaemonError::InvalidRequest(_) => ErrorCode::InvalidRequest,
            DaemonError::OperationUnsupported(_) => ErrorCode::OperationUnsupported,
        }
    }

    /// The broad category (unsupported, permission-denied, unavailable,
    /// timeout, failure), taken from the underlying error where there is one.
    pub fn category(&self) -> ErrorCategory {
        match self {
            DaemonError::State(error) => error.category(),
            DaemonError::Flush(error) => error.category(),
            DaemonError::Operation(error) => error.category(),
            DaemonError::Source(error) => match error.code() {
                glint_sources::SourceErrorCode::PermissionDenied => ErrorCategory::PermissionDenied,
                glint_sources::SourceErrorCode::Unsupported => ErrorCategory::Unsupported,
                _ => ErrorCategory::Failure,
            },
            DaemonError::OperationUnsupported(_) => ErrorCategory::Unsupported,
            DaemonError::InvalidRequest(_) => ErrorCategory::Failure,
            DaemonError::FallbackDisabled(_) => ErrorCategory::Unavailable,
            DaemonError::InvalidState { .. } | DaemonError::DefaultThemeInvalid(_) => {
                ErrorCategory::Failure
            }
        }
    }

    /// The installer error behind [`ErrorCode::StateUnavailable`], whose own
    /// code is `GLINT_INSTALL_*` (`GLINT_INSTALL_STORAGE` wraps a
    /// `GLINT_STORAGE_*` error).
    pub fn install_error(&self) -> Option<&InstallError> {
        match self {
            DaemonError::State(error) | DaemonError::Operation(error) => Some(error),
            _ => None,
        }
    }

    /// The staging error behind a failed `theme-import` operation
    /// (`GLINT_SOURCE_*`, possibly wrapping a `GLINT_PACKAGE_*` verdict).
    pub fn source_error(&self) -> Option<&SourceError> {
        match self {
            DaemonError::Source(error) => Some(error),
            _ => None,
        }
    }
}

impl fmt::Display for DaemonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DaemonError::State(error) => write!(f, "state unavailable: {error}"),
            DaemonError::InvalidState { step, requires } => {
                write!(f, "{step} called before {requires}")
            }
            DaemonError::FallbackDisabled(reason) => write!(
                f,
                "the active theme cannot be used ({reason}) and shell.use_default_fallback is off"
            ),
            DaemonError::DefaultThemeInvalid(detail) => {
                write!(f, "the bundled default theme is invalid: {detail}")
            }
            DaemonError::Flush(error) => write!(f, "flushing state failed: {error}"),
            DaemonError::Operation(error) => write!(f, "{error}"),
            DaemonError::Source(error) => write!(f, "{error}"),
            DaemonError::InvalidRequest(detail) => f.write_str(detail),
            DaemonError::OperationUnsupported(name) => {
                write!(f, "this daemon does not perform `{name}` operations")
            }
        }
    }
}

impl std::error::Error for DaemonError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DaemonError::State(error) => Some(error),
            DaemonError::Flush(error) => Some(error),
            DaemonError::Operation(error) => Some(error),
            DaemonError::Source(error) => Some(error),
            _ => None,
        }
    }
}

impl From<InstallError> for DaemonError {
    fn from(error: InstallError) -> Self {
        DaemonError::State(error)
    }
}
