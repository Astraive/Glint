//! Routing CLI commands through the running core (CLI-006).
//!
//! While a Glint core runs it holds the installer lock: it is the single
//! writer of package and profile state. Commands that read or change that
//! state therefore ask it over the same typed IPC API the manager uses
//! (`glint-ipc`), and only open the installer themselves when no core runs.
//! The decision never silently diverges:
//!
//! | Endpoint record | Connect | Instance lock | Route |
//! |---|---|---|---|
//! | present | ok | - | daemon |
//! | present | unavailable (stale record) | free | offline |
//! | present | unavailable | held | exit 6 |
//! | absent or invalid | - | free | offline |
//! | absent or invalid | - | held (core booting, not serving IPC) | exit 6 |
//! | - | timeout | - | exit 8 |
//! | - | permission denied | - | exit 5 |
//! | - | busy, version mismatch | - | exit 6 |
//!
//! Every routed result carries `route` (`daemon` or `offline`) in its JSON
//! `data`. Offline build and validation commands (`theme validate`) use the
//! libraries directly and never route.

use std::time::Duration;

use glint_core::{InstanceProbe, probe_instance};
use glint_ipc::{
    Client, ClientOptions, Endpoint, ErrorCode as IpcCode, GlintRequest, IpcError,
    OperationErrorKind, ResponseBody,
};
use glint_storage::GlintPaths;
use serde_json::Value;

use crate::error::{CliError, ErrorKind, codes};

/// Overrides every IPC deadline, in milliseconds (scripts on slow
/// machines; tests).
pub const TIMEOUT_ENV: &str = "GLINT_IPC_TIMEOUT_MS";

/// Deadline for the handshake and for reads.
const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// Deadline for operations: longer than the core's own 60 s operation
/// deadline, so the core's typed timeout arrives first.
const OPERATION_TIMEOUT: Duration = Duration::from_secs(90);

/// Where a state command runs.
#[derive(Debug)]
pub enum Route {
    /// Through the running core.
    Daemon(Box<Client>),
    /// Directly on the store: no core runs.
    Offline,
}

impl Route {
    /// `daemon` or `offline`, as reported in JSON `data.route`.
    pub fn name(&self) -> &'static str {
        match self {
            Route::Daemon(_) => "daemon",
            Route::Offline => "offline",
        }
    }
}

fn timeout_override() -> Option<Duration> {
    std::env::var(TIMEOUT_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|ms| *ms > 0)
        .map(Duration::from_millis)
}

/// Client options: `operation` selects the longer deadline.
pub fn client_options(operation: bool) -> ClientOptions {
    let timeout = timeout_override().unwrap_or(if operation {
        OPERATION_TIMEOUT
    } else {
        READ_TIMEOUT
    });
    ClientOptions::default()
        .with_timeout(timeout)
        .with_name(format!("glint-cli/{}", env!("CARGO_PKG_VERSION")))
}

/// Whether a core holds the single-instance lock.
pub fn core_running(paths: &GlintPaths) -> bool {
    matches!(probe_instance(paths), Ok(InstanceProbe::Running(_)))
}

/// The error for a core that runs but cannot be asked.
pub fn core_unreachable(reason: &str) -> CliError {
    CliError::new(
        ErrorKind::DaemonUnavailable,
        codes::DAEMON_UNAVAILABLE,
        format!(
            "a Glint core is running but {reason}; nothing was changed. \
             Retry once it is ready, or stop it to work offline"
        ),
    )
    .with_detail("route", "daemon")
}

/// Decide where a state command runs (see the module table). `operation`
/// selects the operation deadline for the returned client.
///
/// # Errors
///
/// Exit 6 when a core runs but cannot be reached, 8 on a timeout, 5 when
/// the endpoint refuses access.
pub fn route(paths: &GlintPaths, operation: bool) -> Result<Route, CliError> {
    let endpoint = match Endpoint::read(paths) {
        Ok(endpoint) => endpoint,
        Err(error) if error.code() == IpcCode::PermissionDenied => {
            return Err(ipc_error(&error));
        }
        Err(_) => {
            // No (usable) endpoint record: a core that holds the instance
            // lock is booting or not serving; never mutate behind it.
            return if core_running(paths) {
                Err(core_unreachable("it does not serve IPC (yet)"))
            } else {
                Ok(Route::Offline)
            };
        }
    };
    match Client::connect_with(&endpoint, client_options(operation)) {
        Ok(client) => Ok(Route::Daemon(Box::new(client))),
        Err(error) if error.code() == IpcCode::Unavailable && !core_running(paths) => {
            // A record left by a core that crashed: nothing serves it and
            // nothing holds the instance lock.
            Ok(Route::Offline)
        }
        Err(error) => Err(ipc_error(&error)),
    }
}

/// Send one request on a daemon route.
///
/// # Errors
///
/// The mapped IPC error.
pub fn request(client: &mut Client, request: GlintRequest) -> Result<ResponseBody, CliError> {
    client.request(request).map_err(|error| ipc_error(&error))
}

/// An unexpected response body: the core answered with the wrong type.
pub fn unexpected(body: &ResponseBody) -> CliError {
    CliError::new(
        ErrorKind::Failure,
        IpcCode::ProtocolViolation.as_str(),
        format!("the core sent an unexpected reply: {body:?}"),
    )
    .with_detail("route", "daemon")
}

/// Map an IPC error to the CLI's exit code classes: a failed operation
/// keeps the component's code and kind; reaching the core maps to
/// `daemon-unavailable` (6), `timeout` (8) or `permission-denied` (5).
pub fn ipc_error(error: &IpcError) -> CliError {
    let ipc_code = error.code().as_str();
    if let Some(failure) = error.operation() {
        let kind = match failure.kind {
            OperationErrorKind::NotFound => ErrorKind::NotFound,
            OperationErrorKind::InvalidRequest => ErrorKind::Usage,
            OperationErrorKind::InvalidPackage => ErrorKind::InvalidPackage,
            OperationErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
            OperationErrorKind::Unsupported => ErrorKind::Unsupported,
            _ => ErrorKind::Failure,
        };
        let mut cli = CliError::new(kind, &failure.code, error.message());
        for (key, value) in &failure.details {
            cli = cli.with_detail(key, value.clone());
        }
        return cli
            .with_detail("ipcCode", ipc_code)
            .with_detail("route", "daemon");
    }
    let kind = match error.code() {
        IpcCode::Timeout => ErrorKind::Timeout,
        IpcCode::PermissionDenied => ErrorKind::PermissionDenied,
        IpcCode::Unavailable
        | IpcCode::Busy
        | IpcCode::NotReady
        | IpcCode::ShuttingDown
        | IpcCode::EndpointInUse
        | IpcCode::EndpointInvalid
        | IpcCode::VersionUnsupported => ErrorKind::DaemonUnavailable,
        IpcCode::UnknownRequest | IpcCode::OperationUnsupported => ErrorKind::Unsupported,
        _ => ErrorKind::Failure,
    };
    CliError::new(kind, ipc_code, error.message())
        .with_detail("ipcCode", ipc_code)
        .with_detail("ipcCategory", Value::from(error.category().as_str()))
        .with_detail("route", "daemon")
}

#[cfg(test)]
mod tests {
    use glint_ipc::{ErrorCode as IpcCode, IpcError, OperationErrorKind, OperationFailure};

    use super::ipc_error;
    use crate::error::ErrorKind;

    #[test]
    fn ipc_errors_map_to_exit_code_kinds() {
        let cases = [
            (IpcCode::Unavailable, ErrorKind::DaemonUnavailable),
            (IpcCode::Busy, ErrorKind::DaemonUnavailable),
            (IpcCode::ShuttingDown, ErrorKind::DaemonUnavailable),
            (IpcCode::VersionUnsupported, ErrorKind::DaemonUnavailable),
            (IpcCode::Timeout, ErrorKind::Timeout),
            (IpcCode::PermissionDenied, ErrorKind::PermissionDenied),
            (IpcCode::UnknownRequest, ErrorKind::Unsupported),
            (IpcCode::OperationUnsupported, ErrorKind::Unsupported),
            (IpcCode::MalformedFrame, ErrorKind::Failure),
        ];
        for (code, kind) in cases {
            let cli = ipc_error(&IpcError::new(code, "x"));
            assert_eq!(cli.kind, kind, "{code}");
            assert_eq!(cli.code, code.as_str());
            assert_eq!(cli.details["route"], "daemon");
        }
    }

    #[test]
    fn operation_failures_keep_the_component_code() {
        let cases = [
            (OperationErrorKind::NotFound, ErrorKind::NotFound),
            (OperationErrorKind::InvalidRequest, ErrorKind::Usage),
            (
                OperationErrorKind::InvalidPackage,
                ErrorKind::InvalidPackage,
            ),
            (
                OperationErrorKind::PermissionDenied,
                ErrorKind::PermissionDenied,
            ),
            (OperationErrorKind::Unsupported, ErrorKind::Unsupported),
            (OperationErrorKind::Failure, ErrorKind::Failure),
        ];
        for (operation, kind) in cases {
            let error = IpcError::operation_failed(
                OperationFailure::new("GLINT_INSTALL_X", operation)
                    .with_detail("installCode", "GLINT_INSTALL_X"),
                "failed",
            );
            let cli = ipc_error(&error);
            assert_eq!(cli.kind, kind);
            assert_eq!(cli.code, "GLINT_INSTALL_X");
            assert_eq!(cli.details["installCode"], "GLINT_INSTALL_X");
            assert_eq!(cli.details["ipcCode"], "GLINT_IPC_OPERATION_FAILED");
        }
    }
}
