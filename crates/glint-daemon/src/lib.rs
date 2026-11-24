//! Glint's per-user state authority (COR-001).
//!
//! In the single-process MVP (ADR 0001, `docs/adr/`) the daemon is a library hosted by
//! the core process, next to `glint-shell` and composed by `glint-core`.
//! It owns everything that writes profile and package state: the state
//! database and the package store (through `glint-installer`), the merged
//! application config and capability discovery. The shell never sees a
//! storage handle; it receives a [`ThemeSelection`], which is plain data.
//!
//! [`DaemonService`] is the typed contract the core drives, one call per
//! boot stage:
//!
//! 1. [`open_state`](DaemonService::open_state): open the database
//!    (migrating or recovering it), take the installer lock and run startup
//!    recovery.
//! 2. [`resolve_config`](DaemonService::resolve_config): bundled defaults
//!    plus the user `config.toml`; a rejected user config is reported and
//!    the defaults are used.
//! 3. [`discover_capabilities`](DaemonService::discover_capabilities):
//!    through a `glint_platform::CapabilityAdapter`. Until the native
//!    adapters exist, [`Daemon::headless`] uses an adapter that advertises
//!    nothing.
//! 4. [`select_theme`](DaemonService::select_theme): the active profile's
//!    installed theme through the capability gate, or the bundled default
//!    theme with a typed [`FallbackReason`].
//!
//! [`shutdown`](DaemonService::shutdown) mirrors the active-theme record,
//! closes the database and releases the installer lock.
//!
//! An interrupted apply found at startup is reported in
//! [`StateReport::recovery`] and left journalled: rolling it back needs the
//! native adapters that made the changes (COR-006, native host epics).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod daemon;
mod error;
mod service;

pub use daemon::{BUNDLED_DEFAULT_MANIFEST, Daemon, HEADLESS_ADAPTER};
pub use error::{DaemonError, ErrorCode};
pub use service::{
    CapabilityReport, ConfigReport, DaemonService, DaemonStatus, FallbackReason, FlushReport,
    SkippedSurface, StateReport, ThemeOrigin, ThemeSelection, UserConfig,
};
