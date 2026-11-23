//! CAP-003: a theme's required and optional capabilities are validated
//! against the host before activation. A missing required capability blocks
//! activation before any state change and keeps the current theme; a missing
//! optional capability only disables controls.

use std::cell::Cell;

use glint_platform::mock::MockAdapter;
use glint_platform::{
    ActivationRefusal, Availability, Capability, CapabilityError, CapabilityErrorCode,
    CapabilityId, CapabilityRequirements, CapabilityRole, CapabilityService, CapabilitySnapshot,
    CheckOutcome, IntegrationEffect, UnavailableKind, evaluate, evaluate_manifest, gate_activation,
};
use glint_schema::{ThemeManifest, parse_theme_manifest};
use serde_json::json;

fn id(s: &str) -> CapabilityId {
    CapabilityId::parse(s).unwrap()
}

fn manifest(required: &[&str], optional: &[&str]) -> ThemeManifest {
    let doc = json!({
        "schemaVersion": 1,
        "kind": "theme",
        "id": "dev.example.gate",
        "name": "Gate Fixture",
        "version": "0.1.0",
        "author": {"name": "Glint Test Fixtures"},
        "license": "MIT",
        "compatibility": {"glint": ">=0.1.0", "platforms": ["windows"]},
        "requiredCapabilities": required,
        "optionalCapabilities": optional,
    });
    parse_theme_manifest(&doc.to_string()).expect("fixture manifest parses")
}

fn host(capabilities: &[(&str, Availability)]) -> CapabilitySnapshot {
    let mock = MockAdapter::new("mock");
    for (raw, availability) in capabilities {
        mock.set(Capability::new(
            id(raw),
            availability.clone(),
            IntegrationEffect::GlintOwnedUi,
        ));
    }
    let mut service = CapabilityService::new(mock);
    service.refresh().unwrap();
    service.snapshot().clone()
}

fn unavailable(kind: UnavailableKind, detail: &str) -> Availability {
    Availability::unavailable(kind, detail).unwrap()
}

fn blocker_codes(refusal: &ActivationRefusal) -> Vec<(String, CapabilityErrorCode)> {
    refusal
        .blockers()
        .iter()
        .map(|b| (b.capability.clone(), b.error.code()))
        .collect()
}

#[test]
fn satisfied_requirements_allow_activation_without_degradation() {
    let snapshot = host(&[
        ("surface.panel", Availability::Supported),
        ("system.audio.read", Availability::Supported),
    ]);
    let report = evaluate_manifest(
        &manifest(&["surface.panel"], &["system.audio.read"]),
        &snapshot,
    )
    .unwrap();
    assert_eq!(report.theme_id(), "dev.example.gate");
    assert_eq!(report.snapshot_generation(), snapshot.generation());
    assert!(!report.is_degraded());
    assert_eq!(report.checks().len(), 2);
    assert!(
        report
            .checks()
            .iter()
            .all(|c| c.outcome == CheckOutcome::Available)
    );
}

#[test]
fn a_theme_without_capabilities_activates() {
    let report = evaluate_manifest(&manifest(&[], &[]), &host(&[])).unwrap();
    assert!(report.checks().is_empty());
    assert!(!report.is_degraded());
}

#[test]
fn each_missing_required_capability_is_listed_with_its_typed_reason() {
    let snapshot = host(&[
        ("surface.panel", Availability::Supported),
        ("desktop.windows.list", Availability::PermissionRequired),
        (
            "system.brightness.write",
            unavailable(UnavailableKind::HardwareUnavailable, "no backlight"),
        ),
    ]);
    let refusal = evaluate_manifest(
        &manifest(
            &[
                "surface.panel",
                "desktop.windows.list",
                "system.brightness.write",
                "host.layer-shell",
            ],
            &[],
        ),
        &snapshot,
    )
    .unwrap_err();

    assert_eq!(refusal.code(), CapabilityErrorCode::ThemeIncompatible);
    assert_eq!(refusal.theme_id(), "dev.example.gate");
    assert_eq!(
        blocker_codes(&refusal),
        vec![
            (
                "desktop.windows.list".to_owned(),
                CapabilityErrorCode::PermissionRequired
            ),
            (
                "system.brightness.write".to_owned(),
                CapabilityErrorCode::HardwareUnavailable
            ),
            (
                "host.layer-shell".to_owned(),
                CapabilityErrorCode::UnknownCapability
            ),
        ]
    );
    // The installer can present every requirement, not just the blockers.
    assert_eq!(refusal.checks().len(), 4);
    assert_eq!(refusal.checks()[0].outcome, CheckOutcome::Available);
    let shown = refusal.to_string();
    for name in [
        "theme-incompatible",
        "desktop.windows.list",
        "system.brightness.write",
        "host.layer-shell",
    ] {
        assert!(shown.contains(name), "{shown}");
    }
}

#[test]
fn nothing_discovered_means_nothing_required_is_available() {
    let mock = MockAdapter::new("mock").with(Capability::new(
        id("surface.panel"),
        Availability::Supported,
        IntegrationEffect::GlintOwnedUi,
    ));
    let service = CapabilityService::new(mock); // never refreshed
    let refusal =
        evaluate_manifest(&manifest(&["surface.panel"], &[]), service.snapshot()).unwrap_err();
    assert_eq!(
        blocker_codes(&refusal),
        vec![(
            "surface.panel".to_owned(),
            CapabilityErrorCode::UnknownCapability
        )]
    );
}

#[test]
fn an_adapter_outage_blocks_required_capabilities() {
    let mock = MockAdapter::new("mock").with(Capability::new(
        id("surface.panel"),
        Availability::Supported,
        IntegrationEffect::GlintOwnedUi,
    ));
    let mut service = CapabilityService::new(mock.clone());
    service.refresh().unwrap();
    mock.fail_with(Some(CapabilityError::new(
        CapabilityErrorCode::Timeout,
        None,
        "slow",
    )));
    assert!(service.refresh().is_err());
    let refusal =
        evaluate_manifest(&manifest(&["surface.panel"], &[]), service.snapshot()).unwrap_err();
    assert_eq!(
        refusal.blockers()[0].error.code(),
        CapabilityErrorCode::TemporaryUnavailable
    );
}

#[test]
fn missing_optional_capabilities_only_disable_controls() {
    let snapshot = host(&[
        ("surface.panel", Availability::Supported),
        ("system.media.control", Availability::PermissionRequired),
        (
            "system.network.read",
            unavailable(UnavailableKind::TemporaryUnavailable, "restarting"),
        ),
    ]);
    let report = evaluate_manifest(
        &manifest(
            &["surface.panel"],
            &[
                "system.media.control",
                "system.network.read",
                "system.bluetooth.read",
            ],
        ),
        &snapshot,
    )
    .unwrap();

    assert!(report.is_degraded());
    let disabled: Vec<(&str, CapabilityErrorCode)> = report
        .disabled()
        .map(|c| match &c.outcome {
            CheckOutcome::Disabled { error } => (c.id.as_str(), error.code()),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        disabled,
        vec![
            (
                "system.media.control",
                CapabilityErrorCode::PermissionRequired
            ),
            (
                "system.network.read",
                CapabilityErrorCode::TemporaryUnavailable
            ),
            (
                "system.bluetooth.read",
                CapabilityErrorCode::UnknownCapability
            ),
        ]
    );
    assert!(
        report
            .disabled()
            .all(|c| c.role == CapabilityRole::Optional)
    );
    let gated: Vec<&str> = report.permission_gated().map(|c| c.id.as_str()).collect();
    assert_eq!(gated, vec!["system.media.control"]);
}

#[test]
fn read_only_and_experimental_are_usable_but_reported_as_limited() {
    let snapshot = host(&[
        ("system.brightness.write", Availability::ReadOnly),
        ("desktop.workspaces.switch", Availability::Experimental),
    ]);
    let report = evaluate_manifest(
        &manifest(&["system.brightness.write"], &["desktop.workspaces.switch"]),
        &snapshot,
    )
    .unwrap();
    assert!(report.is_degraded());
    let limited: Vec<&str> = report.limited().map(|c| c.id.as_str()).collect();
    assert_eq!(
        limited,
        vec!["system.brightness.write", "desktop.workspaces.switch"]
    );
    assert_eq!(report.disabled().count(), 0);
}

#[test]
fn a_declared_fallback_can_satisfy_a_required_capability() {
    let snapshot = host(&[
        (
            "host.layer-shell",
            unavailable(UnavailableKind::UnsupportedSession, "no layer shell"),
        ),
        ("host.transparent-overlay", Availability::Supported),
    ]);
    let requirements = CapabilityRequirements::from_manifest(&manifest(&["host.layer-shell"], &[]))
        .unwrap()
        .with_fallback(&id("host.layer-shell"), id("host.transparent-overlay"));

    let report = evaluate(&requirements, &snapshot).unwrap();
    assert!(report.is_degraded());
    assert_eq!(
        report.checks()[0].outcome,
        CheckOutcome::Fallback {
            fallback: id("host.transparent-overlay")
        }
    );
}

#[test]
fn an_unusable_fallback_still_blocks_and_is_listed() {
    let snapshot = host(&[(
        "host.layer-shell",
        unavailable(UnavailableKind::UnsupportedSession, "no layer shell"),
    )]);
    let requirements = CapabilityRequirements::from_manifest(&manifest(&["host.layer-shell"], &[]))
        .unwrap()
        .with_fallback(&id("host.layer-shell"), id("host.transparent-overlay"));

    let refusal = evaluate(&requirements, &snapshot).unwrap_err();
    let blocker = &refusal.blockers()[0];
    assert_eq!(blocker.capability, "host.layer-shell");
    assert_eq!(
        blocker.error.code(),
        CapabilityErrorCode::UnsupportedSession
    );
    assert_eq!(blocker.fallbacks, vec![id("host.transparent-overlay")]);
}

#[test]
fn v1_manifests_declare_no_fallbacks() {
    let requirements =
        CapabilityRequirements::from_manifest(&manifest(&["surface.panel"], &["host.appbar"]))
            .unwrap();
    assert_eq!(requirements.required().len(), 1);
    assert!(requirements.required()[0].fallbacks.is_empty());
    assert_eq!(requirements.optional(), &[id("host.appbar")]);
    // A fallback for something that is not required is ignored, not added.
    let same = requirements
        .clone()
        .with_fallback(&id("host.appbar"), id("host.transparent-overlay"));
    assert_eq!(same, requirements);
}

#[test]
fn every_unusable_state_blocks_a_required_capability() {
    let mock = MockAdapter::all_states();
    let all = mock.capabilities();
    let mut service = CapabilityService::new(mock);
    service.refresh().unwrap();

    let ids: Vec<&str> = all.iter().map(|c| c.id.as_str()).collect();
    let refusal = evaluate_manifest(&manifest(&ids, &[]), service.snapshot()).unwrap_err();
    let blocked: Vec<String> = refusal
        .blockers()
        .iter()
        .map(|b| b.capability.clone())
        .collect();
    let expected: Vec<String> = all
        .iter()
        .filter(|c| !c.availability.is_usable())
        .map(|c| c.id.to_string())
        .collect();
    assert_eq!(blocked, expected);
    assert_eq!(expected.len(), 1 + UnavailableKind::ALL.len());
}

#[test]
fn refused_activation_never_reaches_the_state_change() {
    let snapshot = host(&[("surface.panel", Availability::Supported)]);
    let mut active_theme = "official.default".to_owned();
    let activations = Cell::new(0);

    let refused = gate_activation(
        &manifest(&["surface.panel", "native.wallpaper.set"], &[]),
        &snapshot,
        |_report| {
            activations.set(activations.get() + 1);
            active_theme = "dev.example.gate".to_owned();
        },
    );
    let refusal = refused.unwrap_err();
    assert_eq!(refusal.code(), CapabilityErrorCode::ThemeIncompatible);
    assert_eq!(activations.get(), 0);
    assert_eq!(active_theme, "official.default", "current theme is kept");

    let allowed = gate_activation(
        &manifest(&["surface.panel"], &["native.wallpaper.set"]),
        &snapshot,
        |report| {
            activations.set(activations.get() + 1);
            active_theme = report.theme_id().to_owned();
            report.disabled().count()
        },
    );
    assert_eq!(allowed.unwrap(), 1, "optional wallpaper control disabled");
    assert_eq!(activations.get(), 1);
    assert_eq!(active_theme, "dev.example.gate");
}

#[test]
fn reports_and_refusals_serialize_for_the_installer() {
    let snapshot = host(&[("system.audio.read", Availability::PermissionRequired)]);
    let report = evaluate_manifest(&manifest(&[], &["system.audio.read"]), &snapshot).unwrap();
    let value = serde_json::to_value(&report).unwrap();
    assert_eq!(value["themeId"], "dev.example.gate");
    assert_eq!(value["checks"][0]["id"], "system.audio.read");
    assert_eq!(value["checks"][0]["role"], "optional");
    assert_eq!(value["checks"][0]["outcome"]["kind"], "disabled");
    assert_eq!(
        value["checks"][0]["outcome"]["error"]["code"],
        "permission-required"
    );

    let refusal = evaluate_manifest(&manifest(&["system.audio.read"], &[]), &snapshot).unwrap_err();
    let value = serde_json::to_value(&refusal).unwrap();
    assert_eq!(value["code"], "theme-incompatible");
    assert_eq!(value["blockers"][0]["capability"], "system.audio.read");
    assert_eq!(value["blockers"][0]["error"]["code"], "permission-required");
}
