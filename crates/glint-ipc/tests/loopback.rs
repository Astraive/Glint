//! The real transport (named pipe on Windows, Unix socket elsewhere) with
//! an in-process server under a temporary data root.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use glint_ipc::codec::{
    MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES, decode_server_frame, encode_frame, read_frame,
    write_frame,
};
use glint_ipc::{
    API_VERSION, Client, ClientFrame, ClientOptions, Control, CoreSnapshot, Delta, Endpoint,
    ErrorCode, GlintRequest, Hello, Hub, IpcError, Outcome, RawConnection, RequestEnvelope,
    ResponseBody, ServerFrame, ServerHandle, ServerOptions, StateSummary, Update, VersionRange,
    serve,
};
use glint_storage::GlintPaths;

const WAIT: Duration = Duration::from_secs(10);

#[derive(Default)]
struct Recorder {
    shutdowns: AtomicUsize,
    delay: Option<Duration>,
}

impl Control for Recorder {
    fn shutdown(&self) -> Result<bool, IpcError> {
        if let Some(delay) = self.delay {
            std::thread::sleep(delay);
        }
        Ok(self.shutdowns.fetch_add(1, Ordering::SeqCst) == 0)
    }
}

struct Fixture {
    _tmp: tempfile::TempDir,
    paths: GlintPaths,
    hub: Arc<Hub>,
    control: Arc<Recorder>,
    server: Option<ServerHandle>,
}

impl Fixture {
    fn new() -> Self {
        Self::with(ServerOptions::named("glint-test/0"), Recorder::default())
    }

    fn with(mut options: ServerOptions, control: Recorder) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let paths = GlintPaths::under_root(tmp.path()).unwrap();
        paths.ensure_dirs().unwrap();
        let hub = Arc::new(Hub::new(CoreSnapshot::new(std::process::id())));
        hub.publish(Delta::Lifecycle {
            state: "ready".into(),
        });
        let control = Arc::new(control);
        options.drain_timeout = Duration::from_secs(2);
        let server = serve(&paths, Arc::clone(&hub), control.clone(), options).unwrap();
        Self {
            _tmp: tmp,
            paths,
            hub,
            control,
            server: Some(server),
        }
    }

    fn endpoint(&self) -> Endpoint {
        self.server.as_ref().unwrap().endpoint().clone()
    }

    fn client(&self) -> Client {
        Client::connect(&self.paths).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.hub.close("test over");
        if let Some(server) = self.server.take() {
            server.stop();
        }
    }
}

fn send(raw: &mut RawConnection, frame: &ClientFrame) {
    write_frame(raw, &encode_frame(frame, MAX_REQUEST_BYTES).unwrap()).unwrap();
}

fn send_bytes(raw: &mut RawConnection, body: &[u8]) {
    let mut frame = (body.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(body);
    write_frame(raw, &frame).unwrap();
}

fn receive(raw: &mut RawConnection) -> Option<ServerFrame> {
    read_frame(raw, MAX_RESPONSE_BYTES)
        .unwrap()
        .map(|body| decode_server_frame(&body).unwrap())
}

fn error_of(frame: Option<ServerFrame>) -> (Option<String>, IpcError) {
    match frame {
        Some(ServerFrame::Response(response)) => match response.result {
            Outcome::Error(error) => (response.request_id, error),
            Outcome::Ok(body) => panic!("expected an error, got {body:?}"),
        },
        other => panic!("expected a response, got {other:?}"),
    }
}

fn handshake(raw: &mut RawConnection) {
    send(
        raw,
        &ClientFrame::Hello(Hello::new(VersionRange::new(1, 1), "raw-test")),
    );
    assert!(matches!(receive(raw), Some(ServerFrame::Welcome(_))));
}

/// Read until the server closes; a closed pipe may surface as an error.
fn closes(raw: &mut RawConnection) -> bool {
    match read_frame(raw, MAX_RESPONSE_BYTES) {
        Ok(None) | Err(_) => true,
        Ok(Some(_)) => false,
    }
}

#[test]
fn requests_are_answered_from_the_published_snapshot() {
    let fx = Fixture::new();
    let mut client = fx.client();
    assert_eq!(client.api_version(), API_VERSION);
    assert_eq!(client.welcome().server, "glint-test/0");
    client.ping().unwrap();
    let status = client.status().unwrap();
    assert_eq!(status.lifecycle, "ready");
    assert_eq!(status.sequence, 1);
    assert_eq!(status.pid, std::process::id());

    // Not booted yet: typed not-ready, not a fake value.
    let error = client.request(GlintRequest::GetState).unwrap_err();
    assert_eq!(error.code(), ErrorCode::NotReady);

    fx.hub.publish(Delta::Booted {
        state: StateSummary::new(4, 2, false),
        config: glint_ipc::ConfigSummary::new("absent", 0, true),
        capabilities: glint_ipc::CapabilitySummary::new("headless", 1, 0, 0, None),
        theme: Box::new(glint_ipc::ThemeSummary::bundled(
            "official.default",
            "1.0.0",
            "no-active-theme",
        )),
    });
    assert_eq!(
        client.request(GlintRequest::GetState).unwrap(),
        ResponseBody::State(StateSummary::new(4, 2, false))
    );
    let ResponseBody::Theme(theme) = client.request(GlintRequest::GetTheme).unwrap() else {
        panic!("theme");
    };
    assert_eq!(theme.theme_id, "official.default");
    assert_eq!(client.snapshot().unwrap().sequence, 2);
}

#[test]
fn a_subscription_starts_from_a_snapshot_and_receives_ordered_deltas() {
    let mut options = ServerOptions::named("glint-test/0");
    options.heartbeat = Duration::from_millis(100);
    let fx = Fixture::with(options, Recorder::default());
    let mut subscription = fx.client().subscribe().unwrap();
    assert_eq!(subscription.sequence(), 1);
    assert_eq!(subscription.snapshot().lifecycle, "ready");

    fx.hub.publish(Delta::Lifecycle {
        state: "stopping".into(),
    });
    fx.hub.publish(Delta::ShutdownStarted {
        reason: "requested".into(),
    });
    let mut deltas = Vec::new();
    while deltas.len() < 2 {
        match subscription.next(WAIT).unwrap() {
            Update::Delta { sequence, .. } => deltas.push(sequence),
            Update::Heartbeat { .. } => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(deltas, [2, 3]);
    assert_eq!(subscription.snapshot(), &fx.hub.snapshot().1);

    // Quiet: heartbeats carry the last sequence number.
    assert_eq!(
        subscription.next(WAIT).unwrap(),
        Update::Heartbeat { sequence: 3 }
    );

    fx.hub.close("stopped");
    loop {
        match subscription.next(WAIT).unwrap() {
            Update::Closed { reason } => {
                assert_eq!(reason, "stopped");
                break;
            }
            Update::Heartbeat { .. } => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    // Subscribing after close is refused with a typed error.
    let error = fx.client().subscribe().unwrap_err();
    assert_eq!(error.code(), ErrorCode::ShuttingDown);
}

#[test]
fn a_reconnecting_client_receives_the_current_snapshot() {
    let fx = Fixture::new();
    let mut subscription = fx.client().subscribe().unwrap();
    assert_eq!(subscription.sequence(), 1);
    // Changes the client never saw as deltas.
    fx.hub.publish(Delta::Lifecycle {
        state: "stopping".into(),
    });
    fx.hub.publish(Delta::Daemon {
        status: "stopped".into(),
    });
    subscription.reconnect(Some(&fx.paths)).unwrap();
    let (sequence, snapshot) = fx.hub.snapshot();
    assert_eq!(subscription.sequence(), sequence);
    assert_eq!(subscription.snapshot(), &snapshot);
    assert_eq!(subscription.snapshot().daemon, "stopped");
}

#[test]
fn a_version_mismatch_is_refused_at_the_handshake() {
    let fx = Fixture::new();
    let options = ClientOptions::default().with_versions(VersionRange::new(2, 3));
    let error = Client::connect_with(&fx.endpoint(), options).unwrap_err();
    assert_eq!(error.code(), ErrorCode::VersionUnsupported);

    // A request carrying another version than the negotiated one is
    // refused too, and the connection stays usable.
    let mut raw = RawConnection::connect(&fx.endpoint()).unwrap();
    handshake(&mut raw);
    send(
        &mut raw,
        &ClientFrame::Request(RequestEnvelope::new("r1", 2, GlintRequest::Ping)),
    );
    let (id, error) = error_of(receive(&mut raw));
    assert_eq!(id.as_deref(), Some("r1"));
    assert_eq!(error.code(), ErrorCode::VersionUnsupported);
    send(
        &mut raw,
        &ClientFrame::Request(RequestEnvelope::new("r2", 1, GlintRequest::Ping)),
    );
    assert!(matches!(receive(&mut raw), Some(ServerFrame::Response(_))));
}

#[test]
fn unknown_requests_and_malformed_frames_get_typed_errors_and_the_connection_survives() {
    let fx = Fixture::new();
    let mut raw = RawConnection::connect(&fx.endpoint()).unwrap();
    handshake(&mut raw);

    send_bytes(
        &mut raw,
        br#"{"kind":"request","request_id":"r1","api_version":1,"request":{"type":"run-command","payload":{"cmd":"calc.exe"}}}"#,
    );
    let (id, error) = error_of(receive(&mut raw));
    assert_eq!(id.as_deref(), Some("r1"));
    assert_eq!(error.code(), ErrorCode::UnknownRequest);

    send_bytes(&mut raw, b"{not json");
    let (id, error) = error_of(receive(&mut raw));
    assert_eq!(id, None);
    assert_eq!(error.code(), ErrorCode::MalformedFrame);

    send(
        &mut raw,
        &ClientFrame::Hello(Hello::new(VersionRange::new(1, 1), "again")),
    );
    assert_eq!(
        error_of(receive(&mut raw)).1.code(),
        ErrorCode::ProtocolViolation
    );

    send(
        &mut raw,
        &ClientFrame::Request(RequestEnvelope::new("r2", 1, GlintRequest::Ping)),
    );
    let Some(ServerFrame::Response(response)) = receive(&mut raw) else {
        panic!("ping");
    };
    assert_eq!(response.request_id.as_deref(), Some("r2"));
    assert_eq!(response.result, Outcome::Ok(ResponseBody::Pong));
}

#[test]
fn an_oversized_frame_is_refused_and_the_connection_closed() {
    let fx = Fixture::new();
    let mut raw = RawConnection::connect(&fx.endpoint()).unwrap();
    handshake(&mut raw);
    // Only the length is sent: the server must refuse without reading on.
    write_frame(&mut raw, &(MAX_REQUEST_BYTES + 1).to_be_bytes()).unwrap();
    let (_, error) = error_of(receive(&mut raw));
    assert_eq!(error.code(), ErrorCode::FrameTooLarge);
    assert!(closes(&mut raw));
}

#[test]
fn a_request_before_the_handshake_is_refused_and_the_connection_closed() {
    let fx = Fixture::new();
    let mut raw = RawConnection::connect(&fx.endpoint()).unwrap();
    send(
        &mut raw,
        &ClientFrame::Request(RequestEnvelope::new("r1", 1, GlintRequest::Shutdown)),
    );
    let (id, error) = error_of(receive(&mut raw));
    assert_eq!(id.as_deref(), Some("r1"));
    assert_eq!(error.code(), ErrorCode::HandshakeRequired);
    assert!(closes(&mut raw));
    assert_eq!(fx.control.shutdowns.load(Ordering::SeqCst), 0);
}

#[test]
fn shutdown_goes_through_the_hosts_control() {
    let fx = Fixture::new();
    let mut client = fx.client();
    assert!(client.shutdown().unwrap());
    assert!(!client.shutdown().unwrap());
    assert_eq!(fx.control.shutdowns.load(Ordering::SeqCst), 2);
}

#[test]
fn a_slow_answer_times_out_and_poisons_the_connection() {
    let control = Recorder {
        delay: Some(Duration::from_millis(1500)),
        ..Recorder::default()
    };
    let fx = Fixture::with(ServerOptions::named("glint-test/0"), control);
    let options = ClientOptions::default().with_timeout(Duration::from_millis(200));
    let mut client = Client::connect_with(&fx.endpoint(), options).unwrap();
    let error = client.shutdown().unwrap_err();
    assert_eq!(error.code(), ErrorCode::Timeout);
    // The late answer must not be mistaken for the next request's.
    let error = client.ping().unwrap_err();
    assert_eq!(error.code(), ErrorCode::Timeout);
    // A new connection works.
    fx.client().ping().unwrap();
}

#[test]
fn connections_over_the_limit_are_refused_as_busy() {
    let mut options = ServerOptions::named("glint-test/0");
    options.max_connections = 1;
    let fx = Fixture::with(options, Recorder::default());
    let mut first = fx.client();
    first.ping().unwrap();
    let error = Client::connect(&fx.paths).unwrap_err();
    assert_eq!(error.code(), ErrorCode::Busy);
    // Once the first leaves, a new client gets in.
    drop(first);
    let mut admitted = false;
    for _ in 0..100 {
        if let Ok(mut client) = Client::connect(&fx.paths) {
            client.ping().unwrap();
            admitted = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(admitted);
}

#[test]
fn stopping_removes_the_endpoint_record() {
    let mut fx = Fixture::new();
    assert!(fx.paths.ipc_endpoint_file().is_file());
    fx.hub.close("stopped");
    fx.server.take().unwrap().stop();
    assert!(!fx.paths.ipc_endpoint_file().exists());
    let error = Client::connect(&fx.paths).unwrap_err();
    assert_eq!(error.code(), ErrorCode::Unavailable);
}

/// A host that does not own state refuses every theme operation with a
/// typed `unsupported` error; nothing reaches a writer.
#[test]
fn operations_are_refused_by_a_host_without_state() {
    let fixture = Fixture::new();
    let mut client = fixture.client();
    for request in [
        GlintRequest::ThemeList,
        GlintRequest::ThemeUse {
            theme: "default".into(),
        },
        GlintRequest::SafeMode,
    ] {
        let error = client.request(request).unwrap_err();
        assert_eq!(error.code(), ErrorCode::OperationUnsupported);
        assert_eq!(error.category(), glint_ipc::ErrorCategory::Unsupported);
    }
    // The connection stays usable after a refused operation.
    client.ping().unwrap();
}

struct Operator;

impl Control for Operator {
    fn shutdown(&self) -> Result<bool, IpcError> {
        Ok(true)
    }

    fn operate(&self, request: &GlintRequest) -> Result<ResponseBody, IpcError> {
        match request {
            GlintRequest::ThemeList => Ok(ResponseBody::ThemeList(glint_ipc::ThemeListReply::new(
                Vec::new(),
                None,
            ))),
            other => Err(IpcError::operation_failed(
                glint_ipc::OperationFailure::new(
                    "GLINT_INSTALL_NOT_INSTALLED",
                    glint_ipc::OperationErrorKind::NotFound,
                ),
                format!("{} found nothing", other.type_name()),
            )),
        }
    }
}

/// Operations reach the host's `Control::operate`; its typed failure comes
/// back intact, and a core that is shutting down refuses them.
#[test]
fn operations_reach_the_host_and_failures_keep_their_codes() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = GlintPaths::under_root(tmp.path()).unwrap();
    paths.ensure_dirs().unwrap();
    let hub = Arc::new(Hub::new(CoreSnapshot::new(std::process::id())));
    let server = serve(
        &paths,
        Arc::clone(&hub),
        Arc::new(Operator),
        ServerOptions::named("glint-test/0"),
    )
    .unwrap();
    let mut client = Client::connect(&paths).unwrap();
    assert!(matches!(
        client.request(GlintRequest::ThemeList).unwrap(),
        ResponseBody::ThemeList(_)
    ));
    let error = client
        .request(GlintRequest::ThemeRemove { theme: "x".into() })
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::OperationFailed);
    let failure = error.operation().unwrap();
    assert_eq!(failure.code, "GLINT_INSTALL_NOT_INSTALLED");
    assert_eq!(failure.kind, glint_ipc::OperationErrorKind::NotFound);

    hub.publish(Delta::ShutdownStarted {
        reason: "requested".into(),
    });
    let error = client.request(GlintRequest::SafeMode).unwrap_err();
    assert_eq!(error.code(), ErrorCode::ShuttingDown);
    drop(client);
    hub.close("stopped");
    server.stop();
}
