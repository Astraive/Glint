//! Bundled default theme, reserved ID, lookup and safe mode (CLI-002).
//!
//! Run with `cargo test -p glint-installer --test bundled`.

mod support;

use std::fs;
use std::path::Path;

use glint_installer::{
    BundledTheme, DEFAULT_PROFILE, DEFAULT_THEME_ID, ErrorCode, SAFE_MODE_PROFILE, is_bundled,
};
use support::Home;

/// Pack the repository's default theme as the front end's build script does.
fn bundled(home: &Home) -> BundledTheme {
    let theme = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes/default");
    let out = home.sources.join("default.glint");
    glint_package::pack_directory(&theme, &out).unwrap();
    BundledTheme::new(Box::leak(fs::read(&out).unwrap().into_boxed_slice()))
}

#[test]
fn safe_mode_installs_repairs_and_activates_the_bundled_theme() {
    let home = Home::new();
    let theme = bundled(&home);
    let mut installer = home.open();

    let first = installer.safe_mode(&theme).unwrap();
    assert_eq!(first.package.id, DEFAULT_THEME_ID);
    assert_eq!(first.package.digest, theme.digest());
    assert!(is_bundled(&first.package));
    assert_eq!(first.package.local_path, None);
    assert_eq!(first.active.profile_id, SAFE_MODE_PROFILE);
    assert_eq!(first.previous, None);

    // Damage the store entry; ensure_bundled repairs it from the bytes.
    let entry = home.paths.store_dir().join(theme.digest());
    let archive = entry.join("source.glint");
    let mut permissions = fs::metadata(&archive).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    fs::set_permissions(&archive, permissions).unwrap();
    fs::write(&archive, b"damaged").unwrap();
    let repaired = installer.ensure_bundled(&theme).unwrap();
    assert_eq!(repaired.digest, theme.digest());
    assert_eq!(
        glint_package::inspect_archive_in(
            &archive,
            &glint_package::PackageLimits::default(),
            &home.sources
        )
        .unwrap()
        .sha256_hex(),
        theme.digest()
    );
    assert_eq!(installer.installed().unwrap().len(), 1);
    // The safe-mode profile still points at the restored digest.
    assert_eq!(
        installer.active_theme().unwrap().unwrap().package_digest,
        theme.digest()
    );
}

#[test]
fn use_theme_targets_the_user_profile_not_safe_mode() {
    let home = Home::new();
    let theme = bundled(&home);
    let mut installer = home.open();
    let staged = home.stage(&installer, "dev.example.a", "1.0.0");
    let imported = installer
        .import(staged.candidate, staged.origin, &theme)
        .unwrap();
    assert!(!imported.already_installed);
    assert_eq!(
        installer.active_theme().unwrap(),
        None,
        "import never activates"
    );

    installer.safe_mode(&theme).unwrap();
    let used = installer.use_theme(&imported.package.digest).unwrap();
    assert_eq!(used.active.profile_id, DEFAULT_PROFILE);
    assert_eq!(used.previous.unwrap().profile_id, SAFE_MODE_PROFILE);
}

#[test]
fn reserved_id_is_refused_unless_the_bytes_are_bundled() {
    let home = Home::new();
    let theme = bundled(&home);
    let mut installer = home.open();
    let staged = home.stage(&installer, DEFAULT_THEME_ID, "0.1.0");
    let error = installer
        .import(staged.candidate, staged.origin, &theme)
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::ReservedId);
    assert!(installer.installed().unwrap().is_empty());
    assert!(home.staging_entries().is_empty());

    // The bundled bytes themselves import as the bundled release.
    let staged = home.stage_path(&installer, home.sources.join("default.glint"));
    let imported = installer
        .import(staged.candidate, staged.origin, &theme)
        .unwrap();
    assert!(is_bundled(&imported.package));
}

#[test]
fn find_resolves_ids_versions_digests_and_the_default_alias() {
    let home = Home::new();
    let theme = bundled(&home);
    let mut installer = home.open();
    let mut digests = Vec::new();
    for version in ["1.0.0", "2.0.0"] {
        let staged = home.stage(&installer, "dev.example.m", version);
        digests.push(
            installer
                .install(staged.candidate, staged.origin)
                .unwrap()
                .digest,
        );
    }
    let error = installer.find("dev.example.m").unwrap_err();
    assert_eq!(error.code(), ErrorCode::Ambiguous);
    assert_eq!(
        installer.find("dev.example.m@2.0.0").unwrap().digest,
        digests[1]
    );
    assert_eq!(installer.find(&digests[0]).unwrap().version, "1.0.0");
    assert_eq!(
        installer
            .find(&format!("sha256:{}", digests[1]))
            .unwrap()
            .version,
        "2.0.0"
    );
    assert_eq!(
        installer.find("default").unwrap_err().code(),
        ErrorCode::NotInstalled
    );
    installer.ensure_bundled(&theme).unwrap();
    assert_eq!(installer.find("default").unwrap().id, DEFAULT_THEME_ID);
    assert_eq!(
        installer.find("dev.example.none").unwrap_err().code(),
        ErrorCode::NotInstalled
    );
}
