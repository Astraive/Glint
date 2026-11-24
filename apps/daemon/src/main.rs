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
//! Only one core runs per user (per data root). A second launch stops
//! before opening the database, prints `glint-daemon: already-running
//! pid=<pid>` and exits with code 9. Handing the request to the running
//! instance needs the IPC transport (COR-003).
//!
//! Console Ctrl+C, a closed console window, `WM_ENDSESSION` and SIGTERM
//! terminate the process without this clean path until the native host
//! crates forward them to the `ShutdownSignal`; the OS still releases both
//! locks, and every state write is an already-committed transaction.

#![forbid(unsafe_code)]

use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use glint_core::{
    CoreError, ErrorCode, RuntimeOptions, ShutdownReason, ShutdownSignal, headless_runtime,
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
  Reads `shutdown` from stdin, or stops when stdin closes.\n\
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

    let mut runtime = headless_runtime(RuntimeOptions::resolve()).with_observer(|event| say(event));
    if let Err(error) = runtime.start() {
        if error.code() == ErrorCode::AlreadyRunning {
            match error.owner() {
                Some(owner) => say(format_args!("already-running pid={}", owner.pid)),
                None => say("already-running pid=unknown"),
            }
            return ExitCode::from(EXIT_ALREADY_RUNNING);
        }
        report_error(&error);
        return ExitCode::from(EXIT_FAILURE);
    }

    let signal = ShutdownSignal::new();
    if args.once {
        signal.trigger(ShutdownReason::Requested);
    } else {
        watch_stdin(signal.clone());
    }
    match runtime.run_until(&signal) {
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
