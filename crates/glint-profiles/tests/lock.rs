//! Profile locks and missing-package fallback (PRF-002).
//!
//! Run with `cargo test -p glint-profiles --test lock`.

mod common;

use common::*;
use glint_profiles::{
    Fallback, HostEnvironment, Layer, LockCode, LockedThemes, ResolveCode, ResolvedPlan, Severity,
    ThemeInput, ThemeSet, check_lock, generate_lock, resolve,
};
use glint_schema::{
    LockProblemCode, PackageOrigin, PackageRelease, Sha256Digest, parse_profile_lock,
};
use serde_json::json;

fn mix() -> glint_schema::Profile {
    profile(json!({
        "schemaVersion": 1,
        "id": "profile.custom-mix",
        "name": "Custom Mix",
        "baseTheme": { "id": "dev.example.mono", "version": "1.0.0" },
        "surfaces": {
            "dock": { "theme": "dev.example.glassdock", "version": "1.1.0" },
            "notification": { "theme": "dev.example.notifs", "version": "0.4.0" }
        }
    }))
}

fn installed() -> Vec<PackageRelease> {
    vec![
        default_theme().release,
        notifs().release,
        mono().release,
        glassdock().release,
        // Another version of an unrelated release is ignored.
        release("dev.example.mono", "2.0.0"),
    ]
}

fn all_themes() -> ThemeSet {
    ThemeSet::new()
        .with(mono())
        .with(glassdock())
        .with(notifs())
}

fn rereleased(input: &ThemeInput, release: PackageRelease) -> ThemeInput {
    ThemeInput::new(release, input.manifest.clone(), input.tokens.clone()).unwrap()
}

fn fallback_of<'p>(plan: &'p ResolvedPlan, id: &str) -> &'p Fallback {
    plan.surfaces[id]
        .fallback
        .as_ref()
        .unwrap_or_else(|| panic!("{id} did not fall back"))
}

fn assert_fallback(fallback: &Fallback, requested: Layer, theme: &str, reason: &str, to: Layer) {
    assert_eq!(fallback.requested, requested);
    assert_eq!(fallback.theme, theme);
    assert_eq!(fallback.reason, reason);
    assert_eq!(fallback.to, to);
}

fn codes(plan: &ResolvedPlan) -> Vec<(ResolveCode, String, Option<String>)> {
    plan.diagnostics
        .iter()
        .map(|d| (d.code, d.pointer.clone(), d.surface.clone()))
        .collect()
}

// ------------------------------------------------------------ generation

#[test]
fn the_lock_pins_exact_installed_releases_deterministically() {
    let lock = generate_lock(&mix(), &installed()).unwrap();
    assert_eq!(lock.profile, "profile.custom-mix");
    assert_eq!(
        lock.packages,
        vec![glassdock().release, mono().release, notifs().release],
        "sorted by key, exact origin and digest, nothing unreferenced"
    );

    let mut shuffled = installed();
    shuffled.reverse();
    let again = generate_lock(&mix(), &shuffled).unwrap();
    assert_eq!(again.to_json_string(), lock.to_json_string());

    let text = lock.to_json_string();
    assert!(text.contains("\"origin\": \"registry:https://registry.example/glint\""));
    assert!(text.contains(&format!("\"digest\": \"{}\"", mono().release.digest)));
    let reread = parse_profile_lock(&text).unwrap();
    assert_eq!(reread, lock);
    assert!(check_lock(&mix(), &reread, &installed()).is_clean());
}

#[test]
fn a_profile_without_themes_locks_nothing() {
    let p = profile(json!({ "schemaVersion": 1, "id": "default", "name": "Default" }));
    let lock = generate_lock(&p, &installed()).unwrap();
    assert!(lock.packages.is_empty());
}

#[test]
fn generation_refuses_missing_and_ambiguous_references() {
    let mut inventory: Vec<_> = installed()
        .into_iter()
        .filter(|r| r.key.id() != "dev.example.glassdock")
        .collect();
    // The same id and version from two origins: the lock cannot choose.
    inventory.push(PackageRelease::new(
        notifs().release.key.clone(),
        PackageOrigin::local(),
        notifs().release.version.clone(),
        Sha256Digest::from_bytes([9; 32]),
    ));
    let issues = generate_lock(&mix(), &inventory).unwrap_err();
    let found: Vec<_> = issues
        .iter()
        .map(|i| (i.code, i.pointer.as_str()))
        .collect();
    assert_eq!(
        found,
        [
            (LockCode::NotInstalled, "/surfaces/dock"),
            (LockCode::Ambiguous, "/surfaces/notification"),
        ]
    );
}

// ------------------------------------------------------------------ drift

#[test]
fn drift_between_lock_and_installed_releases_is_reported() {
    let lock = generate_lock(&mix(), &installed()).unwrap();
    let mono_release = mono().release;
    let glass = glassdock().release;
    let mut inventory: Vec<_> = installed()
        .into_iter()
        .filter(|r| r.key.id() != "dev.example.notifs" && r != &mono_release && r != &glass)
        .collect();
    // Same id, version and origin, other bytes.
    inventory.push(PackageRelease::new(
        mono_release.key.clone(),
        mono_release.origin.clone(),
        mono_release.version.clone(),
        Sha256Digest::from_bytes([7; 32]),
    ));
    // Same id and version from another origin.
    inventory.push(PackageRelease::new(
        glass.key.clone(),
        PackageOrigin::git("https://git.example/themes", None).unwrap(),
        glass.version.clone(),
        glass.digest.clone(),
    ));
    let report = check_lock(&mix(), &lock, &inventory);
    assert!(report.problems.is_empty());
    let drift: Vec<_> = report
        .drift
        .iter()
        .map(|d| (d.code, d.pointer.as_str(), d.key.id()))
        .collect();
    assert_eq!(
        drift,
        [
            (
                LockCode::OriginMismatch,
                "/packages/0",
                "dev.example.glassdock"
            ),
            (LockCode::DigestMismatch, "/packages/1", "dev.example.mono"),
            (LockCode::NotInstalled, "/packages/2", "dev.example.notifs"),
        ]
    );
    assert!(!report.is_clean());
}

#[test]
fn drift_between_profile_and_lock_is_reported() {
    let lock = generate_lock(&mix(), &installed()).unwrap();
    let moved = profile(json!({
        "schemaVersion": 1,
        "id": "profile.custom-mix",
        "name": "Custom Mix",
        "baseTheme": { "id": "dev.example.mono", "version": "2.0.0" },
        "surfaces": { "dock": { "theme": "dev.example.glassdock", "version": "1.1.0" } }
    }));
    let report = check_lock(&moved, &lock, &installed());
    let codes: Vec<_> = report.problems.iter().map(|p| p.code).collect();
    assert_eq!(
        codes,
        [LockProblemCode::VersionMismatch, LockProblemCode::Unused]
    );
    assert!(report.drift.is_empty());
}

// --------------------------------------------------------------- fallback

#[test]
fn a_missing_override_package_resolves_to_the_default_surface_and_reports_it() {
    // No base theme: the override's only fallback is the bundled default.
    let p = profile(json!({
        "schemaVersion": 1, "id": "work-setup", "name": "Work",
        "surfaces": { "panel": { "theme": "dev.example.glassdock", "version": "1.1.0" } }
    }));
    let plan = resolve(
        &p,
        &ThemeSet::new(),
        &default_theme(),
        &HostEnvironment::new(),
    );
    let panel = &plan.surfaces["panel"];
    assert_eq!(panel.layer, Layer::Default);
    assert_eq!(panel.theme.key.id(), "official.default");
    assert_fallback(
        fallback_of(&plan, "panel"),
        Layer::SurfaceOverride,
        "dev.example.glassdock@1.1.0",
        "missing",
        Layer::Default,
    );
    let panel_ptr = "/surfaces/panel".to_owned();
    assert_eq!(
        codes(&plan),
        [
            (
                ResolveCode::ThemeUnavailable,
                panel_ptr.clone(),
                Some("panel".to_owned())
            ),
            (
                ResolveCode::FallbackToDefault,
                panel_ptr,
                Some("panel".to_owned())
            ),
        ]
    );
    assert_eq!(plan.diagnostics[1].severity, Severity::Warning);
    assert_eq!(
        plan.to_json()["surfaces"]["panel"]["fallback"],
        json!({
            "requested": "surface-override",
            "theme": "dev.example.glassdock@1.1.0",
            "reason": "missing",
            "to": "default"
        })
    );
}

#[test]
fn a_missing_override_falls_back_to_the_base_theme_first() {
    let themes = ThemeSet::new().with(mono()).with(notifs());
    let plan = resolve(&mix(), &themes, &default_theme(), &HostEnvironment::new());
    let fallback = fallback_of(&plan, "dock");
    assert_eq!(fallback.to, Layer::BaseTheme);
    assert_eq!(plan.surfaces["dock"].theme.key.id(), "dev.example.mono");
    assert!(
        plan.diagnostics
            .iter()
            .any(|d| d.code == ResolveCode::FallbackToBase && d.pointer == "/surfaces/dock")
    );
    // Surfaces that resolved as asked carry no fallback.
    assert!(plan.surfaces["notification"].fallback.is_none());
    assert!(plan.surfaces["panel"].fallback.is_none());
}

#[test]
fn a_missing_base_theme_reports_every_surface_and_the_tokens() {
    let themes = ThemeSet::new().with(glassdock()).with(notifs());
    let plan = resolve(&mix(), &themes, &default_theme(), &HostEnvironment::new());
    // The overrides still apply; the base surfaces come from the default.
    assert_eq!(plan.surfaces["dock"].layer, Layer::SurfaceOverride);
    assert_eq!(plan.surfaces["panel"].layer, Layer::Default);
    assert_fallback(
        fallback_of(&plan, "panel"),
        Layer::BaseTheme,
        "dev.example.mono@1.0.0",
        "missing",
        Layer::Default,
    );
    assert!(plan.tokens.values().all(|t| t.layer == Layer::Default));
    let base_fallbacks: Vec<_> = plan
        .diagnostics
        .iter()
        .filter(|d| d.code == ResolveCode::FallbackToDefault)
        .map(|d| (d.pointer.as_str(), d.surface.as_deref()))
        .collect();
    assert_eq!(
        base_fallbacks,
        [("/baseTheme", Some("panel")), ("/baseTheme", None)]
    );
}

#[test]
fn a_surface_with_no_fallback_is_unresolved_not_silent() {
    let themes = ThemeSet::new().with(mono()).with(glassdock());
    let plan = resolve(&mix(), &themes, &default_theme(), &HostEnvironment::new());
    assert!(!plan.surfaces.contains_key("notification"));
    let found: Vec<_> = plan
        .diagnostics
        .iter()
        .filter(|d| d.surface.as_deref() == Some("notification"))
        .map(|d| d.code)
        .collect();
    assert_eq!(
        found,
        [
            ResolveCode::ThemeUnavailable,
            ResolveCode::SurfaceUnresolved
        ]
    );
}

#[test]
fn a_corrupt_package_falls_back_with_its_reason() {
    let mut themes = all_themes();
    themes.mark_corrupt(
        "dev.example.glassdock",
        "1.1.0".parse().unwrap(),
        "store entry fails verification",
    );
    let plan = resolve(&mix(), &themes, &default_theme(), &HostEnvironment::new());
    assert_eq!(fallback_of(&plan, "dock").reason, "corrupt");
    assert_eq!(plan.surfaces["dock"].layer, Layer::BaseTheme);
}

// ------------------------------------------------------- locked resolution

#[test]
fn resolving_through_the_lock_uses_only_pinned_releases() {
    let lock = generate_lock(&mix(), &installed()).unwrap();
    let themes = all_themes();
    let locked = LockedThemes::new(&lock, &themes);
    let plan = resolve(&mix(), &locked, &default_theme(), &HostEnvironment::new());
    assert!(plan.diagnostics.is_empty(), "{:#?}", plan.diagnostics);
    assert_eq!(
        plan,
        resolve(&mix(), &themes, &default_theme(), &HostEnvironment::new())
    );
}

#[test]
fn a_release_that_drifted_from_the_lock_is_not_loaded() {
    let lock = generate_lock(&mix(), &installed()).unwrap();
    // The dock theme was reinstalled with other bytes; the notification
    // theme now comes from another origin.
    let glass = glassdock();
    let mut other_bytes = glass.release.clone();
    other_bytes.digest = Sha256Digest::from_bytes([5; 32]);
    let notifs_input = notifs();
    let mut other_origin = notifs_input.release.clone();
    other_origin.origin = PackageOrigin::local();
    let themes = ThemeSet::new()
        .with(mono())
        .with(rereleased(&glass, other_bytes))
        .with(rereleased(&notifs_input, other_origin));
    let plan = resolve(
        &mix(),
        &LockedThemes::new(&lock, &themes),
        &default_theme(),
        &HostEnvironment::new(),
    );
    assert_eq!(fallback_of(&plan, "dock").reason, "digest-mismatch");
    assert_eq!(plan.surfaces["dock"].theme, mono().release);
    assert!(!plan.surfaces.contains_key("notification"));
    let messages: Vec<_> = plan
        .diagnostics
        .iter()
        .filter(|d| d.code == ResolveCode::ThemeUnavailable)
        .map(|d| d.message.as_str())
        .collect();
    assert!(messages[0].contains("is not the pinned"), "{messages:?}");
    assert!(
        messages[1].contains("installed origin local"),
        "{messages:?}"
    );
}

#[test]
fn unpinned_or_repinned_themes_are_not_loaded() {
    let lock = generate_lock(
        &profile(json!({
            "schemaVersion": 1, "id": "profile.custom-mix", "name": "Custom Mix",
            "baseTheme": { "id": "dev.example.mono", "version": "1.0.0" }
        })),
        &installed(),
    )
    .unwrap();
    let plan = resolve(
        &mix(),
        &LockedThemes::new(&lock, &all_themes()),
        &default_theme(),
        &HostEnvironment::new(),
    );
    assert_eq!(fallback_of(&plan, "dock").reason, "unlocked");

    let mut v2 = profile(json!({
        "schemaVersion": 1, "id": "profile.custom-mix", "name": "Custom Mix",
        "baseTheme": { "id": "dev.example.mono", "version": "2.0.0" }
    }));
    v2.name = "moved".to_owned();
    let mono2 = theme(
        "dev.example.mono",
        "2.0.0",
        json!({ "dock": { "entry": "dock.html" } }),
        json!({}),
        None,
    );
    let plan = resolve(
        &v2,
        &LockedThemes::new(&lock, &ThemeSet::new().with(mono2)),
        &default_theme(),
        &HostEnvironment::new(),
    );
    assert_eq!(fallback_of(&plan, "panel").reason, "lock-version-mismatch");
    assert!(!plan.surfaces.contains_key("dock"));
}

#[test]
fn lock_codes_are_stable() {
    let names: Vec<_> = LockCode::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(
        names,
        [
            "GLINT_LOCK_NOT_INSTALLED",
            "GLINT_LOCK_AMBIGUOUS",
            "GLINT_LOCK_DIGEST_MISMATCH",
            "GLINT_LOCK_ORIGIN_MISMATCH"
        ]
    );
    assert_eq!(
        ResolveCode::FallbackToBase.as_str(),
        "GLINT_RESOLVE_FALLBACK_TO_BASE"
    );
    assert_eq!(
        ResolveCode::FallbackToDefault.as_str(),
        "GLINT_RESOLVE_FALLBACK_TO_DEFAULT"
    );
}
