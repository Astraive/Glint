//! Serving the core over local IPC (COR-003, ADR 0010).
//!
//! [`CoreIpc`] owns the [`Hub`] clients read and subscribe to. The runtime's
//! observer feeds it every lifecycle event ([`CoreIpc::observe`]), boot
//! publishes the stage reports ([`CoreIpc::publish_ready`]), and
//! [`CoreIpc::serve`] binds the per-user endpoint once the runtime is
//! ready. Reads are answered from that published snapshot, never from the
//! daemon, so the daemon stays the only holder of state; the one action,
//! `shutdown`, fires the same [`ShutdownSignal`] as the control channel.

use std::sync::Arc;

use glint_daemon::{ThemeOrigin, UserConfig};
use glint_ipc::{
    CapabilitySummary, ConfigSummary, Control, CoreSnapshot, Delta, Hub, IpcError, ServerHandle,
    ServerOptions, StateSummary, StatusReply, ThemeSummary, WarningInfo,
};
use glint_storage::{ErrorCategory, GlintPaths};

use crate::error::{CoreError, ErrorCode};
use crate::lifecycle::{LifecycleEvent, ShutdownReason, ShutdownStage};
use crate::runtime::ReadyReport;
use crate::signal::ShutdownSignal;

/// The core's IPC state: the published snapshot and its subscribers.
#[derive(Debug, Clone)]
pub struct CoreIpc {
    hub: Arc<Hub>,
}

impl Default for CoreIpc {
    fn default() -> Self {
        Self::new()
    }
}

struct SignalControl(ShutdownSignal);

impl Control for SignalControl {
    fn shutdown(&self) -> Result<bool, IpcError> {
        Ok(self.0.trigger(ShutdownReason::Requested))
    }
}

impl CoreIpc {
    /// A hub for this process, at lifecycle `created`.
    pub fn new() -> Self {
        Self {
            hub: Arc::new(Hub::new(CoreSnapshot::new(std::process::id()))),
        }
    }

    /// The hub.
    pub fn hub(&self) -> &Arc<Hub> {
        &self.hub
    }

    /// Publish a lifecycle event (call it from the runtime's observer).
    pub fn observe(&self, event: &LifecycleEvent) {
        match event {
            LifecycleEvent::StateChanged { to, .. } => {
                self.hub.publish(Delta::Lifecycle { state: to.name() });
            }
            LifecycleEvent::Warning {
                stage,
                code,
                message,
            } => {
                self.hub.publish(Delta::Warning {
                    warning: WarningInfo::new(stage.as_str(), *code, message.clone()),
                });
            }
            LifecycleEvent::ShutdownStarted { reason } => {
                self.hub.publish(Delta::ShutdownStarted {
                    reason: reason.as_str().to_owned(),
                });
            }
            LifecycleEvent::ShutdownStepCompleted { stage, summary } => {
                self.hub.publish(Delta::ShutdownStep {
                    stage: stage.as_str().to_owned(),
                    summary: summary.clone(),
                });
                if *stage == ShutdownStage::FlushState {
                    self.hub.publish(Delta::Daemon {
                        status: "stopped".to_owned(),
                    });
                }
            }
            // Boot progress is summarised by `publish_ready`.
            LifecycleEvent::StageCompleted { .. } => {}
        }
    }

    /// Publish what boot found, once the runtime is ready.
    pub fn publish_ready(&self, report: &ReadyReport) {
        self.hub.publish(Delta::Daemon {
            status: "serving".to_owned(),
        });
        self.hub.publish(booted(report));
    }

    /// Bind the per-user endpoint and serve until the returned handle is
    /// stopped. A `shutdown` request fires `signal`.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::IpcFailed`] with the `GLINT_IPC_*` code as its source
    /// (`GLINT_IPC_ENDPOINT_IN_USE` when the name is taken).
    pub fn serve(
        &self,
        paths: &GlintPaths,
        signal: &ShutdownSignal,
    ) -> Result<ServerHandle, CoreError> {
        let options = ServerOptions::named(format!("glint-daemon/{}", env!("CARGO_PKG_VERSION")));
        glint_ipc::serve(
            paths,
            Arc::clone(&self.hub),
            Arc::new(SignalControl(signal.clone())),
            options,
        )
        .map_err(|error| CoreError::ipc(&error))
    }

    /// End every subscription after the deltas already published (call it
    /// once shutdown has finished, before stopping the server).
    pub fn close(&self, reason: &str) {
        self.hub.close(reason);
    }
}

/// Ask the core serving `paths` for its status over IPC (a second launch
/// uses it to confirm the running instance answers).
///
/// # Errors
///
/// The [`IpcError`]: `GLINT_IPC_UNAVAILABLE` when nothing serves IPC.
pub fn probe_ipc(paths: &GlintPaths) -> Result<StatusReply, IpcError> {
    glint_ipc::Client::connect(paths)?.status()
}

fn count(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

fn booted(report: &ReadyReport) -> Delta {
    let state = StateSummary::new(
        report.state.schema_version,
        count(report.state.installed_packages),
        report.state.database_recovered.is_some(),
    );
    let user_config = match report.config.user_config {
        UserConfig::Absent => "absent",
        UserConfig::Applied => "applied",
        UserConfig::Rejected { .. } => "rejected",
        _ => "unknown",
    };
    let config = ConfigSummary::new(
        user_config,
        count(report.config.diagnostics.len()),
        report.config.use_default_fallback,
    );
    let capabilities = CapabilitySummary::new(
        report.capabilities.adapter.clone(),
        report.capabilities.generation,
        count(report.capabilities.advertised),
        count(report.capabilities.usable),
        report
            .capabilities
            .adapter_error
            .as_ref()
            .map(|error| error.code().as_str().to_owned()),
    );
    let selection = &report.theme;
    let theme = match &selection.origin {
        ThemeOrigin::Installed {
            profile_id,
            digest,
            revision,
            ..
        } => ThemeSummary::installed(
            selection.theme_id.clone(),
            selection.version.clone(),
            profile_id.clone(),
            *revision,
            digest.clone(),
        ),
        ThemeOrigin::BundledDefault { reason } => ThemeSummary::bundled(
            selection.theme_id.clone(),
            selection.version.clone(),
            reason.as_str(),
        ),
        _ => ThemeSummary::bundled(
            selection.theme_id.clone(),
            selection.version.clone(),
            "unknown",
        ),
    }
    .with_surfaces(
        selection.surfaces.iter().map(|s| s.id.clone()).collect(),
        selection.skipped.iter().map(|s| s.id.clone()).collect(),
    );
    Delta::Booted {
        state,
        config,
        capabilities,
        theme: Box::new(theme),
    }
}

impl CoreError {
    pub(crate) fn ipc(error: &IpcError) -> Self {
        let category = match error.category() {
            glint_ipc::ErrorCategory::Unsupported => ErrorCategory::Unsupported,
            glint_ipc::ErrorCategory::PermissionDenied => ErrorCategory::PermissionDenied,
            glint_ipc::ErrorCategory::Unavailable => ErrorCategory::Unavailable,
            glint_ipc::ErrorCategory::Timeout => ErrorCategory::Timeout,
            _ => ErrorCategory::Failure,
        };
        Self::new(ErrorCode::IpcFailed, None, category, error.message())
            .with_source_code(error.code().as_str())
    }
}
