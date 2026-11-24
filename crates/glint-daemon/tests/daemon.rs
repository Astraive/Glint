//! The daemon's boot stages against a real temporary state directory
//! (COR-001). Synthetic themes are generated per test; nothing touches the
//! network or the real desktop.

use std::fs;
use std::path::Path;

use glint_daemon::{
    Daemon, DaemonService, DaemonStatus, ErrorCode, FallbackReason, ThemeOrigin, UserConfig,
};
use glint_installer::{InstallOptions, Installer};
use glint_platform::mock::MockAdapter;
use glint_platform::{Availability, Capability, CapabilityId, IntegrationEffect};
use glint_sources::{LocalSource, PackageSource, ThemeSource};
use glint_storage::{GlintPaths, Storage};

struct Home {
    tmp: tempfile::TempDir,
    paths: GlintPaths,
}

impl Home {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let paths = GlintPaths::under_root(tmp.path().join("glint")).unwrap();
        Self { tmp, paths }
    }

    /// Install (and activate as `profile`) a synthetic theme whose panel
    /// capability is required or optional. Returns the digest.
    fn install(&self, id: &str, required: bool, profile: &str) -> String {
        let dir = self.tmp.path().join(format!("src-{id}"));
        write_theme(&dir, id, required);
        let mut installer = Installer::open(&self.paths).unwrap();
        let staged = LocalSource::new()
            .stage(
                &ThemeSource::LocalDirectory { path: dir },
                &installer.staging_root(),
            )
            .unwrap();
        installer
            .install_with(
                staged.candidate,
                staged.origin,
                &InstallOptions::activate(profile),
            )
            .unwrap()
            .digest
    }

    fn write_config(&self, text: &str) {
        fs::create_dir_all(self.paths.config_dir()).unwrap();
        fs::write(self.paths.config_file(), text).unwrap();
    }
}

fn write_theme(dir: &Path, id: &str, required: bool) {
    let capability = if required {
        r#""requiredCapabilities": ["surface.panel"],"#
    } else {
        r#""optionalCapabilities": ["surface.panel"],"#
    };
    let manifest = format!(
        r#"{{
  "schemaVersion": 1,
  "kind": "theme",
  "id": "{id}",
  "name": "Fixture {id}",
  "version": "1.2.3",
  "author": {{ "name": "Fixture Author" }},
  "license": "MIT",
  "compatibility": {{ "glint": ">=0.0.0", "platforms": ["windows", "linux", "macos"] }},
  "renderer": {{ "kind": "glint-web", "entrypointVersion": 1 }},
  {capability}
  "surfaces": {{ "panel": {{ "entry": "dist/panel/index.html" }} }}
}}"#
    );
    let files = [
        ("glint.json", manifest.as_str()),
        (
            "dist/panel/index.html",
            "<!doctype html><html><head></head><body>panel</body></html>",
        ),
    ];
    for (name, content) in files {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
}

fn panel_adapter() -> MockAdapter {
    MockAdapter::new("test").with(Capability::new(
        CapabilityId::parse("surface.panel").unwrap(),
        Availability::Supported,
        IntegrationEffect::GlintOwnedUi,
    ))
}

fn boot(daemon: &mut Daemon, paths: &GlintPaths) -> glint_daemon::ThemeSelection {
    daemon.open_state(paths).unwrap();
    daemon.resolve_config().unwrap();
    daemon.discover_capabilities().unwrap();
    daemon.select_theme().unwrap()
}

#[test]
fn a_fresh_home_boots_into_the_bundled_default_theme() {
    let home = Home::new();
    let mut daemon = Daemon::headless();
    assert_eq!(daemon.status(), DaemonStatus::Created);

    let state = daemon.open_state(&home.paths).unwrap();
    assert_eq!(state.installed_packages, 0);
    assert_eq!(state.active, None);
    assert_eq!(state.database_recovered, None);
    assert!(home.paths.database_file().is_file());
    assert_eq!(daemon.status(), DaemonStatus::StateOpen);

    let config = daemon.resolve_config().unwrap();
    assert_eq!(config.user_config, UserConfig::Absent);
    assert!(config.use_default_fallback);

    let capabilities = daemon.discover_capabilities().unwrap();
    assert_eq!(capabilities.adapter, "headless");
    assert_eq!((capabilities.advertised, capabilities.usable), (0, 0));
    assert_eq!(capabilities.adapter_error, None);

    let theme = daemon.select_theme().unwrap();
    assert_eq!(theme.theme_id, "official.default");
    assert_eq!(
        theme.origin,
        ThemeOrigin::BundledDefault {
            reason: FallbackReason::NoActiveTheme
        }
    );
    // The headless adapter advertises no surface capability, so nothing is
    // presented: the panel is skipped with the gate's reason.
    assert!(theme.surfaces.is_empty());
    assert_eq!(theme.skipped.len(), 1);
    assert_eq!(theme.skipped[0].id, "panel");
    assert_eq!(theme.skipped[0].capability.as_str(), "surface.panel");
    assert_eq!(daemon.status(), DaemonStatus::Serving);

    let flush = daemon.shutdown().unwrap();
    assert!(flush.state_closed);
    assert_eq!(daemon.status(), DaemonStatus::Stopped);
}

#[test]
fn an_advertised_panel_capability_lets_the_default_panel_through() {
    let home = Home::new();
    let mut daemon = Daemon::new(panel_adapter());
    let theme = boot(&mut daemon, &home.paths);
    assert!(theme.is_fallback());
    assert_eq!(theme.surfaces.len(), 1);
    assert_eq!(theme.surfaces[0].id, "panel");
    assert!(theme.skipped.is_empty());
}

#[test]
fn the_installed_active_theme_is_selected() {
    let home = Home::new();
    let digest = home.install("fixture.optional", false, "work");
    let mut daemon = Daemon::new(panel_adapter());
    let state = daemon.open_state(&home.paths).unwrap();
    assert_eq!(state.installed_packages, 1);
    assert_eq!(state.active.as_ref().unwrap().package_digest, digest);
    daemon.resolve_config().unwrap();
    daemon.discover_capabilities().unwrap();
    let theme = daemon.select_theme().unwrap();
    assert_eq!(theme.theme_id, "fixture.optional");
    assert_eq!(theme.version, "1.2.3");
    match &theme.origin {
        ThemeOrigin::Installed {
            profile_id,
            digest: selected,
            revision,
            root,
        } => {
            assert_eq!(profile_id, "work");
            assert_eq!(selected, &digest);
            assert_eq!(*revision, 1);
            assert!(root.join("glint.json").is_file());
        }
        other => panic!("expected the installed theme, got {other:?}"),
    }
    assert_eq!(theme.surfaces.len(), 1);
}

#[test]
fn a_theme_refused_by_the_capability_gate_falls_back() {
    let home = Home::new();
    let digest = home.install("fixture.required", true, "default");
    // Headless: surface.panel is not advertised, so the required capability
    // blocks activation and the bundled default is used instead.
    let mut daemon = Daemon::headless();
    let theme = boot(&mut daemon, &home.paths);
    assert_eq!(theme.theme_id, "official.default");
    match &theme.origin {
        ThemeOrigin::BundledDefault {
            reason: FallbackReason::Refused { digest: d, refusal },
        } => {
            assert_eq!(d, &digest);
            assert_eq!(refusal.code().as_str(), "theme-incompatible");
            assert_eq!(refusal.blockers().len(), 1);
        }
        other => panic!("expected a refusal fallback, got {other:?}"),
    }
}

#[test]
fn an_active_release_missing_from_the_index_falls_back() {
    let home = Home::new();
    let missing = "ab".repeat(32);
    let mut storage = Storage::open(&home.paths).unwrap();
    storage.set_active_theme("default", &missing).unwrap();
    drop(storage);

    let mut daemon = Daemon::headless();
    let theme = boot(&mut daemon, &home.paths);
    assert_eq!(
        theme.origin,
        ThemeOrigin::BundledDefault {
            reason: FallbackReason::NotInstalled { digest: missing }
        }
    );
}

#[test]
fn a_disabled_fallback_is_reported_not_faked() {
    let home = Home::new();
    home.install("fixture.required", true, "default");
    home.write_config("[shell]\nuse_default_fallback = false\n");
    let mut daemon = Daemon::headless();
    daemon.open_state(&home.paths).unwrap();
    let config = daemon.resolve_config().unwrap();
    assert_eq!(config.user_config, UserConfig::Applied);
    assert!(!config.use_default_fallback);
    daemon.discover_capabilities().unwrap();
    let error = daemon.select_theme().unwrap_err();
    assert_eq!(error.code(), ErrorCode::FallbackDisabled);
    assert_eq!(error.code().as_str(), "GLINT_DAEMON_FALLBACK_DISABLED");
    assert!(matches!(
        error,
        glint_daemon::DaemonError::FallbackDisabled(FallbackReason::Refused { .. })
    ));
}

#[test]
fn a_rejected_user_config_is_reported_and_defaults_apply() {
    let home = Home::new();
    home.write_config("[shell]\nuse_default_fallback = \"maybe\"\n");
    let mut daemon = Daemon::headless();
    daemon.open_state(&home.paths).unwrap();
    let config = daemon.resolve_config().unwrap();
    match &config.user_config {
        UserConfig::Rejected { code, .. } => assert!(code.is_some()),
        other => panic!("expected a rejected config, got {other:?}"),
    }
    assert!(config.use_default_fallback);
    assert!(daemon.config().unwrap().shell.use_default_fallback);
}

#[test]
fn stages_out_of_order_are_refused() {
    let home = Home::new();
    let mut daemon = Daemon::headless();
    assert_eq!(
        daemon.select_theme().unwrap_err().code(),
        ErrorCode::InvalidState
    );
    assert_eq!(
        daemon.resolve_config().unwrap_err().code(),
        ErrorCode::InvalidState
    );
    daemon.open_state(&home.paths).unwrap();
    assert_eq!(
        daemon.discover_capabilities().unwrap_err().code(),
        ErrorCode::InvalidState
    );
    assert_eq!(
        daemon.open_state(&home.paths).unwrap_err().code(),
        ErrorCode::InvalidState
    );
}

#[test]
fn the_daemon_is_the_only_writer_while_it_runs_and_shutdown_releases_it() {
    let home = Home::new();
    let mut daemon = Daemon::headless();
    boot(&mut daemon, &home.paths);
    // A second installer (any other writer) is refused while the daemon
    // holds the installer lock.
    let busy = Installer::open(&home.paths).unwrap_err();
    assert_eq!(busy.code().as_str(), "GLINT_INSTALL_BUSY");

    daemon.shutdown().unwrap();
    // Shutdown released the lock and closed the database.
    drop(Installer::open(&home.paths).unwrap());
    // A second shutdown is a no-op.
    assert!(!daemon.shutdown().unwrap().state_closed);
    assert_eq!(
        daemon.select_theme().unwrap_err().code(),
        ErrorCode::InvalidState
    );
}

#[test]
fn shutdown_mirrors_the_active_theme_record() {
    let home = Home::new();
    let digest = home.install("fixture.optional", false, "default");
    fs::remove_file(home.paths.active_record_file()).unwrap();
    let mut daemon = Daemon::new(panel_adapter());
    boot(&mut daemon, &home.paths);
    let flush = daemon.shutdown().unwrap();
    assert_eq!(flush.active.unwrap().package_digest, digest);
    let record = fs::read_to_string(home.paths.active_record_file()).unwrap();
    assert!(record.contains(&digest));
}

fn booted(home: &Home) -> Daemon {
    let mut daemon = Daemon::headless();
    daemon.open_state(&home.paths).unwrap();
    daemon.resolve_config().unwrap();
    daemon.discover_capabilities().unwrap();
    daemon.select_theme().unwrap();
    daemon
}

/// The embedded default theme archive is exactly what the deterministic
/// packer writes for `themes/default`, as the CLI's is (CLI-006).
#[test]
fn bundled_default_archive_matches_a_fresh_pack() {
    let theme = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes/default");
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("default.glint");
    let digest = glint_package::pack_directory(&theme, &out).unwrap();
    assert_eq!(
        fs::read(&out).unwrap(),
        glint_daemon::BUNDLED_DEFAULT_THEME.archive()
    );
    assert_eq!(
        digest.sha256_hex(),
        glint_daemon::BUNDLED_DEFAULT_THEME.digest()
    );
}

/// Theme operations run on the daemon's own installer once it serves
/// (CLI-006): import, list, use, export, remove and safe mode, with typed
/// failures that change nothing.
#[test]
fn theme_operations_run_on_the_daemons_installer() {
    use glint_daemon::{ThemeOperation, ThemeOutcome};

    let home = Home::new();
    let mut fresh = Daemon::headless();
    let early = fresh
        .theme_operation(ThemeOperation::List)
        .expect_err("not serving yet");
    assert_eq!(early.code(), ErrorCode::InvalidState);

    let mut daemon = booted(&home);
    let dir = home.tmp.path().join("src-dev.example.ops");
    write_theme(&dir, "dev.example.ops", false);
    let ThemeOutcome::Imported { imported, active } = daemon
        .theme_operation(ThemeOperation::Import { path: dir.clone() })
        .unwrap()
    else {
        panic!("import outcome");
    };
    assert!(!imported.already_installed);
    assert_eq!(active, None, "imports never activate");

    let relative = daemon
        .theme_operation(ThemeOperation::Import {
            path: "relative/theme".into(),
        })
        .unwrap_err();
    assert_eq!(relative.code(), ErrorCode::InvalidRequest);

    let ThemeOutcome::Activated {
        activated,
        safe_mode,
    } = daemon
        .theme_operation(ThemeOperation::Use {
            theme: "dev.example.ops".into(),
        })
        .unwrap()
    else {
        panic!("use outcome");
    };
    assert!(!safe_mode);
    assert_eq!(activated.package.id, "dev.example.ops");

    let in_use = daemon
        .theme_operation(ThemeOperation::Remove {
            theme: "dev.example.ops".into(),
        })
        .unwrap_err();
    assert_eq!(in_use.code(), ErrorCode::OperationFailed);
    assert_eq!(
        in_use.install_error().unwrap().code().as_str(),
        "GLINT_INSTALL_IN_USE"
    );

    let missing = daemon
        .theme_operation(ThemeOperation::Use {
            theme: "dev.example.missing".into(),
        })
        .unwrap_err();
    assert_eq!(
        missing.install_error().unwrap().code().as_str(),
        "GLINT_INSTALL_NOT_INSTALLED"
    );

    let out = home.tmp.path().join("ops.glint");
    let ThemeOutcome::Exported { exported } = daemon
        .theme_operation(ThemeOperation::Export {
            theme: "dev.example.ops".into(),
            destination: out.clone(),
        })
        .unwrap()
    else {
        panic!("export outcome");
    };
    assert!(out.is_file());
    assert!(exported.is_canonical());

    let ThemeOutcome::Activated { safe_mode, .. } =
        daemon.theme_operation(ThemeOperation::SafeMode).unwrap()
    else {
        panic!("safe mode outcome");
    };
    assert!(safe_mode);
    let ThemeOutcome::Listed { packages, active } =
        daemon.theme_operation(ThemeOperation::List).unwrap()
    else {
        panic!("list outcome");
    };
    assert_eq!(packages.len(), 2);
    assert_eq!(active.unwrap().profile_id, "safe-mode");

    // The previously active release is no longer used by the safe-mode
    // profile but still by `default`.
    daemon.shutdown().unwrap();
    let stopped = daemon.theme_operation(ThemeOperation::List).unwrap_err();
    assert_eq!(stopped.code(), ErrorCode::InvalidState);
}
