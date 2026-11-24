//! Typed CLI failures and the stable exit code contract.
//!
//! Every failure the CLI reports is a [`CliError`] whose [`ErrorKind`] fixes
//! the process exit code. The numbers and kind names are a public contract
//! documented in `docs/cli/reference.md`: they are only ever added, never
//! renumbered or reused for a different meaning.

use serde_json::{Map, Value};

/// Exit code of a successful command.
pub const EXIT_OK: u8 = 0;

/// Failure classes, each with its own exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// A failure that fits no other class (I/O error, internal error).
    Failure,
    /// The command line is malformed: unknown command or option, missing or
    /// extra argument, interactive mode requested without a terminal.
    Usage,
    /// The command or operation is recognised but not implemented or not
    /// supported here. Never reported as success.
    Unsupported,
    /// A theme package or manifest failed validation.
    InvalidPackage,
    /// The operating system refused access.
    PermissionDenied,
    /// The command needs the Glint daemon and it is not reachable.
    DaemonUnavailable,
    /// A named file, directory, theme or profile does not exist.
    NotFound,
    /// An operation did not finish in time.
    Timeout,
}

impl ErrorKind {
    /// Every kind, in exit code order.
    pub const ALL: &'static [ErrorKind] = &[
        ErrorKind::Failure,
        ErrorKind::Usage,
        ErrorKind::Unsupported,
        ErrorKind::InvalidPackage,
        ErrorKind::PermissionDenied,
        ErrorKind::DaemonUnavailable,
        ErrorKind::NotFound,
        ErrorKind::Timeout,
    ];

    /// The process exit code for this kind.
    pub const fn exit_code(self) -> u8 {
        match self {
            ErrorKind::Failure => 1,
            ErrorKind::Usage => 2,
            ErrorKind::Unsupported => 3,
            ErrorKind::InvalidPackage => 4,
            ErrorKind::PermissionDenied => 5,
            ErrorKind::DaemonUnavailable => 6,
            ErrorKind::NotFound => 7,
            ErrorKind::Timeout => 8,
        }
    }

    /// The stable kebab-case name used in JSON output.
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorKind::Failure => "failure",
            ErrorKind::Usage => "usage",
            ErrorKind::Unsupported => "unsupported",
            ErrorKind::InvalidPackage => "invalid-package",
            ErrorKind::PermissionDenied => "permission-denied",
            ErrorKind::DaemonUnavailable => "daemon-unavailable",
            ErrorKind::NotFound => "not-found",
            ErrorKind::Timeout => "timeout",
        }
    }
}

/// CLI-level error codes (`GLINT_CLI_*`). Errors coming from the library
/// crates keep their own `GLINT_SOURCE_*` / `GLINT_PACKAGE_*` /
/// `GLINT_MANIFEST_*` codes.
pub mod codes {
    /// Malformed arguments: unknown option, missing or extra argument.
    pub const USAGE: &str = "GLINT_CLI_USAGE";
    /// The command word is not part of the command tree.
    pub const UNKNOWN_COMMAND: &str = "GLINT_CLI_UNKNOWN_COMMAND";
    /// A command group was given without a subcommand, or the interactive
    /// chooser was requested without a terminal.
    pub const MISSING_SUBCOMMAND: &str = "GLINT_CLI_MISSING_SUBCOMMAND";
    /// The command is part of the proposed tree but not implemented yet.
    pub const NOT_IMPLEMENTED: &str = "GLINT_CLI_NOT_IMPLEMENTED";
    /// An I/O failure inside the CLI itself (not a package verdict).
    pub const IO: &str = "GLINT_CLI_IO";
    /// A Glint core holds the single-instance lock but cannot be asked over
    /// IPC (booting, not serving, or it started mid-command); nothing was
    /// changed (CLI-006).
    pub const DAEMON_UNAVAILABLE: &str = "GLINT_CLI_DAEMON_UNAVAILABLE";
}

/// A typed CLI failure.
#[derive(Debug, Clone, PartialEq)]
pub struct CliError {
    /// The failure class; fixes the exit code.
    pub kind: ErrorKind,
    /// The most specific stable code (`GLINT_*`).
    pub code: String,
    /// Human-readable message. Not a contract; match on `kind` and `code`.
    pub message: String,
    /// Extra machine-readable context (underlying codes, JSON Pointer,
    /// owning ticket for unimplemented commands).
    pub details: Map<String, Value>,
}

impl CliError {
    /// A new error without details.
    pub fn new(kind: ErrorKind, code: &str, message: impl Into<String>) -> Self {
        Self {
            kind,
            code: code.to_owned(),
            message: message.into(),
            details: Map::new(),
        }
    }

    /// A usage error with [`codes::USAGE`].
    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Usage, codes::USAGE, message)
    }

    /// Add one detail field.
    pub fn with_detail(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.details.insert(key.to_owned(), value.into());
        self
    }

    /// The process exit code.
    pub fn exit_code(&self) -> u8 {
        self.kind.exit_code()
    }
}

#[cfg(test)]
mod tests {
    use super::{EXIT_OK, ErrorKind};

    #[test]
    fn exit_codes_are_distinct_nonzero_and_stable() {
        let codes: Vec<u8> = ErrorKind::ALL.iter().map(|k| k.exit_code()).collect();
        assert_eq!(codes, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(!codes.contains(&EXIT_OK));
    }

    #[test]
    fn kind_names_are_stable() {
        let names: Vec<&str> = ErrorKind::ALL.iter().map(|k| k.as_str()).collect();
        assert_eq!(
            names,
            [
                "failure",
                "usage",
                "unsupported",
                "invalid-package",
                "permission-denied",
                "daemon-unavailable",
                "not-found",
                "timeout",
            ]
        );
    }
}
