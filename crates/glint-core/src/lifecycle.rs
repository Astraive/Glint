//! Typed lifecycle states, boot stages and events.

use std::fmt;

/// One step of the boot sequence, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum BootStage {
    /// Resolve Glint's configuration, data and cache roots.
    ResolvePaths,
    /// Take the per-user single-instance lock, before the database is
    /// opened (COR-002).
    AcquireInstance,
    /// Open the state database and installer and run startup recovery.
    OpenState,
    /// Merge the bundled defaults with the user config.
    ResolveConfig,
    /// Discover host capabilities.
    DiscoverCapabilities,
    /// Load the active theme or fall back to the bundled default.
    SelectTheme,
    /// Present the selected theme's surfaces.
    StartShell,
}

impl BootStage {
    /// Every stage, in boot order.
    pub const ALL: &'static [BootStage] = &[
        BootStage::ResolvePaths,
        BootStage::AcquireInstance,
        BootStage::OpenState,
        BootStage::ResolveConfig,
        BootStage::DiscoverCapabilities,
        BootStage::SelectTheme,
        BootStage::StartShell,
    ];

    /// A stable kebab-case name.
    pub const fn as_str(self) -> &'static str {
        match self {
            BootStage::ResolvePaths => "resolve-paths",
            BootStage::AcquireInstance => "acquire-instance",
            BootStage::OpenState => "open-state",
            BootStage::ResolveConfig => "resolve-config",
            BootStage::DiscoverCapabilities => "discover-capabilities",
            BootStage::SelectTheme => "select-theme",
            BootStage::StartShell => "start-shell",
        }
    }
}

impl fmt::Display for BootStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where the runtime is in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LifecycleState {
    /// Constructed; nothing started.
    Created,
    /// Booting; the stage in progress.
    Starting(BootStage),
    /// Booted; the daemon serves and the shell presents.
    Ready,
    /// Shutting down.
    Stopping,
    /// Shut down cleanly or after a reported failure; every resource is
    /// released.
    Stopped,
    /// Boot failed at a stage; whatever had started was released.
    Failed(BootStage),
}

impl LifecycleState {
    /// A stable name (`starting:open-state`, `ready`, `failed:select-theme`).
    pub fn name(&self) -> String {
        match self {
            LifecycleState::Created => "created".into(),
            LifecycleState::Starting(stage) => format!("starting:{stage}"),
            LifecycleState::Ready => "ready".into(),
            LifecycleState::Stopping => "stopping".into(),
            LifecycleState::Stopped => "stopped".into(),
            LifecycleState::Failed(stage) => format!("failed:{stage}"),
        }
    }
}

impl fmt::Display for LifecycleState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

/// Why the runtime is shutting down.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ShutdownReason {
    /// The host asked (an API call, a `shutdown` command).
    Requested,
    /// The controlling channel closed (the parent went away).
    ControlChannelClosed,
    /// The OS is ending the session or stopping the process.
    SessionEnding,
    /// The runtime was dropped while running.
    Dropped,
}

impl ShutdownReason {
    /// A stable kebab-case name.
    pub const fn as_str(&self) -> &'static str {
        match self {
            ShutdownReason::Requested => "requested",
            ShutdownReason::ControlChannelClosed => "control-channel-closed",
            ShutdownReason::SessionEnding => "session-ending",
            ShutdownReason::Dropped => "dropped",
        }
    }
}

impl fmt::Display for ShutdownReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One step of the shutdown sequence, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ShutdownStage {
    /// Close surfaces and release work-area reservations.
    CloseShell,
    /// Flush state, close the database, release the installer lock.
    FlushState,
    /// Remove the owner record and release the instance lock.
    ReleaseInstance,
}

impl ShutdownStage {
    /// A stable kebab-case name.
    pub const fn as_str(self) -> &'static str {
        match self {
            ShutdownStage::CloseShell => "close-shell",
            ShutdownStage::FlushState => "flush-state",
            ShutdownStage::ReleaseInstance => "release-instance",
        }
    }
}

impl fmt::Display for ShutdownStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Something the runtime did, delivered to the observer and kept in
/// [`Runtime::events`](crate::Runtime::events).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LifecycleEvent {
    /// The lifecycle state changed.
    StateChanged {
        /// The previous state.
        from: LifecycleState,
        /// The new state.
        to: LifecycleState,
    },
    /// A boot stage finished.
    StageCompleted {
        /// The stage.
        stage: BootStage,
        /// A one-line summary (not stable).
        summary: String,
    },
    /// Boot continued, but something is degraded and must be shown to the
    /// user (a recovered database, a rejected config, a fallback theme, a
    /// skipped surface, ...).
    Warning {
        /// The stage that found it.
        stage: BootStage,
        /// A stable code (`GLINT_CORE_WARN_*`).
        code: &'static str,
        /// Detail (not stable).
        message: String,
    },
    /// Shutdown began.
    ShutdownStarted {
        /// Why.
        reason: ShutdownReason,
    },
    /// A shutdown stage finished (successfully or not).
    ShutdownStepCompleted {
        /// The stage.
        stage: ShutdownStage,
        /// A one-line summary (not stable).
        summary: String,
    },
}

impl fmt::Display for LifecycleEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LifecycleEvent::StateChanged { from, to } => write!(f, "state {from} -> {to}"),
            LifecycleEvent::StageCompleted { stage, summary } => {
                write!(f, "stage {stage}: {summary}")
            }
            LifecycleEvent::Warning {
                stage,
                code,
                message,
            } => write!(f, "warning {stage} {code}: {message}"),
            LifecycleEvent::ShutdownStarted { reason } => write!(f, "shutdown {reason}"),
            LifecycleEvent::ShutdownStepCompleted { stage, summary } => {
                write!(f, "shutdown-step {stage}: {summary}")
            }
        }
    }
}

/// Stable warning codes carried by [`LifecycleEvent::Warning`].
pub mod warnings {
    /// The state database was corrupt and was quarantined and recreated.
    pub const DATABASE_RECOVERED: &str = "GLINT_CORE_WARN_DATABASE_RECOVERED";
    /// Startup recovery rolled back or cleaned up interrupted work.
    pub const RECOVERY_PERFORMED: &str = "GLINT_CORE_WARN_RECOVERY_PERFORMED";
    /// An interrupted apply is journalled and needs the native adapters to
    /// roll back; the previous theme stays active.
    pub const PENDING_APPLY: &str = "GLINT_CORE_WARN_PENDING_APPLY";
    /// The user config was refused; bundled defaults apply.
    pub const CONFIG_REJECTED: &str = "GLINT_CORE_WARN_CONFIG_REJECTED";
    /// A config merge diagnostic (ignored key, weakened security setting).
    pub const CONFIG_DIAGNOSTIC: &str = "GLINT_CORE_WARN_CONFIG_DIAGNOSTIC";
    /// Capability discovery failed; nothing is usable.
    pub const CAPABILITIES_UNAVAILABLE: &str = "GLINT_CORE_WARN_CAPABILITIES_UNAVAILABLE";
    /// The bundled default theme is presented instead of the active one.
    pub const THEME_FALLBACK: &str = "GLINT_CORE_WARN_THEME_FALLBACK";
    /// A declared surface is hidden because its capability is unusable.
    pub const SURFACE_SKIPPED: &str = "GLINT_CORE_WARN_SURFACE_SKIPPED";
    /// The surface backend failed to open a surface.
    pub const SURFACE_FAILED: &str = "GLINT_CORE_WARN_SURFACE_FAILED";
}
