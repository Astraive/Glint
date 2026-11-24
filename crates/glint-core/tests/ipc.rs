//! Serving the runtime over local IPC in process (COR-003): the hub follows
//! the lifecycle, reads come from the published snapshot and an IPC
//! `shutdown` takes the runtime's clean shutdown path.

use std::thread;

use glint_core::{
    CoreIpc, ErrorCode, GlintPaths, RuntimeOptions, ShutdownReason, ShutdownSignal,
    headless_runtime, probe_ipc,
};
use glint_ipc::{Client, GlintRequest, ResponseBody};

#[test]
fn an_ipc_shutdown_request_runs_the_clean_shutdown() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = GlintPaths::under_root(tmp.path().join("glint")).unwrap();
    let ipc = CoreIpc::new();
    let observer = ipc.clone();
    let mut runtime = headless_runtime(RuntimeOptions::with_paths(paths.clone()))
        .with_observer(move |event| observer.observe(event));
    let ready = runtime.start().unwrap();
    ipc.publish_ready(ready);
    let signal = ShutdownSignal::new();
    let server = ipc.serve(&paths, &signal).unwrap();

    let status = probe_ipc(&paths).unwrap();
    assert_eq!(status.lifecycle, "ready");
    assert_eq!(status.daemon, "serving");
    let mut client = Client::connect(&paths).unwrap();
    let ResponseBody::State(state) = client.request(GlintRequest::GetState).unwrap() else {
        panic!("state");
    };
    assert_eq!(state.installed_packages, 0);
    assert!(!state.database_recovered);

    let requester = thread::spawn(move || client.shutdown().unwrap());
    let report = runtime.run_until(&signal).unwrap();
    assert!(requester.join().unwrap());
    assert_eq!(report.reason, ShutdownReason::Requested);
    assert!(report.is_clean());

    let (_, snapshot) = ipc.hub().snapshot();
    assert_eq!(snapshot.lifecycle, "stopped");
    assert_eq!(snapshot.daemon, "stopped");
    assert_eq!(snapshot.shutdown_reason.as_deref(), Some("requested"));

    ipc.close("stopped");
    server.stop();
    assert!(!paths.ipc_endpoint_file().exists());
}

#[test]
fn probing_without_a_running_core_is_unavailable() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = GlintPaths::under_root(tmp.path().join("glint")).unwrap();
    paths.ensure_dirs().unwrap();
    let error = probe_ipc(&paths).unwrap_err();
    assert_eq!(error.code(), glint_ipc::ErrorCode::Unavailable);
    assert_eq!(ErrorCode::IpcFailed.as_str(), "GLINT_CORE_IPC_FAILED");
}

/// Theme operations over IPC run on the runtime's thread through the
/// daemon (CLI-006): the reply is typed, the change is published before
/// the reply, so a second client sees it, and failures keep the
/// installer's code.
#[test]
fn operations_run_on_the_runtime_and_reach_a_second_client() {
    use glint_core::operation_channel;
    use glint_ipc::{Delta, ErrorCode as IpcCode, OperationErrorKind};

    let tmp = tempfile::tempdir().unwrap();
    let paths = GlintPaths::under_root(tmp.path().join("glint")).unwrap();
    let ipc = CoreIpc::new();
    let observer = ipc.clone();
    let mut runtime = headless_runtime(RuntimeOptions::with_paths(paths.clone()))
        .with_observer(move |event| observer.observe(event));
    let ready = runtime.start().unwrap();
    ipc.publish_ready(ready);
    let signal = ShutdownSignal::new();
    let (sender, inbox) = operation_channel();
    let server = ipc.serve_with_operations(&paths, &signal, sender).unwrap();

    let watcher = Client::connect(&paths).unwrap().subscribe().unwrap();
    let client_paths = paths.clone();
    let client = thread::spawn(move || {
        let mut client = Client::connect(&client_paths).unwrap();
        let ResponseBody::ThemeActivated(reply) = client.request(GlintRequest::SafeMode).unwrap()
        else {
            panic!("safe mode reply");
        };
        assert!(reply.safe_mode);
        assert_eq!(reply.active.profile_id, "safe-mode");
        // Visible to another client as soon as the reply arrived.
        let mut reader = Client::connect(&client_paths).unwrap();
        let ResponseBody::Theme(theme) = reader.request(GlintRequest::GetTheme).unwrap() else {
            panic!("theme");
        };
        assert_eq!(theme.origin, "installed");
        assert_eq!(theme.profile_id.as_deref(), Some("safe-mode"));
        let ResponseBody::State(state) = reader.request(GlintRequest::GetState).unwrap() else {
            panic!("state");
        };
        assert_eq!(state.installed_packages, 1);

        let error = client
            .request(GlintRequest::ThemeRemove {
                theme: "dev.example.missing".into(),
            })
            .unwrap_err();
        assert_eq!(error.code(), IpcCode::OperationFailed);
        let failure = error.operation().unwrap();
        assert_eq!(failure.code, "GLINT_INSTALL_NOT_INSTALLED");
        assert_eq!(failure.kind, OperationErrorKind::NotFound);

        let error = client
            .request(GlintRequest::ThemeImport {
                path: "relative.glint".into(),
            })
            .unwrap_err();
        assert_eq!(
            error.operation().unwrap().kind,
            OperationErrorKind::InvalidRequest
        );
        client.shutdown().unwrap();
    });
    let report = runtime.run_serving(&signal, &inbox).unwrap();
    client.join().unwrap();
    assert!(report.is_clean());

    // The subscriber saw the operation's deltas in order.
    let mut watcher = watcher;
    let mut saw_theme = false;
    while let Ok(update) = watcher.next(std::time::Duration::from_secs(5)) {
        match update {
            glint_ipc::Update::Delta {
                delta: Delta::Theme { theme },
                ..
            } => {
                assert_eq!(theme.profile_id.as_deref(), Some("safe-mode"));
                saw_theme = true;
            }
            glint_ipc::Update::Delta { .. } | glint_ipc::Update::Heartbeat { .. } => {}
            _ => break,
        }
        if saw_theme {
            break;
        }
    }
    assert!(saw_theme, "subscriber saw the theme delta");

    ipc.close("stopped");
    server.stop();
}

/// A core served without an operation channel refuses operations: nothing
/// but the daemon on the runtime thread may write.
#[test]
fn a_core_without_operations_refuses_them() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = GlintPaths::under_root(tmp.path().join("glint")).unwrap();
    let ipc = CoreIpc::new();
    let mut runtime = headless_runtime(RuntimeOptions::with_paths(paths.clone()));
    let ready = runtime.start().unwrap();
    ipc.publish_ready(ready);
    let signal = ShutdownSignal::new();
    let server = ipc.serve(&paths, &signal).unwrap();
    let mut client = Client::connect(&paths).unwrap();
    let error = client.request(GlintRequest::ThemeList).unwrap_err();
    assert_eq!(error.code(), glint_ipc::ErrorCode::OperationUnsupported);
    drop(client);
    runtime.shutdown(ShutdownReason::Requested).unwrap();
    ipc.close("stopped");
    server.stop();
}
