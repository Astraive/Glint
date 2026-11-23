//! Surface manifest tests (SRF-001): standard and custom IDs parse, invalid
//! entrypoints and unnamespaced custom IDs are rejected, and every case in
//! the shared corpus `tests/package-fixtures/surfaces/cases.json` produces
//! its expected definition or error code and JSON Pointer.
//!
//! Run with `cargo test -p glint-surfaces --test surface_manifest`.

use std::collections::BTreeSet;
use std::path::PathBuf;

use glint_platform::CapabilityId;
use glint_schema::parse_theme_manifest;
use glint_surfaces::{
    CATALOG, CUSTOM_SURFACE_CAPABILITY, PlacementKind, StandardSurface, SurfaceDefinition,
    SurfaceErrorCode, SurfaceKind, parse_surface_id, surface_definitions,
};
use serde_json::{Value, json};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_json(path: &str) -> Value {
    let text = std::fs::read_to_string(repo_root().join(path)).expect(path);
    serde_json::from_str(&text).expect(path)
}

/// Every key in `expected` must be present in `actual` with an equal value;
/// nested objects are compared the same way, arrays and scalars exactly.
fn assert_subset(expected: &Value, actual: &Value, at: &str) {
    match (expected, actual) {
        (Value::Object(want), Value::Object(have)) => {
            for (key, value) in want {
                let got = have
                    .get(key)
                    .unwrap_or_else(|| panic!("{at}/{key} missing in {actual}"));
                assert_subset(value, got, &format!("{at}/{key}"));
            }
        }
        _ => assert_eq!(expected, actual, "at {at}"),
    }
}

#[test]
fn corpus_valid_cases_resolve_as_expected() {
    let cases = read_json("tests/package-fixtures/surfaces/cases.json");
    let valid = cases["valid"].as_array().expect("valid");
    assert!(valid.len() >= 10);
    for case in valid {
        let name = case["name"].as_str().expect("name");
        let id = case["id"].as_str().expect("id");
        let definition = SurfaceDefinition::parse(id, &case["surface"])
            .unwrap_or_else(|err| panic!("{name}: {err}"));
        let actual = serde_json::to_value(&definition).expect("serializes");
        assert_eq!(actual["id"], json!(id), "{name}");
        assert_subset(&case["expect"], &actual, name);
    }
}

#[test]
fn corpus_invalid_cases_report_code_and_pointer() {
    let cases = read_json("tests/package-fixtures/surfaces/cases.json");
    for case in cases["invalid"].as_array().expect("invalid") {
        let name = case["name"].as_str().expect("name");
        let id = case["id"].as_str().expect("id");
        let err = match SurfaceDefinition::parse(id, &case["surface"]) {
            Ok(definition) => panic!("{name}: expected an error, parsed {definition:?}"),
            Err(err) => err,
        };
        assert_eq!(
            err.code().as_str(),
            case["code"].as_str().unwrap(),
            "{name}: {err}"
        );
        assert_eq!(
            err.pointer(),
            case["pointer"].as_str().unwrap(),
            "{name}: {err}"
        );
        assert!(!err.message().is_empty());
        assert!(err.to_string().starts_with(err.code().as_str()));
    }
}

#[test]
fn corpus_covers_every_error_code() {
    let cases = read_json("tests/package-fixtures/surfaces/cases.json");
    let used: BTreeSet<&str> = cases["invalid"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case["code"].as_str().unwrap())
        .collect();
    for code in SurfaceErrorCode::ALL {
        assert!(used.contains(code.as_str()), "no case for {code}");
    }
}

#[test]
fn error_codes_are_stable_unique_and_prefixed() {
    let strings: BTreeSet<&str> = SurfaceErrorCode::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(strings.len(), SurfaceErrorCode::ALL.len());
    for code in &strings {
        assert!(code.starts_with("GLINT_SURFACE_"), "{code}");
    }
    assert_eq!(
        SurfaceErrorCode::InvalidEntrypoint.as_str(),
        "GLINT_SURFACE_INVALID_ENTRYPOINT"
    );
    assert_eq!(
        SurfaceErrorCode::InvalidCustomId.as_str(),
        "GLINT_SURFACE_INVALID_CUSTOM_ID"
    );
}

/// The smallest valid definition for each catalog row's default placement.
fn minimal_surface(kind: StandardSurface) -> Value {
    let placement = match kind.entry().default_placement() {
        PlacementKind::Edge => json!({ "height": 40 }),
        _ => json!({}),
    };
    json!({ "entry": format!("dist/{kind}/index.html"), "placement": placement })
}

#[test]
fn every_standard_surface_id_parses() {
    let ids: Vec<&str> = StandardSurface::ALL.iter().map(|k| k.as_str()).collect();
    assert_eq!(
        ids,
        [
            "panel",
            "dock",
            "launcher",
            "control-center",
            "notification",
            "notification-center",
            "desktop",
            "calendar",
            "power-menu",
            "workspace-switcher",
            "window-switcher",
            "volume-osd",
            "brightness-osd",
            "media-overlay",
        ]
    );
    for kind in StandardSurface::ALL {
        assert_eq!(
            parse_surface_id(kind.as_str()),
            Ok(SurfaceKind::Standard(*kind))
        );
        let definition = SurfaceDefinition::parse(kind.as_str(), &minimal_surface(*kind))
            .unwrap_or_else(|err| panic!("{kind}: {err}"));
        assert_eq!(definition.kind, SurfaceKind::Standard(*kind));
        assert_eq!(definition.placement.kind, kind.entry().default_placement());
        assert_eq!(definition.behavior.focus, kind.entry().default_focus);
    }
}

#[test]
fn custom_ids_parse_only_when_namespaced() {
    for id in [
        "custom:dev.example.clock",
        "custom:official.mono",
        "custom:io.github.someone.weather-2",
    ] {
        match parse_surface_id(id) {
            Ok(SurfaceKind::Custom(custom)) => {
                assert_eq!(custom.as_str(), id);
                assert_eq!(custom.namespace(), &id["custom:".len()..]);
            }
            other => panic!("{id}: {other:?}"),
        }
    }
    let long = format!("custom:dev.{}", "a".repeat(130));
    for id in [
        "custom:clock",
        "custom:",
        "custom:.clock",
        "custom:dev.",
        "custom:dev.-clock",
        "custom:dev.clock!",
        long.as_str(),
    ] {
        let err = parse_surface_id(id).expect_err(id);
        assert_eq!(err.code(), SurfaceErrorCode::InvalidCustomId, "{id}");
    }
}

#[test]
fn catalog_capabilities_are_valid_and_follow_the_surface_namespace() {
    assert_eq!(CATALOG.len(), StandardSurface::ALL.len());
    for (index, kind) in StandardSurface::ALL.iter().enumerate() {
        let entry = kind.entry();
        assert_eq!(entry, &CATALOG[index]);
        assert_eq!(entry.capability, format!("surface.{}", entry.id));
        CapabilityId::parse(entry.capability).expect("valid capability");
        let mut seen = BTreeSet::new();
        for optional in entry.optional_capabilities {
            let id = CapabilityId::parse(optional).expect("valid capability");
            assert_ne!(id.namespace().as_str(), "surface", "{optional}");
            assert!(seen.insert(*optional), "duplicate {optional}");
        }
        assert!(!entry.placements.is_empty());
        let unique: BTreeSet<_> = entry.placements.iter().collect();
        assert_eq!(unique.len(), entry.placements.len(), "{kind}");
        let default_is_desktop = entry.default_placement() == PlacementKind::Desktop;
        assert_eq!(entry.default_anchor.is_none(), default_is_desktop, "{kind}");
    }
    CapabilityId::parse(CUSTOM_SURFACE_CAPABILITY).expect("valid capability");
}

/// Minimal manifest (an SCH-001 programmable theme) with the given surfaces.
fn manifest_with(surfaces: Value) -> String {
    json!({
        "schemaVersion": 1,
        "kind": "theme",
        "id": "dev.example.surfaces",
        "name": "Surfaces",
        "version": "0.1.0",
        "author": { "name": "Glint Test Fixtures" },
        "license": "MIT",
        "compatibility": { "glint": ">=0.1.0", "platforms": ["windows"] },
        "renderer": { "kind": "glint-web", "entrypointVersion": 1 },
        "surfaces": surfaces
    })
    .to_string()
}

#[test]
fn manifest_surfaces_become_definitions_in_key_order() {
    let manifest = parse_theme_manifest(&manifest_with(json!({
        "panel": {
            "entry": "dist/panel/index.html",
            "placement": { "anchor": "top", "height": 44 },
            "behavior": { "focus": "non-activating", "reserveWorkArea": true }
        },
        "launcher": { "entry": "dist/launcher/index.html" },
        "custom:dev.example.clock": {
            "entry": "dist/clock/index.html",
            "placement": { "kind": "overlay", "anchor": "top-right" },
            "behavior": { "focus": "none", "clickThrough": true }
        }
    })))
    .expect("manifest parses");
    let definitions = surface_definitions(&manifest).expect("surfaces validate");
    let ids: Vec<&str> = definitions.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(ids, ["custom:dev.example.clock", "launcher", "panel"]);
    let panel = &definitions[2];
    assert!(panel.behavior.reserve_work_area);
    assert_eq!(panel.entrypoint, "dist/panel/index.html");
    assert_eq!(panel.capabilities.required[0].as_str(), "surface.panel");
    assert_eq!(
        definitions[0].capabilities.required[0].as_str(),
        "surface.custom"
    );
}

#[test]
fn manifest_errors_point_into_the_manifest() {
    let cases = [
        (
            json!({ "taskbar": { "entry": "a.html" } }),
            SurfaceErrorCode::UnknownKind,
            "/surfaces/taskbar",
        ),
        (
            json!({ "launcher": { "entry": "a.html", "placement": { "anchor": "top", "x": 1 } } }),
            SurfaceErrorCode::UnknownField,
            "/surfaces/launcher/placement/x",
        ),
        (
            json!({ "launcher": { "entry": "a.html", "behavior": { "preload": "x.js" } } }),
            SurfaceErrorCode::UnknownField,
            "/surfaces/launcher/behavior/preload",
        ),
    ];
    // glint-schema already refuses an unnamespaced custom key itself.
    let err = parse_theme_manifest(&manifest_with(
        json!({ "custom:clock": { "entry": "a.html" } }),
    ))
    .expect_err("schema refuses custom:clock");
    assert_eq!(err.pointer(), "/surfaces/custom:clock");
    for (surfaces, code, pointer) in cases {
        let manifest = parse_theme_manifest(&manifest_with(surfaces)).expect("schema accepts");
        let err = surface_definitions(&manifest).expect_err(pointer);
        assert_eq!((err.code(), err.pointer()), (code, pointer), "{err}");
    }
}

#[test]
fn shared_manifest_fixtures_with_surfaces() {
    // The SRF-001 fixture validates end to end.
    let text = std::fs::read_to_string(
        repo_root().join("tests/package-fixtures/manifests/valid/surface-behavior.json"),
    )
    .expect("fixture");
    let manifest = parse_theme_manifest(&text).expect("schema accepts");
    let definitions = surface_definitions(&manifest).expect("surfaces validate");
    assert_eq!(definitions.len(), 2);

    // The plan's illustrative `"position": "center"` is not part of the
    // proposed placement format (`anchor` is used for every kind); the
    // glint-schema fixture modelled on the plan is refused here on purpose.
    let text = std::fs::read_to_string(
        repo_root().join("tests/package-fixtures/manifests/valid/full-programmable.json"),
    )
    .expect("fixture");
    let manifest = parse_theme_manifest(&text).expect("schema accepts");
    let err = surface_definitions(&manifest).expect_err("position is unknown");
    assert_eq!(err.code(), SurfaceErrorCode::UnknownField);
    assert_eq!(err.pointer(), "/surfaces/launcher/placement/position");
}

#[test]
fn entrypoint_rules_agree_with_the_manifest_parser() {
    let entries = [
        "dist/launcher/index.html",
        "index.htm",
        "a/b/c/D.HTML",
        "dist/launcher.js",
        "",
        "https://example.com/a.html",
        "http://localhost/a.html",
        "file:///C:/a.html",
        "file:a.html",
        "data:text/html,hi.html",
        "//cdn.example.com/a.html",
        "/abs/a.html",
        "C:/a.html",
        "a\\b.html",
        "a//b.html",
        "a/./b.html",
        "a/../b.html",
        "../a.html",
        "a/b./c.html",
        "a /b.html",
        "aux/a.html",
        "COM1.html",
        "a?.html",
        "a|b.html",
        "a\u{7}.html",
        "a.html:stream",
        "a.html/",
    ];
    for entry in entries {
        let schema_ok = parse_theme_manifest(&manifest_with(json!({
            "launcher": { "entry": entry }
        })))
        .is_ok();
        let surface = SurfaceDefinition::parse("launcher", &json!({ "entry": entry }));
        assert_eq!(surface.is_ok(), schema_ok, "{entry:?}: {surface:?}");
        if let Err(err) = surface {
            assert_eq!(err.code(), SurfaceErrorCode::InvalidEntrypoint, "{entry:?}");
            assert_eq!(err.pointer(), "/surfaces/launcher/entry");
        }
    }
}
