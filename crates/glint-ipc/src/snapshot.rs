//! The core state a client can read and subscribe to: a [`CoreSnapshot`]
//! and the ordered [`Delta`]s that change it.
//!
//! These are wire types, plain data owned by this crate. `glint-core` maps
//! its lifecycle events and boot reports onto them; nothing here holds a
//! handle into the daemon, so serving a read never touches the state
//! database (the daemon stays the only writer).

use serde::{Deserialize, Serialize};

/// The most warnings a snapshot keeps; older ones are dropped first.
pub const MAX_SNAPSHOT_WARNINGS: usize = 64;

/// Everything a client needs to render the core's current state. A fresh
/// subscriber receives one; a reconnecting client receives the current one,
/// so it never depends on deltas it missed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CoreSnapshot {
    /// The runtime's lifecycle state name (`ready`, `stopping`,
    /// `starting:open-state`, ...).
    pub lifecycle: String,
    /// The daemon service's status (`created`, `serving`, `stopped`).
    pub daemon: String,
    /// The core process's ID.
    pub pid: u32,
    /// What opening state found, once known.
    pub state: Option<StateSummary>,
    /// The effective configuration, once resolved.
    pub config: Option<ConfigSummary>,
    /// Capability discovery, once done.
    pub capabilities: Option<CapabilitySummary>,
    /// The selected theme, once selected.
    pub theme: Option<ThemeSummary>,
    /// Degraded outcomes, oldest first (at most
    /// [`MAX_SNAPSHOT_WARNINGS`]).
    pub warnings: Vec<WarningInfo>,
    /// Why the core is shutting down, once it is.
    pub shutdown_reason: Option<String>,
}

impl CoreSnapshot {
    /// The snapshot of a core that has not booted yet.
    pub fn new(pid: u32) -> Self {
        Self {
            lifecycle: "created".to_owned(),
            daemon: "created".to_owned(),
            pid,
            state: None,
            config: None,
            capabilities: None,
            theme: None,
            warnings: Vec::new(),
            shutdown_reason: None,
        }
    }

    /// Apply one delta in place. Deltas are total: applying a valid
    /// sequence to the snapshot it started from always yields the server's
    /// snapshot at the last sequence number.
    pub fn apply(&mut self, delta: &Delta) {
        match delta {
            Delta::Lifecycle { state } => self.lifecycle.clone_from(state),
            Delta::Daemon { status } => self.daemon.clone_from(status),
            Delta::Warning { warning } => {
                if self.warnings.len() >= MAX_SNAPSHOT_WARNINGS {
                    self.warnings.remove(0);
                }
                self.warnings.push(warning.clone());
            }
            Delta::Booted {
                state,
                config,
                capabilities,
                theme,
            } => {
                self.state = Some(state.clone());
                self.config = Some(config.clone());
                self.capabilities = Some(capabilities.clone());
                self.theme = Some(ThemeSummary::clone(theme));
            }
            Delta::ShutdownStarted { reason } => self.shutdown_reason = Some(reason.clone()),
            Delta::ShutdownStep { .. } => {}
        }
    }
}

/// One change to a [`CoreSnapshot`], carried with a sequence number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Delta {
    /// The lifecycle state changed.
    Lifecycle {
        /// The new state name.
        state: String,
    },
    /// The daemon service's status changed.
    Daemon {
        /// The new status.
        status: String,
    },
    /// A degraded outcome was reported.
    Warning {
        /// The warning.
        warning: WarningInfo,
    },
    /// Boot finished: the reports of every daemon stage.
    Booted {
        /// What opening state found.
        state: StateSummary,
        /// The effective configuration.
        config: ConfigSummary,
        /// Capability discovery.
        capabilities: CapabilitySummary,
        /// The selected theme.
        theme: Box<ThemeSummary>,
    },
    /// Shutdown began.
    ShutdownStarted {
        /// Why (`requested`, `control-channel-closed`, ...).
        reason: String,
    },
    /// A shutdown step finished. Progress only; the snapshot does not
    /// change.
    ShutdownStep {
        /// The step (`close-shell`, `flush-state`, `release-instance`).
        stage: String,
        /// A one-line summary (not stable).
        summary: String,
    },
}

/// A degraded outcome (`GLINT_CORE_WARN_*`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WarningInfo {
    /// The boot stage that found it.
    pub stage: String,
    /// The stable warning code.
    pub code: String,
    /// Detail (not stable).
    pub message: String,
}

impl WarningInfo {
    /// A warning.
    pub fn new(
        stage: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            stage: stage.into(),
            code: code.into(),
            message: message.into(),
        }
    }
}

/// What the daemon's `open_state` found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct StateSummary {
    /// The database schema version.
    pub schema_version: u32,
    /// Installed packages.
    pub installed_packages: u64,
    /// Whether the database was quarantined and recreated.
    pub database_recovered: bool,
}

impl StateSummary {
    /// A summary.
    pub fn new(schema_version: u32, installed_packages: u64, database_recovered: bool) -> Self {
        Self {
            schema_version,
            installed_packages,
            database_recovered,
        }
    }
}

/// What the daemon's `resolve_config` produced. Paths are left out: they
/// identify the user and no client needs them to render state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ConfigSummary {
    /// `absent`, `applied` or `rejected`.
    pub user_config: String,
    /// The number of merge diagnostics.
    pub diagnostics: u64,
    /// `shell.use_default_fallback`.
    pub use_default_fallback: bool,
}

impl ConfigSummary {
    /// A summary.
    pub fn new(
        user_config: impl Into<String>,
        diagnostics: u64,
        use_default_fallback: bool,
    ) -> Self {
        Self {
            user_config: user_config.into(),
            diagnostics,
            use_default_fallback,
        }
    }
}

/// What the daemon's `discover_capabilities` found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CapabilitySummary {
    /// The adapter's name.
    pub adapter: String,
    /// The snapshot generation.
    pub generation: u64,
    /// Advertised capabilities.
    pub advertised: u64,
    /// Usable capabilities.
    pub usable: u64,
    /// The discovery error's stable code, if discovery failed.
    pub discovery_error: Option<String>,
}

impl CapabilitySummary {
    /// A summary.
    pub fn new(
        adapter: impl Into<String>,
        generation: u64,
        advertised: u64,
        usable: u64,
        discovery_error: Option<String>,
    ) -> Self {
        Self {
            adapter: adapter.into(),
            generation,
            advertised,
            usable,
            discovery_error,
        }
    }
}

/// The theme the daemon selected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ThemeSummary {
    /// Manifest `id`.
    pub theme_id: String,
    /// Manifest `version`.
    pub version: String,
    /// `installed` or `bundled-default`.
    pub origin: String,
    /// The active profile, for an installed theme.
    pub profile_id: Option<String>,
    /// The profile revision, for an installed theme.
    pub revision: Option<u64>,
    /// The release digest, for an installed theme.
    pub digest: Option<String>,
    /// Why the bundled default was chosen (`no-active-theme`, ...).
    pub fallback_reason: Option<String>,
    /// Presented surface IDs, in manifest order.
    pub surfaces: Vec<String>,
    /// Surface IDs left out because a capability is unusable.
    pub skipped: Vec<String>,
}

impl ThemeSummary {
    /// The bundled default theme, chosen for `fallback_reason`.
    pub fn bundled(
        theme_id: impl Into<String>,
        version: impl Into<String>,
        fallback_reason: impl Into<String>,
    ) -> Self {
        Self {
            theme_id: theme_id.into(),
            version: version.into(),
            origin: "bundled-default".to_owned(),
            profile_id: None,
            revision: None,
            digest: None,
            fallback_reason: Some(fallback_reason.into()),
            surfaces: Vec::new(),
            skipped: Vec::new(),
        }
    }

    /// An installed theme from the active profile.
    pub fn installed(
        theme_id: impl Into<String>,
        version: impl Into<String>,
        profile_id: impl Into<String>,
        revision: u64,
        digest: impl Into<String>,
    ) -> Self {
        Self {
            theme_id: theme_id.into(),
            version: version.into(),
            origin: "installed".to_owned(),
            profile_id: Some(profile_id.into()),
            revision: Some(revision),
            digest: Some(digest.into()),
            fallback_reason: None,
            surfaces: Vec::new(),
            skipped: Vec::new(),
        }
    }

    /// With these presented and skipped surface IDs.
    #[must_use]
    pub fn with_surfaces(mut self, surfaces: Vec<String>, skipped: Vec<String>) -> Self {
        self.surfaces = surfaces;
        self.skipped = skipped;
        self
    }
}
