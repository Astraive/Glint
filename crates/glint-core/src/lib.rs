//! Glint's core runtime (COR-001): lifecycle, boot orchestration and
//! shutdown of the daemon and shell services in one process.
//!
//! The MVP runs as a single process (ADR 0001) but keeps the daemon
//! (`glint-daemon`: state authority) and the shell (`glint-shell`: surface
//! presentation) in separate crates with typed service contracts,
//! [`DaemonService`] and [`ShellService`]. [`Runtime`] composes them; neither
//! crate depends on the other or on this one. When the process split lands
//! (COR-007) the runtime talks to IPC clients implementing the same traits:
//! the transport changes, the semantics do not.
//!
//! # Boot sequence
//!
//! [`Runtime::start`] walks the [`BootStage`]s in order, emitting typed
//! [`LifecycleEvent`]s:
//!
//! 1. `resolve-paths`: [`GlintPaths`] from `GLINT_HOME` or the OS, Glint's
//!    directories created.
//! 2. `open-state`: the daemon opens the state database and the installer
//!    and runs startup recovery.
//! 3. `resolve-config`: bundled defaults merged with the user config.
//! 4. `discover-capabilities`: through the daemon's capability adapter
//!    (an adapter that advertises nothing when headless).
//! 5. `select-theme`: the active profile's theme through the capability
//!    gate, or the bundled default theme.
//! 6. `start-shell`: the shell presents the selected surfaces.
//!
//! Degraded outcomes (recovered database, rejected config, fallback theme,
//! skipped surface) are reported as [`LifecycleEvent::Warning`]s with stable
//! `GLINT_CORE_WARN_*` codes; a failed stage releases whatever had started
//! and returns a [`CoreError`] naming the stage. The runtime never modifies
//! the OS shell itself; only a native [`SurfaceBackend`] could, and the
//! headless one does not.
//!
//! # Example
//!
//! ```no_run
//! use glint_core::{RuntimeOptions, ShutdownReason, headless_runtime};
//!
//! let mut runtime = headless_runtime(RuntimeOptions::resolve());
//! let ready = runtime.start()?;
//! println!("presenting {} {}", ready.theme.theme_id, ready.theme.version);
//! let report = runtime.shutdown(ShutdownReason::Requested)?;
//! assert!(report.is_clean());
//! # Ok::<(), glint_core::CoreError>(())
//! ```
//!
//! [`SurfaceBackend`]: glint_shell::SurfaceBackend

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod error;
mod lifecycle;
mod runtime;

pub use error::{CoreError, ErrorCode};
pub use glint_daemon::{Daemon, DaemonService};
pub use glint_shell::{HeadlessBackend, Shell, ShellService};
pub use glint_storage::GlintPaths;
pub use lifecycle::{
    BootStage, LifecycleEvent, LifecycleState, ShutdownReason, ShutdownStage, warnings,
};
pub use runtime::{ReadyReport, Runtime, RuntimeOptions, ShutdownReport};

/// The runtime Glint runs today: the real daemon with the headless
/// capability adapter (advertises nothing) and the shell over the headless
/// surface backend (opens no window). Native adapters and hosts replace
/// both once the native host crates exist.
pub fn headless_runtime(options: RuntimeOptions) -> Runtime<Daemon, Shell<HeadlessBackend>> {
    Runtime::new(
        Daemon::headless(),
        Shell::new(HeadlessBackend::new()),
        options,
    )
}
