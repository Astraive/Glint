//! What the manager's commands return: the core's state read over local
//! IPC, or an honest reason why it could not be read.
//!
//! Portable on purpose (no Tauri types), so it compiles and is tested on
//! every OS even though the window host is Windows-only for now. Each call
//! opens a fresh connection with a short deadline and never retries, so a
//! stopped or hung core shows up as `unreachable` within the deadline
//! instead of a stale value. The wire types are forwarded as they are:
//! `glint-ipc` already leaves paths out of them.

use std::time::{Duration, Instant};

use glint_ipc::{Client, ClientOptions, CoreSnapshot, Endpoint, IpcError, StatusReply};
use glint_storage::{GlintPaths, StorageError};
use serde::Serialize;

/// How long the manager waits for the handshake and for each answer.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

/// The client name the manager sends in its `hello`.
pub const CLIENT_NAME: &str = concat!("glint-desktop/", env!("CARGO_PKG_VERSION"));

/// Why the core could not be read. `code` is the stable `GLINT_IPC_*` or
/// `GLINT_STORAGE_*` code; `category` keeps unsupported, permission-denied,
/// unavailable, timeout and failure apart so the UI never shows one as
/// another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    /// The stable code.
    pub code: String,
    /// `unsupported`, `permission-denied`, `unavailable`, `timeout` or
    /// `failure`.
    pub category: &'static str,
    /// Human-readable detail (not stable).
    pub message: String,
}

/// The code for a failure inside the manager itself (a worker thread that
/// did not finish), never a core state.
pub const BRIDGE_FAILED: &str = "GLINT_DESKTOP_BRIDGE_FAILED";

impl Problem {
    /// A failure inside the manager's own bridge.
    pub fn bridge_failed(message: impl Into<String>) -> Self {
        Self {
            code: BRIDGE_FAILED.to_owned(),
            category: "failure",
            message: message.into(),
        }
    }
}

impl From<&IpcError> for Problem {
    fn from(error: &IpcError) -> Self {
        Self {
            code: error.code().as_str().to_owned(),
            category: error.category().as_str(),
            message: error.message().to_owned(),
        }
    }
}

impl From<&StorageError> for Problem {
    fn from(error: &StorageError) -> Self {
        use glint_storage::ErrorCategory as C;
        let category = match error.category() {
            C::Unsupported => "unsupported",
            C::PermissionDenied => "permission-denied",
            C::Unavailable => "unavailable",
            C::Timeout => "timeout",
            _ => "failure",
        };
        Self {
            code: error.code().as_str().to_owned(),
            category,
            message: error.to_string(),
        }
    }
}

/// The answer to the `daemon_status` command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum DaemonStatus {
    /// The core answered.
    Connected {
        /// The negotiated IPC API version.
        api_version: u32,
        /// The server's name from its `welcome`.
        server: String,
        /// `get-status`.
        status: StatusReply,
        /// The sequence number `snapshot` reflects.
        sequence: u64,
        /// `get-snapshot`.
        snapshot: Box<CoreSnapshot>,
    },
    /// The core could not be read; nothing is shown as if it were.
    Unreachable {
        /// Why.
        problem: Problem,
    },
}

/// The answer to the `daemon_ping` command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum PingResult {
    /// The core answered `pong`.
    Connected {
        /// Connect, handshake and ping, in milliseconds.
        round_trip_ms: u64,
    },
    /// No answer.
    Unreachable {
        /// Why.
        problem: Problem,
    },
}

/// Where the manager looks for the core: `GLINT_HOME` or the OS
/// directories, exactly like the daemon.
#[derive(Debug, Clone)]
pub struct Bridge {
    paths: Result<GlintPaths, Problem>,
    timeout: Duration,
}

impl Bridge {
    /// Resolve the core's paths from the environment.
    pub fn from_env() -> Self {
        Self {
            paths: GlintPaths::resolve().map_err(|error| Problem::from(&error)),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Talk to the core serving `paths`.
    pub fn new(paths: GlintPaths) -> Self {
        Self {
            paths: Ok(paths),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// With this per-answer deadline.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn connect(&self) -> Result<Client, Problem> {
        let paths = self.paths.as_ref().map_err(Clone::clone)?;
        let endpoint = Endpoint::read(paths).map_err(|error| Problem::from(&error))?;
        let options = ClientOptions::default()
            .with_timeout(self.timeout)
            .with_name(CLIENT_NAME);
        Client::connect_with(&endpoint, options).map_err(|error| Problem::from(&error))
    }

    /// `get-status` and `get-snapshot` on one fresh connection.
    pub fn status(&self) -> DaemonStatus {
        let read = || -> Result<DaemonStatus, Problem> {
            let mut client = self.connect()?;
            let status = client.status().map_err(|error| Problem::from(&error))?;
            let reply = client.snapshot().map_err(|error| Problem::from(&error))?;
            Ok(DaemonStatus::Connected {
                api_version: client.api_version(),
                server: client.welcome().server.clone(),
                status,
                sequence: reply.sequence,
                snapshot: Box::new(reply.snapshot),
            })
        };
        read().unwrap_or_else(|problem| DaemonStatus::Unreachable { problem })
    }

    /// `ping` on a fresh connection.
    pub fn ping(&self) -> PingResult {
        let started = Instant::now();
        let ping = || -> Result<(), Problem> {
            self.connect()?
                .ping()
                .map_err(|error| Problem::from(&error))
        };
        match ping() {
            Ok(()) => PingResult::Connected {
                round_trip_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            },
            Err(problem) => PingResult::Unreachable { problem },
        }
    }
}
