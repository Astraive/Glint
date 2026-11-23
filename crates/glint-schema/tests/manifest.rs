//! Manifest parser tests for the *proposed* `glint.json` v1 format (SCH-001).
//!
//! Run with `cargo test -p glint-schema --test manifest`.

use std::collections::BTreeSet;
use std::path::PathBuf;

use glint_schema::{
    ErrorCode, FORMAT_STATUS, ManifestError, PackageKind, Platform, SUPPORTED_SCHEMA_VERSION,
    parse_theme_manifest,
};
use serde_json::{Value, json};

/// Smallest manifest the parser accepts: a declarative theme with no surfaces.
fn minimal() -> Value {
    json!({
        "schemaVersion": 1,
        "kind": "theme",
        "id": "dev.example.minimal",
        "name": "Minimal",
        "version": "0.1.0",
        "author": { "name": "Example Developer" },
        "license": "MIT",
        "compatibility": { "glint": ">=0.1.0", "platforms": ["windows"] }
    })
}

/// The example root manifest from
/// `docs/plan/04-themes/manifest-and-glint-package-spec.md`.
const PLAN_EXAMPLE: &str = r#"{
  "schemaVersion": 1,
  "kind": "theme",
  "id": "dev.example.midnight",
  "name": "Midnight",
  "version": "1.0.0",
  "author": { "name": "Example Developer" },
  "description": "Monochrome Glint desktop theme",
  "license": "MIT",
  "compatibility": {
    "glint": ">=0.1.0",
    "sdk": "^0.1.0",
    "platforms": ["windows", "linux", "macos"]
  },
  "renderer": { "kind": "glint-web", "entrypointVersion": 1 },
  "requiredCapabilities": ["surface.launcher"],
  "optionalCapabilities": ["desktop.workspaces.read", "system.audio.write"],
  "tokens": "theme/tokens.json",
  "assets": { "wallpaper": "assets/wallpaper.png", "preview": "preview.png" },
  "layouts": { "default": "layouts/default.json" },
  "surfaces": {
    "panel": { "entry": "dist/panel/index.html", "placement": { "anchor": "top", "height": 48 } },
    "launcher": { "entry": "dist/launcher/index.html", "placement": { "kind": "floating", "position": "center" } },
    "control-center": { "entry": "dist/control-center/index.html", "placement": { "kind": "popup", "anchor": "top-right" } }
  }
}"#;

fn parse(value: &Value) -> Result<glint_schema::ThemeManifest, ManifestError> {
    parse_theme_manifest(&value.to_string())
}

fn expect_err(value: &Value) -> ManifestError {
    match parse(value) {
        Ok(manifest) => panic!("expected an error, parsed {manifest:?}"),
        Err(err) => err,
    }
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

fn without(mut value: Value, key: &str) -> Value {
    value.as_object_mut().expect("object").remove(key);
    value
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_json(path: PathBuf) -> Value {
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{} is not JSON: {e}", path.display()))
}

// ---------------------------------------------------------------- valid input

#[test]
fn minimal_manifest_parses() {
    let manifest = parse(&minimal()).expect("minimal manifest is valid");
    assert_eq!(manifest.schema_version, SUPPORTED_SCHEMA_VERSION);
    assert_eq!(manifest.kind, PackageKind::Theme);
    assert_eq!(manifest.id, "dev.example.minimal");
    assert_eq!(manifest.version, semver::Version::new(0, 1, 0));
    assert_eq!(manifest.name, "Minimal");
    assert_eq!(manifest.author.name, "Example Developer");
    assert_eq!(manifest.license, "MIT");
    assert_eq!(manifest.compatibility.platforms, vec![Platform::Windows]);
    assert!(manifest.compatibility.sdk.is_none());
    assert!(manifest.renderer.is_none());
    assert!(manifest.surfaces.is_empty());
    assert!(manifest.required_capabilities.is_empty());
}

#[test]
fn plan_example_manifest_parses() {
    let manifest = parse_theme_manifest(PLAN_EXAMPLE).expect("plan example is valid");
    assert_eq!(manifest.id, "dev.example.midnight");
    assert_eq!(
        manifest.description.as_deref(),
        Some("Monochrome Glint desktop theme")
    );
    assert_eq!(manifest.compatibility.platforms.len(), 3);
    assert!(manifest.compatibility.sdk.is_some());
    let renderer = manifest.renderer.as_ref().expect("renderer present");
    assert_eq!(renderer.entrypoint_version, 1);
    assert_eq!(manifest.required_capabilities, vec!["surface.launcher"]);
    assert_eq!(manifest.optional_capabilities.len(), 2);
    assert_eq!(manifest.tokens.as_deref(), Some("theme/tokens.json"));
    assert_eq!(
        manifest.assets.get("wallpaper").map(String::as_str),
        Some("assets/wallpaper.png")
    );
    assert_eq!(
        manifest.layouts.get("default").map(String::as_str),
        Some("layouts/default.json")
    );
    let ids: Vec<&str> = manifest.surfaces.keys().map(String::as_str).collect();
    assert_eq!(ids, vec!["control-center", "launcher", "panel"]);
    let panel = &manifest.surfaces["panel"];
    assert_eq!(panel.entry, "dist/panel/index.html");
    assert_eq!(
        panel.placement.as_ref().and_then(|p| p.get("height")),
        Some(&json!(48))
    );
}

#[test]
fn leading_byte_order_mark_is_tolerated() {
    let text = format!("\u{feff}{}", minimal());
    assert!(parse_theme_manifest(&text).is_ok());
}

#[test]
fn format_is_labelled_proposed() {
    assert_eq!(FORMAT_STATUS, "proposed");
}

// ------------------------------------------------- ticket-mandated rejections

#[test]
fn absent_id_is_rejected() {
    let err = expect_err(&without(minimal(), "id"));
    assert_eq!(err.code(), ErrorCode::MissingId);
    assert_eq!(err.code().as_str(), "GLINT_MANIFEST_MISSING_ID");
    assert_eq!(err.pointer(), "/id");
}

#[test]
fn unsupported_schema_version_is_rejected() {
    for bad in [json!(0), json!(2), json!(1.5), json!(-1)] {
        let err = expect_err(&with(minimal(), "/schemaVersion", bad.clone()));
        assert_eq!(
            err.code(),
            ErrorCode::UnsupportedSchemaVersion,
            "schemaVersion {bad}"
        );
        assert_eq!(err.pointer(), "/schemaVersion");
    }
}

#[test]
fn schema_version_of_wrong_type_or_absent_is_rejected() {
    let err = expect_err(&with(minimal(), "/schemaVersion", json!("1")));
    assert_eq!(err.code(), ErrorCode::InvalidType);
    let err = expect_err(&without(minimal(), "schemaVersion"));
    assert_eq!(err.code(), ErrorCode::MissingSchemaVersion);
}

#[test]
fn wrong_kind_is_rejected() {
    for bad in ["widget", "profile", "Theme", ""] {
        let err = expect_err(&with(minimal(), "/kind", json!(bad)));
        assert_eq!(err.code(), ErrorCode::WrongKind, "kind {bad:?}");
        assert_eq!(err.pointer(), "/kind");
    }
    assert_eq!(
        expect_err(&without(minimal(), "kind")).code(),
        ErrorCode::MissingKind
    );
}

#[test]
fn ticket_rejections_have_distinct_codes() {
    let codes: BTreeSet<&str> = [
        expect_err(&without(minimal(), "id")).code().as_str(),
        expect_err(&with(minimal(), "/schemaVersion", json!(2)))
            .code()
            .as_str(),
        expect_err(&with(minimal(), "/kind", json!("widget")))
            .code()
            .as_str(),
    ]
    .into_iter()
    .collect();
    assert_eq!(codes.len(), 3);
}

#[test]
fn unknown_top_level_field_is_rejected() {
    for field in ["scripts", "postInstall", "integrations", "capabilities"] {
        let err = expect_err(&with(minimal(), &format!("/{field}"), json!({})));
        assert_eq!(err.code(), ErrorCode::UnknownField, "field {field}");
        assert_eq!(err.pointer(), format!("/{field}"));
    }
}

#[test]
fn unknown_security_sensitive_nested_fields_are_rejected() {
    let base: Value = serde_json::from_str(PLAN_EXAMPLE).expect("json");
    let cases = [
        ("/renderer/allowNetwork", json!(true)),
        ("/renderer/nodeIntegration", json!(true)),
        ("/compatibility/minimumOs", json!("10")),
        ("/surfaces/panel/preload", json!("evil.js")),
    ];
    for (pointer, value) in cases {
        let err = expect_err(&with(base.clone(), pointer, value));
        assert_eq!(err.code(), ErrorCode::UnknownField, "{pointer}");
        assert_eq!(err.pointer(), pointer);
    }
}

#[test]
fn unknown_author_fields_are_ignored_not_rejected() {
    // SCH-003: `author` is non-sensitive display metadata; unknown keys are
    // reported as warnings by parse_theme_manifest_with_diagnostics.
    let base: Value = serde_json::from_str(PLAN_EXAMPLE).expect("json");
    let manifest = parse(&with(base, "/author/token", json!("secret"))).expect("accepted");
    assert_eq!(manifest.author.name, "Example Developer");
}

#[test]
fn duplicate_keys_are_rejected() {
    let text = r#"{"schemaVersion":1,"kind":"theme","id":"dev.example.a","id":"dev.example.b",
        "name":"x","version":"1.0.0","author":{"name":"a"},"license":"MIT",
        "compatibility":{"glint":">=0.1.0","platforms":["windows"]}}"#;
    let err = parse_theme_manifest(text).expect_err("duplicate key");
    assert_eq!(err.code(), ErrorCode::DuplicateKey);
}

// ------------------------------------------------------ field-level validation

#[test]
fn invalid_id_format_is_rejected() {
    for bad in [
        "midnight",
        "Dev.Example.Midnight",
        "dev..midnight",
        ".dev.midnight",
        "dev.midnight.",
        "dev.example/midnight",
        "dev.-x.midnight",
        "dev.example.midnight-",
        "",
    ] {
        let err = expect_err(&with(minimal(), "/id", json!(bad)));
        assert_eq!(err.code(), ErrorCode::InvalidId, "id {bad:?}");
        assert_eq!(err.pointer(), "/id");
    }
    let long = format!("dev.{}", "a".repeat(200));
    assert_eq!(
        expect_err(&with(minimal(), "/id", json!(long))).code(),
        ErrorCode::InvalidId
    );
    assert_eq!(
        expect_err(&with(minimal(), "/id", json!(7))).code(),
        ErrorCode::InvalidType
    );
}

#[test]
fn valid_id_forms_are_accepted() {
    for good in [
        "official.mono",
        "community.glassdock",
        "me.minimal-notifs",
        "dev.example2.mid-night",
    ] {
        assert!(
            parse(&with(minimal(), "/id", json!(good))).is_ok(),
            "id {good:?}"
        );
    }
}

#[test]
fn invalid_semver_is_rejected() {
    for bad in ["1.0", "v1.0.0", "1.0.0.0", "latest", ""] {
        let err = expect_err(&with(minimal(), "/version", json!(bad)));
        assert_eq!(err.code(), ErrorCode::InvalidVersion, "version {bad:?}");
        assert_eq!(err.pointer(), "/version");
    }
    let ok = parse(&with(minimal(), "/version", json!("1.2.3-beta.1"))).expect("pre-release");
    assert_eq!(ok.version.pre.as_str(), "beta.1");
}

#[test]
fn invalid_compatibility_is_rejected() {
    let err = expect_err(&with(minimal(), "/compatibility/glint", json!("newest")));
    assert_eq!(err.code(), ErrorCode::InvalidVersionRequirement);
    assert_eq!(err.pointer(), "/compatibility/glint");

    let err = expect_err(&with(
        minimal(),
        "/compatibility/platforms",
        json!(["windows", "beos"]),
    ));
    assert_eq!(err.code(), ErrorCode::InvalidValue);
    assert_eq!(err.pointer(), "/compatibility/platforms/1");

    let err = expect_err(&with(minimal(), "/compatibility/platforms", json!([])));
    assert_eq!(err.code(), ErrorCode::InvalidValue);

    let err = expect_err(&with(
        minimal(),
        "/compatibility/platforms",
        json!(["linux", "linux"]),
    ));
    assert_eq!(err.code(), ErrorCode::InvalidValue);
    assert_eq!(err.pointer(), "/compatibility/platforms/1");
}

#[test]
fn missing_required_fields_report_their_pointer() {
    for field in ["name", "version", "author", "license", "compatibility"] {
        let err = expect_err(&without(minimal(), field));
        assert_eq!(err.code(), ErrorCode::MissingField, "field {field}");
        assert_eq!(err.pointer(), format!("/{field}"));
    }
    let err = expect_err(&with(
        minimal(),
        "/compatibility",
        json!({ "platforms": ["windows"] }),
    ));
    assert_eq!(err.code(), ErrorCode::MissingField);
    assert_eq!(err.pointer(), "/compatibility/glint");
}

#[test]
fn blank_text_fields_are_rejected() {
    for pointer in ["/name", "/license", "/author/name"] {
        let err = expect_err(&with(minimal(), pointer, json!("   ")));
        assert_eq!(err.code(), ErrorCode::InvalidValue, "{pointer}");
        assert_eq!(err.pointer(), pointer);
    }
}

#[test]
fn unsafe_package_paths_are_rejected() {
    for bad in [
        "../outside.json",
        "theme/../../x.json",
        "/etc/passwd",
        "C:/Windows/win.ini",
        "C:relative.json",
        "\\\\server\\share\\x.json",
        "theme\\tokens.json",
        "https://example.com/tokens.json",
        "file:///c/tokens.json",
        "theme//tokens.json",
        "./theme/tokens.json",
        "theme/tokens.json/",
        "",
    ] {
        let err = expect_err(&with(minimal(), "/tokens", json!(bad)));
        assert_eq!(err.code(), ErrorCode::InvalidPath, "path {bad:?}");
        assert_eq!(err.pointer(), "/tokens");
    }
    let base: Value = serde_json::from_str(PLAN_EXAMPLE).expect("json");
    let err = expect_err(&with(
        base.clone(),
        "/assets/wallpaper",
        json!("../wall.png"),
    ));
    assert_eq!(err.code(), ErrorCode::InvalidPath);
    assert_eq!(err.pointer(), "/assets/wallpaper");
    let err = expect_err(&with(
        base,
        "/surfaces/panel/entry",
        json!("https://evil.example/panel.html"),
    ));
    assert_eq!(err.code(), ErrorCode::InvalidPath);
    assert_eq!(err.pointer(), "/surfaces/panel/entry");
}

#[test]
fn surface_entry_must_be_html() {
    let base: Value = serde_json::from_str(PLAN_EXAMPLE).expect("json");
    let err = expect_err(&with(
        base,
        "/surfaces/panel/entry",
        json!("dist/panel/index.js"),
    ));
    assert_eq!(err.code(), ErrorCode::InvalidPath);
    assert_eq!(err.pointer(), "/surfaces/panel/entry");
}

#[test]
fn surfaces_require_a_renderer() {
    let base: Value = serde_json::from_str(PLAN_EXAMPLE).expect("json");
    let err = expect_err(&without(base, "renderer"));
    assert_eq!(err.code(), ErrorCode::MissingField);
    assert_eq!(err.pointer(), "/renderer");
}

#[test]
fn unsupported_renderer_is_rejected() {
    let base: Value = serde_json::from_str(PLAN_EXAMPLE).expect("json");
    let err = expect_err(&with(base.clone(), "/renderer/kind", json!("native-dll")));
    assert_eq!(err.code(), ErrorCode::UnsupportedRenderer);
    assert_eq!(err.pointer(), "/renderer/kind");
    let err = expect_err(&with(base, "/renderer/entrypointVersion", json!(2)));
    assert_eq!(err.code(), ErrorCode::UnsupportedRenderer);
    assert_eq!(err.pointer(), "/renderer/entrypointVersion");
}

#[test]
fn invalid_capabilities_are_rejected() {
    let cases = [
        (json!(["Surface.Panel"]), "/requiredCapabilities/0"),
        (json!(["panel"]), "/requiredCapabilities/0"),
        (json!(["shell.exec"]), "/requiredCapabilities/0"),
        (
            json!(["surface.panel", "surface.panel"]),
            "/requiredCapabilities/1",
        ),
    ];
    for (caps, pointer) in cases {
        let err = expect_err(&with(minimal(), "/requiredCapabilities", caps.clone()));
        assert_eq!(err.code(), ErrorCode::InvalidCapability, "{caps}");
        assert_eq!(err.pointer(), pointer);
    }
    let both = with(
        with(minimal(), "/requiredCapabilities", json!(["surface.panel"])),
        "/optionalCapabilities",
        json!(["surface.panel"]),
    );
    let err = expect_err(&both);
    assert_eq!(err.code(), ErrorCode::InvalidCapability);
    assert_eq!(err.pointer(), "/optionalCapabilities/0");
}

#[test]
fn invalid_map_keys_are_rejected() {
    let base: Value = serde_json::from_str(PLAN_EXAMPLE).expect("json");
    let err = expect_err(&with(
        base.clone(),
        "/surfaces/Bad Surface",
        json!({ "entry": "a.html" }),
    ));
    assert_eq!(err.code(), ErrorCode::InvalidValue);
    assert_eq!(err.pointer(), "/surfaces/Bad Surface");
    let ok = with(
        base.clone(),
        "/surfaces/custom:dev.example.clock",
        json!({ "entry": "dist/clock/index.html" }),
    );
    assert!(parse(&ok).is_ok());
    let mut tricky = base;
    tricky["assets"]
        .as_object_mut()
        .expect("assets object")
        .insert("a~b/c".to_owned(), json!("x.png"));
    let err = expect_err(&tricky);
    assert_eq!(err.code(), ErrorCode::InvalidValue);
    // JSON Pointer escaping (RFC 6901): `~` -> `~0`, `/` -> `~1`.
    assert_eq!(err.pointer(), "/assets/a~0b~1c");
}

#[test]
fn malformed_json_and_non_object_root_are_rejected() {
    assert_eq!(
        parse_theme_manifest("{ not json").unwrap_err().code(),
        ErrorCode::InvalidJson
    );
    assert_eq!(
        parse_theme_manifest("").unwrap_err().code(),
        ErrorCode::InvalidJson
    );
    let err = parse_theme_manifest("[1]").unwrap_err();
    assert_eq!(err.code(), ErrorCode::NotAnObject);
    assert_eq!(err.pointer(), "");
}

#[test]
fn oversized_manifest_is_rejected() {
    let padding = "a".repeat(glint_schema::MAX_MANIFEST_BYTES);
    let text = with(minimal(), "/description", json!(padding)).to_string();
    assert_eq!(
        parse_theme_manifest(&text).unwrap_err().code(),
        ErrorCode::TooLarge
    );
}

// ------------------------------------------------------- diagnostic stability

#[test]
fn error_codes_are_stable() {
    let expected = [
        (ErrorCode::InvalidJson, "GLINT_MANIFEST_INVALID_JSON"),
        (ErrorCode::TooLarge, "GLINT_MANIFEST_TOO_LARGE"),
        (ErrorCode::DuplicateKey, "GLINT_MANIFEST_DUPLICATE_KEY"),
        (ErrorCode::NotAnObject, "GLINT_MANIFEST_NOT_AN_OBJECT"),
        (
            ErrorCode::MissingSchemaVersion,
            "GLINT_MANIFEST_MISSING_SCHEMA_VERSION",
        ),
        (
            ErrorCode::UnsupportedSchemaVersion,
            "GLINT_MANIFEST_UNSUPPORTED_SCHEMA_VERSION",
        ),
        (ErrorCode::MissingKind, "GLINT_MANIFEST_MISSING_KIND"),
        (ErrorCode::WrongKind, "GLINT_MANIFEST_WRONG_KIND"),
        (ErrorCode::MissingId, "GLINT_MANIFEST_MISSING_ID"),
        (ErrorCode::InvalidId, "GLINT_MANIFEST_INVALID_ID"),
        (ErrorCode::MissingField, "GLINT_MANIFEST_MISSING_FIELD"),
        (ErrorCode::UnknownField, "GLINT_MANIFEST_UNKNOWN_FIELD"),
        (ErrorCode::InvalidType, "GLINT_MANIFEST_INVALID_TYPE"),
        (ErrorCode::InvalidValue, "GLINT_MANIFEST_INVALID_VALUE"),
        (ErrorCode::InvalidVersion, "GLINT_MANIFEST_INVALID_VERSION"),
        (
            ErrorCode::InvalidVersionRequirement,
            "GLINT_MANIFEST_INVALID_VERSION_REQUIREMENT",
        ),
        (ErrorCode::InvalidPath, "GLINT_MANIFEST_INVALID_PATH"),
        (
            ErrorCode::InvalidCapability,
            "GLINT_MANIFEST_INVALID_CAPABILITY",
        ),
        (
            ErrorCode::UnsupportedRenderer,
            "GLINT_MANIFEST_UNSUPPORTED_RENDERER",
        ),
    ];
    assert_eq!(expected.len(), ErrorCode::ALL.len());
    for (code, text) in expected {
        assert_eq!(code.as_str(), text);
        assert!(ErrorCode::ALL.contains(&code));
    }
    let unique: BTreeSet<&str> = ErrorCode::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(unique.len(), ErrorCode::ALL.len());
}

#[test]
fn error_display_includes_code_and_pointer() {
    let err = expect_err(&without(minimal(), "id"));
    let text = err.to_string();
    assert!(
        text.starts_with("GLINT_MANIFEST_MISSING_ID at /id: "),
        "{text}"
    );
    let root = parse_theme_manifest("[]").unwrap_err().to_string();
    assert!(
        root.starts_with("GLINT_MANIFEST_NOT_AN_OBJECT at (root): "),
        "{root}"
    );
}

// --------------------------------------------- repository fixture and schema

#[test]
fn valid_minimal_fixture_parses() {
    let path = repo_root().join("tests/package-fixtures/valid-minimal/glint.json");
    let text = std::fs::read_to_string(&path).expect("fixture readable");
    let manifest = parse_theme_manifest(&text).expect("fixture is valid");
    assert_eq!(manifest.kind, PackageKind::Theme);
}

#[test]
fn json_schema_is_valid_json_and_marked_proposed() {
    let schema = read_json(repo_root().join("schemas/glint-theme-v1.schema.json"));
    assert_eq!(
        schema["$schema"],
        json!("https://json-schema.org/draft/2020-12/schema")
    );
    assert_eq!(schema["x-glint-status"], json!(FORMAT_STATUS));
    let comment = schema["$comment"].as_str().expect("$comment");
    assert!(comment.to_lowercase().contains("proposed"), "{comment}");
    assert_eq!(schema["type"], json!("object"));
    assert_eq!(schema["additionalProperties"], json!(false));
    assert_eq!(schema["properties"]["schemaVersion"]["const"], json!(1));
    assert_eq!(schema["properties"]["kind"]["const"], json!("theme"));
}

#[test]
fn json_schema_covers_ticket_fields() {
    let schema = read_json(repo_root().join("schemas/glint-theme-v1.schema.json"));
    let props = schema["properties"].as_object().expect("properties");
    for field in [
        "schemaVersion",
        "kind",
        "id",
        "version",
        "name",
        "author",
        "license",
        "compatibility",
        "renderer",
        "requiredCapabilities",
        "optionalCapabilities",
        "tokens",
        "assets",
        "layouts",
        "surfaces",
    ] {
        assert!(props.contains_key(field), "schema lacks {field}");
    }
}

#[test]
fn json_schema_and_parser_agree_on_fields() {
    let schema = read_json(repo_root().join("schemas/glint-theme-v1.schema.json"));
    let props: BTreeSet<String> = schema["properties"]
        .as_object()
        .expect("properties")
        .keys()
        .cloned()
        .collect();
    let required: BTreeSet<String> = schema["required"]
        .as_array()
        .expect("required")
        .iter()
        .map(|v| v.as_str().expect("string").to_owned())
        .collect();

    // The fixture carries exactly the schema's required fields, and every key
    // in the fixture and in the plan example is a declared schema property.
    let fixture = read_json(repo_root().join("tests/package-fixtures/valid-minimal/glint.json"));
    let fixture_keys: BTreeSet<String> = fixture
        .as_object()
        .expect("object")
        .keys()
        .filter(|k| *k != "$schema")
        .cloned()
        .collect();
    assert_eq!(fixture_keys, required);
    let example: Value = serde_json::from_str(PLAN_EXAMPLE).expect("json");
    for key in example.as_object().expect("object").keys() {
        assert!(props.contains(key), "plan example key {key} not in schema");
    }

    // Removing any schema-required field makes the parser fail; every
    // schema property is accepted by the parser (no UnknownField).
    for field in &required {
        assert!(
            parse(&without(fixture.clone(), field)).is_err(),
            "parser accepts manifest without schema-required {field}"
        );
    }
    let accepted = parse_theme_manifest(PLAN_EXAMPLE).map(|_| ());
    assert!(accepted.is_ok());
    for prop in &props {
        if example.get(prop).is_none() && prop != "$schema" && prop != "description" {
            panic!("schema property {prop} not exercised by the plan example");
        }
    }
    assert!(parse(&with(minimal(), "/$schema", json!("../../schemas/x.json"))).is_ok());
}
