//! [`Runtime`]: boots the daemon and shell services in one process, keeps
//! the lifecycle state and shuts both down cleanly.

use std::fmt;
use std::time::Duration;

use glint_daemon::{
    CapabilityReport, ConfigReport, DaemonService, FlushReport, StateReport, ThemeOperation,
    ThemeOrigin, ThemeOutcome, ThemeSelection, UserConfig,
};
use glint_shell::{CloseReport, PresentReport, PresentRequest, ShellError, ShellService};
use glint_storage::GlintPaths;

use crate::error::CoreError;
use crate::instance::InstanceLock;
use crate::lifecycle::{
    BootStage, LifecycleEvent, LifecycleState, ShutdownReason, ShutdownStage, warnings,
};
use crate::operations::{OperationError, OperationInbox, OperationReport};
use crate::signal::ShutdownSignal;

/// How often [`Runtime::run_serving`] checks the shutdown signal while no
/// operation is queued.
const SERVE_POLL: Duration = Duration::from_millis(25);

/// How the runtime finds its paths.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RuntimeOptions {
    /// Explicit paths; `None` resolves them from `GLINT_HOME` or the OS
    /// ([`GlintPaths::resolve`]).
    pub paths: Option<GlintPaths>,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self::resolve()
    }
}

impl RuntimeOptions {
    /// Resolve paths from `GLINT_HOME` or the OS.
    pub fn resolve() -> Self {
        Self { paths: None }
    }

    /// Use these paths (tests, portable runs).
    pub fn with_paths(paths: GlintPaths) -> Self {
        Self { paths: Some(paths) }
    }
}

/// Everything boot found, kept while the runtime is ready.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReadyReport {
    /// The resolved paths.
    pub paths: GlintPaths,
    /// What opening state and startup recovery found.
    pub state: StateReport,
    /// The effective config.
    pub config: ConfigReport,
    /// What capability discovery found.
    pub capabilities: CapabilityReport,
    /// The selected theme.
    pub theme: ThemeSelection,
    /// What the shell presented.
    pub shell: PresentReport,
}

/// What shutdown released.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ShutdownReport {
    /// Why it shut down.
    pub reason: ShutdownReason,
    /// Surfaces closed and reservations released.
    pub shell: CloseReport,
    /// State flushed and closed.
    pub flush: FlushReport,
    /// Whether the single-instance lock was released.
    pub instance_released: bool,
    /// Steps that failed. Every later step still ran.
    pub errors: Vec<CoreError>,
}

impl ShutdownReport {
    /// True when every step succeeded.
    pub fn is_clean(&self) -> bool {
        self.errors.is_empty()
    }
}

type Observer = Box<dyn FnMut(&LifecycleEvent) + Send>;

/// The single-process core runtime (ADR 0001): the daemon and shell
/// services, composed through their traits.
///
/// [`start`](Self::start) runs the boot sequence; [`shutdown`](Self::shutdown)
/// closes the shell, flushes state and releases every lock. Dropping a
/// running runtime shuts it down with [`ShutdownReason::Dropped`].
pub struct Runtime<D: DaemonService, S: ShellService> {
    daemon: D,
    shell: S,
    options: RuntimeOptions,
    state: LifecycleState,
    events: Vec<LifecycleEvent>,
    observer: Option<Observer>,
    ready: Option<ReadyReport>,
    instance: Option<InstanceLock>,
}

impl<D: DaemonService, S: ShellService> fmt::Debug for Runtime<D, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Runtime")
            .field("state", &self.state)
            .field("events", &self.events.len())
            .finish_non_exhaustive()
    }
}

impl<D: DaemonService, S: ShellService> Runtime<D, S> {
    /// A runtime over `daemon` and `shell`; nothing starts until
    /// [`start`](Self::start).
    pub fn new(daemon: D, shell: S, options: RuntimeOptions) -> Self {
        Self {
            daemon,
            shell,
            options,
            state: LifecycleState::Created,
            events: Vec::new(),
            observer: None,
            ready: None,
            instance: None,
        }
    }

    /// Call `observer` with every lifecycle event as it happens.
    #[must_use]
    pub fn with_observer(mut self, observer: impl FnMut(&LifecycleEvent) + Send + 'static) -> Self {
        self.observer = Some(Box::new(observer));
        self
    }

    /// The lifecycle state.
    pub fn state(&self) -> LifecycleState {
        self.state
    }

    /// Every event so far, in order.
    pub fn events(&self) -> &[LifecycleEvent] {
        &self.events
    }

    /// What boot found, while ready.
    pub fn ready_report(&self) -> Option<&ReadyReport> {
        self.ready.as_ref()
    }

    /// The daemon service.
    pub fn daemon(&self) -> &D {
        &self.daemon
    }

    /// The shell service.
    pub fn shell(&self) -> &S {
        &self.shell
    }

    fn emit(&mut self, event: LifecycleEvent) {
        if let Some(observer) = self.observer.as_mut() {
            observer(&event);
        }
        self.events.push(event);
    }

    fn transition(&mut self, to: LifecycleState) {
        let from = self.state;
        self.state = to;
        self.emit(LifecycleEvent::StateChanged { from, to });
    }

    fn warn(&mut self, stage: BootStage, code: &'static str, message: impl Into<String>) {
        self.emit(LifecycleEvent::Warning {
            stage,
            code,
            message: message.into(),
        });
    }

    fn completed(&mut self, stage: BootStage, summary: impl Into<String>) {
        self.emit(LifecycleEvent::StageCompleted {
            stage,
            summary: summary.into(),
        });
    }

    /// Run the boot sequence: resolve paths, open state (with recovery),
    /// resolve config, discover capabilities, select the theme, present it.
    ///
    /// On failure everything that had started is released, the state
    /// becomes [`LifecycleState::Failed`] and the error names the stage. The
    /// native desktop is never modified by a failed boot.
    ///
    /// # Errors
    ///
    /// A [`CoreError`]; [`ErrorCode::InvalidState`](crate::ErrorCode::InvalidState)
    /// unless the runtime is [`LifecycleState::Created`].
    pub fn start(&mut self) -> Result<&ReadyReport, CoreError> {
        if self.state != LifecycleState::Created {
            return Err(CoreError::invalid_state(format!(
                "start called while {}",
                self.state
            )));
        }
        match self.boot() {
            Ok(report) => {
                self.ready = Some(report);
                self.transition(LifecycleState::Ready);
                Ok(self.ready.as_ref().expect("just set"))
            }
            Err(error) => {
                // Release whatever had started; their own errors are
                // secondary to the boot failure being reported.
                let _ = self.shell.close_all();
                let _ = self.daemon.shutdown();
                if let Some(lock) = self.instance.take() {
                    let _ = lock.release();
                }
                let stage = error.stage().unwrap_or(BootStage::ResolvePaths);
                self.transition(LifecycleState::Failed(stage));
                Err(error)
            }
        }
    }

    fn boot(&mut self) -> Result<ReadyReport, CoreError> {
        let paths = self.stage_paths()?;
        self.stage_instance(&paths)?;
        let state = self.stage_state(&paths)?;
        let config = self.stage_config()?;
        let capabilities = self.stage_capabilities()?;
        let theme = self.stage_theme()?;
        let shell = self.stage_shell(&theme)?;
        Ok(ReadyReport {
            paths,
            state,
            config,
            capabilities,
            theme,
            shell,
        })
    }

    fn stage_paths(&mut self) -> Result<GlintPaths, CoreError> {
        let stage = BootStage::ResolvePaths;
        self.transition(LifecycleState::Starting(stage));
        let paths = match self.options.paths.clone() {
            Some(paths) => paths,
            None => GlintPaths::resolve().map_err(|error| CoreError::paths(&error))?,
        };
        paths
            .ensure_dirs()
            .map_err(|error| CoreError::paths(&error))?;
        self.completed(stage, format!("data {}", paths.data_dir().display()));
        Ok(paths)
    }

    fn stage_instance(&mut self, paths: &GlintPaths) -> Result<(), CoreError> {
        let stage = BootStage::AcquireInstance;
        self.transition(LifecycleState::Starting(stage));
        let lock = InstanceLock::acquire(paths)
            .map_err(|error| CoreError::instance(Some(stage), &error))?;
        self.completed(stage, format!("pid {}", lock.owner().pid));
        self.instance = Some(lock);
        Ok(())
    }

    fn stage_state(&mut self, paths: &GlintPaths) -> Result<StateReport, CoreError> {
        let stage = BootStage::OpenState;
        self.transition(LifecycleState::Starting(stage));
        let report = self
            .daemon
            .open_state(paths)
            .map_err(|error| CoreError::daemon(Some(stage), &error))?;
        if let Some(reason) = &report.database_recovered {
            self.warn(stage, warnings::DATABASE_RECOVERED, reason.clone());
        }
        let mut recovered = report.recovery.clone();
        let pending = recovered.pending_apply.take();
        recovered.database_recovered = false;
        if !recovered.is_clean() {
            self.warn(
                stage,
                warnings::RECOVERY_PERFORMED,
                format!(
                    "rolled back {} install(s), restored {} removal(s), re-indexed {}, cleaned {} staging and {} trash entr(ies)",
                    recovered.rolled_back_installs.len(),
                    recovered.restored_removals.len(),
                    recovered.reindexed.len(),
                    recovered.cleaned_staging,
                    recovered.cleaned_trash,
                ),
            );
        }
        if let Some(pending) = pending {
            self.warn(
                stage,
                warnings::PENDING_APPLY,
                format!(
                    "interrupted apply of {} ({}) is journalled; the previous theme stays active until the native adapters roll it back",
                    pending.target_profile_id, pending.target_digest
                ),
            );
        }
        self.completed(
            stage,
            format!(
                "schema {}, {} package(s) installed",
                report.schema_version, report.installed_packages
            ),
        );
        Ok(report)
    }

    fn stage_config(&mut self) -> Result<ConfigReport, CoreError> {
        let stage = BootStage::ResolveConfig;
        self.transition(LifecycleState::Starting(stage));
        let report = self
            .daemon
            .resolve_config()
            .map_err(|error| CoreError::daemon(Some(stage), &error))?;
        if let UserConfig::Rejected { message, .. } = &report.user_config {
            self.warn(stage, warnings::CONFIG_REJECTED, message.clone());
        }
        for diagnostic in &report.diagnostics {
            self.warn(
                stage,
                warnings::CONFIG_DIAGNOSTIC,
                format!("{} at {}", diagnostic.code.as_str(), diagnostic.pointer),
            );
        }
        let summary = match report.user_config {
            UserConfig::Absent => "bundled defaults",
            UserConfig::Applied => "user config applied",
            UserConfig::Rejected { .. } => "user config rejected, bundled defaults",
            _ => "resolved",
        };
        self.completed(stage, summary);
        Ok(report)
    }

    fn stage_capabilities(&mut self) -> Result<CapabilityReport, CoreError> {
        let stage = BootStage::DiscoverCapabilities;
        self.transition(LifecycleState::Starting(stage));
        let report = self
            .daemon
            .discover_capabilities()
            .map_err(|error| CoreError::daemon(Some(stage), &error))?;
        if let Some(error) = &report.adapter_error {
            self.warn(
                stage,
                warnings::CAPABILITIES_UNAVAILABLE,
                format!("{}: {}", error.code().as_str(), error.message()),
            );
        }
        self.completed(
            stage,
            format!(
                "adapter {}: {} advertised, {} usable",
                report.adapter, report.advertised, report.usable
            ),
        );
        Ok(report)
    }

    fn stage_theme(&mut self) -> Result<ThemeSelection, CoreError> {
        let stage = BootStage::SelectTheme;
        self.transition(LifecycleState::Starting(stage));
        let selection = self
            .daemon
            .select_theme()
            .map_err(|error| CoreError::daemon(Some(stage), &error))?;
        if let ThemeOrigin::BundledDefault { reason } = &selection.origin {
            self.warn(
                stage,
                warnings::THEME_FALLBACK,
                format!("{}: {reason}", reason.as_str()),
            );
        }
        for skipped in &selection.skipped {
            self.warn(
                stage,
                warnings::SURFACE_SKIPPED,
                format!(
                    "{} needs {} ({})",
                    skipped.id,
                    skipped.capability.as_str(),
                    skipped.error.code().as_str()
                ),
            );
        }
        let source = if selection.is_fallback() {
            "bundled default"
        } else {
            "installed"
        };
        self.completed(
            stage,
            format!(
                "{} {} ({source}), {} surface(s)",
                selection.theme_id,
                selection.version,
                selection.surfaces.len()
            ),
        );
        Ok(selection)
    }

    fn stage_shell(&mut self, theme: &ThemeSelection) -> Result<PresentReport, CoreError> {
        let stage = BootStage::StartShell;
        self.transition(LifecycleState::Starting(stage));
        let request = PresentRequest::new(
            theme.theme_id.clone(),
            theme.version.clone(),
            theme.surfaces.clone(),
        );
        let report = self
            .shell
            .present(request)
            .map_err(|error| CoreError::shell(Some(stage), &error))?;
        for failure in &report.failed {
            self.warn(
                stage,
                warnings::SURFACE_FAILED,
                format!("{}: {}", failure.id, failure.error),
            );
        }
        self.completed(
            stage,
            format!(
                "backend {}: {} surface(s) open, {} reservation(s)",
                report.backend,
                report.opened.len(),
                report.reservations
            ),
        );
        Ok(report)
    }

    /// Wait until `signal` fires, then [`shutdown`](Self::shutdown) with its
    /// reason.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::InvalidState`](crate::ErrorCode::InvalidState) unless the
    /// runtime is [`LifecycleState::Ready`] (checked before waiting).
    pub fn run_until(&mut self, signal: &ShutdownSignal) -> Result<ShutdownReport, CoreError> {
        if self.state != LifecycleState::Ready {
            return Err(CoreError::invalid_state(format!(
                "run_until called while {}",
                self.state
            )));
        }
        let reason = signal.wait();
        self.shutdown(reason)
    }

    /// Like [`run_until`](Self::run_until), but also perform the theme
    /// operations IPC clients queue in `inbox` (CLI-006), one at a time on
    /// this thread, until `signal` fires. Operations still queued then are
    /// refused with [`ErrorCode::InvalidState`](crate::ErrorCode::InvalidState).
    ///
    /// # Errors
    ///
    /// [`ErrorCode::InvalidState`](crate::ErrorCode::InvalidState) unless the
    /// runtime is [`LifecycleState::Ready`].
    pub fn run_serving(
        &mut self,
        signal: &ShutdownSignal,
        inbox: &OperationInbox,
    ) -> Result<ShutdownReport, CoreError> {
        if self.state != LifecycleState::Ready {
            return Err(CoreError::invalid_state(format!(
                "run_serving called while {}",
                self.state
            )));
        }
        let reason = loop {
            if let Some(reason) = signal.reason() {
                break reason;
            }
            if let Some(job) = inbox.next(SERVE_POLL) {
                let result = self.perform(job.operation);
                // The client may have given up (deadline); the operation
                // still ran and its state change is published.
                let _ = job.reply.send(result);
            }
        };
        inbox.refuse_pending(&CoreError::invalid_state(
            "the core is shutting down; nothing was changed",
        ));
        self.shutdown(reason)
    }

    /// Perform one theme operation on the daemon (CLI-006). After an
    /// operation that changes state the theme is selected again and, when
    /// the selection changed, the shell re-presents it; problems with that
    /// are [`LifecycleEvent::Warning`]s, not failures of the operation,
    /// which has already been committed.
    ///
    /// # Errors
    ///
    /// [`OperationError::Core`] unless the runtime is ready;
    /// [`OperationError::Daemon`] when the daemon refused or failed it
    /// (nothing was changed).
    pub fn perform(
        &mut self,
        operation: ThemeOperation,
    ) -> Result<OperationReport, OperationError> {
        if self.state != LifecycleState::Ready {
            return Err(OperationError::Core(CoreError::invalid_state(format!(
                "{} requested while {}",
                operation.name(),
                self.state
            ))));
        }
        let name = operation.name();
        let changes_state = operation.changes_state();
        let outcome = self
            .daemon
            .theme_operation(operation)
            .map_err(OperationError::Daemon)?;
        if !changes_state {
            self.emit(LifecycleEvent::OperationCompleted {
                operation: name,
                summary: "read only".to_owned(),
            });
            return Ok(OperationReport {
                outcome,
                installed_packages: None,
                theme: None,
            });
        }
        let installed_packages = match self.daemon.theme_operation(ThemeOperation::List) {
            Ok(ThemeOutcome::Listed { packages, .. }) => Some(packages.len()),
            _ => None,
        };
        if let (Some(count), Some(ready)) = (installed_packages, self.ready.as_mut()) {
            ready.state.installed_packages = count;
        }
        let theme = self.reselect_theme();
        let summary = match &theme {
            Some(selection) => format!(
                "presenting {} {}, {} package(s) installed",
                selection.theme_id,
                selection.version,
                installed_packages.map_or_else(|| "?".to_owned(), |n| n.to_string())
            ),
            None => format!(
                "selection unchanged, {} package(s) installed",
                installed_packages.map_or_else(|| "?".to_owned(), |n| n.to_string())
            ),
        };
        self.emit(LifecycleEvent::OperationCompleted {
            operation: name,
            summary,
        });
        Ok(OperationReport {
            outcome,
            installed_packages,
            theme,
        })
    }

    /// Select the theme again and re-present it if the selection changed.
    /// Returns the new selection when it changed.
    fn reselect_theme(&mut self) -> Option<ThemeSelection> {
        let selection = match self.daemon.select_theme() {
            Ok(selection) => selection,
            Err(error) => {
                self.warn(
                    BootStage::SelectTheme,
                    warnings::RESELECT_FAILED,
                    format!(
                        "{}: the previous theme stays presented",
                        CoreError::daemon(None, &error)
                    ),
                );
                return None;
            }
        };
        if self
            .ready
            .as_ref()
            .is_some_and(|ready| ready.theme == selection)
        {
            return None;
        }
        if let ThemeOrigin::BundledDefault { reason } = &selection.origin {
            self.warn(
                BootStage::SelectTheme,
                warnings::THEME_FALLBACK,
                format!("{}: {reason}", reason.as_str()),
            );
        }
        if let Err(error) = self.shell.close_all() {
            self.warn(
                BootStage::StartShell,
                warnings::SURFACE_FAILED,
                format!("closing the previous surfaces: {error}"),
            );
        }
        let request = PresentRequest::new(
            selection.theme_id.clone(),
            selection.version.clone(),
            selection.surfaces.clone(),
        );
        let shell = match self.shell.present(request) {
            Ok(report) => {
                for failure in &report.failed {
                    self.warn(
                        BootStage::StartShell,
                        warnings::SURFACE_FAILED,
                        format!("{}: {}", failure.id, failure.error),
                    );
                }
                Some(report)
            }
            Err(error) => {
                self.warn(
                    BootStage::StartShell,
                    warnings::SURFACE_FAILED,
                    format!("presenting {}: {error}", selection.theme_id),
                );
                None
            }
        };
        if let Some(ready) = self.ready.as_mut() {
            ready.theme = selection.clone();
            if let Some(shell) = shell {
                ready.shell = shell;
            }
        }
        Some(selection)
    }

    /// Shut down: close the shell (releasing every surface and work-area
    /// reservation), flush and close state (releasing the installer lock),
    /// then release the single-instance lock. Every step runs even if an earlier one failed; failures are
    /// listed in [`ShutdownReport::errors`].
    ///
    /// # Errors
    ///
    /// [`ErrorCode::InvalidState`](crate::ErrorCode::InvalidState) unless the
    /// runtime is [`LifecycleState::Ready`].
    pub fn shutdown(&mut self, reason: ShutdownReason) -> Result<ShutdownReport, CoreError> {
        if self.state != LifecycleState::Ready {
            return Err(CoreError::invalid_state(format!(
                "shutdown called while {}",
                self.state
            )));
        }
        Ok(self.stop(reason))
    }

    fn stop(&mut self, reason: ShutdownReason) -> ShutdownReport {
        self.transition(LifecycleState::Stopping);
        self.emit(LifecycleEvent::ShutdownStarted {
            reason: reason.clone(),
        });
        let mut errors = Vec::new();

        let shell = match self.shell.close_all() {
            Ok(report) => report,
            Err(error) => {
                errors.push(CoreError::shell(None, &error));
                match error {
                    ShellError::CloseIncomplete(report) => report,
                    _ => CloseReport::default(),
                }
            }
        };
        let summary = format!(
            "{} surface(s) closed, {} reservation(s) released, {} failed",
            shell.closed.len(),
            shell.reservations_released,
            shell.failed.len()
        );
        self.emit(LifecycleEvent::ShutdownStepCompleted {
            stage: ShutdownStage::CloseShell,
            summary,
        });

        let (flush, summary) = match self.daemon.shutdown() {
            Ok(report) if report.state_closed => (report, "state flushed and closed"),
            Ok(report) => (report, "no state was open"),
            Err(error) => {
                errors.push(CoreError::daemon(None, &error));
                (
                    FlushReport::default(),
                    "state flush failed; database closed",
                )
            }
        };
        self.emit(LifecycleEvent::ShutdownStepCompleted {
            stage: ShutdownStage::FlushState,
            summary: summary.to_owned(),
        });

        // Last, so a new instance can only start once state is closed.
        let instance_released = match self.instance.take() {
            None => false,
            Some(lock) => match lock.release() {
                Ok(()) => true,
                Err(error) => {
                    errors.push(CoreError::instance(None, &error));
                    true
                }
            },
        };
        self.emit(LifecycleEvent::ShutdownStepCompleted {
            stage: ShutdownStage::ReleaseInstance,
            summary: if instance_released {
                "instance lock released".to_owned()
            } else {
                "no instance lock was held".to_owned()
            },
        });

        self.ready = None;
        self.transition(LifecycleState::Stopped);
        ShutdownReport {
            reason,
            shell,
            flush,
            instance_released,
            errors,
        }
    }
}

impl<D: DaemonService, S: ShellService> Drop for Runtime<D, S> {
    fn drop(&mut self) {
        if self.state == LifecycleState::Ready {
            let _ = self.stop(ShutdownReason::Dropped);
        }
    }
}
