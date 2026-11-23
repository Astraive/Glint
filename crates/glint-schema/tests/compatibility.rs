//! Version negotiation and forward-compatibility tests (SCH-003).
//!
//! Run with `cargo test -p glint-schema --test compatibility`. The rules are
//! specified in `docs/architecture/manifest-compatibility.md`.

use std::collections::BTreeSet;

use glint_schema::{
    ErrorCode, HostVersions, IncompatibilityCode, IncompatibilityReport, NegotiationError,
    SUPPORTED_ENTRYPOINT_VERSION, SUPPORTED_SCHEMA_VERSION, THEME_INCOMPATIBLE, WarningCode,
    check_compatibility, check_versions, negotiate_theme_manifest, parse_theme_manifest,
    parse_theme_manifest_with_diagnostics, probe_manifest_versions,
};
use semver::Version;
use serde_json::{Value, json};

/// A programmable theme declaring every negotiated axis.
fn programmable() -> Value {
    json!({
        "schemaVersion": 1,
        "kind": "theme",
        "id": "dev.example.midnight",
        "name": "Midnight",
        "version": "1.0.0",
        "author": { "name": "Example Developer" },
        "license": "MIT",
        "compatibility": { "glint": ">=0.1.0", "sdk": "^0.1.0", "platforms": ["windows"] },
        "renderer": { "kind": "glint-web", "entrypointVersion": 1 },
        "surfaces": { "panel": { "entry": "dist/panel/index.html" } }
    })
}

fn with(mut value: Value, pointer: &str, replacement: Value) -> Value {
    let (parent, key) = pointer.rsplit_once('/').expect("pointer has a parent");
    let target = if parent.is_empty() {
        &mut value
    } else {
        value.pointer_mut(parent).expect("parent exists")
    };
    target
        .as_object_mut()
        .expect("parent is an object")
        .insert(key.to_owned(), replacement);
    value
}

fn host(glint: &str, sdk: &str) -> HostVersions {
    HostVersions::new(
        Version::parse(glint).expect("glint version"),
        Version::parse(sdk).expect("sdk version"),
    )
}

fn incompatible(value: &Value, host: &HostVersions) -> IncompatibilityReport {
    match negotiate_theme_manifest(&value.to_string(), host) {
        Err(NegotiationError::Incompatible(report)) => report,
        other => panic!("expected theme-incompatible, got {other:?}"),
    }
}

fn codes(report: &IncompatibilityReport) -> Vec<IncompatibilityCode> {
    report.reasons().iter().map(|r| r.code()).collect()
}

// ------------------------------------------------------------ host defaults

#[test]
fn host_defaults_to_the_versions_this_crate_understands() {
    let host = host("0.1.0", "0.1.0");
    assert_eq!(host.schema_versions, vec![SUPPORTED_SCHEMA_VERSION]);
    assert_eq!(host.entrypoint_versions, vec![SUPPORTED_ENTRYPOINT_VERSION]);
}

#[test]
fn matching_host_is_compatible() {
    let parsed = negotiate_theme_manifest(&programmable().to_string(), &host("0.1.0", "0.1.4"))
        .expect("compatible");
    assert_eq!(parsed.manifest.id, "dev.example.midnight");
    assert!(parsed.warnings.is_empty());
    let manifest = parse_theme_manifest(&programmable().to_string()).expect("valid");
    assert!(check_compatibility(&manifest, &host("2.0.0", "0.1.9")).is_ok());
}

#[test]
fn declarative_theme_without_sdk_or_renderer_ignores_those_axes() {
    let mut value = programmable();
    let root = value.as_object_mut().expect("object");
    root.remove("renderer");
    root.remove("surfaces");
    root["compatibility"]
        .as_object_mut()
        .expect("compatibility")
        .remove("sdk");
    let mut host = host("0.1.0", "9.9.9");
    host.entrypoint_versions.clear();
    assert!(negotiate_theme_manifest(&value.to_string(), &host).is_ok());
}

// --------------------------------------------------------- individual axes

#[test]
fn newer_sdk_requirement_is_theme_incompatible_naming_the_required_version() {
    let value = with(programmable(), "/compatibility/sdk", json!(">=0.3.0"));
    let report = incompatible(&value, &host("0.1.0", "0.1.0"));
    assert_eq!(report.category(), THEME_INCOMPATIBLE);
    assert_eq!(report.category(), "theme-incompatible");
    assert_eq!(codes(&report), vec![IncompatibilityCode::SdkVersion]);
    let reason = &report.reasons()[0];
    assert_eq!(reason.code().as_str(), "GLINT_COMPAT_SDK_VERSION");
    assert_eq!(reason.pointer(), "/compatibility/sdk");
    assert_eq!(reason.required(), ">=0.3.0");
    assert_eq!(reason.available(), "0.1.0");
    let text = report.to_string();
    assert!(text.starts_with("theme-incompatible: "), "{text}");
    assert!(text.contains(">=0.3.0"), "{text}");
    assert!(text.contains("0.1.0"), "{text}");
}

#[test]
fn glint_range_is_checked() {
    let value = with(programmable(), "/compatibility/glint", json!(">=0.5.0"));
    let report = incompatible(&value, &host("0.4.9", "0.1.0"));
    assert_eq!(codes(&report), vec![IncompatibilityCode::GlintVersion]);
    let reason = &report.reasons()[0];
    assert_eq!(reason.pointer(), "/compatibility/glint");
    assert_eq!(reason.required(), ">=0.5.0");
    assert_eq!(reason.available(), "0.4.9");

    // An upper bound is honoured too: a theme may refuse newer hosts.
    let value = with(
        programmable(),
        "/compatibility/glint",
        json!(">=0.1.0, <0.2.0"),
    );
    let report = incompatible(&value, &host("0.2.0", "0.1.0"));
    assert_eq!(codes(&report), vec![IncompatibilityCode::GlintVersion]);
}

#[test]
fn future_schema_version_is_theme_incompatible_not_a_parse_error() {
    // A v2 document may use fields this parser does not know; negotiation
    // reads only the frozen version envelope and stops before full parsing.
    let value = with(programmable(), "/schemaVersion", json!(2));
    let value = with(value, "/futureField", json!({ "anything": true }));
    let report = incompatible(&value, &host("0.1.0", "0.1.0"));
    assert_eq!(codes(&report), vec![IncompatibilityCode::SchemaVersion]);
    let reason = &report.reasons()[0];
    assert_eq!(reason.code().as_str(), "GLINT_COMPAT_SCHEMA_VERSION");
    assert_eq!(reason.pointer(), "/schemaVersion");
    assert_eq!(reason.required(), "2");
    assert_eq!(reason.available(), "1");

    // The strict parser keeps reporting its own stable code.
    assert_eq!(
        parse_theme_manifest(&value.to_string()).unwrap_err().code(),
        ErrorCode::UnsupportedSchemaVersion
    );
}

#[test]
fn future_entrypoint_version_is_theme_incompatible() {
    let value = with(
        programmable(),
        "/renderer",
        json!({ "kind": "glint-web", "entrypointVersion": 2 }),
    );
    let report = incompatible(&value, &host("0.1.0", "0.1.0"));
    assert_eq!(codes(&report), vec![IncompatibilityCode::EntrypointVersion]);
    let reason = &report.reasons()[0];
    assert_eq!(reason.code().as_str(), "GLINT_COMPAT_ENTRYPOINT_VERSION");
    assert_eq!(reason.pointer(), "/renderer/entrypointVersion");
    assert_eq!(reason.required(), "2");
    assert_eq!(reason.available(), "1");
}

#[test]
fn every_failing_axis_is_reported_independently_in_a_fixed_order() {
    let value = with(programmable(), "/schemaVersion", json!(3));
    let value = with(value, "/compatibility/glint", json!(">=9.0.0"));
    let value = with(value, "/compatibility/sdk", json!("^2.0.0"));
    let value = with(
        value,
        "/renderer",
        json!({ "kind": "glint-web", "entrypointVersion": 7 }),
    );
    let report = incompatible(&value, &host("0.1.0", "0.1.0"));
    assert_eq!(
        codes(&report),
        vec![
            IncompatibilityCode::SchemaVersion,
            IncompatibilityCode::GlintVersion,
            IncompatibilityCode::SdkVersion,
            IncompatibilityCode::EntrypointVersion,
        ]
    );
    let pointers: Vec<&str> = report.reasons().iter().map(|r| r.pointer()).collect();
    assert_eq!(
        pointers,
        vec![
            "/schemaVersion",
            "/compatibility/glint",
            "/compatibility/sdk",
            "/renderer/entrypointVersion"
        ]
    );
}

#[test]
fn host_may_support_several_schema_and_entrypoint_versions() {
    let value = with(programmable(), "/schemaVersion", json!(2));
    let versions = probe_manifest_versions(&value.to_string()).expect("probe");
    let mut host = host("0.1.0", "0.1.0");
    host.schema_versions = vec![1, 2];
    assert!(check_versions(&versions, &host).is_ok());
    host.schema_versions = vec![1, 3];
    let report = check_versions(&versions, &host).unwrap_err();
    assert_eq!(report.reasons()[0].available(), "1, 3");
}

#[test]
fn pre_release_hosts_follow_semver_matching() {
    // `>=0.1.0` does not match a pre-release host (semver crate semantics);
    // a theme that targets pre-releases must say so explicitly.
    let report = incompatible(&programmable(), &host("0.2.0-alpha.1", "0.1.0"));
    assert_eq!(codes(&report), vec![IncompatibilityCode::GlintVersion]);
    let value = with(
        programmable(),
        "/compatibility/glint",
        json!(">=0.2.0-alpha.1"),
    );
    assert!(negotiate_theme_manifest(&value.to_string(), &host("0.2.0-alpha.1", "0.1.0")).is_ok());
}

#[test]
fn incompatibility_codes_are_stable_and_unique() {
    let expected = [
        (
            IncompatibilityCode::SchemaVersion,
            "GLINT_COMPAT_SCHEMA_VERSION",
        ),
        (
            IncompatibilityCode::GlintVersion,
            "GLINT_COMPAT_GLINT_VERSION",
        ),
        (IncompatibilityCode::SdkVersion, "GLINT_COMPAT_SDK_VERSION"),
        (
            IncompatibilityCode::EntrypointVersion,
            "GLINT_COMPAT_ENTRYPOINT_VERSION",
        ),
    ];
    assert_eq!(expected.len(), IncompatibilityCode::ALL.len());
    for (code, text) in expected {
        assert_eq!(code.as_str(), text);
        assert_eq!(code.to_string(), text);
    }
    let unique: BTreeSet<&str> = IncompatibilityCode::ALL
        .iter()
        .map(|c| c.as_str())
        .collect();
    assert_eq!(unique.len(), IncompatibilityCode::ALL.len());
}

// ------------------------------------------------------- envelope probing

#[test]
fn probe_reads_only_the_version_envelope() {
    let value = with(programmable(), "/schemaVersion", json!(5));
    let value = with(value, "/kind", json!("something-new"));
    let versions = probe_manifest_versions(&value.to_string()).expect("probe");
    assert_eq!(versions.schema_version, 5);
    assert_eq!(versions.glint.to_string(), ">=0.1.0");
    assert_eq!(
        versions.sdk.as_ref().map(ToString::to_string).as_deref(),
        Some("^0.1.0")
    );
    assert_eq!(versions.entrypoint_version, Some(1));
}

#[test]
fn malformed_envelope_is_invalid_not_incompatible() {
    let host = host("0.1.0", "0.1.0");
    let cases = [
        (
            with(programmable(), "/compatibility/sdk", json!("newest")),
            ErrorCode::InvalidVersionRequirement,
            "/compatibility/sdk",
        ),
        (
            with(programmable(), "/schemaVersion", json!("2")),
            ErrorCode::InvalidType,
            "/schemaVersion",
        ),
        (
            with(programmable(), "/compatibility", json!([])),
            ErrorCode::InvalidType,
            "/compatibility",
        ),
        (
            with(
                programmable(),
                "/renderer",
                json!({ "kind": "glint-web", "entrypointVersion": "1" }),
            ),
            ErrorCode::InvalidType,
            "/renderer/entrypointVersion",
        ),
    ];
    for (value, code, pointer) in cases {
        match negotiate_theme_manifest(&value.to_string(), &host) {
            Err(NegotiationError::Invalid(err)) => {
                assert_eq!(err.code(), code, "{pointer}");
                assert_eq!(err.pointer(), pointer);
            }
            other => panic!("{pointer}: expected invalid, got {other:?}"),
        }
    }
    let mut missing = programmable();
    missing
        .as_object_mut()
        .expect("object")
        .remove("schemaVersion");
    match negotiate_theme_manifest(&missing.to_string(), &host) {
        Err(NegotiationError::Invalid(err)) => {
            assert_eq!(err.code(), ErrorCode::MissingSchemaVersion);
        }
        other => panic!("expected invalid, got {other:?}"),
    }
}

#[test]
fn compatible_but_invalid_manifest_is_invalid() {
    let value = with(programmable(), "/id", json!("Not An Id"));
    match negotiate_theme_manifest(&value.to_string(), &host("0.1.0", "0.1.0")) {
        Err(NegotiationError::Invalid(err)) => assert_eq!(err.code(), ErrorCode::InvalidId),
        other => panic!("expected invalid, got {other:?}"),
    }
}

// ------------------------------------------------ unknown-field sensitivity

#[test]
fn unknown_sensitive_fields_are_rejected() {
    let cases = [
        "/scripts",
        "/migrations",
        "/postInstall",
        "/capabilities",
        "/integrations",
        "/compatibility/minimumOs",
        "/renderer/nodeIntegration",
        "/renderer/allowNetwork",
        "/surfaces/panel/preload",
    ];
    for pointer in cases {
        let value = with(programmable(), pointer, json!("x"));
        let err = parse_theme_manifest_with_diagnostics(&value.to_string()).unwrap_err();
        assert_eq!(err.code(), ErrorCode::UnknownField, "{pointer}");
        assert_eq!(err.pointer(), pointer);
        match negotiate_theme_manifest(&value.to_string(), &host("0.1.0", "0.1.0")) {
            Err(NegotiationError::Invalid(err)) => {
                assert_eq!(err.code(), ErrorCode::UnknownField, "{pointer}");
            }
            other => panic!("{pointer}: expected rejection, got {other:?}"),
        }
    }
}

#[test]
fn unknown_non_sensitive_author_fields_warn_and_are_ignored() {
    let value = with(programmable(), "/author/pronouns", json!("they/them"));
    let value = with(value, "/author/avatar", json!({ "size": 64 }));
    let parsed = parse_theme_manifest_with_diagnostics(&value.to_string()).expect("accepted");
    let warnings: Vec<(WarningCode, &str)> = parsed
        .warnings
        .iter()
        .map(|w| (w.code(), w.pointer()))
        .collect();
    // Sorted by key, like every other diagnostic.
    assert_eq!(
        warnings,
        vec![
            (WarningCode::UnknownField, "/author/avatar"),
            (WarningCode::UnknownField, "/author/pronouns"),
        ]
    );
    assert_eq!(
        parsed.warnings[0].code().as_str(),
        "GLINT_MANIFEST_WARN_UNKNOWN_FIELD"
    );
    assert!(
        parsed.warnings[0]
            .to_string()
            .starts_with("GLINT_MANIFEST_WARN_UNKNOWN_FIELD at /author/avatar: ")
    );
    assert_eq!(parsed.manifest.author.name, "Example Developer");

    // The plain parser accepts the same document and drops the warnings.
    assert!(parse_theme_manifest(&value.to_string()).is_ok());
    let negotiated =
        negotiate_theme_manifest(&value.to_string(), &host("0.1.0", "0.1.0")).expect("ok");
    assert_eq!(negotiated.warnings.len(), 2);
}

#[test]
fn ignored_fields_still_obey_document_level_rules() {
    let text = r#"{"schemaVersion":1,"kind":"theme","id":"dev.example.a","name":"x",
        "version":"1.0.0","author":{"name":"a","extra":1,"extra":2},"license":"MIT",
        "compatibility":{"glint":">=0.1.0","platforms":["windows"]}}"#;
    let err = parse_theme_manifest_with_diagnostics(text).unwrap_err();
    assert_eq!(err.code(), ErrorCode::DuplicateKey);
}

#[test]
fn warning_codes_are_stable() {
    assert_eq!(WarningCode::ALL, &[WarningCode::UnknownField]);
    assert_eq!(
        WarningCode::UnknownField.as_str(),
        "GLINT_MANIFEST_WARN_UNKNOWN_FIELD"
    );
}

#[test]
fn json_schema_mirrors_the_sensitivity_table() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../schemas/glint-theme-v1.schema.json");
    let schema: Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("schema")).expect("json");
    let closed = [
        "/additionalProperties",
        "/properties/compatibility/additionalProperties",
        "/properties/renderer/additionalProperties",
        "/$defs/surface/additionalProperties",
    ];
    for pointer in closed {
        assert_eq!(schema.pointer(pointer), Some(&json!(false)), "{pointer}");
    }
    assert_eq!(
        schema.pointer("/properties/author/additionalProperties"),
        Some(&json!(true))
    );
}
