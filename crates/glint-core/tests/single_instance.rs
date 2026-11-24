//! Per-user single instance and clean shutdown (COR-002).
//!
//! Cross-process cases re-run this test binary as a child process
//! (`child_holds_the_instance_lock`, active only when `GLINT_TEST_CHILD_HOME`
//! is set) that takes the lock and waits on stdin, so the parent can watch a
//! real second process hold, release or crash with the lock.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use glint_core::{
    BootStage, ErrorCode, GlintPaths, InstanceError, InstanceLock, InstanceProbe, LifecycleState,
    Runtime, RuntimeOptions, Shell, ShutdownReason, ShutdownSignal, headless_runtime,
    probe_instance,
};
use glint_daemon::{
    CapabilityReport, ConfigReport, Daemon, DaemonError, DaemonService, DaemonStatus, FlushReport,
    StateReport, ThemeSelection,
};
use glint_shell::HeadlessBackend;

const CHILD_ENV: &str = "GLINT_TEST_CHILD_HOME";
const LOCKED: &str = "CHILD-LOCKED";

fn home() -> (tempfile::TempDir, GlintPaths) {
    let tmp = tempfile::tempdir().unwrap();
    let paths = GlintPaths::under_root(tmp.path().join("glint")).unwrap();
    paths.ensure_dirs().unwrap();
    (tmp, paths)
}

/// Counts `open_state` calls, to prove a refused second instance never
/// opened the database.
struct SpyDaemon {
    inner: Daemon,
    opened: Arc<Mutex<usize>>,
}

impl DaemonService for SpyDaemon {
    fn open_state(&mut self, paths: &GlintPaths) -> Result<StateReport, DaemonError> {
        *self.opened.lock().unwrap() += 1;
        self.inner.open_state(paths)
    }
    fn resolve_config(&mut self) -> Result<ConfigReport, DaemonError> {
        self.inner.resolve_config()
    }
    fn discover_capabilities(&mut self) -> Result<CapabilityReport, DaemonError> {
        self.inner.discover_capabilities()
    }
    fn select_theme(&mut self) -> Result<ThemeSelection, DaemonError> {
        self.inner.select_theme()
    }
    fn status(&self) -> DaemonStatus {
        self.inner.status()
    }
    fn shutdown(&mut self) -> Result<FlushReport, DaemonError> {
        self.inner.shutdown()
    }
}

#[test]
fn a_second_lock_in_the_same_process_is_refused_and_names_the_owner() {
    let (_tmp, paths) = home();
    let first = InstanceLock::acquire(&paths).unwrap();
    assert_eq!(first.owner().pid, std::process::id());
    assert!(paths.instance_owner_file().is_file());
    match InstanceLock::acquire(&paths).unwrap_err() {
        InstanceError::AlreadyRunning { owner, .. } => {
            assert_eq!(owner.unwrap().pid, std::process::id());
        }
        other => panic!("expected AlreadyRunning, got {other:?}"),
    }
    assert!(matches!(
        probe_instance(&paths).unwrap(),
        InstanceProbe::Running(Some(_))
    ));

    first.release().unwrap();
    assert!(!paths.instance_owner_file().exists());
    assert_eq!(probe_instance(&paths).unwrap(), InstanceProbe::Free);
    drop(InstanceLock::acquire(&paths).unwrap());
}

#[test]
fn locks_under_different_roots_do_not_interfere() {
    let (_a, first) = home();
    let (_b, second) = home();
    let _held = InstanceLock::acquire(&first).unwrap();
    drop(InstanceLock::acquire(&second).unwrap());
}

#[test]
fn a_second_runtime_stops_before_opening_the_database() {
    let (_tmp, paths) = home();
    let mut first = headless_runtime(RuntimeOptions::with_paths(paths.clone()));
    first.start().unwrap();

    let opened = Arc::new(Mutex::new(0));
    let mut second = Runtime::new(
        SpyDaemon {
            inner: Daemon::headless(),
            opened: Arc::clone(&opened),
        },
        Shell::new(HeadlessBackend::new()),
        RuntimeOptions::with_paths(paths.clone()),
    );
    let error = second.start().unwrap_err();
    assert_eq!(error.code(), ErrorCode::AlreadyRunning);
    assert_eq!(error.code().as_str(), "GLINT_CORE_ALREADY_RUNNING");
    assert_eq!(error.stage(), Some(BootStage::AcquireInstance));
    assert_eq!(error.category(), glint_storage::ErrorCategory::Unavailable);
    assert_eq!(error.owner().unwrap().pid, std::process::id());
    assert_eq!(*opened.lock().unwrap(), 0, "the database was not opened");
    assert_eq!(
        second.state(),
        LifecycleState::Failed(BootStage::AcquireInstance)
    );

    // The first instance is unaffected and still owns the lock.
    assert_eq!(first.state(), LifecycleState::Ready);
    assert!(matches!(
        probe_instance(&paths).unwrap(),
        InstanceProbe::Running(Some(_))
    ));

    let report = first.shutdown(ShutdownReason::Requested).unwrap();
    assert!(report.is_clean());
    assert!(report.instance_released);
    assert_eq!(probe_instance(&paths).unwrap(), InstanceProbe::Free);

    // Shutdown released the lock: a new runtime boots.
    let mut third = headless_runtime(RuntimeOptions::with_paths(paths));
    third.start().unwrap();
    third.shutdown(ShutdownReason::Requested).unwrap();
}

#[test]
fn a_failed_boot_releases_the_instance_lock() {
    let (_tmp, paths) = home();
    let holder = glint_installer::Installer::open(&paths).unwrap();
    let mut runtime = headless_runtime(RuntimeOptions::with_paths(paths.clone()));
    let error = runtime.start().unwrap_err();
    assert_eq!(error.stage(), Some(BootStage::OpenState));
    assert_eq!(probe_instance(&paths).unwrap(), InstanceProbe::Free);
    drop(holder);
}

#[test]
fn run_until_shuts_down_when_any_trigger_fires() {
    let (_tmp, paths) = home();
    let mut runtime = headless_runtime(RuntimeOptions::with_paths(paths.clone()));
    runtime.start().unwrap();
    let signal = ShutdownSignal::new();
    let trigger = signal.clone();
    let waker = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        assert!(trigger.trigger(ShutdownReason::SessionEnding));
        assert!(!trigger.trigger(ShutdownReason::Requested), "first wins");
    });
    let report = runtime.run_until(&signal).unwrap();
    waker.join().unwrap();
    assert_eq!(report.reason, ShutdownReason::SessionEnding);
    assert!(report.is_clean());
    assert_eq!(runtime.state(), LifecycleState::Stopped);
    assert_eq!(probe_instance(&paths).unwrap(), InstanceProbe::Free);
}

#[test]
fn shutdown_signal_waits_with_a_timeout() {
    let signal = ShutdownSignal::new();
    assert_eq!(signal.wait_timeout(Duration::from_millis(10)), None);
    signal.trigger(ShutdownReason::ControlChannelClosed);
    assert_eq!(
        signal.wait_timeout(Duration::from_millis(10)),
        Some(ShutdownReason::ControlChannelClosed)
    );
    assert_eq!(signal.wait(), ShutdownReason::ControlChannelClosed);
}

// ------------------------------------------------------------ cross-process

/// Child mode: take the lock under `$GLINT_TEST_CHILD_HOME`, announce it,
/// hold it until stdin closes, then release it cleanly.
#[test]
fn child_holds_the_instance_lock() {
    let Some(root) = std::env::var_os(CHILD_ENV) else {
        return;
    };
    let paths = GlintPaths::under_root(root).unwrap();
    let lock = InstanceLock::acquire(&paths).unwrap();
    println!("{LOCKED} {}", lock.owner().pid);
    std::io::stdout().flush().unwrap();
    let mut sink = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut sink);
    lock.release().unwrap();
}

fn spawn_holder(root: &Path) -> Child {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "child_holds_the_instance_lock",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ENV, root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    loop {
        line.clear();
        assert!(
            reader.read_line(&mut line).unwrap() > 0,
            "child exited before taking the lock"
        );
        if line.contains(LOCKED) {
            break;
        }
    }
    // Keep draining so the child never writes into a closed pipe.
    std::thread::spawn(move || std::io::copy(&mut reader, &mut std::io::sink()));
    child
}

fn root_of(paths: &GlintPaths) -> &Path {
    paths.data_dir().parent().unwrap()
}

#[test]
fn another_process_holding_the_lock_blocks_this_one_until_it_releases() {
    let (_tmp, paths) = home();
    let mut child = spawn_holder(root_of(&paths));

    match InstanceLock::acquire(&paths).unwrap_err() {
        InstanceError::AlreadyRunning { owner, .. } => {
            assert_eq!(owner.unwrap().pid, child.id());
        }
        other => panic!("expected AlreadyRunning, got {other:?}"),
    }
    let mut runtime = headless_runtime(RuntimeOptions::with_paths(paths.clone()));
    let error = runtime.start().unwrap_err();
    assert_eq!(error.code(), ErrorCode::AlreadyRunning);
    assert_eq!(error.owner().unwrap().pid, child.id());
    assert!(
        !paths.database_file().exists(),
        "the refused instance never created the database"
    );

    // Clean release: closing the child's stdin makes it release the lock.
    drop(child.stdin.take());
    assert!(child.wait().unwrap().success());
    assert!(!paths.instance_owner_file().exists());
    drop(InstanceLock::acquire(&paths).unwrap());
}

#[test]
fn a_crashed_owner_never_leaves_a_stale_lock() {
    let (_tmp, paths) = home();
    let mut child = spawn_holder(root_of(&paths));
    let crashed = child.id();
    child.kill().unwrap();
    child.wait().unwrap();
    // The OS released the lock; the stale owner record is replaced.
    assert!(paths.instance_owner_file().exists());
    let lock = InstanceLock::acquire(&paths).unwrap();
    assert_ne!(lock.owner().pid, crashed);
    assert_eq!(lock.owner().pid, std::process::id());
}
