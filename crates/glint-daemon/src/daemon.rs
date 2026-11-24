//! [`Daemon`]: the in-process implementation of [`DaemonService`].

use std::fs::File;
use std::io::Read;
use std::path::Path;

use glint_installer::Installer;
use glint_platform::mock::MockAdapter;
use glint_platform::{
    Capability, CapabilityAdapter, CapabilityError, CapabilityService, CapabilitySnapshot,
    evaluate_manifest,
};
use glint_schema::{
    AppConfig, ConfigSource, MAX_CONFIG_BYTES, MAX_MANIFEST_BYTES, ThemeManifest, bundled_defaults,
    parse_config_layer, parse_theme_manifest, resolve_config,
};
use glint_storage::{ActiveTheme, GlintPaths};
use glint_surfaces::{SurfaceDefinition, surface_definitions};

use crate::error::DaemonError;
use crate::service::{
    CapabilityReport, ConfigReport, DaemonService, DaemonStatus, FallbackReason, FlushReport,
    SkippedSurface, StateReport, ThemeOrigin, ThemeSelection, UserConfig,
};

/// The bundled default theme's manifest (`themes/default/glint.json`),
/// embedded so the fallback never depends on the package store.
pub const BUNDLED_DEFAULT_MANIFEST: &str = include_str!("../../../themes/default/glint.json");

/// Name of the built-in adapter used when no native adapter is supplied.
pub const HEADLESS_ADAPTER: &str = "headless";

/// Lets the capability service own any adapter behind one concrete type.
struct BoxedAdapter(Box<dyn CapabilityAdapter + Send>);

impl CapabilityAdapter for BoxedAdapter {
    fn name(&self) -> &str {
        self.0.name()
    }

    fn discover(&self) -> Result<Vec<Capability>, CapabilityError> {
        self.0.discover()
    }
}

/// The per-user state authority, hosted in the core process (ADR 0001).
///
/// It alone holds the [`Installer`], and with it the only read-write handle
/// to the state database and the package store: exactly one owner writes
/// profile and package state. Everything it hands out through
/// [`DaemonService`] is owned, read-only data.
pub struct Daemon {
    capabilities: CapabilityService<BoxedAdapter>,
    installer: Option<Installer>,
    config: Option<AppConfig>,
    status: DaemonStatus,
}

impl std::fmt::Debug for Daemon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Daemon")
            .field("status", &self.status)
            .field("adapter", &self.capabilities.snapshot().adapter())
            .field("installer", &self.installer)
            .finish_non_exhaustive()
    }
}

impl Daemon {
    /// A daemon that discovers capabilities through `adapter` (a native
    /// adapter, or a [`MockAdapter`] in tests).
    pub fn new(adapter: impl CapabilityAdapter + Send + 'static) -> Self {
        Self {
            capabilities: CapabilityService::new(BoxedAdapter(Box::new(adapter))),
            installer: None,
            config: None,
            status: DaemonStatus::Created,
        }
    }

    /// A daemon whose adapter advertises no capability at all
    /// ([`HEADLESS_ADAPTER`]). It claims nothing it cannot provide, so every
    /// surface is skipped and the desktop is never touched; used in CI and
    /// until the native adapters exist.
    pub fn headless() -> Self {
        Self::new(MockAdapter::new(HEADLESS_ADAPTER))
    }

    /// The installer, read-only, while state is open. Mutating calls need
    /// `&mut Installer`, which never leaves the daemon.
    pub fn installer(&self) -> Option<&Installer> {
        self.installer.as_ref()
    }

    /// The effective configuration, once resolved.
    pub fn config(&self) -> Option<&AppConfig> {
        self.config.as_ref()
    }

    /// The current capability snapshot.
    pub fn capability_snapshot(&self) -> &CapabilitySnapshot {
        self.capabilities.snapshot()
    }

    fn require(&self, step: &'static str, at_least: DaemonStatus) -> Result<(), DaemonError> {
        let order = |status| match status {
            DaemonStatus::Created => 0,
            DaemonStatus::StateOpen => 1,
            DaemonStatus::Configured => 2,
            DaemonStatus::CapabilitiesKnown => 3,
            DaemonStatus::Serving => 4,
            DaemonStatus::Stopped => 5,
        };
        let requires = match at_least {
            DaemonStatus::StateOpen => "open_state",
            DaemonStatus::Configured => "resolve_config",
            DaemonStatus::CapabilitiesKnown => "discover_capabilities",
            _ => "boot",
        };
        if self.status == DaemonStatus::Stopped || order(self.status) < order(at_least) {
            return Err(DaemonError::InvalidState { step, requires });
        }
        Ok(())
    }

    fn open_installer(&self) -> Result<&Installer, DaemonError> {
        self.installer.as_ref().ok_or(DaemonError::InvalidState {
            step: "select_theme",
            requires: "open_state",
        })
    }

    /// The installed active theme, or why it cannot be used.
    fn load_installed(
        &self,
        installer: &Installer,
        active: &ActiveTheme,
    ) -> Result<Result<ThemeSelection, FallbackReason>, DaemonError> {
        let digest = active.package_digest.clone();
        let Some(package) = installer.package(&digest)? else {
            return Ok(Err(FallbackReason::NotInstalled { digest }));
        };
        let manifest = match read_manifest(&package.path.join("glint.json")) {
            Ok(manifest) => manifest,
            Err(detail) => return Ok(Err(FallbackReason::ManifestUnusable { digest, detail })),
        };
        let origin = ThemeOrigin::Installed {
            profile_id: active.profile_id.clone(),
            digest: digest.clone(),
            revision: active.revision,
            root: package.path.clone(),
        };
        match self.selection(origin, &manifest) {
            Ok(selection) => Ok(Ok(selection)),
            Err(SelectionError::Surfaces(detail)) => {
                Ok(Err(FallbackReason::ManifestUnusable { digest, detail }))
            }
            Err(SelectionError::Refused(refusal)) => Ok(Err(FallbackReason::Refused {
                digest,
                refusal: Box::new(refusal),
            })),
        }
    }

    fn selection(
        &self,
        origin: ThemeOrigin,
        manifest: &ThemeManifest,
    ) -> Result<ThemeSelection, SelectionError> {
        let snapshot = self.capabilities.snapshot();
        let definitions = surface_definitions(manifest)
            .map_err(|error| SelectionError::Surfaces(error.to_string()))?;
        let activation = evaluate_manifest(manifest, snapshot).map_err(SelectionError::Refused)?;
        let (surfaces, skipped) = partition_surfaces(definitions, snapshot);
        Ok(ThemeSelection {
            origin,
            theme_id: manifest.id.clone(),
            version: manifest.version.to_string(),
            activation,
            surfaces,
            skipped,
        })
    }
}

enum SelectionError {
    Surfaces(String),
    Refused(glint_platform::ActivationRefusal),
}

/// Split surfaces into those whose required capabilities are usable now and
/// those that must stay hidden.
fn partition_surfaces(
    definitions: Vec<SurfaceDefinition>,
    snapshot: &CapabilitySnapshot,
) -> (Vec<SurfaceDefinition>, Vec<SkippedSurface>) {
    let mut shown = Vec::new();
    let mut skipped = Vec::new();
    for definition in definitions {
        let blocker = definition
            .capabilities
            .required
            .iter()
            .find_map(|id| snapshot.require(id).err().map(|error| (id.clone(), error)));
        match blocker {
            None => shown.push(definition),
            Some((capability, error)) => skipped.push(SkippedSurface {
                id: definition.id,
                capability,
                error,
            }),
        }
    }
    (shown, skipped)
}

/// Read and validate an installed `glint.json`, bounded by
/// [`MAX_MANIFEST_BYTES`].
fn read_manifest(path: &Path) -> Result<ThemeManifest, String> {
    let text = read_bounded(path, MAX_MANIFEST_BYTES)?;
    parse_theme_manifest(&text).map_err(|error| error.to_string())
}

fn read_bounded(path: &Path, limit: usize) -> Result<String, String> {
    let file = File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut text = String::new();
    let cap = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
    file.take(cap)
        .read_to_string(&mut text)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if text.len() > limit {
        return Err(format!("{} is larger than {limit} bytes", path.display()));
    }
    Ok(text)
}

impl DaemonService for Daemon {
    fn open_state(&mut self, paths: &GlintPaths) -> Result<StateReport, DaemonError> {
        if self.status != DaemonStatus::Created {
            return Err(DaemonError::InvalidState {
                step: "open_state",
                requires: "a new daemon",
            });
        }
        let installer = Installer::open(paths)?;
        let report = StateReport {
            data_dir: paths.data_dir().to_path_buf(),
            schema_version: installer.storage().schema_version(),
            database_recovered: installer
                .storage()
                .recovery()
                .map(|recovery| recovery.reason.clone()),
            recovery: installer.startup_recovery().clone(),
            installed_packages: installer.installed()?.len(),
            active: installer.active_theme()?,
        };
        self.installer = Some(installer);
        self.status = DaemonStatus::StateOpen;
        Ok(report)
    }

    fn resolve_config(&mut self) -> Result<ConfigReport, DaemonError> {
        self.require("resolve_config", DaemonStatus::StateOpen)?;
        let path = self.open_installer()?.paths().config_file();
        let defaults = bundled_defaults();
        let mut rejected = None;
        let user = if path.is_file() {
            match read_bounded(&path, MAX_CONFIG_BYTES) {
                Ok(text) => match parse_config_layer(&text, ConfigSource::User) {
                    Ok(layer) => Some(layer),
                    Err(error) => {
                        rejected = Some(UserConfig::Rejected {
                            code: Some(error.code()),
                            message: error.to_string(),
                        });
                        None
                    }
                },
                Err(message) => {
                    rejected = Some(UserConfig::Rejected {
                        code: None,
                        message,
                    });
                    None
                }
            }
        } else {
            None
        };
        let resolved = match resolve_config(&defaults, user.as_ref(), &[]) {
            Ok(resolved) => resolved,
            Err(error) if user.is_some() => {
                rejected = Some(UserConfig::Rejected {
                    code: Some(error.code()),
                    message: error.to_string(),
                });
                resolve_config(&defaults, None, &[]).map_err(|error| {
                    DaemonError::DefaultThemeInvalid(format!("bundled config: {error}"))
                })?
            }
            Err(error) => {
                return Err(DaemonError::DefaultThemeInvalid(format!(
                    "bundled config: {error}"
                )));
            }
        };
        let user_config = match (rejected, user.is_some()) {
            (Some(rejected), _) => rejected,
            (None, true) => UserConfig::Applied,
            (None, false) => UserConfig::Absent,
        };
        let report = ConfigReport {
            path,
            user_config,
            diagnostics: resolved.diagnostics,
            use_default_fallback: resolved.config.shell.use_default_fallback,
        };
        self.config = Some(resolved.config);
        self.status = DaemonStatus::Configured;
        Ok(report)
    }

    fn discover_capabilities(&mut self) -> Result<CapabilityReport, DaemonError> {
        self.require("discover_capabilities", DaemonStatus::Configured)?;
        // A failed discovery is recorded in the snapshot, which then reports
        // every capability unusable (fail closed); it is not a boot failure.
        let adapter_error = self.capabilities.refresh().err();
        let snapshot = self.capabilities.snapshot();
        let report = CapabilityReport {
            adapter: snapshot.adapter().to_owned(),
            generation: snapshot.generation(),
            advertised: snapshot.iter().count(),
            usable: snapshot
                .iter()
                .filter(|capability| capability.availability.is_usable())
                .count(),
            adapter_error,
        };
        self.status = DaemonStatus::CapabilitiesKnown;
        Ok(report)
    }

    fn select_theme(&mut self) -> Result<ThemeSelection, DaemonError> {
        self.require("select_theme", DaemonStatus::CapabilitiesKnown)?;
        let installer = self.open_installer()?;
        let reason = match installer.active_theme()? {
            None => FallbackReason::NoActiveTheme,
            Some(active) => match self.load_installed(installer, &active)? {
                Ok(selection) => {
                    self.status = DaemonStatus::Serving;
                    return Ok(selection);
                }
                Err(reason) => reason,
            },
        };
        let use_fallback = self
            .config
            .as_ref()
            .is_none_or(|config| config.shell.use_default_fallback);
        if !use_fallback {
            return Err(DaemonError::FallbackDisabled(reason));
        }
        let manifest = parse_theme_manifest(BUNDLED_DEFAULT_MANIFEST)
            .map_err(|error| DaemonError::DefaultThemeInvalid(error.to_string()))?;
        let selection = self
            .selection(ThemeOrigin::BundledDefault { reason }, &manifest)
            .map_err(|error| match error {
                SelectionError::Surfaces(detail) => DaemonError::DefaultThemeInvalid(detail),
                SelectionError::Refused(refusal) => {
                    DaemonError::DefaultThemeInvalid(refusal.to_string())
                }
            })?;
        self.status = DaemonStatus::Serving;
        Ok(selection)
    }

    fn status(&self) -> DaemonStatus {
        self.status
    }

    fn shutdown(&mut self) -> Result<FlushReport, DaemonError> {
        let Some(installer) = self.installer.take() else {
            self.status = DaemonStatus::Stopped;
            return Ok(FlushReport::default());
        };
        self.status = DaemonStatus::Stopped;
        // Every state change already committed with `synchronous = FULL`;
        // flushing means bringing the active-theme record up to date before
        // the connection closes. Dropping the installer closes the database
        // and releases the installer lock, also on the error path.
        let storage = installer.storage();
        let flushed = storage
            .write_active_record()
            .and_then(|()| storage.active_theme());
        drop(installer);
        let active = flushed.map_err(DaemonError::Flush)?;
        Ok(FlushReport {
            state_closed: true,
            active,
        })
    }
}
