//! `glint-daemon`: the Glint core process (COR-001, COR-002).
//!
//! Thin wiring only: argument parsing, the stdin control channel and exit
//! codes. The lifecycle lives in `glint-core`. In the MVP the daemon and
//! shell services share this one process (ADR 0001) and run headless: the
//! capability adapter advertises nothing and the surface backend opens no
//! window, so the OS shell is never modified.
//!
//! Every lifecycle event is printed to stdout as one line,
//! `glint-daemon: <event>`. The process runs until it reads `shutdown` on
//! stdin or stdin closes, then shuts down cleanly (COR-002): surfaces
//! closed, state flushed, the single-instance lock released.
//!
//! Once ready the core serves local IPC (COR-003, ADR 0010) on a per-user
//! named pipe or Unix socket and prints `glint-daemon: ipc listening
//! <endpoint>`. Clients read state, subscribe to a snapshot plus ordered
//! deltas, or send `shutdown`, which takes the same clean path as the
//! stdin `shutdown` line. If the endpoint cannot be served the core shuts
//! down again and exits with code 1.
//!
//! Only one core runs per user (per data root). A second launch stops
//! before opening the database, asks the running instance for its status
//! over IPC, prints `glint-daemon: already-running pid=<pid> ipc=<state>`
//! (the running core's lifecycle state, or `unreachable`) and exits with
//! code 9.
//!
//! Console Ctrl+C, a closed console window, `WM_ENDSESSION` and SIGTERM
//! terminate the process without this clean path until the native host
//! crates forward them to the `ShutdownSignal`; the OS still releases both
//! locks, and every state write is an already-committed transaction.

#![forbid(unsafe_code)]

use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use glint_core::{
    CoreError, CoreIpc, ErrorCode, GlintPaths, RuntimeOptions, ShutdownReason, ShutdownSignal,
    headless_runtime, probe_ipc,
};

/// Clean shutdown.
const EXIT_OK: u8 = 0;
/// Boot failed, or shutdown left something unreleased.
const EXIT_FAILURE: u8 = 1;
/// Bad arguments.
const EXIT_USAGE: u8 = 2;
/// Another Glint core already runs for this user; nothing was opened.
const EXIT_ALREADY_RUNNING: u8 = 9;

const USAGE: &str = "usage: glint-daemon [--once]\n\n\
  Runs the Glint core (daemon and shell services in one process, headless).\n\
  Serves local IPC for this user. Reads `shutdown` from stdin (or takes an\n\
  IPC `shutdown` request), or stops when stdin closes.\n\
  Exit codes: 0 clean shutdown, 1 failure, 2 usage, 9 already running.\n\n\
  --once   shut down right after boot (smoke test)\n";

struct Args {
    once: bool,
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut args = Args { once: false };
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--once" => args.once = true,
            "-h" | "--help" => return Ok(None),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(Some(args))
}

fn say(line: impl std::fmt::Display) {
    let mut out = io::stdout().lock();
    // A closed stdout must not stop a clean shutdown.
    let _ = writeln!(out, "glint-daemon: {line}");
    let _ = out.flush();
}

fn report_error(error: &CoreError) {
    say(format_args!("error {error}"));
}

/// Fire `signal` when the controlling stdin asks for shutdown or closes.
fn watch_stdin(signal: ShutdownSignal) {
    std::thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            match line {
                Ok(line) if line.trim() == "shutdown" => {
                    signal.trigger(ShutdownReason::Requested);
                    return;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        signal.trigger(ShutdownReason::ControlChannelClosed);
    });
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(Some(args)) => args,
        Ok(None) => {
            print!("{USAGE}");
            return ExitCode::from(EXIT_OK);
        }
        Err(message) => {
            eprint!("glint-daemon: {message}\n\n{USAGE}");
            return ExitCode::from(EXIT_USAGE);
        }
    };

    let ipc = CoreIpc::new();
    let observer = ipc.clone();
    let mut runtime = headless_runtime(RuntimeOptions::resolve()).with_observer(move |event| {
        say(event);
        observer.observe(event);
    });
    let paths = match runtime.start() {
        Ok(ready) => {
            ipc.publish_ready(ready);
            ready.paths.clone()
        }
        Err(error) => {
            if error.code() == ErrorCode::AlreadyRunning {
                let pid = error
                    .owner()
                    .map_or_else(|| "unknown".to_owned(), |owner| owner.pid.to_string());
                say(format_args!(
                    "already-running pid={pid} ipc={}",
                    running_instance_state()
                ));
                return ExitCode::from(EXIT_ALREADY_RUNNING);
            }
            report_error(&error);
            return ExitCode::from(EXIT_FAILURE);
        }
    };

    let signal = ShutdownSignal::new();
    let server = match ipc.serve(&paths, &signal) {
        Ok(server) => server,
        Err(error) => {
            // A core nobody can reach is not reported as running.
            report_error(&error);
            let _ = runtime.shutdown(ShutdownReason::Requested);
            return ExitCode::from(EXIT_FAILURE);
        }
    };
    say(format_args!("ipc listening {}", server.endpoint()));

    if args.once {
        signal.trigger(ShutdownReason::Requested);
    } else {
        watch_stdin(signal.clone());
    }
    let outcome = runtime.run_until(&signal);
    // Subscribers receive every shutdown delta, then `closed`.
    ipc.close("the core stopped");
    server.stop();
    match outcome {
        Ok(report) if report.is_clean() => ExitCode::from(EXIT_OK),
        Ok(report) => {
            report.errors.iter().for_each(report_error);
            ExitCode::from(EXIT_FAILURE)
        }
        Err(error) => {
            report_error(&error);
            ExitCode::from(EXIT_FAILURE)
        }
    }
}

/// The running instance's lifecycle state over IPC, or `unreachable`.
fn running_instance_state() -> String {
    GlintPaths::resolve()
        .ok()
        .and_then(|paths| probe_ipc(&paths).ok())
        .map_or_else(|| "unreachable".to_owned(), |status| status.lifecycle)
}
