//! Serving the core over local IPC (COR-003, ADR 0010).
//!
//! [`CoreIpc`] owns the [`Hub`] clients read and subscribe to. The runtime's
//! observer feeds it every lifecycle event ([`CoreIpc::observe`]), boot
//! publishes the stage reports ([`CoreIpc::publish_ready`]), and
//! [`CoreIpc::serve`] binds the per-user endpoint once the runtime is
//! ready. Reads are answered from that published snapshot, never from the
//! daemon, so the daemon stays the only holder of state; `shutdown` fires
//! the same [`ShutdownSignal`] as the control channel.
//!
//! Theme operations (CLI-006) are queued through an [`OperationSender`] to
//! the runtime's thread, which runs them on the daemon
//! ([`Runtime::run_serving`](crate::Runtime::run_serving)). The control
//! publishes the resulting state and theme deltas before it answers, so a
//! second client that reads after the answer sees the change.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use glint_daemon::{
    DaemonError, ThemeOperation, ThemeOrigin, ThemeOutcome, ThemeSelection, UserConfig,
};
use glint_installer::{InstallError, InstalledPackage};
use glint_ipc::{
    ActiveInfo, CapabilitySummary, ConfigSummary, Control, CoreSnapshot, Delta, GlintRequest, Hub,
    IpcError, OperationErrorKind, OperationFailure, PackageInfo, ResponseBody, ServerHandle,
    ServerOptions, StateSummary, StatusReply, ThemeActivationReply, ThemeExportReply,
    ThemeImportReply, ThemeListReply, ThemeRemoveReply, ThemeSummary, WarningInfo,
};
use glint_storage::{ActiveTheme, ErrorCategory, GlintPaths};
use serde_json::Value;

use crate::error::{CoreError, ErrorCode};
use crate::lifecycle::{LifecycleEvent, ShutdownReason, ShutdownStage};
use crate::operations::{OperationError, OperationReport, OperationSender, Undelivered};
use crate::runtime::ReadyReport;
use crate::signal::ShutdownSignal;

/// How long an IPC client's operation may wait for the runtime (queueing
/// plus running). Longer than any local import or export; a client that
/// waits less sees its own timeout first.
pub const OPERATION_DEADLINE: Duration = Duration::from_secs(60);

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

struct SignalControl {
    signal: ShutdownSignal,
    hub: Arc<Hub>,
    operations: Option<OperationSender>,
}

impl Control for SignalControl {
    fn shutdown(&self) -> Result<bool, IpcError> {
        Ok(self.signal.trigger(ShutdownReason::Requested))
    }

    fn operate(&self, request: &GlintRequest) -> Result<ResponseBody, IpcError> {
        let Some(operations) = &self.operations else {
            return Err(IpcError::new(
                glint_ipc::ErrorCode::OperationUnsupported,
                format!(
                    "this core does not perform `{}` operations",
                    request.type_name()
                ),
            ));
        };
        let operation = to_operation(request)?;
        let name = operation.name();
        match operations.perform(operation, OPERATION_DEADLINE) {
            Ok(Ok(report)) => {
                publish_changes(&self.hub, &report);
                response_body(report.outcome)
            }
            Ok(Err(OperationError::Daemon(error))) => Err(operation_error(&error)),
            Ok(Err(OperationError::Core(error))) => Err(IpcError::new(
                if self.signal.reason().is_some() || self.hub.closed().is_some() {
                    glint_ipc::ErrorCode::ShuttingDown
                } else {
                    glint_ipc::ErrorCode::NotReady
                },
                format!("{name} was not performed: {}", error.message()),
            )),
            Err(Undelivered::Stopped) => Err(IpcError::new(
                glint_ipc::ErrorCode::ShuttingDown,
                format!("the core stopped before performing {name}"),
            )),
            Err(Undelivered::TimedOut) => Err(IpcError::new(
                glint_ipc::ErrorCode::Timeout,
                format!(
                    "{name} did not finish within {} s; it may still complete",
                    OPERATION_DEADLINE.as_secs()
                ),
            )),
        }
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
            // Boot progress is summarised by `publish_ready`; operations
            // publish their own state and theme deltas.
            LifecycleEvent::StageCompleted { .. } | LifecycleEvent::OperationCompleted { .. } => {}
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
        self.serve_inner(paths, signal, None)
    }

    /// Like [`serve`](Self::serve), and also accept theme operations
    /// (CLI-006): each is queued to `operations`, whose inbox the runtime
    /// drains in [`Runtime::run_serving`](crate::Runtime::run_serving).
    ///
    /// # Errors
    ///
    /// As [`serve`](Self::serve).
    pub fn serve_with_operations(
        &self,
        paths: &GlintPaths,
        signal: &ShutdownSignal,
        operations: OperationSender,
    ) -> Result<ServerHandle, CoreError> {
        self.serve_inner(paths, signal, Some(operations))
    }

    fn serve_inner(
        &self,
        paths: &GlintPaths,
        signal: &ShutdownSignal,
        operations: Option<OperationSender>,
    ) -> Result<ServerHandle, CoreError> {
        let options = ServerOptions::named(format!("glint-daemon/{}", env!("CARGO_PKG_VERSION")));
        let control = SignalControl {
            signal: signal.clone(),
            hub: Arc::clone(&self.hub),
            operations,
        };
        glint_ipc::serve(paths, Arc::clone(&self.hub), Arc::new(control), options)
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
    Delta::Booted {
        state,
        config,
        capabilities,
        theme: Box::new(theme_summary(&report.theme)),
    }
}

fn theme_summary(selection: &ThemeSelection) -> ThemeSummary {
    match &selection.origin {
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
    )
}

/// Map a wire request onto the daemon's operation. Paths must be absolute:
/// the core's working directory is not the client's.
fn to_operation(request: &GlintRequest) -> Result<ThemeOperation, IpcError> {
    let path = |value: &str, what: &str| {
        let path = std::path::PathBuf::from(value);
        if value.is_empty() || value.contains('\0') || !path.is_absolute() {
            return Err(IpcError::operation_failed(
                OperationFailure::new(
                    glint_daemon::ErrorCode::InvalidRequest.as_str(),
                    OperationErrorKind::InvalidRequest,
                ),
                format!("{what} must be an absolute path"),
            ));
        }
        Ok(path)
    };
    Ok(match request {
        GlintRequest::ThemeList => ThemeOperation::List,
        GlintRequest::ThemeImport { path: value } => ThemeOperation::Import {
            path: path(value, "the package path")?,
        },
        GlintRequest::ThemeUse { theme } => ThemeOperation::Use {
            theme: theme.clone(),
        },
        GlintRequest::ThemeRemove { theme } => ThemeOperation::Remove {
            theme: theme.clone(),
        },
        GlintRequest::ThemeExport { theme, destination } => ThemeOperation::Export {
            theme: theme.clone(),
            destination: path(destination, "the export destination")?,
        },
        GlintRequest::SafeMode => ThemeOperation::SafeMode,
        other => {
            return Err(IpcError::new(
                glint_ipc::ErrorCode::UnknownRequest,
                format!("{} is not a theme operation", other.type_name()),
            ));
        }
    })
}

/// Publish what an operation changed, before its answer is sent.
fn publish_changes(hub: &Hub, report: &OperationReport) {
    if let Some(installed) = report.installed_packages {
        let (_, snapshot) = hub.snapshot();
        if let Some(mut state) = snapshot.state
            && state.installed_packages != count(installed)
        {
            state.installed_packages = count(installed);
            hub.publish(Delta::State { state });
        }
    }
    if let Some(selection) = &report.theme {
        hub.publish(Delta::Theme {
            theme: Box::new(theme_summary(selection)),
        });
    }
}

fn package_info(package: &InstalledPackage) -> PackageInfo {
    PackageInfo::new(
        package.id.clone(),
        package.version.clone(),
        package.digest.clone(),
        package.origin.clone(),
        package.source_kind.clone(),
        package.source_locator.clone(),
        package.archive_size,
        package.installed_at,
        glint_installer::is_bundled(package),
    )
}

fn active_info(active: &ActiveTheme) -> ActiveInfo {
    ActiveInfo::new(
        active.profile_id.clone(),
        active.package_digest.clone(),
        active.revision,
    )
}

fn response_body(outcome: ThemeOutcome) -> Result<ResponseBody, IpcError> {
    Ok(match outcome {
        ThemeOutcome::Listed { packages, active } => ResponseBody::ThemeList(ThemeListReply::new(
            packages.iter().map(package_info).collect(),
            active.as_ref().map(active_info),
        )),
        ThemeOutcome::Imported { imported, active } => {
            ResponseBody::ThemeImported(Box::new(ThemeImportReply::new(
                package_info(&imported.package),
                imported.already_installed,
                active.as_ref().map(active_info),
            )))
        }
        ThemeOutcome::Activated {
            activated,
            safe_mode,
        } => ResponseBody::ThemeActivated(Box::new(ThemeActivationReply::new(
            package_info(&activated.package),
            active_info(&activated.active),
            activated.previous.as_ref().map(active_info),
            safe_mode,
        ))),
        ThemeOutcome::Removed { package } => ResponseBody::ThemeRemoved(ThemeRemoveReply::new(
            package.id,
            package.version,
            package.digest,
        )),
        ThemeOutcome::Exported { exported } => {
            let canonical_path = exported.path.display().to_string();
            ResponseBody::ThemeExported(ThemeExportReply::new(
                exported.id,
                exported.version,
                exported.installed_digest,
                canonical_path,
                exported.digest,
                exported.size,
                count(exported.files),
            ))
        }
        // `ThemeOutcome` is non-exhaustive: a new outcome without a wire
        // reply is an internal error, never a fake success.
        other => {
            return Err(IpcError::new(
                glint_ipc::ErrorCode::Internal,
                format!("no wire reply for operation outcome {other:?}"),
            ));
        }
    })
}

/// Strip a library's own `CODE: ` prefix from its display string.
fn without_code(display: &str, code: &str) -> String {
    display
        .strip_prefix(code)
        .and_then(|rest| rest.strip_prefix(": "))
        .unwrap_or(display)
        .to_owned()
}

/// True when an OS permission error is somewhere in `error`'s chain.
fn permission_denied(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut next = Some(error);
    while let Some(error) = next {
        if error
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::PermissionDenied)
        {
            return true;
        }
        next = error.source();
    }
    false
}

/// The typed failure of a daemon operation: the most specific component
/// code, the failure kind and the code chain, as the offline CLI reports
/// them for the same error.
fn operation_error(error: &DaemonError) -> IpcError {
    let (failure, message) = match error {
        DaemonError::Source(source) => {
            let source_code = source.code().as_str();
            let mut kind = match source_code {
                "GLINT_SOURCE_NOT_FOUND" => OperationErrorKind::NotFound,
                "GLINT_SOURCE_PERMISSION_DENIED" => OperationErrorKind::PermissionDenied,
                "GLINT_SOURCE_UNSUPPORTED" => OperationErrorKind::Unsupported,
                "GLINT_SOURCE_NOT_A_FILE"
                | "GLINT_SOURCE_NOT_A_DIRECTORY"
                | "GLINT_SOURCE_PACKAGE_REJECTED" => OperationErrorKind::InvalidPackage,
                _ => OperationErrorKind::Failure,
            };
            let mut code = source_code.to_owned();
            let mut details = BTreeMap::new();
            details.insert("sourceCode".to_owned(), Value::from(source_code));
            if let Some(package) = source.package_error() {
                code = package.code().as_str().to_owned();
                kind = if permission_denied(package) {
                    OperationErrorKind::PermissionDenied
                } else {
                    match code.as_str() {
                        "GLINT_PACKAGE_IO"
                        | "GLINT_PACKAGE_STAGING_ROOT_INVALID"
                        | "GLINT_PACKAGE_STAGING_CONFLICT" => OperationErrorKind::Failure,
                        _ => OperationErrorKind::InvalidPackage,
                    }
                };
                details.insert("packageCode".to_owned(), Value::from(code.clone()));
                if let Some(manifest) = package.manifest_error() {
                    code = manifest.code().as_str().to_owned();
                    details.insert("manifestCode".to_owned(), Value::from(code.clone()));
                    details.insert("pointer".to_owned(), Value::from(manifest.pointer()));
                }
                if let Some(surface) = package.surface_error() {
                    code = surface.code().as_str().to_owned();
                    details.insert("surfaceCode".to_owned(), Value::from(code.clone()));
                    details.insert("pointer".to_owned(), Value::from(surface.pointer()));
                }
            }
            let mut failure = OperationFailure::new(code, kind);
            failure.details = details;
            (failure, without_code(&source.to_string(), source_code))
        }
        DaemonError::Operation(install) => install_failure(install),
        DaemonError::InvalidRequest(detail) => (
            OperationFailure::new(error.code().as_str(), OperationErrorKind::InvalidRequest),
            detail.clone(),
        ),
        DaemonError::OperationUnsupported(_) => (
            OperationFailure::new(error.code().as_str(), OperationErrorKind::Unsupported),
            error.to_string(),
        ),
        other => (
            OperationFailure::new(other.code().as_str(), OperationErrorKind::Failure),
            other.to_string(),
        ),
    };
    IpcError::operation_failed(failure, message)
}

fn install_failure(error: &InstallError) -> (OperationFailure, String) {
    let install_code = error.code().as_str();
    let kind = match install_code {
        "GLINT_INSTALL_NOT_INSTALLED" => OperationErrorKind::NotFound,
        "GLINT_INSTALL_AMBIGUOUS" | "GLINT_INSTALL_EXPORT_DESTINATION_INVALID" => {
            OperationErrorKind::InvalidRequest
        }
        _ => match error.category() {
            ErrorCategory::PermissionDenied => OperationErrorKind::PermissionDenied,
            _ => OperationErrorKind::Failure,
        },
    };
    let mut code = install_code.to_owned();
    let mut failure =
        OperationFailure::new(install_code, kind).with_detail("installCode", install_code);
    if let Some(outcome) = error.identity_outcome() {
        code = outcome.code().to_owned();
        failure = failure.with_detail("identityCode", outcome.code());
    }
    if let Some(package) = error.package_error() {
        code = package.code().as_str().to_owned();
        failure = failure.with_detail("packageCode", package.code().as_str());
    }
    if let Some(storage) = error.storage_error() {
        code = storage.code().as_str().to_owned();
        failure = failure.with_detail("storageCode", storage.code().as_str());
    }
    match error {
        InstallError::Ambiguous { candidates, .. } => {
            failure = failure.with_detail("candidates", candidates.clone());
        }
        InstallError::InUse { profiles, .. } => {
            failure = failure.with_detail("profiles", profiles.clone());
        }
        _ => {}
    }
    failure.code = code;
    (failure, without_code(&error.to_string(), install_code))
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
