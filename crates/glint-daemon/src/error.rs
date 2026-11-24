//! Typed daemon errors with stable codes.

use std::fmt;

use glint_installer::InstallError;
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
}

impl ErrorCode {
    /// Every code, in declaration order.
    pub const ALL: &'static [ErrorCode] = &[
        ErrorCode::StateUnavailable,
        ErrorCode::InvalidState,
        ErrorCode::FallbackDisabled,
        ErrorCode::DefaultThemeInvalid,
        ErrorCode::FlushFailed,
    ];

    /// The stable string form, e.g. `GLINT_DAEMON_STATE_UNAVAILABLE`.
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorCode::StateUnavailable => "GLINT_DAEMON_STATE_UNAVAILABLE",
            ErrorCode::InvalidState => "GLINT_DAEMON_INVALID_STATE",
            ErrorCode::FallbackDisabled => "GLINT_DAEMON_FALLBACK_DISABLED",
            ErrorCode::DefaultThemeInvalid => "GLINT_DAEMON_DEFAULT_THEME_INVALID",
            ErrorCode::FlushFailed => "GLINT_DAEMON_FLUSH_FAILED",
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
        }
    }

    /// The broad category (unsupported, permission-denied, unavailable,
    /// timeout, failure), taken from the underlying error where there is one.
    pub fn category(&self) -> ErrorCategory {
        match self {
            DaemonError::State(error) => error.category(),
            DaemonError::Flush(error) => error.category(),
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
            DaemonError::State(error) => Some(error),
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
        }
    }
}

impl std::error::Error for DaemonError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DaemonError::State(error) => Some(error),
            DaemonError::Flush(error) => Some(error),
            _ => None,
        }
    }
}

impl From<InstallError> for DaemonError {
    fn from(error: InstallError) -> Self {
        DaemonError::State(error)
    }
}
