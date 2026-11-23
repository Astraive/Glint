//! Origin collision (INS-004): provenance is recorded for every install,
//! and a second origin with the same package ID is refused per ADR 0006
//! (*Proposed*) with the original left intact.
//!
//! The ADR's test table is reproduced against the real installed inventory
//! (row numbers in the test names). Only local sources can be staged today,
//! so "installed from Git" is simulated by rewriting the recorded origin of
//! an installed release in the state database; the classification then runs
//! against that inventory exactly as it would for a real Git install.
//!
//! Run with `cargo test -p glint-installer --test provenance`.

mod support;

use glint_installer::{ErrorCode, InstalledPackage, Installer, package_origin};
use support::Home;

const ID: &str = "dev.example.a";
const GIT_ORIGIN: &str = "git:https://host.example/u/r";

/// Pretend `digest` was installed from `origin`.
fn set_origin(installer: &Installer, digest: &str, origin: &str) {
    installer
        .storage()
        .connection()
        .execute(
            "UPDATE packages SET origin = ?1, source_kind = 'git' WHERE digest = ?2",
            [origin, digest],
        )
        .unwrap();
}

/// The installed inventory, store and staging, for "unchanged" checks.
fn snapshot(home: &Home, installer: &Installer) -> (Vec<InstalledPackage>, Vec<String>) {
    assert!(home.staging_entries().is_empty(), "refused staging cleaned");
    (installer.installed().unwrap(), home.store_entries())
}

fn identity_code(error: &glint_installer::InstallError) -> &'static str {
    assert_eq!(error.code(), ErrorCode::IdentityConflict, "{error}");
    error.identity_outcome().unwrap().code()
}

#[test]
fn provenance_is_recorded_and_survives_reopening() {
    let home = Home::new();
    let mut installer = home.open();
    let staged = home.stage(&installer, ID, "1.0.0");
    let expected_path = staged.origin.local_path.clone().unwrap();
    let installed = installer.install(staged.candidate, staged.origin).unwrap();

    assert_eq!(installed.origin, "local");
    assert_eq!(installed.source_kind, "local-archive");
    assert_eq!(
        installed.local_path.as_deref(),
        Some(expected_path.to_string_lossy().as_ref())
    );
    assert_eq!(installed.registry, None);
    assert_eq!(installed.git_commit, None);
    assert_eq!(installed.signing, "unsigned");
    assert!(installed.retrieved_at > 0);
    assert_eq!(installed.id, ID);
    assert_eq!(installed.version, "1.0.0");
    assert_eq!(installed.digest.len(), 64);
    assert!(installed.path.starts_with(home.paths.store_dir()));
    drop(installer);

    let installer = home.open();
    assert_eq!(
        installer.package(&installed.digest).unwrap().unwrap(),
        installed
    );
}

#[test]
fn provenance_is_rebuilt_from_the_store_after_database_loss() {
    let home = Home::new();
    let mut installer = home.open();
    let staged = home.stage(&installer, ID, "1.0.0");
    let installed = installer.install(staged.candidate, staged.origin).unwrap();
    drop(installer);
    std::fs::write(home.paths.database_file(), b"garbage garbage garbage").unwrap();

    let installer = home.open();
    assert_eq!(
        installer.startup_recovery().reindexed,
        vec![installed.digest.clone()]
    );
    assert_eq!(
        installer.package(&installed.digest).unwrap().unwrap(),
        installed
    );
}

#[test]
fn row_01_and_13_a_new_key_installs_whatever_else_is_installed() {
    let home = Home::new();
    let mut installer = home.open();
    let b = home.stage(&installer, "dev.example.b", "1.0.0");
    let b = installer.install(b.candidate, b.origin).unwrap();
    set_origin(&installer, &b.digest, GIT_ORIGIN);
    let a = home.stage(&installer, ID, "1.0.0");
    installer.install(a.candidate, a.origin).unwrap();
    assert_eq!(installer.installed().unwrap().len(), 2);
}

#[test]
fn row_02_the_same_release_again_is_idempotent() {
    let home = Home::new();
    let mut installer = home.open();
    let first = home.stage(&installer, ID, "1.0.0");
    let first = installer.install(first.candidate, first.origin).unwrap();
    let again = home.stage(&installer, ID, "1.0.0");
    let again = installer.install(again.candidate, again.origin).unwrap();
    assert_eq!(first, again);
    assert_eq!(installer.installed().unwrap().len(), 1);
}

#[test]
fn row_08_another_local_version_is_added_and_the_old_one_kept() {
    let home = Home::new();
    let mut installer = home.open();
    let old = home.stage(&installer, ID, "1.0.0");
    let old = installer.install(old.candidate, old.origin).unwrap();
    let new = home.stage(&installer, ID, "1.1.0");
    installer.install(new.candidate, new.origin).unwrap();
    let versions: Vec<_> = installer
        .installed()
        .unwrap()
        .into_iter()
        .map(|p| p.version)
        .collect();
    assert_eq!(versions, vec!["1.0.0", "1.1.0"]);
    assert!(installer.package(&old.digest).unwrap().is_some());
}

#[test]
fn row_04_same_origin_and_version_with_other_bytes_is_refused() {
    let home = Home::new();
    let mut installer = home.open();
    let original = home.stage(&installer, ID, "1.0.0");
    installer
        .install(original.candidate, original.origin)
        .unwrap();
    let before = snapshot(&home, &installer);

    let changed = home.stage_path(&installer, home.archive_variant(ID, "1.0.0", "republished"));
    let error = installer
        .install(changed.candidate, changed.origin)
        .unwrap_err();
    assert_eq!(identity_code(&error), "GLINT_IDENTITY_DIGEST_CONFLICT");
    assert_eq!(snapshot(&home, &installer), before, "original intact");
}

#[test]
fn rows_05_and_07_the_same_key_from_another_origin_is_refused() {
    let home = Home::new();
    let mut installer = home.open();
    let from_git = home.stage(&installer, ID, "1.0.0");
    let from_git = installer
        .install(from_git.candidate, from_git.origin)
        .unwrap();
    set_origin(&installer, &from_git.digest, GIT_ORIGIN);
    let before = snapshot(&home, &installer);

    for version in ["2.0.0", "1.0.1"] {
        let local = home.stage(&installer, ID, version);
        let error = installer
            .install(local.candidate, local.origin)
            .unwrap_err();
        assert_eq!(identity_code(&error), "GLINT_IDENTITY_ORIGIN_COLLISION");
        assert!(error.to_string().contains(GIT_ORIGIN), "{error}");
        assert_eq!(snapshot(&home, &installer), before, "original intact");
    }
    let kept = installer.package(&from_git.digest).unwrap().unwrap();
    assert_eq!(kept.origin, GIT_ORIGIN);
}

#[test]
fn row_06_equal_bytes_from_another_origin_are_not_a_trusted_duplicate() {
    let home = Home::new();
    let mut installer = home.open();
    let from_git = home.stage(&installer, ID, "1.0.0");
    let from_git = installer
        .install(from_git.candidate, from_git.origin)
        .unwrap();
    set_origin(&installer, &from_git.digest, GIT_ORIGIN);
    let before = snapshot(&home, &installer);

    // The very same archive, now imported locally: same digest.
    let local = home.stage(&installer, ID, "1.0.0");
    assert_eq!(local.candidate.sha256_hex(), from_git.digest);
    let error = installer
        .install(local.candidate, local.origin)
        .unwrap_err();
    assert_eq!(identity_code(&error), "GLINT_IDENTITY_ORIGIN_COLLISION");
    assert_eq!(snapshot(&home, &installer), before);
    assert_eq!(
        installer.package(&from_git.digest).unwrap().unwrap().origin,
        GIT_ORIGIN,
        "provenance not overwritten"
    );
}

#[test]
fn a_refused_collision_does_not_activate() {
    let home = Home::new();
    let mut installer = home.open();
    let first = home.stage(&installer, ID, "1.0.0");
    let first = installer
        .install_with(
            first.candidate,
            first.origin,
            &glint_installer::InstallOptions::activate("default"),
        )
        .unwrap();
    set_origin(&installer, &first.digest, GIT_ORIGIN);
    let local = home.stage(&installer, ID, "2.0.0");
    installer
        .install_with(
            local.candidate,
            local.origin,
            &glint_installer::InstallOptions::activate("default"),
        )
        .unwrap_err();
    let active = installer.active_theme().unwrap().unwrap();
    assert_eq!(active.package_digest, first.digest);
    assert_eq!(active.revision, 1);
}

#[test]
fn local_sources_map_to_the_shared_local_origin() {
    let home = Home::new();
    let installer = home.open();
    let staged = home.stage(&installer, ID, "1.0.0");
    assert_eq!(package_origin(&staged.origin).unwrap().as_str(), "local");
}
