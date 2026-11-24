//! Spawning the real binary with an isolated `GLINT_HOME`.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Output, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

const BIN: &str = env!("CARGO_BIN_EXE_glint-daemon");
const READY: &str = "glint-daemon: state starting:start-shell -> ready";
const TIMEOUT: Duration = Duration::from_secs(60);

/// A temporary directory used as `GLINT_HOME`.
pub fn glint_home() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn command(home: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(BIN);
    command
        .args(args)
        .env("GLINT_HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

/// Run to completion with stdin closed.
pub fn run_once(home: &Path, args: &[&str]) -> Output {
    let mut command = command(home, args);
    command.stdin(Stdio::null());
    command.output().unwrap()
}

/// A running daemon whose stdout lines are collected on a thread.
pub struct Daemon {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    seen: Vec<String>,
}

fn pump(stdout: ChildStdout) -> Receiver<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

impl Daemon {
    pub fn spawn(home: &Path) -> Self {
        Self::spawn_with(home, &[])
    }

    pub fn spawn_with(home: &Path, args: &[&str]) -> Self {
        let mut child = command(home, args).spawn().unwrap();
        let stdin = child.stdin.take();
        let lines = pump(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            lines,
            seen: Vec::new(),
        }
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Wait for a line containing `needle`; panics after a timeout or EOF.
    pub fn wait_for(&mut self, needle: &str) {
        loop {
            match self.lines.recv_timeout(TIMEOUT) {
                Ok(line) => {
                    let found = line.contains(needle);
                    self.seen.push(line);
                    if found {
                        return;
                    }
                }
                Err(error) => panic!(
                    "no line containing {needle:?} ({error}); output so far:\n{}",
                    self.seen.join("\n")
                ),
            }
        }
    }

    pub fn wait_ready(&mut self) {
        self.wait_for(READY);
    }

    pub fn stdin(&mut self) -> &mut ChildStdin {
        self.stdin.as_mut().unwrap()
    }

    pub fn close_stdin(&mut self) {
        self.stdin = None;
    }

    /// Kill the process without a clean shutdown (a crash or a forced
    /// termination).
    pub fn kill(mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }

    /// Close stdin, wait for exit and return the exit code and all stdout.
    pub fn finish(mut self) -> (Option<i32>, String) {
        self.stdin = None;
        let status = self.child.wait().unwrap();
        while let Ok(line) = self.lines.recv_timeout(Duration::from_secs(5)) {
            self.seen.push(line);
        }
        let mut stderr = String::new();
        if let Some(mut pipe) = self.child.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr);
        }
        let mut out = self.seen.join("\n");
        out.push('\n');
        out.push_str(&stderr);
        (status.code(), out)
    }
}
