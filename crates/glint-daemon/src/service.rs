//! The daemon's service contract (COR-001).
//!
//! [`DaemonService`] is the typed interface the core calls, one method per
//! boot stage plus shutdown. In the single-process MVP (ADR 0001) the core
//! holds a [`Daemon`](crate::Daemon) directly; when the daemon moves into
//! its own process (COR-007) an IPC client implements the same trait, so
//! the requests, results and errors stay the same and only the transport
//! changes. Every value crossing the contract is owned data: no handle to
//! the database or the installer ever leaves the daemon.

use std::fmt;
use std::path::PathBuf;

use glint_installer::RecoveryReport;
use glint_platform::{ActivationRefusal, ActivationReport, CapabilityError, CapabilityId};
use glint_schema::{ConfigDiagnostic, ConfigErrorCode};
use glint_storage::{ActiveTheme, GlintPaths};
use glint_surfaces::SurfaceDefinition;

use crate::error::DaemonError;

/// The daemon's state authority, driven by the core one boot stage at a
/// time. Stages must run in declaration order; calling one early fails with
/// [`ErrorCode::InvalidState`](crate::ErrorCode::InvalidState).
pub trait DaemonService {
    /// Open the state database and the installer (taking the installer
    /// lock) and run startup recovery.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::StateUnavailable`](crate::ErrorCode::StateUnavailable).
    fn open_state(&mut self, paths: &GlintPaths) -> Result<StateReport, DaemonError>;

    /// Merge the bundled defaults with the user `config.toml`. A rejected
    /// user config is reported and the defaults are used.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::InvalidState`](crate::ErrorCode::InvalidState) before
    /// [`open_state`](Self::open_state).
    fn resolve_config(&mut self) -> Result<ConfigReport, DaemonError>;

    /// Discover host capabilities through the capability adapter. A failed
    /// discovery is reported (every capability is then unusable), not
    /// returned as an error.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::InvalidState`](crate::ErrorCode::InvalidState) before
    /// [`resolve_config`](Self::resolve_config).
    fn discover_capabilities(&mut self) -> Result<CapabilityReport, DaemonError>;

    /// Load the active profile's theme and run it through the capability
    /// gate, or fall back to the bundled default theme.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::FallbackDisabled`](crate::ErrorCode::FallbackDisabled),
    /// [`ErrorCode::DefaultThemeInvalid`](crate::ErrorCode::DefaultThemeInvalid)
    /// or [`ErrorCode::InvalidState`](crate::ErrorCode::InvalidState).
    fn select_theme(&mut self) -> Result<ThemeSelection, DaemonError>;

    /// Where the daemon is in its lifecycle.
    fn status(&self) -> DaemonStatus;

    /// Flush state (mirror the active theme record), close the database and
    /// release the installer lock. Safe to call in any state; a second call
    /// does nothing.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::FlushFailed`](crate::ErrorCode::FlushFailed); the
    /// database and the installer lock are released anyway.
    fn shutdown(&mut self) -> Result<FlushReport, DaemonError>;
}

/// Where a daemon is in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DaemonStatus {
    /// Nothing opened yet.
    Created,
    /// State database and installer are open.
    StateOpen,
    /// Configuration resolved.
    Configured,
    /// Capabilities discovered.
    CapabilitiesKnown,
    /// A theme is selected; the daemon is serving.
    Serving,
    /// Shut down; state is closed.
    Stopped,
}

/// What [`DaemonService::open_state`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct StateReport {
    /// The data directory holding the database and the store.
    pub data_dir: PathBuf,
    /// The database schema version after migrations.
    pub schema_version: u32,
    /// Why the database was quarantined and recreated, if it was.
    pub database_recovered: Option<String>,
    /// What installer recovery did (rolled-back installs, cleaned staging,
    /// an interrupted apply, ...).
    pub recovery: RecoveryReport,
    /// Installed packages after recovery.
    pub installed_packages: usize,
    /// The active theme recorded in the database.
    pub active: Option<ActiveTheme>,
}

/// What happened to the user `config.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum UserConfig {
    /// No user config file; the bundled defaults apply.
    Absent,
    /// The user config was merged over the defaults.
    Applied,
    /// The user config was refused; the bundled defaults apply instead.
    Rejected {
        /// The stable `GLINT_CONFIG_*` code, or `None` when the file could
        /// not be read.
        code: Option<ConfigErrorCode>,
        /// Human-readable detail.
        message: String,
    },
}

/// What [`DaemonService::resolve_config`] produced.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConfigReport {
    /// The user config file's path.
    pub path: PathBuf,
    /// Whether it was applied.
    pub user_config: UserConfig,
    /// Merge diagnostics (ignored keys, weakened security settings).
    pub diagnostics: Vec<ConfigDiagnostic>,
    /// `shell.use_default_fallback`.
    pub use_default_fallback: bool,
}

/// What [`DaemonService::discover_capabilities`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CapabilityReport {
    /// The adapter's name (`headless` for the built-in empty adapter).
    pub adapter: String,
    /// Snapshot generation after discovery.
    pub generation: u64,
    /// Capabilities the adapter advertises.
    pub advertised: usize,
    /// Of those, the ones usable now.
    pub usable: usize,
    /// The discovery error, if discovery failed (nothing is usable then).
    pub adapter_error: Option<CapabilityError>,
}

/// Why the bundled default theme is presented instead of an installed one.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FallbackReason {
    /// No profile is active (a fresh installation).
    NoActiveTheme,
    /// The active release is not in the package index.
    NotInstalled {
        /// Its digest.
        digest: String,
    },
    /// The active release's `glint.json` could not be read or is invalid.
    ManifestUnusable {
        /// Its digest.
        digest: String,
        /// What went wrong.
        detail: String,
    },
    /// The capability gate refused the active theme.
    Refused {
        /// Its digest.
        digest: String,
        /// The gate's refusal, listing every blocker.
        refusal: Box<ActivationRefusal>,
    },
}

impl FallbackReason {
    /// A short stable name for logs and reports.
    pub const fn as_str(&self) -> &'static str {
        match self {
            FallbackReason::NoActiveTheme => "no-active-theme",
            FallbackReason::NotInstalled { .. } => "not-installed",
            FallbackReason::ManifestUnusable { .. } => "manifest-unusable",
            FallbackReason::Refused { .. } => "capability-refused",
        }
    }
}

impl fmt::Display for FallbackReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FallbackReason::NoActiveTheme => f.write_str("no profile is active"),
            FallbackReason::NotInstalled { digest } => {
                write!(f, "active release {digest} is not installed")
            }
            FallbackReason::ManifestUnusable { digest, detail } => {
                write!(
                    f,
                    "active release {digest} has an unusable manifest: {detail}"
                )
            }
            FallbackReason::Refused { digest, refusal } => {
                write!(f, "active release {digest} was refused: {refusal}")
            }
        }
    }
}

/// Where the selected theme comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ThemeOrigin {
    /// The active profile's installed release.
    Installed {
        /// The active profile.
        profile_id: String,
        /// The release's digest.
        digest: String,
        /// The profile revision.
        revision: u64,
        /// The extracted, read-only package tree in the store.
        root: PathBuf,
    },
    /// The bundled default theme (`official.default`), embedded in the
    /// binary.
    BundledDefault {
        /// Why it was chosen.
        reason: FallbackReason,
    },
}

/// A surface the theme declares that is not presented, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SkippedSurface {
    /// The surface ID.
    pub id: String,
    /// The capability it cannot be shown without.
    pub capability: CapabilityId,
    /// Why that capability is unusable.
    pub error: CapabilityError,
}

/// The theme the daemon selected and what the shell may present.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ThemeSelection {
    /// Where it comes from.
    pub origin: ThemeOrigin,
    /// Manifest `id`.
    pub theme_id: String,
    /// Manifest `version`.
    pub version: String,
    /// The capability gate's verdict (degraded controls).
    pub activation: ActivationReport,
    /// Surfaces whose required capability is usable, in manifest order.
    pub surfaces: Vec<SurfaceDefinition>,
    /// Declared surfaces left out because their capability is unusable.
    pub skipped: Vec<SkippedSurface>,
}

impl ThemeSelection {
    /// True when the bundled default theme was chosen.
    pub fn is_fallback(&self) -> bool {
        matches!(self.origin, ThemeOrigin::BundledDefault { .. })
    }
}

/// What [`DaemonService::shutdown`] flushed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct FlushReport {
    /// Whether state was open (and therefore flushed and closed).
    pub state_closed: bool,
    /// The active theme mirrored into the active-theme record.
    pub active: Option<ActiveTheme>,
}
