//! The versioned message envelopes (API version 1).
//!
//! Every message is one frame: a 4-byte big-endian length, then that many
//! bytes of UTF-8 JSON (see [`codec`](crate::codec)). A connection starts
//! with the client's [`Hello`] and the server's [`Welcome`] (or an error
//! response, then close). After that the client sends
//! [`RequestEnvelope`]s and the server answers each with exactly one
//! [`ResponseEnvelope`] carrying the same `request_id`, in order.
//!
//! A [`GlintRequest::Subscribe`] turns the connection into a one-way
//! stream: the response carries the current snapshot and its sequence
//! number, and from then on the server only sends [`StreamEvent`]s (ordered
//! deltas, resync snapshots, heartbeats and a final `closed`) and reads
//! nothing more.

use serde::{Deserialize, Serialize};

use crate::error::IpcError;
use crate::snapshot::{
    CapabilitySummary, ConfigSummary, CoreSnapshot, Delta, StateSummary, ThemeSummary,
};

/// The API version this build speaks.
pub const API_VERSION: u32 = 1;

/// The API versions this build supports.
pub const SUPPORTED_VERSIONS: VersionRange = VersionRange {
    min: API_VERSION,
    max: API_VERSION,
};

/// The longest `request_id` accepted.
pub const MAX_REQUEST_ID_LEN: usize = 64;

/// An inclusive range of API versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VersionRange {
    /// The oldest version.
    pub min: u32,
    /// The newest version.
    pub max: u32,
}

impl VersionRange {
    /// A range.
    pub const fn new(min: u32, max: u32) -> Self {
        Self { min, max }
    }

    /// The newest version in both ranges, if any.
    pub fn negotiate(self, other: VersionRange) -> Option<u32> {
        let high = self.max.min(other.max);
        let low = self.min.max(other.min);
        (low <= high).then_some(high)
    }
}

/// A frame from a client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ClientFrame {
    /// The handshake; must be the first frame.
    Hello(Hello),
    /// A request.
    Request(RequestEnvelope),
    /// The client is done; the server closes the connection without an
    /// answer. Lets the server free the connection at once instead of
    /// waiting for the client process to exit.
    Goodbye,
}

/// The client's handshake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Hello {
    /// The API versions the client speaks.
    pub versions: VersionRange,
    /// The client's name and version, for diagnostics (`glint-cli/0.1.0`).
    pub client: String,
}

impl Hello {
    /// A handshake for `client` speaking `versions`.
    pub fn new(versions: VersionRange, client: impl Into<String>) -> Self {
        Self {
            versions,
            client: client.into(),
        }
    }
}

/// One request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RequestEnvelope {
    /// Chosen by the client (1 to [`MAX_REQUEST_ID_LEN`] printable ASCII
    /// characters) and echoed in the response.
    pub request_id: String,
    /// Must equal the negotiated version.
    pub api_version: u32,
    /// What is asked.
    pub request: GlintRequest,
}

impl RequestEnvelope {
    /// An envelope.
    pub fn new(request_id: impl Into<String>, api_version: u32, request: GlintRequest) -> Self {
        Self {
            request_id: request_id.into(),
            api_version,
            request,
        }
    }
}

/// The typed requests of API version 1: reads of what each daemon stage
/// reported, the subscription and shutdown. There is deliberately no
/// generic command, path-taking execution or shell request (threat model
/// B7.3); a new operation is a new variant and a protocol change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum GlintRequest {
    /// Liveness check.
    Ping,
    /// Lifecycle state, daemon status, pid and current sequence number.
    GetStatus,
    /// The whole snapshot and its sequence number.
    GetSnapshot,
    /// What the daemon's `open_state` found.
    GetState,
    /// What the daemon's `resolve_config` produced.
    GetConfig,
    /// What the daemon's `discover_capabilities` found.
    GetCapabilities,
    /// The theme the daemon's `select_theme` chose.
    GetTheme,
    /// The snapshot, then a stream of ordered deltas on this connection.
    Subscribe,
    /// Ask the core to shut down cleanly (the same path as a `shutdown`
    /// line on its control channel).
    Shutdown,
}

impl GlintRequest {
    /// Every request `type` this version knows. Anything else is refused
    /// with `GLINT_IPC_UNKNOWN_REQUEST`.
    pub const TYPES: &'static [&'static str] = &[
        "ping",
        "get-status",
        "get-snapshot",
        "get-state",
        "get-config",
        "get-capabilities",
        "get-theme",
        "subscribe",
        "shutdown",
    ];

    /// The stable `type` name.
    pub const fn type_name(&self) -> &'static str {
        match self {
            GlintRequest::Ping => "ping",
            GlintRequest::GetStatus => "get-status",
            GlintRequest::GetSnapshot => "get-snapshot",
            GlintRequest::GetState => "get-state",
            GlintRequest::GetConfig => "get-config",
            GlintRequest::GetCapabilities => "get-capabilities",
            GlintRequest::GetTheme => "get-theme",
            GlintRequest::Subscribe => "subscribe",
            GlintRequest::Shutdown => "shutdown",
        }
    }
}

/// A frame from the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ServerFrame {
    /// The handshake answer.
    Welcome(Welcome),
    /// The answer to one request, or a connection-level error.
    Response(ResponseEnvelope),
    /// A subscription event.
    Event {
        /// The event.
        event: StreamEvent,
    },
}

/// The server's handshake answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Welcome {
    /// The negotiated version; every request must carry it.
    pub api_version: u32,
    /// The versions the server supports.
    pub supported: VersionRange,
    /// The server's name and version (`glint-daemon/0.0.0`).
    pub server: String,
    /// The largest request frame the server reads.
    pub max_request_bytes: u32,
    /// The largest frame the server sends.
    pub max_response_bytes: u32,
}

impl Welcome {
    /// This build's answer: `api_version` negotiated, the supported range
    /// and frame limits of this crate.
    pub fn new(api_version: u32, server: impl Into<String>) -> Self {
        Self {
            api_version,
            supported: SUPPORTED_VERSIONS,
            server: server.into(),
            max_request_bytes: crate::codec::MAX_REQUEST_BYTES,
            max_response_bytes: crate::codec::MAX_RESPONSE_BYTES,
        }
    }
}

/// The answer to one request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ResponseEnvelope {
    /// The request's ID; `None` when the error concerns the connection or a
    /// frame whose ID could not be read.
    pub request_id: Option<String>,
    /// The connection's API version (the server's newest before the
    /// handshake completes).
    pub api_version: u32,
    /// The result.
    pub result: Outcome,
}

impl ResponseEnvelope {
    /// A successful response.
    pub fn ok(request_id: impl Into<String>, api_version: u32, body: ResponseBody) -> Self {
        Self {
            request_id: Some(request_id.into()),
            api_version,
            result: Outcome::Ok(body),
        }
    }

    /// An error response.
    pub fn error(request_id: Option<String>, api_version: u32, error: IpcError) -> Self {
        Self {
            request_id,
            api_version,
            result: Outcome::Error(error),
        }
    }
}

/// Success or a typed error. Never both, never a fake success.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// The request succeeded.
    Ok(ResponseBody),
    /// The request failed.
    Error(IpcError),
}

/// What a successful request returns, one variant per request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ResponseBody {
    /// For [`GlintRequest::Ping`].
    Pong,
    /// For [`GlintRequest::GetStatus`].
    Status(StatusReply),
    /// For [`GlintRequest::GetSnapshot`].
    Snapshot(Box<SnapshotReply>),
    /// For [`GlintRequest::GetState`].
    State(StateSummary),
    /// For [`GlintRequest::GetConfig`].
    Config(ConfigSummary),
    /// For [`GlintRequest::GetCapabilities`].
    Capabilities(CapabilitySummary),
    /// For [`GlintRequest::GetTheme`].
    Theme(Box<ThemeSummary>),
    /// For [`GlintRequest::Subscribe`]: the starting point of the stream.
    Subscribed(Box<SnapshotReply>),
    /// For [`GlintRequest::Shutdown`].
    ShutdownAccepted(ShutdownReply),
}

/// The core's status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct StatusReply {
    /// The lifecycle state name.
    pub lifecycle: String,
    /// The daemon service's status.
    pub daemon: String,
    /// The core process's ID.
    pub pid: u32,
    /// The current sequence number.
    pub sequence: u64,
}

impl StatusReply {
    /// A status.
    pub fn new(
        lifecycle: impl Into<String>,
        daemon: impl Into<String>,
        pid: u32,
        sequence: u64,
    ) -> Self {
        Self {
            lifecycle: lifecycle.into(),
            daemon: daemon.into(),
            pid,
            sequence,
        }
    }
}

/// A snapshot and the sequence number it reflects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SnapshotReply {
    /// The last delta applied to `snapshot`. The next delta on a stream
    /// carries `sequence + 1`.
    pub sequence: u64,
    /// The state.
    pub snapshot: CoreSnapshot,
}

impl SnapshotReply {
    /// A reply.
    pub fn new(sequence: u64, snapshot: CoreSnapshot) -> Self {
        Self { sequence, snapshot }
    }
}

/// The answer to a shutdown request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ShutdownReply {
    /// `false` when a shutdown had already been requested (the first reason
    /// is kept).
    pub initiated: bool,
}

/// One event on a subscription stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum StreamEvent {
    /// The next change; `sequence` is always the previous one plus one.
    Delta {
        /// This delta's sequence number.
        sequence: u64,
        /// The change.
        delta: Delta,
    },
    /// A full snapshot replacing the client's: sent when the server dropped
    /// deltas this subscriber was too slow to take.
    Snapshot {
        /// The sequence number it reflects.
        sequence: u64,
        /// Why it was sent (`lagged`).
        reason: String,
        /// The state.
        snapshot: Box<CoreSnapshot>,
    },
    /// Nothing changed for a while; `sequence` is the last one sent. Lets
    /// both sides notice a dead peer.
    Heartbeat {
        /// The last sequence number.
        sequence: u64,
    },
    /// The stream ends (the core is stopping); the server closes the
    /// connection after this.
    Closed {
        /// The last sequence number.
        sequence: u64,
        /// Why (not stable).
        reason: String,
    },
}
