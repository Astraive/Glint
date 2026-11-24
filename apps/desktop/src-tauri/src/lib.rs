//! `glint-desktop`: the Glint manager's thin native host (MGR-001,
//! ADR 0011).
//!
//! The interface is the React and UI SDK app in `apps/desktop`, served from
//! assets embedded in this binary (no dev server, no remote content). This
//! crate only wires it to the core:
//!
//! - [`bridge`] reads the core's state over local IPC (`glint-ipc`) and
//!   turns every failure into a typed, honest `unreachable` answer. It is
//!   portable and tested on every OS.
//! - [`navigation`] is the window's navigation policy: only the app's own
//!   embedded origin may load.
//! - On Windows, `run` opens the Tauri window and exposes exactly two
//!   commands, `daemon_status` and `daemon_ping`, each allow-listed in
//!   `capabilities/main.json`. There is no command that writes state, takes
//!   a path or runs anything; the core stays the only writer.
//!
//! Windows is the only host the manager is built for (ADR 0002). Elsewhere
//! the binary reports that the platform is unsupported.

#![forbid(unsafe_code)]

pub mod bridge;
pub mod navigation;

#[cfg(windows)]
mod host;

#[cfg(windows)]
pub use host::run;
