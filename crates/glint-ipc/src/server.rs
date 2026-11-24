//! The IPC server: accepts connections on the endpoint, answers typed
//! requests from the [`Hub`]'s snapshot and streams subscriptions.
//!
//! Reads never reach the daemon: they are answered from the published
//! snapshot, so the daemon stays the only holder of state. The only action
//! is [`GlintRequest::Shutdown`], handed to the host's [`Control`].
//!
//! Bounds: request frames up to [`MAX_REQUEST_BYTES`], response frames up
//! to [`MAX_RESPONSE_BYTES`] (a larger response becomes
//! `GLINT_IPC_INTERNAL`), at most [`ServerOptions::max_connections`]
//! connections (the next one gets `GLINT_IPC_BUSY` and is closed), and a
//! bounded queue per subscriber (a lagging subscriber gets a fresh snapshot
//! instead of unbounded buffering).

use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use glint_storage::GlintPaths;
use interprocess::local_socket::prelude::*;

use crate::codec::{
    self, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES, decode_client_frame, encode_frame, read_frame,
};
use crate::endpoint::{self, Endpoint};
use crate::error::{ErrorCode, IpcError};
use crate::hub::{DEFAULT_QUEUE_CAPACITY, Hub, Next, Subscriber};
use crate::protocol::{
    ClientFrame, GlintRequest, Outcome, ResponseBody, ResponseEnvelope, SUPPORTED_VERSIONS,
    ServerFrame, ShutdownReply, SnapshotReply, StatusReply, StreamEvent, Welcome,
};

/// Actions the host performs for the server.
pub trait Control: Send + Sync + 'static {
    /// Request a clean shutdown. Returns `true` when this call initiated
    /// it, `false` when one was already requested.
    ///
    /// # Errors
    ///
    /// An [`IpcError`] when the host refuses.
    fn shutdown(&self) -> Result<bool, IpcError>;
}

/// Server tuning.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ServerOptions {
    /// The server's name in the [`Welcome`] (`glint-daemon/0.0.0`).
    pub server_name: String,
    /// Most concurrent connections.
    pub max_connections: usize,
    /// How long a quiet subscription waits before a heartbeat.
    pub heartbeat: Duration,
    /// Deltas buffered per subscriber before it is resynced.
    pub queue_capacity: usize,
    /// How long [`ServerHandle::stop`] waits for subscription streams to
    /// deliver their last events.
    pub drain_timeout: Duration,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            server_name: format!("glint-ipc/{}", env!("CARGO_PKG_VERSION")),
            max_connections: 32,
            heartbeat: Duration::from_secs(5),
            queue_capacity: DEFAULT_QUEUE_CAPACITY,
            drain_timeout: Duration::from_secs(2),
        }
    }
}

impl ServerOptions {
    /// Defaults with this server name.
    pub fn named(server_name: impl Into<String>) -> Self {
        Self {
            server_name: server_name.into(),
            ..Self::default()
        }
    }
}

#[derive(Default)]
struct Streams {
    active: Mutex<usize>,
    done: Condvar,
}

struct Shared {
    hub: Arc<Hub>,
    control: Arc<dyn Control>,
    options: ServerOptions,
    connections: AtomicUsize,
    streams: Streams,
}

/// A running server. [`stop`](Self::stop) (or dropping it) stops accepting,
/// lets subscription streams deliver their final events and removes the
/// endpoint record.
pub struct ServerHandle {
    endpoint: Endpoint,
    paths: GlintPaths,
    stop: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
    shared: Arc<Shared>,
}

impl std::fmt::Debug for ServerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerHandle")
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

/// Bind a fresh endpoint for `paths`, write the endpoint record and serve
/// `hub` until stopped.
///
/// # Errors
///
/// `GLINT_IPC_ENDPOINT_IN_USE` when the name is taken, or the transport or
/// record error.
pub fn serve(
    paths: &GlintPaths,
    hub: Arc<Hub>,
    control: Arc<dyn Control>,
    options: ServerOptions,
) -> Result<ServerHandle, IpcError> {
    let endpoint = Endpoint::generate(paths);
    let listener = endpoint.listen()?;
    endpoint.write_record(paths)?;
    let stop = Arc::new(AtomicBool::new(false));
    let shared = Arc::new(Shared {
        hub,
        control,
        options,
        connections: AtomicUsize::new(0),
        streams: Streams::default(),
    });
    let accept = {
        let stop = Arc::clone(&stop);
        let shared = Arc::clone(&shared);
        thread::Builder::new()
            .name("glint-ipc-accept".into())
            .spawn(move || accept_loop(&listener, &stop, &shared))
            .map_err(|error| IpcError::from_io("spawn accept thread", &error))?
    };
    Ok(ServerHandle {
        endpoint,
        paths: paths.clone(),
        stop,
        accept: Some(accept),
        shared,
    })
}

impl ServerHandle {
    /// The endpoint being served.
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Stop: no new connections, wait (bounded) for subscription streams
    /// to finish, remove the endpoint record. Close the hub first so the
    /// streams can end.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(accept) = self.accept.take() {
            let _ = accept.join();
        }
        let deadline = Instant::now() + self.shared.options.drain_timeout;
        let streams = &self.shared.streams;
        let mut active = streams
            .active
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        while *active > 0 {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            active = streams
                .done
                .wait_timeout(active, deadline - now)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        drop(active);
        endpoint::remove_record(&self.paths, &self.endpoint);
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        if self.accept.is_some() {
            self.shutdown();
        }
    }
}

const ACCEPT_POLL: Duration = Duration::from_millis(20);

fn accept_loop(listener: &LocalSocketListener, stop: &AtomicBool, shared: &Arc<Shared>) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok(stream) => {
                let conn = Arc::clone(shared);
                let previous = shared.connections.fetch_add(1, Ordering::SeqCst);
                let spawned =
                    thread::Builder::new()
                        .name("glint-ipc-conn".into())
                        .spawn(move || {
                            let mut stream = stream;
                            if previous >= conn.options.max_connections {
                                refuse_busy(&mut stream, conn.options.max_connections);
                            } else {
                                Connection::new(&mut stream, &conn).run();
                            }
                            conn.connections.fetch_sub(1, Ordering::SeqCst);
                        });
                if spawned.is_err() {
                    shared.connections.fetch_sub(1, Ordering::SeqCst);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_POLL);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            // A client that vanished between connect and accept, or a
            // transient OS error: keep serving.
            Err(_) => thread::sleep(ACCEPT_POLL),
        }
    }
}

fn refuse_busy(stream: &mut LocalSocketStream, limit: usize) {
    let error = IpcError::new(
        ErrorCode::Busy,
        format!("the core is serving its maximum of {limit} connections"),
    );
    let frame = ServerFrame::Response(ResponseEnvelope::error(None, SUPPORTED_VERSIONS.max, error));
    if let Ok(bytes) = encode_frame(&frame, MAX_RESPONSE_BYTES) {
        let _ = codec::write_frame(stream, &bytes);
    }
}

struct Connection<'a, S: Read + Write> {
    stream: &'a mut S,
    shared: &'a Shared,
    api_version: Option<u32>,
}

enum Flow {
    Continue,
    Close,
}

impl<'a, S: Read + Write> Connection<'a, S> {
    fn new(stream: &'a mut S, shared: &'a Shared) -> Self {
        Self {
            stream,
            shared,
            api_version: None,
        }
    }

    fn version(&self) -> u32 {
        self.api_version.unwrap_or(SUPPORTED_VERSIONS.max)
    }

    /// Send a frame; `false` when the peer is gone.
    fn send(&mut self, frame: &ServerFrame) -> bool {
        let bytes = match encode_frame(frame, MAX_RESPONSE_BYTES) {
            Ok(bytes) => bytes,
            Err(error) => {
                // Never send a truncated or oversized frame: replace it with
                // a typed error that names the request.
                let request_id = match frame {
                    ServerFrame::Response(response) => response.request_id.clone(),
                    _ => None,
                };
                let fallback = ServerFrame::Response(ResponseEnvelope::error(
                    request_id,
                    self.version(),
                    error,
                ));
                match encode_frame(&fallback, MAX_RESPONSE_BYTES) {
                    Ok(bytes) => bytes,
                    Err(_) => return false,
                }
            }
        };
        codec::write_frame(self.stream, &bytes).is_ok()
    }

    fn send_error(&mut self, request_id: Option<String>, error: IpcError) -> bool {
        let frame =
            ServerFrame::Response(ResponseEnvelope::error(request_id, self.version(), error));
        self.send(&frame)
    }

    fn run(mut self) {
        loop {
            let body = match read_frame(self.stream, MAX_REQUEST_BYTES) {
                Ok(Some(body)) => body,
                Ok(None) => return,
                Err(error) => {
                    // Oversized or empty frames get a typed answer; after
                    // a bad length the stream is out of sync, so close.
                    if matches!(
                        error,
                        codec::FrameError::TooLarge { .. } | codec::FrameError::Empty
                    ) {
                        self.send_error(None, error.to_ipc());
                    }
                    return;
                }
            };
            if let Flow::Close = self.handle(&body) {
                return;
            }
        }
    }

    fn handle(&mut self, body: &[u8]) -> Flow {
        let frame = match decode_client_frame(body) {
            Ok(frame) => frame,
            Err(decode) => {
                // The length prefix kept the stream in sync: answer and
                // keep the connection, except before the handshake.
                let close = self.api_version.is_none();
                let sent = self.send_error(decode.request_id, decode.error);
                return if sent && !close {
                    Flow::Continue
                } else {
                    Flow::Close
                };
            }
        };
        match frame {
            ClientFrame::Hello(hello) => {
                if self.api_version.is_some() {
                    let error = IpcError::new(ErrorCode::ProtocolViolation, "second hello");
                    return Self::flow(self.send_error(None, error));
                }
                match SUPPORTED_VERSIONS.negotiate(hello.versions) {
                    Some(version) => {
                        self.api_version = Some(version);
                        let welcome = ServerFrame::Welcome(Welcome::new(
                            version,
                            self.shared.options.server_name.clone(),
                        ));
                        Self::flow(self.send(&welcome))
                    }
                    None => {
                        let error = IpcError::new(
                            ErrorCode::VersionUnsupported,
                            format!(
                                "client speaks API {}..={}, server speaks {}..={}",
                                hello.versions.min,
                                hello.versions.max,
                                SUPPORTED_VERSIONS.min,
                                SUPPORTED_VERSIONS.max
                            ),
                        );
                        self.send_error(None, error);
                        Flow::Close
                    }
                }
            }
            ClientFrame::Request(envelope) => {
                let Some(version) = self.api_version else {
                    let error = IpcError::new(
                        ErrorCode::HandshakeRequired,
                        "send hello before any request",
                    );
                    self.send_error(Some(envelope.request_id), error);
                    return Flow::Close;
                };
                if envelope.api_version != version {
                    let error = IpcError::new(
                        ErrorCode::VersionUnsupported,
                        format!(
                            "request carries API {}, the connection negotiated {version}",
                            envelope.api_version
                        ),
                    );
                    return Self::flow(self.send_error(Some(envelope.request_id), error));
                }
                if envelope.request == GlintRequest::Subscribe {
                    self.stream_subscription(envelope.request_id);
                    return Flow::Close;
                }
                let result = match self.answer(&envelope.request) {
                    Ok(body) => Outcome::Ok(body),
                    Err(error) => Outcome::Error(error),
                };
                let frame = ServerFrame::Response(ResponseEnvelope {
                    request_id: Some(envelope.request_id),
                    api_version: version,
                    result,
                });
                Self::flow(self.send(&frame))
            }
            ClientFrame::Goodbye => Flow::Close,
        }
    }

    fn flow(sent: bool) -> Flow {
        if sent { Flow::Continue } else { Flow::Close }
    }

    fn answer(&self, request: &GlintRequest) -> Result<ResponseBody, IpcError> {
        let hub = &self.shared.hub;
        let (sequence, snapshot) = hub.snapshot();
        let not_ready = |what: &str| {
            IpcError::new(
                ErrorCode::NotReady,
                format!("{what} is not known yet (lifecycle {})", snapshot.lifecycle),
            )
        };
        match request {
            GlintRequest::Ping => Ok(ResponseBody::Pong),
            GlintRequest::GetStatus => Ok(ResponseBody::Status(StatusReply {
                lifecycle: snapshot.lifecycle.clone(),
                daemon: snapshot.daemon.clone(),
                pid: snapshot.pid,
                sequence,
            })),
            GlintRequest::GetSnapshot => Ok(ResponseBody::Snapshot(Box::new(SnapshotReply::new(
                sequence,
                snapshot.clone(),
            )))),
            GlintRequest::GetState => snapshot
                .state
                .clone()
                .map(ResponseBody::State)
                .ok_or_else(|| not_ready("state")),
            GlintRequest::GetConfig => snapshot
                .config
                .clone()
                .map(ResponseBody::Config)
                .ok_or_else(|| not_ready("config")),
            GlintRequest::GetCapabilities => snapshot
                .capabilities
                .clone()
                .map(ResponseBody::Capabilities)
                .ok_or_else(|| not_ready("capabilities")),
            GlintRequest::GetTheme => snapshot
                .theme
                .clone()
                .map(|theme| ResponseBody::Theme(Box::new(theme)))
                .ok_or_else(|| not_ready("theme")),
            GlintRequest::Shutdown => self
                .shared
                .control
                .shutdown()
                .map(|initiated| ResponseBody::ShutdownAccepted(ShutdownReply { initiated })),
            GlintRequest::Subscribe => Err(IpcError::new(
                ErrorCode::Internal,
                "subscribe is handled by the stream",
            )),
        }
    }

    fn stream_subscription(&mut self, request_id: String) {
        let version = self.version();
        let Some((subscriber, sequence, snapshot)) = self
            .shared
            .hub
            .subscribe(self.shared.options.queue_capacity)
        else {
            let reason = self.shared.hub.closed().unwrap_or_default();
            let error = IpcError::new(
                ErrorCode::ShuttingDown,
                format!("the core is shutting down: {reason}"),
            );
            self.send_error(Some(request_id), error);
            return;
        };
        let _guard = StreamGuard::enter(&self.shared.streams);
        let frame = ServerFrame::Response(ResponseEnvelope::ok(
            request_id,
            version,
            ResponseBody::Subscribed(Box::new(SnapshotReply::new(sequence, snapshot))),
        ));
        if !self.send(&frame) {
            return;
        }
        self.pump(&subscriber, sequence);
    }

    fn pump(&mut self, subscriber: &Subscriber, mut last: u64) {
        loop {
            let event = match subscriber.next(self.shared.options.heartbeat) {
                Next::Delta(sequence, delta) => {
                    last = sequence;
                    StreamEvent::Delta { sequence, delta }
                }
                Next::Lagged => {
                    let (sequence, snapshot) = self.shared.hub.resync(subscriber);
                    last = sequence;
                    StreamEvent::Snapshot {
                        sequence,
                        reason: "lagged".to_owned(),
                        snapshot: Box::new(snapshot),
                    }
                }
                Next::Idle => StreamEvent::Heartbeat { sequence: last },
                Next::Closed(reason) => {
                    let _ = self.send(&ServerFrame::Event {
                        event: StreamEvent::Closed {
                            sequence: last,
                            reason,
                        },
                    });
                    return;
                }
            };
            if !self.send(&ServerFrame::Event { event }) {
                return;
            }
        }
    }
}

struct StreamGuard<'a>(&'a Streams);

impl<'a> StreamGuard<'a> {
    fn enter(streams: &'a Streams) -> Self {
        *streams
            .active
            .lock()
            .unwrap_or_else(PoisonError::into_inner) += 1;
        Self(streams)
    }
}

impl Drop for StreamGuard<'_> {
    fn drop(&mut self) {
        let mut active = self.0.active.lock().unwrap_or_else(PoisonError::into_inner);
        *active -= 1;
        self.0.done.notify_all();
    }
}
