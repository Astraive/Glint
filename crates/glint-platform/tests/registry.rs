//! CAP-005: the frozen v1 capability registry. Unknown IDs are rejected at
//! validation (registry lookup) and at runtime (service, snapshot and
//! activation gate); generated outputs never drift from the registry.
//!
//! Regenerate the TypeScript module and the spec table with
//! `GLINT_UPDATE_TYPES=1 cargo test -p glint-platform --test registry`.

use std::collections::BTreeSet;
use std::path::PathBuf;

use glint_platform::mock::MockAdapter;
use glint_platform::registry::{
    self, Access, REGISTRY, REGISTRY_VERSION, SPEC_PATH, SPEC_TABLE_MARKERS, Sensitivity,
    TYPESCRIPT_PATH,
};
use glint_platform::{
    Availability, Capability, CapabilityErrorCode, CapabilityId, CapabilityService,
    IntegrationEffect, Namespace, UnavailableKind, evaluate_manifest,
};
use glint_schema::parse_theme_manifest;
use serde_json::json;

/// The frozen v1 ID list. Within v1 IDs may only be appended (through an
/// ADR); removing, renaming or reordering one fails here.
const FROZEN_V1: &[&str] = &[
    "surface.panel",
    "surface.dock",
    "surface.launcher",
    "surface.control-center",
    "surface.notification",
    "surface.notification-center",
    "surface.desktop",
    "surface.calendar",
    "surface.power-menu",
    "surface.workspace-switcher",
    "surface.window-switcher",
    "surface.volume-osd",
    "surface.brightness-osd",
    "surface.media-overlay",
    "surface.custom",
    "system.audio.read",
    "system.audio.write",
    "system.brightness.read",
    "system.brightness.write",
    "system.network.read",
    "system.network.toggle",
    "system.network.ssid",
    "system.bluetooth.read",
    "system.bluetooth.toggle",
    "system.battery.read",
    "system.power.source.read",
    "system.power.sleep",
    "system.power.shutdown",
    "system.media.read",
    "system.media.control",
    "desktop.windows.list",
    "desktop.windows.focus",
    "desktop.workspaces.read",
    "desktop.workspaces.switch",
    "native.wallpaper.set",
    "native.system-theme.apply",
    "native.taskbar.modify",
    "native.status-item.create",
    "host.layer-shell",
    "host.appbar",
    "host.desktop-layer",
    "host.transparent-overlay",
    "host.global-hotkey",
];

/// Syntactically valid IDs that are deliberately not registered.
const UNKNOWN: &[&str] = &[
    "system.shell.exec",
    "system.files.write",
    "surface.taskbar",
    "desktop.windows.close",
    // Plan spellings superseded by the surface catalog names (ADR 0009).
    "surface.notification-overlay",
    "surface.desktop-widget",
    // Folded into system.audio.read / system.network.read (ADR 0009).
    "system.audio.events",
    "system.network.details",
    "system.audio",
];

fn repo_file(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

#[test]
fn registry_matches_the_frozen_v1_list() {
    assert_eq!(REGISTRY_VERSION, 1);
    let ids: Vec<&str> = registry::registered_ids().collect();
    assert_eq!(ids, FROZEN_V1);
}

#[test]
fn every_id_is_valid_unique_and_grouped_by_namespace() {
    let mut seen = BTreeSet::new();
    let mut last = 0usize;
    for spec in REGISTRY {
        let parsed = CapabilityId::parse(spec.id).unwrap_or_else(|e| panic!("{}: {e}", spec.id));
        assert!(seen.insert(spec.id), "duplicate {}", spec.id);
        let ns = Namespace::ALL
            .iter()
            .position(|n| *n == parsed.namespace())
            .unwrap();
        assert!(ns >= last, "{} is out of namespace order", spec.id);
        last = ns;
        assert_eq!(spec.namespace(), parsed.namespace());
        assert!(!spec.summary.trim().is_empty(), "{}", spec.id);
        assert!(!spec.platform_notes.trim().is_empty(), "{}", spec.id);
    }
    for ns in Namespace::ALL {
        assert!(
            REGISTRY.iter().any(|s| s.namespace() == *ns),
            "namespace {ns} has no capability"
        );
    }
}

#[test]
fn access_split_follows_the_namespace_rules() {
    for spec in REGISTRY {
        match spec.namespace() {
            Namespace::Surface | Namespace::Host => {
                assert_eq!(spec.access, Access::Present, "{}", spec.id);
                assert_eq!(spec.effect, IntegrationEffect::GlintOwnedUi, "{}", spec.id);
            }
            Namespace::Native => {
                assert_eq!(spec.access, Access::Write, "{}", spec.id);
                assert_eq!(
                    spec.effect,
                    IntegrationEffect::OsManagedSetting,
                    "{}",
                    spec.id
                );
            }
            _ => assert_ne!(spec.access, Access::Present, "{}", spec.id),
        }
        // Reads never change OS state; writes always do.
        match spec.access {
            Access::Read => assert_eq!(spec.effect, IntegrationEffect::GlintOwnedUi, "{}", spec.id),
            Access::Write => {
                assert_eq!(
                    spec.effect,
                    IntegrationEffect::OsManagedSetting,
                    "{}",
                    spec.id
                )
            }
            _ => {}
        }
        if spec.id.ends_with(".read") {
            assert_eq!(spec.access, Access::Read, "{}", spec.id);
        }
    }
}

#[test]
fn plan_sensitive_properties_are_high() {
    // capabilities-permissions-and-errors.md, "User trust": SSID, foreground
    // application names and window titles need scoping.
    for id in [
        "system.network.ssid",
        "desktop.windows.list",
        "system.media.read",
    ] {
        assert_eq!(registry::lookup(id).unwrap().sensitivity, Sensitivity::High);
    }
    assert_eq!(
        registry::lookup("system.network.read").unwrap().sensitivity,
        Sensitivity::Low
    );
}

#[test]
fn unknown_ids_are_rejected_at_validation() {
    for id in UNKNOWN {
        assert!(
            CapabilityId::parse(id).is_ok(),
            "{id} should be well-formed"
        );
        assert!(!registry::is_registered(id), "{id}");
        let error = registry::require_registered(id).unwrap_err();
        assert_eq!(error.code(), CapabilityErrorCode::UnknownCapability);
        assert_eq!(error.capability().map(CapabilityId::as_str), Some(*id));
    }
    for id in [
        "",
        "Surface.Panel",
        "surface",
        "filesystem.write.any",
        "surface.panel ",
    ] {
        let error = registry::require_registered(id).unwrap_err();
        assert_eq!(
            error.code(),
            CapabilityErrorCode::UnknownCapability,
            "{id:?}"
        );
        assert!(error.capability().is_none(), "{id:?}");
    }
    for id in FROZEN_V1 {
        assert_eq!(registry::require_registered(id).unwrap().id, *id);
    }
}

#[test]
fn an_adapter_cannot_introduce_an_unregistered_capability() {
    let mock = MockAdapter::new("mock")
        .with(Capability::new(
            CapabilityId::parse("surface.panel").unwrap(),
            Availability::Supported,
            IntegrationEffect::GlintOwnedUi,
        ))
        .with(Capability::new(
            CapabilityId::parse("system.shell.exec").unwrap(),
            Availability::Supported,
            IntegrationEffect::OsManagedSetting,
        ));
    let mut service = CapabilityService::new(mock);
    let error = service.refresh().unwrap_err();
    assert_eq!(error.code(), CapabilityErrorCode::AdapterFailure);
    assert_eq!(
        error.capability().map(CapabilityId::as_str),
        Some("system.shell.exec")
    );
    // Fail closed: nothing from the rejected report is usable.
    for id in ["surface.panel", "system.shell.exec"] {
        let id = CapabilityId::parse(id).unwrap();
        assert!(!service.availability(&id).is_usable());
        assert!(service.require(&id).is_err());
    }
}

#[test]
fn unknown_ids_are_unknown_capability_at_runtime() {
    let mock = MockAdapter::new("mock").with(Capability::new(
        CapabilityId::parse("surface.panel").unwrap(),
        Availability::Supported,
        IntegrationEffect::GlintOwnedUi,
    ));
    let mut service = CapabilityService::new(mock);
    service.refresh().unwrap();
    for id in UNKNOWN {
        let id = CapabilityId::parse(id).unwrap();
        match service.availability(&id) {
            Availability::Unavailable { reason } => {
                assert_eq!(reason.kind(), UnavailableKind::NotAdvertised);
                assert!(reason.detail().contains("registry"), "{}", reason.detail());
            }
            other => panic!("{id}: {other:?}"),
        }
        assert_eq!(
            service.require(&id).unwrap_err().code(),
            CapabilityErrorCode::UnknownCapability
        );
        assert_eq!(
            service.require_writable(&id).unwrap_err().code(),
            CapabilityErrorCode::UnknownCapability
        );
    }
}

#[test]
fn the_activation_gate_refuses_unknown_ids_even_when_optional() {
    let mock = MockAdapter::new("mock").with(Capability::new(
        CapabilityId::parse("surface.panel").unwrap(),
        Availability::Supported,
        IntegrationEffect::GlintOwnedUi,
    ));
    let mut service = CapabilityService::new(mock);
    service.refresh().unwrap();
    let manifest = |required: &[&str], optional: &[&str]| {
        parse_theme_manifest(
            &json!({
                "schemaVersion": 1,
                "kind": "theme",
                "id": "dev.example.registry",
                "name": "Registry Fixture",
                "version": "0.1.0",
                "author": {"name": "Glint Test Fixtures"},
                "license": "MIT",
                "compatibility": {"glint": ">=0.1.0", "platforms": ["windows"]},
                "requiredCapabilities": required,
                "optionalCapabilities": optional,
            })
            .to_string(),
        )
        .unwrap()
    };

    assert!(evaluate_manifest(&manifest(&["surface.panel"], &[]), service.snapshot()).is_ok());

    let refusal = evaluate_manifest(
        &manifest(
            &["surface.panel", "system.shell.exec"],
            &["system.audio.read", "surface.taskbar"],
        ),
        service.snapshot(),
    )
    .unwrap_err();
    assert_eq!(refusal.code(), CapabilityErrorCode::ThemeIncompatible);
    let blockers: Vec<(&str, CapabilityErrorCode)> = refusal
        .blockers()
        .iter()
        .map(|b| (b.capability.as_str(), b.error.code()))
        .collect();
    assert_eq!(
        blockers,
        vec![
            ("system.shell.exec", CapabilityErrorCode::UnknownCapability),
            ("surface.taskbar", CapabilityErrorCode::UnknownCapability),
        ]
    );
}

#[test]
fn committed_typescript_matches_the_generator() {
    let generated = registry::typescript_module();
    if std::env::var_os("GLINT_UPDATE_TYPES").is_some() {
        std::fs::write(repo_file(TYPESCRIPT_PATH), &generated).expect("write generated types");
        return;
    }
    let committed = std::fs::read_to_string(repo_file(TYPESCRIPT_PATH))
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        committed == generated,
        "DRIFT: {TYPESCRIPT_PATH} is out of date with crates/glint-platform/src/registry.rs. \
         Run: GLINT_UPDATE_TYPES=1 cargo test -p glint-platform --test registry"
    );
}

#[test]
fn committed_spec_table_matches_the_generator() {
    let (begin, end) = SPEC_TABLE_MARKERS;
    let table = registry::markdown_table();
    let path = repo_file(SPEC_PATH);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    let start = text.find(begin).map(|i| i + begin.len());
    let stop = text.find(end);
    let (Some(start), Some(stop)) = (start, stop) else {
        panic!("{SPEC_PATH} must contain the markers {begin} and {end}");
    };
    let expected = format!("\n\n{table}\n");
    if std::env::var_os("GLINT_UPDATE_TYPES").is_some() {
        let updated = format!("{}{expected}{}", &text[..start], &text[stop..]);
        std::fs::write(&path, updated).expect("write spec table");
        return;
    }
    assert!(
        text[start..stop] == expected,
        "DRIFT: the registry table in {SPEC_PATH} is out of date. \
         Run: GLINT_UPDATE_TYPES=1 cargo test -p glint-platform --test registry"
    );
}

#[test]
fn generated_typescript_names_every_id() {
    let text = registry::typescript_module();
    for spec in REGISTRY {
        assert!(
            text.contains(&format!("  \"{}\",\n", spec.id)),
            "{}",
            spec.id
        );
        assert!(
            text.contains(&format!("  \"{}\": {{\n", spec.id)),
            "{}",
            spec.id
        );
    }
    assert!(text.ends_with("};\n"));
}
