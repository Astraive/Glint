//! A minimal blocking client: handshake, typed requests with a deadline,
//! and subscriptions that verify sequence numbers and reconnect to a fresh
//! snapshot.
//!
//! Each connection has one reader thread that turns frames into channel
//! messages, so every wait has a timeout even where the transport has none
//! (Windows named pipes). A timed-out connection is poisoned: the answer
//! may still arrive, so the next request would read the wrong one.
//! Reconnect instead.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use glint_storage::GlintPaths;
use interprocess::local_socket::prelude::*;

use crate::codec::{self, MAX_RESPONSE_BYTES, decode_server_frame, encode_frame, read_frame};
use crate::endpoint::Endpoint;
use crate::error::{ErrorCode, IpcError};
use crate::protocol::{
    ClientFrame, GlintRequest, Hello, Outcome, RequestEnvelope, ResponseBody, SUPPORTED_VERSIONS,
    ServerFrame, SnapshotReply, StatusReply, StreamEvent, VersionRange, Welcome,
};
use crate::snapshot::{CoreSnapshot, Delta};

/// Client settings.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ClientOptions {
    /// How long to wait for the handshake and for each response.
    pub timeout: Duration,
    /// The client's name in the [`Hello`].
    pub client_name: String,
    /// The API versions to offer.
    pub versions: VersionRange,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(5),
            client_name: format!("glint-ipc/{}", env!("CARGO_PKG_VERSION")),
            versions: SUPPORTED_VERSIONS,
        }
    }
}

impl ClientOptions {
    /// With this response timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// With this client name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.client_name = name.into();
        self
    }

    /// Offering these API versions.
    #[must_use]
    pub fn with_versions(mut self, versions: VersionRange) -> Self {
        self.versions = versions;
        self
    }
}

type Frames = Receiver<Result<ServerFrame, IpcError>>;

fn spawn_reader(stream: Arc<LocalSocketStream>) -> Frames {
    let (tx, rx) = mpsc::channel();
    let _ = thread::Builder::new()
        .name("glint-ipc-client".into())
        .spawn(move || {
            let mut reader = &*stream;
            loop {
                let message = match read_frame(&mut reader, MAX_RESPONSE_BYTES) {
                    Ok(Some(body)) => decode_server_frame(&body),
                    Ok(None) => Err(IpcError::new(
                        ErrorCode::Unavailable,
                        "the core closed the connection",
                    )),
                    Err(error) => Err(error.to_ipc()),
                };
                let last = message.is_err();
                if tx.send(message).is_err() || last {
                    return;
                }
            }
        });
    rx
}

fn receive(frames: &Frames, timeout: Duration) -> Result<ServerFrame, IpcError> {
    match frames.recv_timeout(timeout) {
        Ok(message) => message,
        Err(RecvTimeoutError::Timeout) => Err(IpcError::new(
            ErrorCode::Timeout,
            format!("no answer within {} ms", timeout.as_millis()),
        )),
        Err(RecvTimeoutError::Disconnected) => Err(IpcError::new(
            ErrorCode::Unavailable,
            "the connection is closed",
        )),
    }
}

/// A connection to the core that has completed the handshake.
#[derive(Debug)]
pub struct Client {
    endpoint: Endpoint,
    options: ClientOptions,
    stream: Arc<LocalSocketStream>,
    frames: Frames,
    welcome: Welcome,
    next_id: u64,
    poisoned: Option<IpcError>,
}

impl Client {
    /// Connect to the core serving `paths` (through its endpoint record)
    /// with default options.
    ///
    /// # Errors
    ///
    /// `GLINT_IPC_UNAVAILABLE` when no core serves IPC, or any handshake
    /// error.
    pub fn connect(paths: &GlintPaths) -> Result<Self, IpcError> {
        Self::connect_with(&Endpoint::read(paths)?, ClientOptions::default())
    }

    /// Connect to `endpoint` and complete the handshake.
    ///
    /// # Errors
    ///
    /// The connect error (`GLINT_IPC_UNAVAILABLE`,
    /// `GLINT_IPC_PERMISSION_DENIED`), `GLINT_IPC_VERSION_UNSUPPORTED`,
    /// `GLINT_IPC_BUSY`, `GLINT_IPC_TIMEOUT` or a protocol error.
    pub fn connect_with(endpoint: &Endpoint, options: ClientOptions) -> Result<Self, IpcError> {
        let stream = Arc::new(endpoint.connect()?);
        let frames = spawn_reader(Arc::clone(&stream));
        let hello = ClientFrame::Hello(Hello::new(options.versions, options.client_name.clone()));
        write(&stream, &hello)?;
        let welcome = match receive(&frames, options.timeout)? {
            ServerFrame::Welcome(welcome) => welcome,
            ServerFrame::Response(response) => {
                return Err(match response.result {
                    Outcome::Error(error) => error,
                    Outcome::Ok(_) => violation("a response instead of welcome"),
                });
            }
            ServerFrame::Event { .. } => return Err(violation("an event instead of welcome")),
        };
        if options
            .versions
            .negotiate(VersionRange::new(welcome.api_version, welcome.api_version))
            != Some(welcome.api_version)
        {
            return Err(IpcError::new(
                ErrorCode::VersionUnsupported,
                format!("server chose API {}, not offered", welcome.api_version),
            ));
        }
        Ok(Self {
            endpoint: endpoint.clone(),
            options,
            stream,
            frames,
            welcome,
            next_id: 0,
            poisoned: None,
        })
    }

    /// The negotiated API version.
    pub fn api_version(&self) -> u32 {
        self.welcome.api_version
    }

    /// The server's handshake answer.
    pub fn welcome(&self) -> &Welcome {
        &self.welcome
    }

    /// The endpoint.
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    fn send_request(&mut self, request: GlintRequest) -> Result<String, IpcError> {
        if let Some(error) = &self.poisoned {
            return Err(IpcError::new(
                error.code(),
                format!(
                    "connection unusable after an earlier failure: {}",
                    error.message()
                ),
            ));
        }
        self.next_id += 1;
        let request_id = format!("r{}", self.next_id);
        let frame = ClientFrame::Request(RequestEnvelope::new(
            request_id.clone(),
            self.api_version(),
            request,
        ));
        write(&self.stream, &frame).inspect_err(|error| self.poisoned = Some(error.clone()))?;
        Ok(request_id)
    }

    fn await_response(&mut self, request_id: &str) -> Result<ResponseBody, IpcError> {
        let frame = receive(&self.frames, self.options.timeout)
            .inspect_err(|error| self.poisoned = Some(error.clone()))?;
        let response = match frame {
            ServerFrame::Response(response) => response,
            _ => {
                let error = violation("an unexpected frame instead of a response");
                self.poisoned = Some(error.clone());
                return Err(error);
            }
        };
        if response.request_id.as_deref() != Some(request_id) {
            // A connection-level error carries no ID; anything else that
            // does not match means the stream is out of step.
            if let (None, Outcome::Error(error)) = (&response.request_id, &response.result) {
                self.poisoned = Some(error.clone());
                return Err(error.clone());
            }
            let error = violation(&format!(
                "response for {:?} while waiting for {request_id:?}",
                response.request_id
            ));
            self.poisoned = Some(error.clone());
            return Err(error);
        }
        match response.result {
            Outcome::Ok(body) => Ok(body),
            Outcome::Error(error) => Err(error),
        }
    }

    /// Send one request and wait for its response.
    ///
    /// # Errors
    ///
    /// The server's typed error, or `GLINT_IPC_TIMEOUT` /
    /// `GLINT_IPC_UNAVAILABLE` / a protocol error (after which the
    /// connection is unusable).
    pub fn request(&mut self, request: GlintRequest) -> Result<ResponseBody, IpcError> {
        if request == GlintRequest::Subscribe {
            return Err(IpcError::new(
                ErrorCode::ProtocolViolation,
                "use Client::subscribe for subscriptions",
            ));
        }
        let id = self.send_request(request)?;
        self.await_response(&id)
    }

    /// `ping`.
    ///
    /// # Errors
    ///
    /// As [`request`](Self::request).
    pub fn ping(&mut self) -> Result<(), IpcError> {
        match self.request(GlintRequest::Ping)? {
            ResponseBody::Pong => Ok(()),
            other => Err(unexpected(&other)),
        }
    }

    /// `get-status`.
    ///
    /// # Errors
    ///
    /// As [`request`](Self::request).
    pub fn status(&mut self) -> Result<StatusReply, IpcError> {
        match self.request(GlintRequest::GetStatus)? {
            ResponseBody::Status(status) => Ok(status),
            other => Err(unexpected(&other)),
        }
    }

    /// `get-snapshot`.
    ///
    /// # Errors
    ///
    /// As [`request`](Self::request).
    pub fn snapshot(&mut self) -> Result<SnapshotReply, IpcError> {
        match self.request(GlintRequest::GetSnapshot)? {
            ResponseBody::Snapshot(reply) => Ok(*reply),
            other => Err(unexpected(&other)),
        }
    }

    /// `shutdown`; returns whether this request initiated it.
    ///
    /// # Errors
    ///
    /// As [`request`](Self::request).
    pub fn shutdown(&mut self) -> Result<bool, IpcError> {
        match self.request(GlintRequest::Shutdown)? {
            ResponseBody::ShutdownAccepted(reply) => Ok(reply.initiated),
            other => Err(unexpected(&other)),
        }
    }

    /// Subscribe: this connection becomes the stream.
    ///
    /// # Errors
    ///
    /// As [`request`](Self::request); `GLINT_IPC_SHUTTING_DOWN` when the
    /// core is stopping.
    pub fn subscribe(mut self) -> Result<Subscription, IpcError> {
        let id = self.send_request(GlintRequest::Subscribe)?;
        match self.await_response(&id)? {
            ResponseBody::Subscribed(reply) => Ok(Subscription {
                state: SubscriptionState::new(reply.sequence, reply.snapshot),
                client: self,
            }),
            other => Err(unexpected(&other)),
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // Best effort: lets the server close its side, which ends the
        // reader thread (it holds the other reference to the stream).
        let _ = write(&self.stream, &ClientFrame::Goodbye);
    }
}

fn write(stream: &LocalSocketStream, frame: &ClientFrame) -> Result<(), IpcError> {
    let bytes = encode_frame(frame, codec::MAX_REQUEST_BYTES)?;
    let mut writer = stream;
    codec::write_frame(&mut writer, &bytes).map_err(|error| IpcError::from_io("send", &error))
}

fn violation(message: &str) -> IpcError {
    IpcError::new(
        ErrorCode::ProtocolViolation,
        format!("server sent {message}"),
    )
}

fn unexpected(body: &ResponseBody) -> IpcError {
    violation(&format!("an unexpected response body {body:?}"))
}

/// What a subscription delivered.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Update {
    /// A delta, already applied to the local snapshot.
    Delta {
        /// Its sequence number.
        sequence: u64,
        /// The change.
        delta: Delta,
    },
    /// The local snapshot was replaced (the server resynced a lagging
    /// subscriber).
    Resynced {
        /// The sequence number it reflects.
        sequence: u64,
    },
    /// Nothing changed.
    Heartbeat {
        /// The last sequence number.
        sequence: u64,
    },
    /// The stream ended.
    Closed {
        /// Why (not stable).
        reason: String,
    },
}

/// A local copy of the core snapshot, kept current by applying stream
/// events in order. A delta that does not follow the last sequence number
/// is refused with `GLINT_IPC_SEQUENCE_GAP` and the state is left as it
/// was: the caller resubscribes for a fresh snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionState {
    sequence: u64,
    snapshot: CoreSnapshot,
}

impl SubscriptionState {
    /// Start from `snapshot` at `sequence`.
    pub fn new(sequence: u64, snapshot: CoreSnapshot) -> Self {
        Self { sequence, snapshot }
    }

    /// The last applied sequence number.
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// The current snapshot.
    pub fn snapshot(&self) -> &CoreSnapshot {
        &self.snapshot
    }

    /// Apply one stream event.
    ///
    /// # Errors
    ///
    /// `GLINT_IPC_SEQUENCE_GAP` when a delta or heartbeat shows a missed or
    /// repeated sequence number; `GLINT_IPC_PROTOCOL_VIOLATION` when a
    /// resync snapshot goes backwards.
    pub fn apply(&mut self, event: StreamEvent) -> Result<Update, IpcError> {
        match event {
            StreamEvent::Delta { sequence, delta } => {
                if sequence != self.sequence + 1 {
                    return Err(self.gap(sequence));
                }
                self.snapshot.apply(&delta);
                self.sequence = sequence;
                Ok(Update::Delta { sequence, delta })
            }
            StreamEvent::Snapshot {
                sequence, snapshot, ..
            } => {
                if sequence < self.sequence {
                    return Err(violation(&format!(
                        "a resync snapshot at {sequence}, behind {}",
                        self.sequence
                    )));
                }
                self.sequence = sequence;
                self.snapshot = *snapshot;
                Ok(Update::Resynced { sequence })
            }
            StreamEvent::Heartbeat { sequence } => {
                if sequence != self.sequence {
                    return Err(self.gap(sequence));
                }
                Ok(Update::Heartbeat { sequence })
            }
            StreamEvent::Closed { sequence, reason } => {
                if sequence != self.sequence {
                    return Err(self.gap(sequence));
                }
                Ok(Update::Closed { reason })
            }
        }
    }

    fn gap(&self, got: u64) -> IpcError {
        IpcError::new(
            ErrorCode::SequenceGap,
            format!(
                "expected sequence {} after {}, got {got}; resubscribe",
                self.sequence + 1,
                self.sequence
            ),
        )
    }
}

/// A live subscription: the local snapshot plus the stream that updates it.
#[derive(Debug)]
pub struct Subscription {
    client: Client,
    state: SubscriptionState,
}

impl Subscription {
    /// The current local snapshot.
    pub fn snapshot(&self) -> &CoreSnapshot {
        self.state.snapshot()
    }

    /// The last applied sequence number.
    pub fn sequence(&self) -> u64 {
        self.state.sequence()
    }

    /// The local state.
    pub fn state(&self) -> &SubscriptionState {
        &self.state
    }

    /// Wait up to `timeout` for the next event and apply it.
    ///
    /// # Errors
    ///
    /// `GLINT_IPC_TIMEOUT`, `GLINT_IPC_UNAVAILABLE` (the connection
    /// dropped), `GLINT_IPC_SEQUENCE_GAP` or a protocol error. After any of
    /// these call [`reconnect`](Self::reconnect).
    pub fn next(&mut self, timeout: Duration) -> Result<Update, IpcError> {
        match receive(&self.client.frames, timeout)? {
            ServerFrame::Event { event } => self.state.apply(event),
            ServerFrame::Response(response) => Err(match response.result {
                Outcome::Error(error) => error,
                Outcome::Ok(_) => violation("a response on a subscription stream"),
            }),
            ServerFrame::Welcome(_) => Err(violation("a welcome on a subscription stream")),
        }
    }

    /// Open a new connection to the same endpoint (or the one in `paths`'
    /// current record, when given, for a core that restarted) and subscribe
    /// again; the local snapshot is replaced by the server's current one.
    ///
    /// # Errors
    ///
    /// As [`Client::connect_with`] and [`Client::subscribe`].
    pub fn reconnect(&mut self, paths: Option<&GlintPaths>) -> Result<(), IpcError> {
        let endpoint = match paths {
            Some(paths) => Endpoint::read(paths)?,
            None => self.client.endpoint.clone(),
        };
        let fresh = Client::connect_with(&endpoint, self.client.options.clone())?.subscribe()?;
        *self = fresh;
        Ok(())
    }
}
