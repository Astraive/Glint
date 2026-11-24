//! Glint's versioned, typed local IPC (COR-003, ADR 0010).
//!
//! The core serves one endpoint per user and data root: a named pipe on
//! Windows, a Unix domain socket elsewhere (see [`endpoint`]), both through
//! the portable `interprocess` crate and restricted to the current user.
//! Messages are length-prefixed JSON frames ([`codec`]) with a versioned
//! envelope ([`protocol`]):
//!
//! 1. The client sends [`Hello`] with the API versions it speaks; the
//!    server answers [`Welcome`] with the newest common one, or
//!    `GLINT_IPC_VERSION_UNSUPPORTED` and closes.
//! 2. Each [`RequestEnvelope`] carries a client-chosen `request_id`, the
//!    negotiated `api_version` and a typed [`GlintRequest`]; the server
//!    answers with exactly one [`ResponseEnvelope`] carrying the same ID and
//!    either a typed [`ResponseBody`] or an [`IpcError`] with a stable
//!    `GLINT_IPC_*` code. Unknown request types are refused; there is no
//!    generic command or shell request.
//! 3. [`GlintRequest::Subscribe`] answers with the current [`CoreSnapshot`]
//!    and its sequence number, then streams ordered [`Delta`]s
//!    (`sequence + 1` each), heartbeats, a resync snapshot when the
//!    subscriber lagged, and `closed` when the core stops. A client that
//!    sees a gap, or reconnects, simply subscribes again and gets the
//!    current snapshot.
//!
//! The server ([`serve`]) answers reads from the [`Hub`]'s published
//! snapshot and never touches the daemon's state; the only action, shutdown,
//! goes through the host's [`Control`]. [`Client`] is the minimal blocking
//! client with per-request timeouts.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod client;
pub mod codec;
pub mod endpoint;
mod error;
mod hub;
pub mod protocol;
mod server;
mod snapshot;

use std::io::{self, Read, Write};

pub use client::{Client, ClientOptions, Subscription, SubscriptionState, Update};
pub use endpoint::{Endpoint, PIPE_SDDL, Transport};
pub use error::{ErrorCategory, ErrorCode, IpcError};
pub use hub::{DEFAULT_QUEUE_CAPACITY, Hub, Next, Subscriber};
pub use protocol::{
    API_VERSION, ClientFrame, GlintRequest, Hello, Outcome, RequestEnvelope, ResponseBody,
    ResponseEnvelope, SUPPORTED_VERSIONS, ServerFrame, ShutdownReply, SnapshotReply, StatusReply,
    StreamEvent, VersionRange, Welcome,
};
pub use server::{Control, ServerHandle, ServerOptions, serve};
pub use snapshot::{
    CapabilitySummary, ConfigSummary, CoreSnapshot, Delta, MAX_SNAPSHOT_WARNINGS, StateSummary,
    ThemeSummary, WarningInfo,
};

/// A raw byte stream to an endpoint, without the handshake. For
/// conformance tests and diagnostics that must send frames the typed
/// client never would (malformed, oversized, out of order).
#[derive(Debug)]
pub struct RawConnection {
    stream: interprocess::local_socket::Stream,
}

impl RawConnection {
    /// Connect to `endpoint`.
    ///
    /// # Errors
    ///
    /// The connect error.
    pub fn connect(endpoint: &Endpoint) -> Result<Self, IpcError> {
        Ok(Self {
            stream: endpoint.connect()?,
        })
    }
}

impl Read for RawConnection {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buf)
    }
}

impl Write for RawConnection {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}
