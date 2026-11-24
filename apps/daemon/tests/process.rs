//! The real `glint-daemon` binary, spawned with a temporary `GLINT_HOME`
//! (COR-001, COR-002).

mod support;

#[test]
fn a_second_launch_names_the_running_instance_and_leaves_it_alone() {
    let home = glint_home();
    let mut first = Daemon::spawn(home.path());
    first.wait_ready();

    let second = run_once(home.path(), &["--once"]);
    let stdout = String::from_utf8_lossy(&second.stdout);
    assert_eq!(second.status.code(), Some(9), "{stdout}");
    assert!(
        stdout.contains(&format!(
            "glint-daemon: already-running pid={}",
            first.pid()
        )),
        "{stdout}"
    );
    // It stopped before opening the database.
    assert!(!stdout.contains("starting:open-state"), "{stdout}");

    // The first instance still runs and shuts down cleanly.
    first.stdin().write_all(b"shutdown\n").unwrap();
    let (code, out) = first.finish();
    assert_eq!(code, Some(0), "{out}");
    assert!(out.contains("shutdown-step release-instance: instance lock released"));

    // Shutdown released the lock: the next launch boots, and the database
    // the first instance used is healthy (no recovery needed).
    let third = run_once(home.path(), &["--once"]);
    let stdout = String::from_utf8_lossy(&third.stdout);
    assert_eq!(third.status.code(), Some(0), "{stdout}");
    assert!(
        !stdout.contains("GLINT_CORE_WARN_DATABASE_RECOVERED"),
        "{stdout}"
    );
}

#[test]
fn a_killed_instance_leaves_no_stale_lock_and_a_healthy_database() {
    let home = glint_home();
    let mut first = Daemon::spawn(home.path());
    first.wait_ready();
    first.kill();
    // No clean shutdown ran: the owner record is stale, but the OS released
    // both locks.
    assert!(home.path().join("data").join("instance.owner").is_file());

    let next = run_once(home.path(), &["--once"]);
    let stdout = String::from_utf8_lossy(&next.stdout);
    assert_eq!(next.status.code(), Some(0), "{stdout}");
    assert!(
        !stdout.contains("GLINT_CORE_WARN_DATABASE_RECOVERED"),
        "{stdout}"
    );
    assert!(!home.path().join("data").join("instance.owner").exists());
}

use std::io::Write;

use support::{Daemon, glint_home, run_once};

#[test]
fn once_boots_headless_and_shuts_down_cleanly() {
    let home = glint_home();
    let output = run_once(home.path(), &["--once"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{stdout}");
    assert!(stdout.contains("glint-daemon: state starting:start-shell -> ready"));
    assert!(stdout.contains("GLINT_CORE_WARN_THEME_FALLBACK"));
    assert!(stdout.contains("glint-daemon: shutdown requested"));
    assert!(stdout.ends_with("glint-daemon: state stopping -> stopped\n"));
    assert!(home.path().join("data").join("glint.sqlite3").is_file());
}

#[test]
fn a_shutdown_command_on_stdin_stops_the_daemon() {
    let home = glint_home();
    let mut daemon = Daemon::spawn(home.path());
    daemon.wait_ready();
    daemon.stdin().write_all(b"shutdown\n").unwrap();
    let (code, stdout) = daemon.finish();
    assert_eq!(code, Some(0), "{stdout}");
    assert!(stdout.contains("glint-daemon: shutdown requested"));
}

#[test]
fn closing_stdin_stops_the_daemon() {
    let home = glint_home();
    let mut daemon = Daemon::spawn(home.path());
    daemon.wait_ready();
    daemon.close_stdin();
    let (code, stdout) = daemon.finish();
    assert_eq!(code, Some(0), "{stdout}");
    assert!(stdout.contains("glint-daemon: shutdown control-channel-closed"));
}

#[test]
fn unknown_arguments_are_a_usage_error() {
    let home = glint_home();
    let output = run_once(home.path(), &["--bogus"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn a_boot_failure_is_reported_with_its_stage_and_code() {
    let home = glint_home();
    // A file where the data directory must be.
    std::fs::write(home.path().join("data"), b"not a directory").unwrap();
    let output = run_once(home.path(), &["--once"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains("error GLINT_CORE_PATHS_UNAVAILABLE at resolve-paths"));
    assert!(stdout.contains("state starting:resolve-paths -> failed:resolve-paths"));
}
