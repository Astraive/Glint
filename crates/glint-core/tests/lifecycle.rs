//! Core lifecycle (COR-001): the runtime starts, loads storage, drives the
//! daemon and shell through their service traits and shuts down cleanly.
//! Failure modes use test doubles; the happy path uses the real daemon over
//! a temporary state directory and the headless shell.

use std::fs;
use std::sync::{Arc, Mutex};

use glint_core::{
    BootStage, ErrorCode, GlintPaths, HeadlessBackend, LifecycleEvent, LifecycleState, Runtime,
    RuntimeOptions, Shell, ShutdownReason, ShutdownStage, headless_runtime, warnings,
};
use glint_daemon::{
    CapabilityReport, ConfigReport, Daemon, DaemonError, DaemonService, DaemonStatus, FlushReport,
    StateReport, ThemeSelection,
};
use glint_installer::Installer;
use glint_platform::mock::MockAdapter;
use glint_platform::{Availability, Capability, CapabilityId, IntegrationEffect};
use glint_shell::{BackendError, BackendErrorKind};

fn home() -> (tempfile::TempDir, GlintPaths) {
    let tmp = tempfile::tempdir().unwrap();
    let paths = GlintPaths::under_root(tmp.path().join("glint")).unwrap();
    (tmp, paths)
}

fn panel_adapter() -> MockAdapter {
    MockAdapter::new("test").with(Capability::new(
        CapabilityId::parse("surface.panel").unwrap(),
        Availability::Supported,
        IntegrationEffect::GlintOwnedUi,
    ))
}

fn states(events: &[LifecycleEvent]) -> Vec<LifecycleState> {
    events
        .iter()
        .filter_map(|event| match event {
            LifecycleEvent::StateChanged { to, .. } => Some(*to),
            _ => None,
        })
        .collect()
}

fn warning_codes(events: &[LifecycleEvent]) -> Vec<&'static str> {
    events
        .iter()
        .filter_map(|event| match event {
            LifecycleEvent::Warning { code, .. } => Some(*code),
            _ => None,
        })
        .collect()
}

#[test]
fn core_starts_loads_storage_and_shuts_down_cleanly() {
    let (_tmp, paths) = home();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&observed);
    let mut runtime = headless_runtime(RuntimeOptions::with_paths(paths.clone()))
        .with_observer(move |event| sink.lock().unwrap().push(event.to_string()));
    assert_eq!(runtime.state(), LifecycleState::Created);

    let ready = runtime.start().unwrap();
    assert_eq!(ready.paths, paths);
    assert!(paths.database_file().is_file(), "storage was opened");
    assert_eq!(ready.state.schema_version, glint_storage_latest());
    assert_eq!(ready.theme.theme_id, "official.default");
    assert!(ready.theme.is_fallback());
    assert_eq!(ready.shell.backend, "headless");
    assert_eq!(runtime.state(), LifecycleState::Ready);
    assert_eq!(runtime.daemon().status(), DaemonStatus::Serving);

    // Every boot stage ran, in order.
    let mut expected: Vec<LifecycleState> = BootStage::ALL
        .iter()
        .map(|stage| LifecycleState::Starting(*stage))
        .collect();
    expected.push(LifecycleState::Ready);
    assert_eq!(states(runtime.events()), expected);
    // Degraded outcomes are reported, never hidden.
    let codes = warning_codes(runtime.events());
    assert!(codes.contains(&warnings::THEME_FALLBACK));
    assert!(codes.contains(&warnings::SURFACE_SKIPPED));

    let report = runtime.shutdown(ShutdownReason::Requested).unwrap();
    assert!(report.is_clean(), "{:?}", report.errors);
    assert!(report.flush.state_closed);
    assert!(report.instance_released);
    assert_eq!(runtime.state(), LifecycleState::Stopped);
    assert!(runtime.ready_report().is_none());
    // The installer lock and the database were released.
    drop(Installer::open(&paths).unwrap());

    let lines = observed.lock().unwrap();
    assert!(
        lines
            .iter()
            .any(|line| line == "state starting:start-shell -> ready")
    );
    assert!(lines.iter().any(|line| line == "shutdown requested"));
    assert_eq!(lines.last().unwrap(), "state stopping -> stopped");
}

fn glint_storage_latest() -> u32 {
    // The runtime reports the schema the daemon migrated to.
    let (_tmp, paths) = home();
    let mut daemon = Daemon::headless();
    let version = daemon.open_state(&paths).unwrap().schema_version;
    daemon.shutdown().unwrap();
    version
}

#[test]
fn surfaces_are_presented_and_released_with_their_reservations() {
    let (_tmp, paths) = home();
    let backend = HeadlessBackend::new();
    let mut runtime = Runtime::new(
        Daemon::new(panel_adapter()),
        Shell::new(backend.clone()),
        RuntimeOptions::with_paths(paths),
    );
    let ready = runtime.start().unwrap();
    assert_eq!(ready.shell.opened, vec!["panel"]);
    assert_eq!(backend.open_surfaces(), vec!["panel"]);
    assert_eq!(backend.reservations(), 1);

    let report = runtime.shutdown(ShutdownReason::SessionEnding).unwrap();
    assert_eq!(report.shell.closed, vec!["panel"]);
    assert_eq!(report.shell.reservations_released, 1);
    assert!(backend.open_surfaces().is_empty());
    assert_eq!(backend.reservations(), 0);

    let steps: Vec<ShutdownStage> = runtime
        .events()
        .iter()
        .filter_map(|event| match event {
            LifecycleEvent::ShutdownStepCompleted { stage, .. } => Some(*stage),
            _ => None,
        })
        .collect();
    assert_eq!(
        steps,
        vec![
            ShutdownStage::CloseShell,
            ShutdownStage::FlushState,
            ShutdownStage::ReleaseInstance
        ]
    );
}

#[test]
fn dropping_a_running_runtime_shuts_it_down() {
    let (_tmp, paths) = home();
    let backend = HeadlessBackend::new();
    let mut runtime = Runtime::new(
        Daemon::new(panel_adapter()),
        Shell::new(backend.clone()),
        RuntimeOptions::with_paths(paths.clone()),
    );
    runtime.start().unwrap();
    drop(runtime);
    assert!(backend.open_surfaces().is_empty());
    drop(Installer::open(&paths).unwrap());
}

#[test]
fn unusable_paths_fail_at_the_first_stage_without_opening_state() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("glint");
    fs::create_dir_all(&root).unwrap();
    // A file where the data directory must be.
    fs::write(root.join("data"), b"not a directory").unwrap();
    let paths = GlintPaths::under_root(&root).unwrap();
    let mut runtime = headless_runtime(RuntimeOptions::with_paths(paths));
    let error = runtime.start().unwrap_err();
    assert_eq!(error.code(), ErrorCode::PathsUnavailable);
    assert_eq!(error.stage(), Some(BootStage::ResolvePaths));
    assert!(error.source_code().unwrap().starts_with("GLINT_STORAGE_"));
    assert_eq!(
        runtime.state(),
        LifecycleState::Failed(BootStage::ResolvePaths)
    );
    assert_eq!(runtime.daemon().status(), DaemonStatus::Stopped);
}

#[test]
fn a_busy_installer_lock_is_reported_as_unavailable_state() {
    let (_tmp, paths) = home();
    paths.ensure_dirs().unwrap();
    let holder = Installer::open(&paths).unwrap();
    let mut runtime = headless_runtime(RuntimeOptions::with_paths(paths));
    let error = runtime.start().unwrap_err();
    assert_eq!(error.code(), ErrorCode::DaemonFailed);
    assert_eq!(error.stage(), Some(BootStage::OpenState));
    assert_eq!(error.source_code(), Some("GLINT_INSTALL_BUSY"));
    assert_eq!(error.category(), glint_storage::ErrorCategory::Unavailable);
    drop(holder);
}

#[test]
fn start_and_shutdown_refuse_the_wrong_state() {
    let (_tmp, paths) = home();
    let mut runtime = headless_runtime(RuntimeOptions::with_paths(paths));
    assert_eq!(
        runtime
            .shutdown(ShutdownReason::Requested)
            .unwrap_err()
            .code(),
        ErrorCode::InvalidState
    );
    runtime.start().unwrap();
    assert_eq!(runtime.start().unwrap_err().code(), ErrorCode::InvalidState);
    runtime.shutdown(ShutdownReason::Requested).unwrap();
    assert_eq!(
        runtime
            .shutdown(ShutdownReason::Requested)
            .unwrap_err()
            .code(),
        ErrorCode::InvalidState
    );
}

/// A daemon double that fails at a chosen stage and records shutdown.
struct FailingDaemon {
    inner: Daemon,
    fail_select: bool,
    fail_flush: bool,
    shutdowns: Arc<Mutex<usize>>,
}

impl DaemonService for FailingDaemon {
    fn open_state(&mut self, paths: &GlintPaths) -> Result<StateReport, DaemonError> {
        self.inner.open_state(paths)
    }
    fn resolve_config(&mut self) -> Result<ConfigReport, DaemonError> {
        self.inner.resolve_config()
    }
    fn discover_capabilities(&mut self) -> Result<CapabilityReport, DaemonError> {
        self.inner.discover_capabilities()
    }
    fn select_theme(&mut self) -> Result<ThemeSelection, DaemonError> {
        if self.fail_select {
            return Err(DaemonError::DefaultThemeInvalid("injected".into()));
        }
        self.inner.select_theme()
    }
    fn status(&self) -> DaemonStatus {
        self.inner.status()
    }
    fn shutdown(&mut self) -> Result<FlushReport, DaemonError> {
        *self.shutdowns.lock().unwrap() += 1;
        let report = self.inner.shutdown()?;
        if self.fail_flush {
            return Err(DaemonError::InvalidState {
                step: "shutdown",
                requires: "injected flush failure",
            });
        }
        Ok(report)
    }
}

#[test]
fn a_failed_stage_releases_what_had_started() {
    let (_tmp, paths) = home();
    let shutdowns = Arc::new(Mutex::new(0));
    let backend = HeadlessBackend::new();
    let mut runtime = Runtime::new(
        FailingDaemon {
            inner: Daemon::headless(),
            fail_select: true,
            fail_flush: false,
            shutdowns: Arc::clone(&shutdowns),
        },
        Shell::new(backend.clone()),
        RuntimeOptions::with_paths(paths.clone()),
    );
    let error = runtime.start().unwrap_err();
    assert_eq!(error.stage(), Some(BootStage::SelectTheme));
    assert_eq!(
        error.source_code(),
        Some("GLINT_DAEMON_DEFAULT_THEME_INVALID")
    );
    assert_eq!(
        runtime.state(),
        LifecycleState::Failed(BootStage::SelectTheme)
    );
    assert_eq!(*shutdowns.lock().unwrap(), 1, "state was closed");
    assert_eq!(backend.opened_total(), 0, "the shell never presented");
    // Nothing is left holding the state.
    drop(Installer::open(&paths).unwrap());
}

#[test]
fn shutdown_runs_every_step_and_reports_each_failure() {
    let (_tmp, paths) = home();
    let shutdowns = Arc::new(Mutex::new(0));
    let backend = HeadlessBackend::new();
    let mut runtime = Runtime::new(
        FailingDaemon {
            inner: Daemon::new(panel_adapter()),
            fail_select: false,
            fail_flush: true,
            shutdowns: Arc::clone(&shutdowns),
        },
        Shell::new(backend.clone()),
        RuntimeOptions::with_paths(paths.clone()),
    );
    runtime.start().unwrap();
    backend.fail_close(
        "panel",
        BackendError::new(BackendErrorKind::Timeout, "host hung"),
    );
    let report = runtime.shutdown(ShutdownReason::Requested).unwrap();
    assert!(!report.is_clean());
    let codes: Vec<ErrorCode> = report.errors.iter().map(|e| e.code()).collect();
    assert_eq!(codes, vec![ErrorCode::ShellFailed, ErrorCode::DaemonFailed]);
    assert_eq!(report.shell.failed[0].id, "panel");
    // The flush step still ran after the shell failure.
    assert_eq!(*shutdowns.lock().unwrap(), 1);
    assert_eq!(runtime.state(), LifecycleState::Stopped);
    drop(Installer::open(&paths).unwrap());
}

#[test]
fn daemon_and_shell_crates_have_no_dependency_on_each_other() {
    fn dependencies(manifest: &str) -> &str {
        manifest
            .split("[dependencies]")
            .nth(1)
            .unwrap()
            .split("[dev-dependencies]")
            .next()
            .unwrap()
    }
    let daemon = dependencies(include_str!("../../glint-daemon/Cargo.toml"));
    let shell = dependencies(include_str!("../../glint-shell/Cargo.toml"));
    assert!(!daemon.contains("glint-shell") && !daemon.contains("glint-core"));
    assert!(!shell.contains("glint-daemon") && !shell.contains("glint-core"));
}
