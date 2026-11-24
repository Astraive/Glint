//! Typed core errors with stable codes.

use std::fmt;

use glint_daemon::DaemonError;
use glint_shell::ShellError;
use glint_storage::{ErrorCategory, StorageError};

use crate::lifecycle::BootStage;

/// Stable, machine-readable core error codes.
///
/// The string form returned by [`ErrorCode::as_str`] (`GLINT_CORE_*`) is the
/// stable contract. Codes are only ever added, never renamed or reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ErrorCode {
    /// Glint's directories could not be resolved or created.
    PathsUnavailable,
    /// A daemon stage failed (see [`CoreError::source_code`]).
    DaemonFailed,
    /// The shell failed to start or to release its surfaces.
    ShellFailed,
    /// The call does not fit the runtime's lifecycle state.
    InvalidState,
}

impl ErrorCode {
    /// Every code, in declaration order.
    pub const ALL: &'static [ErrorCode] = &[
        ErrorCode::PathsUnavailable,
        ErrorCode::DaemonFailed,
        ErrorCode::ShellFailed,
        ErrorCode::InvalidState,
    ];

    /// The stable string form, e.g. `GLINT_CORE_PATHS_UNAVAILABLE`.
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorCode::PathsUnavailable => "GLINT_CORE_PATHS_UNAVAILABLE",
            ErrorCode::DaemonFailed => "GLINT_CORE_DAEMON_FAILED",
            ErrorCode::ShellFailed => "GLINT_CORE_SHELL_FAILED",
            ErrorCode::InvalidState => "GLINT_CORE_INVALID_STATE",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A core failure: a stable [`ErrorCode`], the boot stage it happened in
/// (if any), the underlying component's stable code, a category that keeps
/// unsupported, permission-denied, unavailable, timeout and failure apart,
/// and a message (not stable).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreError {
    code: ErrorCode,
    stage: Option<BootStage>,
    source_code: Option<String>,
    category: ErrorCategory,
    message: String,
}

impl CoreError {
    /// A new error.
    pub fn new(
        code: ErrorCode,
        stage: Option<BootStage>,
        category: ErrorCategory,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            stage,
            source_code: None,
            category,
            message: message.into(),
        }
    }

    fn with_source_code(mut self, code: impl Into<String>) -> Self {
        self.source_code = Some(code.into());
        self
    }

    pub(crate) fn paths(error: &StorageError) -> Self {
        Self::new(
            ErrorCode::PathsUnavailable,
            Some(BootStage::ResolvePaths),
            error.category(),
            error.to_string(),
        )
        .with_source_code(error.code().as_str())
    }

    pub(crate) fn daemon(stage: Option<BootStage>, error: &DaemonError) -> Self {
        // The most specific stable code: the installer's or storage's code
        // when the daemon only wraps it.
        let source = match error.install_error() {
            Some(install) => match install.storage_error() {
                Some(storage) => storage.code().as_str().to_owned(),
                None => install.code().as_str().to_owned(),
            },
            None => error.code().as_str().to_owned(),
        };
        Self::new(
            ErrorCode::DaemonFailed,
            stage,
            error.category(),
            error.to_string(),
        )
        .with_source_code(source)
    }

    pub(crate) fn shell(stage: Option<BootStage>, error: &ShellError) -> Self {
        Self::new(
            ErrorCode::ShellFailed,
            stage,
            ErrorCategory::Failure,
            error.to_string(),
        )
        .with_source_code(error.code())
    }

    pub(crate) fn invalid_state(message: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::InvalidState,
            None,
            ErrorCategory::Failure,
            message,
        )
    }

    /// The stable code.
    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// The boot stage that failed, if the error happened during boot.
    pub fn stage(&self) -> Option<BootStage> {
        self.stage
    }

    /// The underlying component's stable code (`GLINT_STORAGE_*`,
    /// `GLINT_INSTALL_*`, `GLINT_DAEMON_*`, `GLINT_SHELL_*`), if any.
    pub fn source_code(&self) -> Option<&str> {
        self.source_code.as_deref()
    }

    /// The broad category.
    pub fn category(&self) -> ErrorCategory {
        self.category
    }

    /// Human-readable detail (not stable).
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code)?;
        if let Some(stage) = self.stage {
            write!(f, " at {stage}")?;
        }
        if let Some(source) = &self.source_code {
            write!(f, " ({source})")?;
        }
        write!(f, ": {}", self.message)
    }
}

impl std::error::Error for CoreError {}
