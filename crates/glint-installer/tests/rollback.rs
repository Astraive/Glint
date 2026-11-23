//! Interrupted apply (INS-005): an injected interruption leaves the
//! previous theme active; partial native restore is reported as
//! rollback-incomplete; an incomplete journal is detected at startup and
//! recovered.
//!
//! Native adapters and surfaces are mocks. The mock "OS" state lives in an
//! `Arc<Mutex<..>>` shared across simulated restarts, as real OS settings
//! outlive a crashed Glint.
//!
//! Run with `cargo test -p glint-installer --test rollback`.

mod support;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use glint_installer::{
    ApplyErrorCode, ApplyPlan, ErrorCode, FaultPoint, InstallOptions, NativeAdapter, NativeChange,
    NativeError, NativeErrorKind, RestoreSemantics, SettingRef, SurfaceHost, UnrestoredReason,
};
use support::Home;

const ID: &str = "dev.example.midnight";

type OsState = Arc<Mutex<BTreeMap<String, String>>>;

fn os_with(values: &[(&str, &str)]) -> OsState {
    Arc::new(Mutex::new(
        values
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
    ))
}

fn read(os: &OsState, key: &str) -> String {
    os.lock().unwrap().get(key).cloned().unwrap_or_default()
}

struct MockAdapter {
    id: String,
    os: OsState,
    semantics: RestoreSemantics,
    fail_apply: Option<String>,
    fail_restore: bool,
}

impl MockAdapter {
    fn new(id: &str, os: &OsState) -> Self {
        Self {
            id: id.to_owned(),
            os: os.clone(),
            semantics: RestoreSemantics::REVERSIBLE,
            fail_apply: None,
            fail_restore: false,
        }
    }

    fn key(&self, setting: &str) -> String {
        format!("{}/{setting}", self.id)
    }
}

impl NativeAdapter for MockAdapter {
    fn id(&self) -> &str {
        &self.id
    }

    fn semantics(&self) -> RestoreSemantics {
        self.semantics
    }

    fn snapshot(&mut self, setting: &str) -> Result<String, NativeError> {
        Ok(read(&self.os, &self.key(setting)))
    }

    fn apply(&mut self, setting: &str, value: &str) -> Result<(), NativeError> {
        if self.fail_apply.as_deref() == Some(setting) {
            // A partial change before failing, as real adapters may do.
            self.os
                .lock()
                .unwrap()
                .insert(self.key(setting), format!("half-{value}"));
            return Err(NativeError::new(NativeErrorKind::Failure, "injected"));
        }
        self.os
            .lock()
            .unwrap()
            .insert(self.key(setting), value.to_owned());
        Ok(())
    }

    fn restore(&mut self, setting: &str, snapshot: &str) -> Result<(), NativeError> {
        if self.fail_restore {
            return Err(NativeError::new(
                NativeErrorKind::PermissionDenied,
                "locked",
            ));
        }
        self.os
            .lock()
            .unwrap()
            .insert(self.key(setting), snapshot.to_owned());
        Ok(())
    }
}

#[derive(Default)]
struct MockSurfaces {
    current: Option<String>,
    staged: Option<String>,
    fail_stage: bool,
}

impl SurfaceHost for MockSurfaces {
    fn stage(&mut self, _profile: &str, digest: &str) -> Result<(), String> {
        if self.fail_stage {
            return Err("renderer exited".into());
        }
        self.staged = Some(digest.to_owned());
        Ok(())
    }

    fn promote(&mut self) {
        self.current = self.staged.take();
    }

    fn discard(&mut self) {
        self.staged = None;
    }
}

const ORIGINAL: &[(&str, &str)] = &[("wallpaper/image", "old.png"), ("cursor/scheme", "classic")];

fn adapters(os: &OsState) -> Vec<Box<dyn NativeAdapter>> {
    vec![
        Box::new(MockAdapter::new("wallpaper", os)),
        Box::new(MockAdapter::new("cursor", os)),
    ]
}

/// A home with release A active and release B installed; returns digests.
fn setup() -> (Home, String, String) {
    let home = Home::new();
    let mut installer = home.open();
    let a = home.stage(&installer, ID, "1.0.0");
    let a = installer
        .install_with(a.candidate, a.origin, &InstallOptions::activate("default"))
        .unwrap();
    let b = home.stage(&installer, ID, "2.0.0");
    let b = installer.install(b.candidate, b.origin).unwrap();
    (home, a.digest, b.digest)
}

fn plan(digest: &str) -> ApplyPlan {
    ApplyPlan::new("default", digest)
        .with_native(NativeChange::new("wallpaper", "image", "new.png"))
        .with_native(NativeChange::new("cursor", "scheme", "neon"))
}

#[test]
fn a_successful_apply_activates_and_changes_native_settings() {
    let (home, a, b) = setup();
    let mut installer = home.open();
    let os = os_with(ORIGINAL);
    let mut surfaces = MockSurfaces {
        current: Some(a.clone()),
        ..MockSurfaces::default()
    };
    let outcome = installer
        .apply(&plan(&b), &mut surfaces, &mut adapters(&os))
        .unwrap();
    assert_eq!(outcome.active.package_digest, b);
    assert_eq!(outcome.previous.unwrap().package_digest, a);
    assert_eq!(read(&os, "wallpaper/image"), "new.png");
    assert_eq!(read(&os, "cursor/scheme"), "neon");
    assert_eq!(surfaces.current.as_deref(), Some(b.as_str()));
    assert!(installer.pending_apply().unwrap().is_none());
}

#[test]
fn a_native_failure_restores_everything_and_keeps_the_previous_theme() {
    let (home, a, b) = setup();
    let mut installer = home.open();
    let os = os_with(ORIGINAL);
    let mut cursor = MockAdapter::new("cursor", &os);
    cursor.fail_apply = Some("scheme".into());
    let mut adapters: Vec<Box<dyn NativeAdapter>> = vec![
        Box::new(MockAdapter::new("wallpaper", &os)),
        Box::new(cursor),
    ];
    let mut surfaces = MockSurfaces {
        current: Some(a.clone()),
        ..MockSurfaces::default()
    };

    let error = installer
        .apply(&plan(&b), &mut surfaces, &mut adapters)
        .unwrap_err();
    assert_eq!(error.code(), ApplyErrorCode::NativeApplyFailed, "{error}");
    let report = error.rollback().unwrap();
    assert!(report.is_complete());
    assert_eq!(report.restored.len(), 2, "the half-applied one too");
    assert_eq!(read(&os, "wallpaper/image"), "old.png");
    assert_eq!(read(&os, "cursor/scheme"), "classic");
    assert_eq!(installer.active_theme().unwrap().unwrap().package_digest, a);
    assert_eq!(surfaces.current.as_deref(), Some(a.as_str()), "old UI kept");
    assert_eq!(surfaces.staged, None, "staged UI discarded");
    assert!(installer.pending_apply().unwrap().is_none());
}

#[test]
fn a_partial_restore_is_reported_as_rollback_incomplete() {
    let (home, a, b) = setup();
    let mut installer = home.open();
    let os = os_with(ORIGINAL);
    let mut wallpaper = MockAdapter::new("wallpaper", &os);
    wallpaper.fail_restore = true;
    let mut cursor = MockAdapter::new("cursor", &os);
    cursor.fail_apply = Some("scheme".into());
    let mut adapters: Vec<Box<dyn NativeAdapter>> = vec![Box::new(wallpaper), Box::new(cursor)];

    let error = installer
        .apply(&plan(&b), &mut MockSurfaces::default(), &mut adapters)
        .unwrap_err();
    assert_eq!(error.code(), ApplyErrorCode::RollbackIncomplete, "{error}");
    let report = error.rollback().unwrap();
    assert_eq!(report.not_restored.len(), 1);
    let unrestored = &report.not_restored[0];
    assert_eq!(
        unrestored.setting,
        SettingRef {
            adapter: "wallpaper".into(),
            setting: "image".into()
        }
    );
    assert!(matches!(
        unrestored.reason,
        UnrestoredReason::RestoreFailed(NativeError {
            kind: NativeErrorKind::PermissionDenied,
            ..
        })
    ));
    assert_eq!(
        read(&os, "wallpaper/image"),
        "new.png",
        "reported, not hidden"
    );
    assert_eq!(read(&os, "cursor/scheme"), "classic");
    assert_eq!(installer.active_theme().unwrap().unwrap().package_digest, a);
}

#[test]
fn irreversible_changes_need_approval_and_are_reported_when_rolled_back() {
    let (home, a, b) = setup();
    let mut installer = home.open();
    let os = os_with(ORIGINAL);
    let mut wallpaper = MockAdapter::new("wallpaper", &os);
    wallpaper.semantics = RestoreSemantics::IRREVERSIBLE;
    let mut cursor = MockAdapter::new("cursor", &os);
    cursor.fail_apply = Some("scheme".into());
    let mut adapters: Vec<Box<dyn NativeAdapter>> = vec![Box::new(wallpaper), Box::new(cursor)];

    let mut plan = plan(&b);
    let error = installer
        .apply(&plan, &mut MockSurfaces::default(), &mut adapters)
        .unwrap_err();
    assert_eq!(error.code(), ApplyErrorCode::IrreversibleNotApproved);
    assert_eq!(read(&os, "wallpaper/image"), "old.png", "nothing changed");

    plan.approve_irreversible = true;
    let error = installer
        .apply(&plan, &mut MockSurfaces::default(), &mut adapters)
        .unwrap_err();
    assert_eq!(error.code(), ApplyErrorCode::RollbackIncomplete);
    let reasons: Vec<_> = error
        .rollback()
        .unwrap()
        .not_restored
        .iter()
        .map(|u| u.reason.clone())
        .collect();
    assert_eq!(reasons, vec![UnrestoredReason::CannotRestore]);
    assert_eq!(installer.active_theme().unwrap().unwrap().package_digest, a);
}

#[test]
fn a_surface_failure_changes_nothing() {
    let (home, a, b) = setup();
    let mut installer = home.open();
    let os = os_with(ORIGINAL);
    let mut surfaces = MockSurfaces {
        current: Some(a.clone()),
        fail_stage: true,
        ..MockSurfaces::default()
    };
    let error = installer
        .apply(&plan(&b), &mut surfaces, &mut adapters(&os))
        .unwrap_err();
    assert_eq!(error.code(), ApplyErrorCode::RendererStartFailed);
    assert_eq!(read(&os, "wallpaper/image"), "old.png");
    assert_eq!(surfaces.current.as_deref(), Some(a.as_str()));
    assert_eq!(installer.active_theme().unwrap().unwrap().package_digest, a);
}

#[test]
fn unknown_adapters_and_uninstalled_targets_are_refused_up_front() {
    let (home, _a, b) = setup();
    let mut installer = home.open();
    let os = os_with(ORIGINAL);
    let plan_unknown =
        ApplyPlan::new("default", &b).with_native(NativeChange::new("taskbar", "x", "y"));
    let error = installer
        .apply(
            &plan_unknown,
            &mut MockSurfaces::default(),
            &mut adapters(&os),
        )
        .unwrap_err();
    assert_eq!(error.code(), ApplyErrorCode::UnknownAdapter);

    let missing = "c".repeat(64);
    let error = installer
        .apply(
            &ApplyPlan::new("default", missing),
            &mut MockSurfaces::default(),
            &mut adapters(&os),
        )
        .unwrap_err();
    assert_eq!(
        error.install_error().map(|e| e.code()),
        Some(ErrorCode::NotInstalled)
    );
}

/// Crash at `point`, restart, check detection, recover, check restoration.
fn crash_and_recover(point: FaultPoint) {
    let (home, a, b) = setup();
    let os = os_with(ORIGINAL);
    {
        let mut installer = home.open();
        installer.set_fault_injector(move |p| p == point);
        let error = installer
            .apply(&plan(&b), &mut MockSurfaces::default(), &mut adapters(&os))
            .unwrap_err();
        assert_eq!(
            error.install_error().map(|e| e.code()),
            Some(ErrorCode::Interrupted),
            "{point:?}"
        );
    }

    let mut installer = home.open();
    let pending = installer
        .startup_recovery()
        .pending_apply
        .clone()
        .unwrap_or_else(|| panic!("{point:?}: interrupted apply detected"));
    assert_eq!(pending.target_digest, b, "{point:?}");
    assert_eq!(pending.prior.as_ref().unwrap().package_digest, a);
    assert_eq!(pending.native.len(), 2);
    // The previous theme is still active before any recovery.
    assert_eq!(
        installer.active_theme().unwrap().unwrap().package_digest,
        a,
        "{point:?}"
    );
    // A new apply is refused until recovery.
    assert_eq!(
        installer
            .apply(&plan(&b), &mut MockSurfaces::default(), &mut adapters(&os))
            .unwrap_err()
            .code(),
        ApplyErrorCode::RecoveryRequired
    );
    // So is removing the release it was switching to.
    assert_eq!(installer.remove(&b).unwrap_err().code(), ErrorCode::InUse);

    let report = installer
        .recover_apply(&mut adapters(&os))
        .unwrap()
        .expect("something to recover");
    assert!(report.is_complete(), "{point:?}: {report:?}");
    assert_eq!(report.active.as_ref().unwrap().package_digest, a);
    assert_eq!(read(&os, "wallpaper/image"), "old.png", "{point:?}");
    assert_eq!(read(&os, "cursor/scheme"), "classic", "{point:?}");
    assert!(installer.pending_apply().unwrap().is_none());
    drop(installer);
    assert!(home.open().startup_recovery().pending_apply.is_none());
}

#[test]
fn an_interruption_at_every_step_leaves_the_previous_theme_active() {
    for point in [
        FaultPoint::ApplyAfterJournal,
        FaultPoint::ApplyAfterSurfaces,
        FaultPoint::ApplyBeforeNative(0),
        FaultPoint::ApplyAfterNative(0),
        FaultPoint::ApplyBeforeNative(1),
        FaultPoint::ApplyAfterNative(1),
        FaultPoint::ApplyBeforeCommit,
    ] {
        crash_and_recover(point);
    }
}

#[test]
fn recovery_after_a_crash_reports_what_cannot_be_restored() {
    let (home, a, b) = setup();
    let os = os_with(ORIGINAL);
    let irreversible = |os: &OsState| -> Vec<Box<dyn NativeAdapter>> {
        let mut wallpaper = MockAdapter::new("wallpaper", os);
        wallpaper.semantics = RestoreSemantics::IRREVERSIBLE;
        vec![
            Box::new(wallpaper),
            Box::new(MockAdapter::new("cursor", os)),
        ]
    };
    {
        let mut installer = home.open();
        installer.set_fault_injector(|p| p == FaultPoint::ApplyBeforeCommit);
        let mut plan = plan(&b);
        plan.approve_irreversible = true;
        installer
            .apply(&plan, &mut MockSurfaces::default(), &mut irreversible(&os))
            .unwrap_err();
    }
    let mut installer = home.open();
    let pending = installer.startup_recovery().pending_apply.clone().unwrap();
    let wallpaper = pending
        .native
        .iter()
        .find(|n| n.setting.adapter == "wallpaper")
        .unwrap();
    assert!(!wallpaper.can_restore && !wallpaper.has_snapshot);
    assert_eq!(pending.touched().count(), 2);

    // The cursor adapter is missing at recovery time too.
    let mut only_wallpaper: Vec<Box<dyn NativeAdapter>> =
        irreversible(&os).into_iter().take(1).collect();
    let report = installer
        .recover_apply(&mut only_wallpaper)
        .unwrap()
        .unwrap();
    let mut reasons: Vec<_> = report
        .not_restored
        .iter()
        .map(|u| (u.setting.adapter.clone(), u.reason.clone()))
        .collect();
    reasons.sort_by(|x, y| x.0.cmp(&y.0));
    assert_eq!(
        reasons,
        vec![
            ("cursor".to_owned(), UnrestoredReason::AdapterMissing),
            ("wallpaper".to_owned(), UnrestoredReason::CannotRestore),
        ]
    );
    assert_eq!(installer.active_theme().unwrap().unwrap().package_digest, a);
}

#[test]
fn a_crash_after_commit_keeps_the_new_theme_and_leaves_nothing_pending() {
    let (home, _a, b) = setup();
    let os = os_with(ORIGINAL);
    {
        let mut installer = home.open();
        installer.set_fault_injector(|p| p == FaultPoint::ApplyAfterCommit);
        installer
            .apply(&plan(&b), &mut MockSurfaces::default(), &mut adapters(&os))
            .unwrap_err();
    }
    let mut installer = home.open();
    assert!(installer.startup_recovery().pending_apply.is_none());
    assert_eq!(installer.active_theme().unwrap().unwrap().package_digest, b);
    assert_eq!(read(&os, "wallpaper/image"), "new.png");
    assert!(
        installer
            .recover_apply(&mut adapters(&os))
            .unwrap()
            .is_none()
    );
}
