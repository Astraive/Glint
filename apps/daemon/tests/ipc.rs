//! Local IPC against the real `glint-daemon` binary under a temporary
//! `GLINT_HOME` (COR-003): requests, a subscription snapshot followed by
//! ordered deltas, reconnect and denial of unknown requests.

mod support;

use std::io::Write as _;
use std::time::Duration;

use glint_ipc::codec::{MAX_RESPONSE_BYTES, decode_server_frame, read_frame, write_frame};
use glint_ipc::{
    Client, ClientFrame, Delta, Endpoint, ErrorCode, GlintRequest, Hello, Outcome, RawConnection,
    ResponseBody, ServerFrame, Update, VersionRange,
};
use glint_storage::GlintPaths;
use support::{Daemon, glint_home, run_once};

const LISTENING: &str = "glint-daemon: ipc listening ";
const WAIT: Duration = Duration::from_secs(30);

#[test]
fn a_subscriber_gets_the_snapshot_then_every_shutdown_delta_in_order() {
    let home = glint_home();
    let mut daemon = Daemon::spawn(home.path());
    daemon.wait_for(LISTENING);
    let paths = GlintPaths::under_root(home.path()).unwrap();

    // Request/response.
    let mut client = Client::connect(&paths).unwrap();
    assert!(client.welcome().server.starts_with("glint-daemon/"));
    let status = client.status().unwrap();
    assert_eq!(status.lifecycle, "ready");
    assert_eq!(status.daemon, "serving");
    assert_eq!(status.pid, daemon.pid());
    let ResponseBody::Theme(theme) = client.request(GlintRequest::GetTheme).unwrap() else {
        panic!("theme");
    };
    assert_eq!(theme.theme_id, "official.default");
    assert_eq!(theme.origin, "bundled-default");
    assert_eq!(theme.fallback_reason.as_deref(), Some("no-active-theme"));
    let ResponseBody::Capabilities(capabilities) =
        client.request(GlintRequest::GetCapabilities).unwrap()
    else {
        panic!("capabilities");
    };
    assert_eq!(capabilities.adapter, "headless");

    // Subscription: the snapshot first.
    let mut subscription = Client::connect(&paths).unwrap().subscribe().unwrap();
    assert_eq!(subscription.snapshot().lifecycle, "ready");
    assert_eq!(subscription.sequence(), status.sequence);
    assert!(
        subscription
            .snapshot()
            .warnings
            .iter()
            .any(|w| w.code == "GLINT_CORE_WARN_THEME_FALLBACK")
    );

    // A shutdown request from another client produces the deltas.
    assert!(client.shutdown().unwrap());
    let mut deltas = Vec::new();
    let reason = loop {
        match subscription.next(WAIT).unwrap() {
            Update::Delta { sequence, delta } => deltas.push((sequence, delta)),
            Update::Heartbeat { .. } => {}
            Update::Closed { reason } => break reason,
            other => panic!("unexpected {other:?}"),
        }
    };
    assert_eq!(reason, "the core stopped");
    // Consecutive sequence numbers (the client library also refuses gaps).
    for (i, (sequence, _)) in deltas.iter().enumerate() {
        assert_eq!(*sequence, status.sequence + 1 + i as u64);
    }
    let kinds: Vec<&Delta> = deltas.iter().map(|(_, delta)| delta).collect();
    assert_eq!(
        kinds.first(),
        Some(&&Delta::Lifecycle {
            state: "stopping".into()
        })
    );
    assert!(kinds.contains(&&Delta::ShutdownStarted {
        reason: "requested".into()
    }));
    assert!(kinds.contains(&&Delta::Daemon {
        status: "stopped".into()
    }));
    assert_eq!(
        kinds.last(),
        Some(&&Delta::Lifecycle {
            state: "stopped".into()
        })
    );
    assert_eq!(subscription.snapshot().lifecycle, "stopped");
    assert_eq!(
        subscription.snapshot().shutdown_reason.as_deref(),
        Some("requested")
    );

    let (code, out) = daemon.finish();
    assert_eq!(code, Some(0), "{out}");
    assert!(out.contains("glint-daemon: shutdown requested"), "{out}");
    assert!(!paths.ipc_endpoint_file().exists());
    assert_eq!(
        Client::connect(&paths).unwrap_err().code(),
        ErrorCode::Unavailable
    );
}

#[test]
fn reconnecting_restores_the_snapshot_and_unknown_requests_are_refused() {
    let home = glint_home();
    let mut daemon = Daemon::spawn(home.path());
    daemon.wait_for(LISTENING);
    let paths = GlintPaths::under_root(home.path()).unwrap();

    let subscription = Client::connect(&paths).unwrap().subscribe().unwrap();
    let first = (subscription.sequence(), subscription.snapshot().clone());
    drop(subscription);
    let mut again = Client::connect(&paths).unwrap().subscribe().unwrap();
    assert_eq!((again.sequence(), again.snapshot().clone()), first);
    again.reconnect(Some(&paths)).unwrap();
    assert_eq!(again.snapshot(), &first.1);

    // No arbitrary command reaches the core.
    let endpoint = Endpoint::read(&paths).unwrap();
    let mut raw = RawConnection::connect(&endpoint).unwrap();
    let hello = ClientFrame::Hello(Hello::new(VersionRange::new(1, 1), "raw"));
    let bytes = glint_ipc::codec::encode_frame(&hello, 1024).unwrap();
    write_frame(&mut raw, &bytes).unwrap();
    let welcome = read_frame(&mut raw, MAX_RESPONSE_BYTES).unwrap().unwrap();
    assert!(matches!(
        decode_server_frame(&welcome).unwrap(),
        ServerFrame::Welcome(_)
    ));
    let body = br#"{"kind":"request","request_id":"x1","api_version":1,"request":{"type":"exec","payload":{"cmd":"calc.exe"}}}"#;
    let mut frame = (body.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(body);
    write_frame(&mut raw, &frame).unwrap();
    let reply = read_frame(&mut raw, MAX_RESPONSE_BYTES).unwrap().unwrap();
    let ServerFrame::Response(response) = decode_server_frame(&reply).unwrap() else {
        panic!("response");
    };
    assert_eq!(response.request_id.as_deref(), Some("x1"));
    let Outcome::Error(error) = response.result else {
        panic!("expected an error");
    };
    assert_eq!(error.code(), ErrorCode::UnknownRequest);
    drop(raw);

    // A second launch reaches the running instance over IPC.
    let second = run_once(home.path(), &["--once"]);
    let stdout = String::from_utf8_lossy(&second.stdout);
    assert_eq!(second.status.code(), Some(9), "{stdout}");
    assert!(
        stdout.contains(&format!("already-running pid={} ipc=ready", daemon.pid())),
        "{stdout}"
    );

    daemon.stdin().write_all(b"shutdown\n").unwrap();
    let (code, out) = daemon.finish();
    assert_eq!(code, Some(0), "{out}");
}
