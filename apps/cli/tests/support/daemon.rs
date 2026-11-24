//! Spawning the real `glint-daemon` binary with an isolated `GLINT_HOME`
//! for the CLI's routing tests (CLI-006, CLI-003).
//!
//! The daemon is another package's binary, so Cargo only builds it for
//! this test when the whole workspace is tested. When it is missing next
//! to `glint`, it is built once with the same Cargo, target directory and
//! `--locked` (`cargo build -p glint-daemon-app`).

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::OnceLock;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

const READY: &str = "glint-daemon: ipc listening";
const TIMEOUT: Duration = Duration::from_secs(60);

/// The `glint-daemon` binary, built on first use when absent.
pub fn binary() -> PathBuf {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            let glint = PathBuf::from(env!("CARGO_BIN_EXE_glint"));
            let daemon =
                glint.with_file_name(format!("glint-daemon{}", std::env::consts::EXE_SUFFIX));
            if !daemon.is_file() {
                let status = Command::new(env!("CARGO"))
                    .args(["build", "--locked", "-j", "2", "-p", "glint-daemon-app"])
                    .current_dir(env!("CARGO_MANIFEST_DIR"))
                    .status()
                    .expect("run cargo build for glint-daemon");
                assert!(status.success(), "building glint-daemon failed");
            }
            assert!(daemon.is_file(), "{} is missing", daemon.display());
            daemon
        })
        .clone()
}

/// A running core whose stdout lines are collected on a thread.
pub struct Daemon {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    seen: Vec<String>,
}

impl Daemon {
    /// Start a core for `home` and wait until it serves IPC.
    pub fn start(home: &Path) -> Self {
        let mut child = Command::new(binary())
            .env("GLINT_HOME", home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn glint-daemon");
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut daemon = Self {
            child,
            stdin,
            lines,
            seen: Vec::new(),
        };
        daemon.wait_for(READY);
        daemon
    }

    /// The core's process ID.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Wait for a stdout line containing `needle`.
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

    /// Ask for a clean shutdown on stdin; return the exit code and stdout.
    pub fn stop(mut self) -> (Option<i32>, String) {
        if let Some(mut stdin) = self.stdin.take() {
            let _ = writeln!(stdin, "shutdown");
        }
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

impl Drop for Daemon {
    fn drop(&mut self) {
        if self.stdin.is_some() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
