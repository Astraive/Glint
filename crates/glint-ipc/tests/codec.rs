//! In-process codec, negotiation, hub and subscription-state tests (no
//! transport).

use std::io::Cursor;
use std::time::Duration;

use glint_ipc::codec::{
    FrameError, MAX_REQUEST_BYTES, decode_client_frame, encode_frame, read_frame, valid_request_id,
};
use glint_ipc::{
    ClientFrame, CoreSnapshot, Delta, ErrorCategory, ErrorCode, GlintRequest, Hub, Next,
    StreamEvent, SubscriptionState, Update, VersionRange, WarningInfo,
};

fn framed(body: &[u8]) -> Vec<u8> {
    let mut frame = (body.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(body);
    frame
}

#[test]
fn frames_round_trip() {
    let frame = ClientFrame::Request(glint_ipc::RequestEnvelope::new("r1", 1, GlintRequest::Ping));
    let bytes = encode_frame(&frame, MAX_REQUEST_BYTES).unwrap();
    let mut cursor = Cursor::new(bytes);
    let body = read_frame(&mut cursor, MAX_REQUEST_BYTES).unwrap().unwrap();
    assert_eq!(decode_client_frame(&body).unwrap(), frame);
    // A clean end of stream at a frame boundary.
    assert!(
        read_frame(&mut cursor, MAX_REQUEST_BYTES)
            .unwrap()
            .is_none()
    );
}

#[test]
fn an_oversized_frame_is_refused_before_its_body_is_read() {
    let mut header = (MAX_REQUEST_BYTES + 1).to_be_bytes().to_vec();
    header.extend_from_slice(b"{}");
    let mut cursor = Cursor::new(header);
    let error = read_frame(&mut cursor, MAX_REQUEST_BYTES).unwrap_err();
    assert!(
        matches!(error, FrameError::TooLarge { declared, .. } if declared == MAX_REQUEST_BYTES + 1)
    );
    assert_eq!(cursor.position(), 4, "only the length was read");
    assert_eq!(error.to_ipc().code(), ErrorCode::FrameTooLarge);
}

#[test]
fn an_oversized_message_is_never_encoded() {
    let big = "x".repeat(100);
    let error = encode_frame(&big, 16).unwrap_err();
    assert_eq!(error.code(), ErrorCode::Internal);
}

#[test]
fn empty_and_truncated_frames_are_typed() {
    let error = read_frame(&mut Cursor::new(vec![0, 0, 0, 0]), 1024).unwrap_err();
    assert!(matches!(error, FrameError::Empty));
    assert_eq!(error.to_ipc().code(), ErrorCode::MalformedFrame);

    let error = read_frame(&mut Cursor::new(vec![0, 0]), 1024).unwrap_err();
    assert!(matches!(error, FrameError::Truncated));

    let mut short = 10_u32.to_be_bytes().to_vec();
    short.extend_from_slice(b"{}");
    let error = read_frame(&mut Cursor::new(short), 1024).unwrap_err();
    assert!(matches!(error, FrameError::Truncated));
    assert_eq!(error.to_ipc().code(), ErrorCode::Unavailable);
}

#[test]
fn malformed_frames_are_typed_and_keep_the_request_id_when_readable() {
    for body in [
        &b"not json"[..],
        b"[1,2]",
        b"{}",
        b"{\"kind\":7}",
        b"{\"kind\":\"launch\"}",
        b"{\"kind\":\"hello\"}",
        b"{\"kind\":\"request\",\"api_version\":1,\"request\":{\"type\":\"ping\"}}",
        b"{\"kind\":\"request\",\"request_id\":\"\",\"api_version\":1,\"request\":{\"type\":\"ping\"}}",
    ] {
        let error = decode_client_frame(body).unwrap_err();
        assert_eq!(
            error.error.code(),
            ErrorCode::MalformedFrame,
            "{}",
            String::from_utf8_lossy(body)
        );
        assert_eq!(error.request_id, None);
    }
    for body in [
        &b"{\"kind\":\"request\",\"request_id\":\"r9\",\"request\":{\"type\":\"ping\"}}"[..],
        b"{\"kind\":\"request\",\"request_id\":\"r9\",\"api_version\":1}",
        b"{\"kind\":\"request\",\"request_id\":\"r9\",\"api_version\":1,\"request\":{}}",
        b"{\"kind\":\"request\",\"request_id\":\"r9\",\"api_version\":99999999999,\"request\":{\"type\":\"ping\"}}",
    ] {
        let error = decode_client_frame(body).unwrap_err();
        assert_eq!(error.error.code(), ErrorCode::MalformedFrame);
        assert_eq!(error.request_id.as_deref(), Some("r9"));
    }
}

#[test]
fn unknown_request_types_are_refused_not_executed() {
    for kind in ["run-command", "exec", "shell", "GetStatus", "get_status"] {
        let body = format!(
            "{{\"kind\":\"request\",\"request_id\":\"r1\",\"api_version\":1,\"request\":{{\"type\":\"{kind}\",\"payload\":{{\"cmd\":\"calc.exe\"}}}}}}"
        );
        let error = decode_client_frame(body.as_bytes()).unwrap_err();
        assert_eq!(error.error.code(), ErrorCode::UnknownRequest, "{kind}");
        assert_eq!(error.error.category(), ErrorCategory::Unsupported);
        assert_eq!(error.request_id.as_deref(), Some("r1"));
    }
}

#[test]
fn request_ids_are_bounded_printable_ascii() {
    assert!(valid_request_id("r1"));
    assert!(valid_request_id(&"a".repeat(64)));
    assert!(!valid_request_id(""));
    assert!(!valid_request_id(&"a".repeat(65)));
    assert!(!valid_request_id("r\n1"));
    assert!(!valid_request_id("r\u{e9}"));
}

#[test]
fn version_negotiation_picks_the_newest_common_version() {
    assert_eq!(
        VersionRange::new(1, 1).negotiate(VersionRange::new(1, 1)),
        Some(1)
    );
    assert_eq!(
        VersionRange::new(1, 3).negotiate(VersionRange::new(2, 5)),
        Some(3)
    );
    assert_eq!(
        VersionRange::new(1, 1).negotiate(VersionRange::new(2, 3)),
        None
    );
    assert_eq!(
        VersionRange::new(4, 5).negotiate(VersionRange::new(1, 3)),
        None
    );
}

#[test]
fn categories_keep_unsupported_denied_unavailable_timeout_and_failure_apart() {
    assert_eq!(
        ErrorCode::UnknownRequest.category(),
        ErrorCategory::Unsupported
    );
    assert_eq!(
        ErrorCode::PermissionDenied.category(),
        ErrorCategory::PermissionDenied
    );
    assert_eq!(
        ErrorCode::Unavailable.category(),
        ErrorCategory::Unavailable
    );
    assert_eq!(ErrorCode::Timeout.category(), ErrorCategory::Timeout);
    assert_eq!(ErrorCode::Internal.category(), ErrorCategory::Failure);
}

fn lifecycle(state: &str) -> Delta {
    Delta::Lifecycle {
        state: state.into(),
    }
}

#[test]
fn a_subscriber_sees_its_snapshot_then_every_later_delta_in_order() {
    let hub = Hub::new(CoreSnapshot::new(1));
    hub.publish(lifecycle("ready"));
    let (subscriber, sequence, snapshot) = hub.subscribe(8).unwrap();
    assert_eq!(sequence, 1);
    assert_eq!(snapshot.lifecycle, "ready");
    hub.publish(lifecycle("stopping"));
    hub.publish(lifecycle("stopped"));
    let mut state = SubscriptionState::new(sequence, snapshot);
    for expected in [2, 3] {
        let Next::Delta(seq, delta) = subscriber.next(Duration::from_secs(1)) else {
            panic!("expected a delta");
        };
        assert_eq!(seq, expected);
        state
            .apply(StreamEvent::Delta {
                sequence: seq,
                delta,
            })
            .unwrap();
    }
    assert_eq!(state.snapshot(), &hub.snapshot().1);
    assert_eq!(subscriber.next(Duration::from_millis(10)), Next::Idle);
    hub.close("stopped");
    assert_eq!(
        subscriber.next(Duration::from_millis(10)),
        Next::Closed("stopped".into())
    );
    assert!(hub.subscribe(8).is_none());
}

#[test]
fn a_lagging_subscriber_is_resynced_instead_of_buffering_without_bound() {
    let hub = Hub::new(CoreSnapshot::new(1));
    let (subscriber, _, _) = hub.subscribe(2).unwrap();
    for i in 0..10 {
        hub.publish(Delta::Warning {
            warning: WarningInfo::new("open-state", "GLINT_CORE_WARN_TEST", format!("{i}")),
        });
    }
    assert_eq!(subscriber.next(Duration::from_millis(10)), Next::Lagged);
    let (sequence, snapshot) = hub.resync(&subscriber);
    assert_eq!(sequence, 10);
    assert_eq!(snapshot.warnings.len(), 10);
    hub.publish(lifecycle("ready"));
    assert_eq!(
        subscriber.next(Duration::from_millis(10)),
        Next::Delta(11, lifecycle("ready"))
    );
}

#[test]
fn dropped_subscribers_are_detached() {
    let hub = Hub::new(CoreSnapshot::new(1));
    let (subscriber, _, _) = hub.subscribe(2).unwrap();
    assert_eq!(hub.subscriber_count(), 1);
    drop(subscriber);
    hub.publish(lifecycle("ready"));
    assert_eq!(hub.subscriber_count(), 0);
}

#[test]
fn a_gap_is_detected_and_leaves_the_local_snapshot_untouched() {
    let mut state = SubscriptionState::new(5, CoreSnapshot::new(1));
    let error = state
        .apply(StreamEvent::Delta {
            sequence: 7,
            delta: lifecycle("ready"),
        })
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::SequenceGap);
    assert_eq!(state.sequence(), 5);
    assert_eq!(state.snapshot().lifecycle, "created");

    // A repeated delta is a gap too.
    let error = state
        .apply(StreamEvent::Delta {
            sequence: 5,
            delta: lifecycle("ready"),
        })
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::SequenceGap);

    // A heartbeat that is ahead reveals missed deltas.
    let error = state
        .apply(StreamEvent::Heartbeat { sequence: 6 })
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::SequenceGap);

    // A resync snapshot replaces the state.
    let mut fresh = CoreSnapshot::new(1);
    fresh.apply(&lifecycle("ready"));
    let update = state
        .apply(StreamEvent::Snapshot {
            sequence: 9,
            reason: "lagged".into(),
            snapshot: Box::new(fresh.clone()),
        })
        .unwrap();
    assert_eq!(update, Update::Resynced { sequence: 9 });
    assert_eq!(state.snapshot(), &fresh);
    assert_eq!(
        state.apply(StreamEvent::Heartbeat { sequence: 9 }).unwrap(),
        Update::Heartbeat { sequence: 9 }
    );
}

#[test]
fn snapshots_keep_a_bounded_number_of_warnings() {
    let mut snapshot = CoreSnapshot::new(1);
    for i in 0..(glint_ipc::MAX_SNAPSHOT_WARNINGS + 5) {
        snapshot.apply(&Delta::Warning {
            warning: WarningInfo::new("s", "c", format!("{i}")),
        });
    }
    assert_eq!(snapshot.warnings.len(), glint_ipc::MAX_SNAPSHOT_WARNINGS);
    assert_eq!(snapshot.warnings[0].message, "5");
}

#[test]
fn framed_helper_matches_the_encoder() {
    let bytes = encode_frame(&ClientFrame::Goodbye, MAX_REQUEST_BYTES).unwrap();
    assert_eq!(bytes, framed(br#"{"kind":"goodbye"}"#));
}
