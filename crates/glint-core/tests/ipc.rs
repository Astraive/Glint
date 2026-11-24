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
