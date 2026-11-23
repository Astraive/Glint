//! Transaction tests (INS-003): double install is idempotent; an injected
//! crash before commit leaves the previous state; `recover_pending` cleans
//! staging; removal is refused while a profile references the package.
//!
//! A "crash" is a fault injector stopping the operation at a named point
//! with no cleanup; the test then drops the installer (releasing its lock),
//! reopens it (which runs `recover_pending`) and inspects the result.
//!
//! Run with `cargo test -p glint-installer --test transaction`.

mod support;

use std::fs;

use glint_installer::{ErrorCode, FaultPoint, InstallOptions, Installer};
use support::Home;

const ID: &str = "dev.example.midnight";

fn crash_at(target: FaultPoint) -> impl FnMut(FaultPoint) -> bool + Send {
    move |point| point == target
}

#[test]
fn install_moves_the_package_into_an_immutable_content_addressed_entry() {
    let home = Home::new();
    let mut installer = home.open();
    let staged = home.stage(&installer, ID, "1.0.0");
    let digest = staged.candidate.sha256_hex();
    let installed = installer.install(staged.candidate, staged.origin).unwrap();

    assert_eq!(installed.digest, digest);
    assert_eq!(installed.id, ID);
    assert_eq!(installed.version, "1.0.0");
    assert_eq!(installed.kind, "theme");
    assert_eq!(installed.source_kind, "local-archive");
    assert_eq!(
        installed.path,
        home.paths.store_dir().join(&digest).join("root")
    );
    assert!(installed.path.join("glint.json").is_file());
    assert!(installed.archive_path.is_file());
    assert!(
        fs::metadata(installed.path.join("glint.json"))
            .unwrap()
            .permissions()
            .readonly()
    );
    assert_eq!(home.store_entries(), vec![digest.clone()]);
    assert!(
        home.staging_entries().is_empty(),
        "staging was moved, not copied"
    );
    assert_eq!(installer.installed().unwrap(), vec![installed]);
}

#[test]
fn double_install_is_idempotent() {
    let home = Home::new();
    let mut installer = home.open();
    let first = home.stage(&installer, ID, "1.0.0");
    let first = installer.install(first.candidate, first.origin).unwrap();
    let second = home.stage(&installer, ID, "1.0.0");
    let second = installer.install(second.candidate, second.origin).unwrap();

    assert_eq!(first, second);
    assert_eq!(installer.installed().unwrap().len(), 1);
    assert_eq!(home.store_entries(), vec![first.digest.clone()]);
    assert!(
        home.staging_entries().is_empty(),
        "duplicate staging discarded"
    );
}

#[test]
fn install_does_not_activate_unless_requested() {
    let home = Home::new();
    let mut installer = home.open();
    let staged = home.stage(&installer, ID, "1.0.0");
    installer.install(staged.candidate, staged.origin).unwrap();
    assert_eq!(installer.active_theme().unwrap(), None);

    let staged = home.stage(&installer, ID, "1.1.0");
    let installed = installer
        .install_with(
            staged.candidate,
            staged.origin,
            &InstallOptions::activate("default"),
        )
        .unwrap();
    let active = installer.active_theme().unwrap().unwrap();
    assert_eq!(active.profile_id, "default");
    assert_eq!(active.package_digest, installed.digest);
}

#[test]
fn installed_packages_survive_without_their_sources() {
    let home = Home::new();
    let mut installer = home.open();
    let staged = home.stage(&installer, ID, "1.0.0");
    let installed = installer.install(staged.candidate, staged.origin).unwrap();
    drop(installer);
    fs::remove_dir_all(&home.sources).unwrap();

    let installer = home.open();
    assert!(installer.startup_recovery().is_clean());
    let package = installer.package(&installed.digest).unwrap().unwrap();
    assert!(
        fs::read_to_string(package.path.join("glint.json"))
            .unwrap()
            .contains(ID)
    );
}

/// Install A (active), then crash while installing B at `point`.
fn interrupted_install(point: FaultPoint) -> (Home, String, String) {
    let home = Home::new();
    let mut installer = home.open();
    let a = home.stage(&installer, ID, "1.0.0");
    let a = installer
        .install_with(a.candidate, a.origin, &InstallOptions::activate("default"))
        .unwrap();
    let b = home.stage(&installer, ID, "2.0.0");
    let b_digest = b.candidate.sha256_hex();
    installer.set_fault_injector(crash_at(point));
    let error = installer
        .install_with(b.candidate, b.origin, &InstallOptions::activate("default"))
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::Interrupted);
    drop(installer);
    (home, a.digest, b_digest)
}

#[test]
fn a_crash_before_commit_leaves_the_previous_state_at_every_point() {
    for point in [
        FaultPoint::InstallAfterJournal,
        FaultPoint::InstallAfterMove,
        FaultPoint::InstallBeforeCommit,
    ] {
        let (home, a, b) = interrupted_install(point);
        let installer = home.open();
        let report = installer.startup_recovery();
        assert_eq!(report.rolled_back_installs, vec![b.clone()], "{point:?}");

        let active = installer.active_theme().unwrap().unwrap();
        assert_eq!(active.package_digest, a, "{point:?}: active theme kept");
        assert_eq!(active.revision, 1, "{point:?}");
        assert!(installer.package(&b).unwrap().is_none(), "{point:?}");
        assert_eq!(home.store_entries(), vec![a.clone()], "{point:?}");
        assert!(home.staging_entries().is_empty(), "{point:?}");
        assert!(installer.package(&a).unwrap().unwrap().path.is_dir());
    }
}

#[test]
fn a_crash_after_commit_keeps_the_new_state() {
    let (home, _a, b) = interrupted_install(FaultPoint::InstallAfterCommit);
    let installer = home.open();
    assert!(installer.startup_recovery().rolled_back_installs.is_empty());
    assert_eq!(installer.active_theme().unwrap().unwrap().package_digest, b);
    assert!(installer.package(&b).unwrap().is_some());
}

#[test]
fn reinstalling_after_a_rolled_back_crash_succeeds() {
    let (home, _a, b) = interrupted_install(FaultPoint::InstallAfterMove);
    let mut installer = home.open();
    let staged = home.stage(&installer, ID, "2.0.0");
    let installed = installer.install(staged.candidate, staged.origin).unwrap();
    assert_eq!(installed.digest, b);
}

#[test]
fn recover_pending_cleans_leftover_staging_and_trash() {
    let home = Home::new();
    let installer = home.open();
    // A staged candidate abandoned by a crashed process.
    let staged = home.stage(&installer, ID, "1.0.0");
    let mut candidate = staged.candidate;
    candidate.keep_staging();
    drop(candidate);
    fs::create_dir(home.paths.trash_dir().join("leftover")).unwrap();
    assert_eq!(home.staging_entries().len(), 1);
    drop(installer);

    let installer = home.open();
    let report = installer.startup_recovery();
    assert_eq!(report.cleaned_staging, 1);
    assert_eq!(report.cleaned_trash, 1);
    assert!(home.staging_entries().is_empty());
    assert!(home.trash_entries().is_empty());
}

#[test]
fn removal_is_refused_while_a_profile_references_the_package() {
    let home = Home::new();
    let mut installer = home.open();
    let a = home.stage(&installer, ID, "1.0.0");
    let a = installer
        .install_with(a.candidate, a.origin, &InstallOptions::activate("default"))
        .unwrap();
    let b = home.stage(&installer, ID, "2.0.0");
    let b = installer.install(b.candidate, b.origin).unwrap();
    installer.pin("work", &b.digest).unwrap();

    for digest in [&a.digest, &b.digest] {
        let error = installer.remove(digest).unwrap_err();
        assert_eq!(error.code(), ErrorCode::InUse, "{error}");
        assert!(installer.package(digest).unwrap().is_some());
    }
    assert_eq!(home.store_entries().len(), 2);
}

#[test]
fn an_unreferenced_package_is_removed() {
    let home = Home::new();
    let mut installer = home.open();
    let a = home.stage(&installer, ID, "1.0.0");
    let a = installer.install(a.candidate, a.origin).unwrap();
    installer.pin("work", &a.digest).unwrap();
    installer.unpin("work", &a.digest).unwrap();
    // The profile still names it as base theme.
    assert_eq!(
        installer.remove(&a.digest).unwrap_err().code(),
        ErrorCode::InUse
    );

    let b = home.stage(&installer, ID, "2.0.0");
    let b = installer.install(b.candidate, b.origin).unwrap();
    installer.remove(&b.digest).unwrap();
    assert!(installer.package(&b.digest).unwrap().is_none());
    assert_eq!(home.store_entries(), vec![a.digest]);
    assert!(home.trash_entries().is_empty());
    assert_eq!(
        installer.remove(&b.digest).unwrap_err().code(),
        ErrorCode::NotInstalled
    );
}

#[test]
fn a_crash_during_removal_is_undone_before_commit_and_finished_after() {
    for (point, survives) in [
        (FaultPoint::RemoveAfterJournal, true),
        (FaultPoint::RemoveAfterMove, true),
        (FaultPoint::RemoveBeforeCommit, true),
        (FaultPoint::RemoveAfterCommit, false),
    ] {
        let home = Home::new();
        let mut installer = home.open();
        let staged = home.stage(&installer, ID, "1.0.0");
        let package = installer.install(staged.candidate, staged.origin).unwrap();
        installer.set_fault_injector(crash_at(point));
        assert_eq!(
            installer.remove(&package.digest).unwrap_err().code(),
            ErrorCode::Interrupted
        );
        drop(installer);

        let installer = home.open();
        let report = installer.startup_recovery();
        let present = installer.package(&package.digest).unwrap();
        assert_eq!(present.is_some(), survives, "{point:?}");
        if survives {
            assert_eq!(report.restored_removals, vec![package.digest.clone()]);
            assert!(package.path.join("glint.json").is_file(), "{point:?}");
            assert_eq!(home.store_entries(), vec![package.digest.clone()]);
        } else {
            assert!(home.store_entries().is_empty(), "{point:?}");
        }
        assert!(home.trash_entries().is_empty(), "{point:?}");
    }
}

#[test]
fn candidates_staged_elsewhere_are_refused() {
    let home = Home::new();
    let mut installer = home.open();
    let elsewhere = home.tmp.path().join("other-staging");
    fs::create_dir(&elsewhere).unwrap();
    let source = glint_sources::ThemeSource::LocalArchive {
        path: home.archive(ID, "1.0.0"),
    };
    let staged = glint_sources::PackageSource::stage(
        &glint_sources::LocalSource::new(),
        &source,
        &elsewhere,
    )
    .unwrap();
    let error = installer
        .install(staged.candidate, staged.origin)
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::CandidateOutsideStaging);
    assert!(home.store_entries().is_empty());
}

#[test]
fn provenance_for_another_package_is_refused() {
    let home = Home::new();
    let mut installer = home.open();
    let a = home.stage(&installer, ID, "1.0.0");
    let b = home.stage(&installer, "dev.example.other", "1.0.0");
    let error = installer.install(a.candidate, b.origin).unwrap_err();
    assert_eq!(error.code(), ErrorCode::OriginMismatch);
    assert!(home.store_entries().is_empty());
}

#[test]
fn a_second_installer_is_refused_while_the_first_holds_the_lock() {
    let home = Home::new();
    let _first = home.open();
    let error = Installer::open(&home.paths).unwrap_err();
    assert_eq!(error.code(), ErrorCode::Busy);
}

#[test]
fn a_corrupt_database_is_rebuilt_from_the_store_keeping_the_active_theme() {
    let home = Home::new();
    let mut installer = home.open();
    let a = home.stage(&installer, ID, "1.0.0");
    let a = installer
        .install_with(a.candidate, a.origin, &InstallOptions::activate("default"))
        .unwrap();
    let b = home.stage(&installer, ID, "2.0.0");
    let b = installer.install(b.candidate, b.origin).unwrap();
    drop(installer);
    fs::write(home.paths.database_file(), b"not a database at all, sorry").unwrap();

    let installer = home.open();
    let report = installer.startup_recovery();
    assert!(report.database_recovered);
    let mut reindexed = report.reindexed.clone();
    reindexed.sort();
    let mut expected = vec![a.digest.clone(), b.digest.clone()];
    expected.sort();
    assert_eq!(reindexed, expected);
    assert_eq!(
        installer.package(&a.digest).unwrap().unwrap().version,
        "1.0.0"
    );
    assert_eq!(
        installer.active_theme().unwrap().unwrap().package_digest,
        a.digest
    );
}

#[test]
fn a_tampered_store_entry_is_not_reindexed() {
    let home = Home::new();
    let mut installer = home.open();
    let a = home.stage(&installer, ID, "1.0.0");
    let a = installer.install(a.candidate, a.origin).unwrap();
    drop(installer);
    // Replace the archive bytes; the entry no longer matches its address.
    let archive = &a.archive_path;
    let mut permissions = fs::metadata(archive).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    fs::set_permissions(archive, permissions).unwrap();
    fs::write(archive, b"tampered").unwrap();
    fs::write(home.paths.database_file(), b"garbage garbage garbage").unwrap();

    let installer = home.open();
    let report = installer.startup_recovery();
    assert!(report.reindexed.is_empty());
    assert_eq!(report.discarded.len(), 1);
    assert!(installer.installed().unwrap().is_empty());
    assert!(home.store_entries().is_empty());
}

/// Threat model B2.6: store entries whose paths exceed Windows `MAX_PATH`
/// (260) install and remove. The root is chosen so the database path stays
/// below 260: bundled SQLite's Windows VFS cannot open a longer database
/// path (`SQLITE_CANTOPEN`), a limit recorded in the threat model.
#[test]
fn installs_store_entries_longer_than_max_path() {
    let segment = "long-directory-name-for-max-path-coverage-xxxxxx";
    let name = [segment; 3].join(std::path::MAIN_SEPARATOR_STR);
    let home = Home::with_root_name(&name);
    assert!(home.paths.database_file().as_os_str().len() < 260);
    let mut installer = home.open();
    let staged = home.stage(&installer, ID, "1.0.0");
    let package = installer.install(staged.candidate, staged.origin).unwrap();
    let deepest = package.path.join("dist").join("panel").join("main.js");
    assert!(deepest.as_os_str().len() > 260, "{}", deepest.display());
    assert!(deepest.is_file());
    installer.remove(&package.digest).unwrap();
    assert!(home.store_entries().is_empty());
}
