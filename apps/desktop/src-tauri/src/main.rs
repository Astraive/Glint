//! `glint-desktop`: the Glint manager window (MGR-001). Thin: everything is
//! in the library.

// A release build is a GUI app with no console window on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

use std::process::ExitCode;

#[cfg(windows)]
fn main() -> ExitCode {
    glint_desktop::run()
}

/// Exit code for "not supported on this platform" (matches the CLI's 3).
#[cfg(not(windows))]
const EXIT_UNSUPPORTED: u8 = 3;

#[cfg(not(windows))]
fn main() -> ExitCode {
    eprintln!(
        "glint-desktop: the manager window is only built for Windows so far (ADR 0002, MGR-001); \
         this platform is unsupported"
    );
    ExitCode::from(EXIT_UNSUPPORTED)
}
