//! Golden JSON fixtures: the typed values serialize to exactly the
//! committed wire format, and the committed files decode back to them.

use std::path::Path;

use glint_ipc::codec::{decode_client_frame, decode_server_frame};
use glint_ipc::{
    CapabilitySummary, ClientFrame, ConfigSummary, CoreSnapshot, Delta, ErrorCode, GlintRequest,
    Hello, IpcError, RequestEnvelope, ResponseBody, ResponseEnvelope, ServerFrame, SnapshotReply,
    StateSummary, StatusReply, StreamEvent, ThemeSummary, VersionRange, WarningInfo, Welcome,
};
use serde_json::Value;

fn fixture(name: &str) -> (Vec<u8>, Value) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.json"));
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let value = serde_json::from_slice(&bytes).unwrap();
    (bytes, value)
}

fn client(name: &str, frame: &ClientFrame) {
    let (bytes, expected) = fixture(name);
    assert_eq!(serde_json::to_value(frame).unwrap(), expected, "{name}");
    assert_eq!(&decode_client_frame(&bytes).unwrap(), frame, "{name}");
}

fn server(name: &str, frame: &ServerFrame) {
    let (bytes, expected) = fixture(name);
    assert_eq!(serde_json::to_value(frame).unwrap(), expected, "{name}");
    assert_eq!(&decode_server_frame(&bytes).unwrap(), frame, "{name}");
}

fn ready_snapshot() -> CoreSnapshot {
    let mut snapshot = CoreSnapshot::new(4242);
    snapshot.apply(&Delta::Lifecycle {
        state: "ready".into(),
    });
    snapshot.apply(&Delta::Daemon {
        status: "serving".into(),
    });
    snapshot.apply(&Delta::Booted {
        state: StateSummary::new(4, 1, false),
        config: ConfigSummary::new("absent", 0, true),
        capabilities: CapabilitySummary::new("headless", 1, 0, 0, None),
        theme: Box::new(
            ThemeSummary::bundled("official.default", "1.0.0", "no-active-theme")
                .with_surfaces(vec!["panel".into()], Vec::new()),
        ),
    });
    snapshot.apply(&Delta::Warning {
        warning: WarningInfo::new(
            "select-theme",
            "GLINT_CORE_WARN_THEME_FALLBACK",
            "no-active-theme: no profile is active",
        ),
    });
    snapshot
}

#[test]
fn client_frames_match_their_fixtures() {
    client(
        "hello",
        &ClientFrame::Hello(Hello::new(VersionRange::new(1, 1), "glint-cli/0.0.0")),
    );
    client(
        "request-get-status",
        &ClientFrame::Request(RequestEnvelope::new("r1", 1, GlintRequest::GetStatus)),
    );
    client("goodbye", &ClientFrame::Goodbye);
}

#[test]
fn server_frames_match_their_fixtures() {
    server(
        "welcome",
        &ServerFrame::Welcome(Welcome::new(1, "glint-daemon/0.0.0")),
    );
    server(
        "response-status",
        &ServerFrame::Response(ResponseEnvelope::ok(
            "r1",
            1,
            ResponseBody::Status(StatusReply::new("ready", "serving", 4242, 17)),
        )),
    );
    server(
        "response-unknown-request",
        &ServerFrame::Response(ResponseEnvelope::error(
            Some("r2".into()),
            1,
            IpcError::new(
                ErrorCode::UnknownRequest,
                "unknown request type \"run-command\"",
            ),
        )),
    );
    server(
        "response-version-unsupported",
        &ServerFrame::Response(ResponseEnvelope::error(
            None,
            1,
            IpcError::new(
                ErrorCode::VersionUnsupported,
                "client speaks API 2..=3, server speaks 1..=1",
            ),
        )),
    );
    server(
        "response-subscribed",
        &ServerFrame::Response(ResponseEnvelope::ok(
            "r3",
            1,
            ResponseBody::Subscribed(Box::new(SnapshotReply::new(17, ready_snapshot()))),
        )),
    );
    server(
        "event-delta-lifecycle",
        &ServerFrame::Event {
            event: StreamEvent::Delta {
                sequence: 18,
                delta: Delta::Lifecycle {
                    state: "stopping".into(),
                },
            },
        },
    );
    server(
        "event-delta-shutdown-started",
        &ServerFrame::Event {
            event: StreamEvent::Delta {
                sequence: 19,
                delta: Delta::ShutdownStarted {
                    reason: "requested".into(),
                },
            },
        },
    );
    server(
        "event-heartbeat",
        &ServerFrame::Event {
            event: StreamEvent::Heartbeat { sequence: 19 },
        },
    );
    server(
        "event-closed",
        &ServerFrame::Event {
            event: StreamEvent::Closed {
                sequence: 19,
                reason: "stopped".into(),
            },
        },
    );
}

#[test]
fn every_error_code_has_a_stable_wire_name() {
    for code in ErrorCode::ALL {
        let wire = serde_json::to_value(code).unwrap();
        assert_eq!(wire, Value::String(code.as_str().to_owned()));
        assert!(code.as_str().starts_with("GLINT_IPC_"));
        let back: ErrorCode = serde_json::from_value(wire).unwrap();
        assert_eq!(back, *code);
    }
}

#[test]
fn every_request_type_is_listed_and_round_trips() {
    let all = [
        GlintRequest::Ping,
        GlintRequest::GetStatus,
        GlintRequest::GetSnapshot,
        GlintRequest::GetState,
        GlintRequest::GetConfig,
        GlintRequest::GetCapabilities,
        GlintRequest::GetTheme,
        GlintRequest::Subscribe,
        GlintRequest::Shutdown,
    ];
    assert_eq!(all.len(), GlintRequest::TYPES.len());
    for request in all {
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["type"], request.type_name());
        assert!(GlintRequest::TYPES.contains(&request.type_name()));
        let back: GlintRequest = serde_json::from_value(value).unwrap();
        assert_eq!(back, request);
    }
}
