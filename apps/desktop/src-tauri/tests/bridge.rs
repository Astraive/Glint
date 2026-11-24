//! The manager's bridge against the real IPC transport (named pipe on
//! Windows, Unix socket elsewhere) with an in-process server under a
//! temporary data root. Runs on every OS: the bridge has no Tauri types.

use std::sync::Arc;
use std::time::Duration;

use glint_desktop::bridge::{Bridge, CLIENT_NAME, DaemonStatus, PingResult};
use glint_ipc::{
    CapabilitySummary, ConfigSummary, Control, CoreSnapshot, Delta, Hub, IpcError, ServerOptions,
    StateSummary, ThemeSummary, WarningInfo, serve,
};
use glint_storage::GlintPaths;

struct NoShutdown;

impl Control for NoShutdown {
    fn shutdown(&self) -> Result<bool, IpcError> {
        panic!("the manager never sends shutdown");
    }
}

fn root() -> (tempfile::TempDir, GlintPaths) {
    let tmp = tempfile::tempdir().unwrap();
    let paths = GlintPaths::under_root(tmp.path()).unwrap();
    paths.ensure_dirs().unwrap();
    (tmp, paths)
}

#[test]
fn a_running_core_is_read_over_ipc() {
    let (_tmp, paths) = root();
    let hub = Arc::new(Hub::new(CoreSnapshot::new(4242)));
    hub.publish(Delta::Lifecycle {
        state: "ready".into(),
    });
    hub.publish(Delta::Booted {
        state: StateSummary::new(4, 0, false),
        config: ConfigSummary::new("absent", 0, true),
        capabilities: CapabilitySummary::new("headless", 1, 0, 0, None),
        theme: Box::new(ThemeSummary::bundled(
            "official.default",
            "0.1.0",
            "no-active-theme",
        )),
    });
    hub.publish(Delta::Warning {
        warning: WarningInfo::new("resolve-config", "GLINT_CORE_WARN_TEST", "synthetic"),
    });
    let server = serve(
        &paths,
        Arc::clone(&hub),
        Arc::new(NoShutdown),
        ServerOptions::named("glint-test/0"),
    )
    .unwrap();

    let bridge = Bridge::new(paths.clone());
    match bridge.status() {
        DaemonStatus::Connected {
            api_version,
            server: name,
            status,
            sequence,
            snapshot,
        } => {
            assert_eq!(api_version, 1);
            assert_eq!(name, "glint-test/0");
            assert_eq!(status.lifecycle, "ready");
            assert_eq!(status.pid, 4242);
            assert_eq!(sequence, 3);
            assert_eq!(snapshot.theme.unwrap().theme_id, "official.default");
            assert_eq!(snapshot.warnings.len(), 1);
        }
        other => panic!("expected connected, got {other:?}"),
    }
    assert!(matches!(bridge.ping(), PingResult::Connected { .. }));

    // The JSON the webview receives: tagged, wire field names.
    let json = serde_json::to_value(bridge.status()).unwrap();
    assert_eq!(json["state"], "connected");
    assert_eq!(json["snapshot"]["lifecycle"], "ready");
    assert!(CLIENT_NAME.starts_with("glint-desktop/"));

    server.stop();
    // Once the core stops, the manager says so instead of showing old state.
    match bridge.status() {
        DaemonStatus::Unreachable { problem } => {
            assert_eq!(problem.category, "unavailable", "{problem:?}");
            assert!(problem.code.starts_with("GLINT_IPC_"), "{problem:?}");
        }
        other => panic!("expected unreachable, got {other:?}"),
    }
}

#[test]
fn no_core_is_reported_as_unavailable_not_as_state() {
    let (_tmp, paths) = root();
    let bridge = Bridge::new(paths).with_timeout(Duration::from_millis(500));
    let status = bridge.status();
    let json = serde_json::to_value(&status).unwrap();
    assert_eq!(json["state"], "unreachable");
    assert_eq!(json["problem"]["code"], "GLINT_IPC_UNAVAILABLE");
    assert_eq!(json["problem"]["category"], "unavailable");
    assert!(json.get("snapshot").is_none());

    match bridge.ping() {
        PingResult::Unreachable { problem } => assert_eq!(problem.category, "unavailable"),
        other => panic!("expected unreachable, got {other:?}"),
    }
}
