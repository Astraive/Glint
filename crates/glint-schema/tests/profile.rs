//! Proposed `profile.json` and `profile.lock` v1 formats (SCH-006).
//!
//! Run with `cargo test -p glint-schema --test profile`. File fixtures live
//! in `tests/profile-fixtures/`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use glint_schema::{
    LocalOnlyReason, LockProblemCode, PackageKind, PackageOrigin, Profile, ProfileError,
    ProfileErrorCode, ProfileLock, SUPPORTED_PROFILE_SCHEMA_VERSION, TokenValue, Wallpaper,
    parse_profile, parse_profile_lock, verify_profile_lock,
};
use serde_json::{Value, json};

const DIGEST: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000001";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(path: &str) -> String {
    let path = repo_root().join("tests/profile-fixtures").join(path);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn example_profile() -> Profile {
    parse_profile(&fixture("custom-mix/profile.json")).expect("example profile parses")
}

fn example_lock() -> ProfileLock {
    parse_profile_lock(&fixture("custom-mix/profile.lock")).expect("example lock parses")
}

fn minimal() -> Value {
    json!({ "schemaVersion": 1, "id": "work-setup", "name": "Work" })
}

fn lock_doc() -> Value {
    json!({
        "schemaVersion": 1,
        "profile": "work-setup",
        "packages": [{
            "kind": "theme",
            "id": "official.mono",
            "version": "1.0.0",
            "origin": "registry:https://registry.example/glint-library",
            "digest": DIGEST
        }]
    })
}

/// Replace the value at a JSON Pointer, creating the last key if needed.
fn with(mut doc: Value, pointer: &str, value: Value) -> Value {
    let (parent, key) = pointer.rsplit_once('/').expect("pointer");
    let target = if parent.is_empty() {
        &mut doc
    } else {
        doc.pointer_mut(parent).expect("parent exists")
    };
    match target {
        Value::Object(map) => {
            map.insert(key.replace("~1", "/").replace("~0", "~"), value);
        }
        Value::Array(items) => items[key.parse::<usize>().expect("index")] = value,
        _ => panic!("parent is not a container"),
    }
    doc
}

fn without(mut doc: Value, pointer: &str) -> Value {
    let (parent, key) = pointer.rsplit_once('/').expect("pointer");
    let target = if parent.is_empty() {
        &mut doc
    } else {
        doc.pointer_mut(parent).expect("parent exists")
    };
    target.as_object_mut().expect("object").remove(key);
    doc
}

fn profile_err(doc: &Value) -> ProfileError {
    parse_profile(&doc.to_string()).expect_err("profile must be rejected")
}

fn lock_err(doc: &Value) -> ProfileError {
    parse_profile_lock(&doc.to_string()).expect_err("lock must be rejected")
}

fn assert_err(err: &ProfileError, code: ProfileErrorCode, pointer: &str) {
    assert_eq!(
        (err.code(), err.pointer()),
        (code, pointer),
        "unexpected error: {err}"
    );
}

// ------------------------------------------------------- acceptance

#[test]
fn example_profile_parses() {
    let profile = example_profile();
    assert_eq!(profile.schema_version, SUPPORTED_PROFILE_SCHEMA_VERSION);
    assert_eq!(profile.id, "profile.custom-mix");
    assert_eq!(profile.name, "Custom Mix");
    let base = profile.base_theme.as_ref().expect("base theme");
    assert_eq!(base.key.id(), "official.mono");
    assert_eq!(base.key.kind(), PackageKind::Theme);
    assert_eq!(base.version.to_string(), "1.0.0");
    assert_eq!(
        profile.surfaces.keys().collect::<Vec<_>>(),
        ["dock", "notification"]
    );
    assert_eq!(profile.surfaces["dock"].key.id(), "community.glassdock");
    assert_eq!(
        profile.wallpaper,
        Some(Wallpaper::UserFile {
            path: "user-assets/wallpaper.png".to_owned()
        })
    );
    assert_eq!(profile.monitors["primary"]["panel"]["height"], json!(44));
    assert_eq!(
        profile.monitors["secondary"]["dock"]["visible"],
        json!(false)
    );
    assert_eq!(
        profile.token_overrides["background.default"],
        TokenValue::String("#0B0C0F".to_owned())
    );
}

#[test]
fn minimal_profile_parses_with_defaults() {
    let profile = parse_profile(&fixture("minimal/profile.json")).expect("minimal parses");
    assert_eq!(profile.id, "default");
    assert!(profile.base_theme.is_none());
    assert!(profile.surfaces.is_empty());
    assert!(profile.wallpaper.is_none());
    assert!(profile.monitors.is_empty());
    assert!(profile.token_overrides.is_empty());
    assert!(profile.local_only_refs().is_empty());
}

#[test]
fn example_lock_parses_and_covers_the_example_profile() {
    let lock = example_lock();
    assert_eq!(lock.profile, "profile.custom-mix");
    assert_eq!(lock.packages.len(), 3);
    assert_eq!(lock.packages[2].origin, PackageOrigin::local());
    assert_eq!(
        lock.packages[1].origin.as_str(),
        "git:https://git.example/community/themes#themes/glassdock"
    );
    assert_eq!(verify_profile_lock(&example_profile(), &lock), []);
}

#[test]
fn lock_without_digest_is_rejected() {
    let err = parse_profile_lock(&fixture("lock-missing-digest/profile.lock"))
        .expect_err("a lock without digest must be rejected");
    assert_err(&err, ProfileErrorCode::MissingField, "/packages/0/digest");
    assert_eq!(err.code().as_str(), "GLINT_PROFILE_MISSING_FIELD");
}

// ------------------------------------------------- private assets

#[test]
fn private_wallpaper_is_flagged_local_only_and_stripped_on_export() {
    let profile = example_profile();
    let refs = profile.local_only_refs();
    assert_eq!(refs.len(), 1, "{refs:?}");
    assert_eq!(refs[0].pointer, "/wallpaper");
    assert_eq!(refs[0].reason, LocalOnlyReason::UserAsset);

    let exported = profile.without_local_only();
    assert!(exported.wallpaper.is_none());
    assert!(exported.local_only_refs().is_empty());
    // Everything portable survives.
    assert_eq!(exported.base_theme, profile.base_theme);
    assert_eq!(exported.surfaces, profile.surfaces);
    assert_eq!(exported.monitors, profile.monitors);
    assert_eq!(exported.token_overrides, profile.token_overrides);
}

#[test]
fn theme_asset_wallpaper_is_portable() {
    let doc = with(minimal(), "/wallpaper", json!({ "asset": "wallpaper" }));
    let profile = parse_profile(&doc.to_string()).unwrap();
    assert_eq!(
        profile.wallpaper,
        Some(Wallpaper::ThemeAsset {
            name: "wallpaper".to_owned()
        })
    );
    assert!(profile.local_only_refs().is_empty());
}

#[test]
fn machine_specific_monitor_keys_are_local_only() {
    let doc = with(
        minimal(),
        "/monitors",
        json!({
            "primary": { "panel": {} },
            "dell-u2720q-a1b2": { "dock": { "visible": false } }
        }),
    );
    let profile = parse_profile(&doc.to_string()).unwrap();
    let refs = profile.local_only_refs();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].pointer, "/monitors/dell-u2720q-a1b2");
    assert_eq!(refs[0].reason, LocalOnlyReason::MachineMonitor);
    let exported = profile.without_local_only();
    assert_eq!(exported.monitors.keys().collect::<Vec<_>>(), ["primary"]);
}

#[test]
fn private_paths_must_stay_inside_the_profile_directory() {
    for bad in [
        "C:/Users/someone/Pictures/w.png",
        "C:\\Users\\someone\\w.png",
        "/home/someone/w.png",
        "../w.png",
        "user-assets/../../w.png",
        "\\\\server\\share\\w.png",
        "file:///w.png",
        "",
    ] {
        let doc = with(minimal(), "/wallpaper", json!({ "path": bad }));
        assert_err(
            &profile_err(&doc),
            ProfileErrorCode::InvalidPath,
            "/wallpaper/path",
        );
    }
}

#[test]
fn wallpaper_needs_exactly_one_source() {
    for value in [
        json!({}),
        json!({ "path": "user-assets/w.png", "asset": "wallpaper" }),
    ] {
        let doc = with(minimal(), "/wallpaper", value);
        assert_err(
            &profile_err(&doc),
            ProfileErrorCode::InvalidValue,
            "/wallpaper",
        );
    }
    let doc = with(
        minimal(),
        "/wallpaper",
        json!({ "url": "https://x.example/w.png" }),
    );
    assert_err(
        &profile_err(&doc),
        ProfileErrorCode::UnknownField,
        "/wallpaper/url",
    );
    let doc = with(minimal(), "/wallpaper", json!({ "asset": "Wallpaper" }));
    assert_err(
        &profile_err(&doc),
        ProfileErrorCode::InvalidValue,
        "/wallpaper/asset",
    );
}

// ----------------------------------------------------- profile errors

#[test]
fn profile_ids_accept_local_names() {
    for id in ["default", "work-setup", "profile.custom-mix", "a1.b-2.c"] {
        let doc = with(minimal(), "/id", json!(id));
        assert_eq!(parse_profile(&doc.to_string()).unwrap().id, id);
    }
    for id in ["", "Work", "work_setup", "-x", "x-", "a..b", ".a", "a b"] {
        let doc = with(minimal(), "/id", json!(id));
        assert_err(
            &profile_err(&doc),
            ProfileErrorCode::InvalidProfileId,
            "/id",
        );
    }
    let doc = with(minimal(), "/id", json!("a".repeat(129)));
    assert_err(
        &profile_err(&doc),
        ProfileErrorCode::InvalidProfileId,
        "/id",
    );
}

#[test]
fn required_profile_fields() {
    assert_err(
        &profile_err(&without(minimal(), "/schemaVersion")),
        ProfileErrorCode::MissingSchemaVersion,
        "/schemaVersion",
    );
    assert_err(
        &profile_err(&without(minimal(), "/id")),
        ProfileErrorCode::MissingField,
        "/id",
    );
    assert_err(
        &profile_err(&without(minimal(), "/name")),
        ProfileErrorCode::MissingField,
        "/name",
    );
    for bad in [json!(0), json!(2), json!(-1), json!(1.5)] {
        assert_err(
            &profile_err(&with(minimal(), "/schemaVersion", bad)),
            ProfileErrorCode::UnsupportedSchemaVersion,
            "/schemaVersion",
        );
    }
    assert_err(
        &profile_err(&with(minimal(), "/schemaVersion", json!("1"))),
        ProfileErrorCode::InvalidType,
        "/schemaVersion",
    );
}

#[test]
fn unknown_fields_are_rejected() {
    let cases = [
        ("/kind", json!("profile")),
        (
            "/baseTheme",
            json!({ "id": "official.mono", "version": "1.0.0", "origin": "local" }),
        ),
        (
            "/surfaces",
            json!({ "dock": { "theme": "a.b", "version": "1.0.0", "x": 1 } }),
        ),
    ];
    let pointers = ["/kind", "/baseTheme/origin", "/surfaces/dock/x"];
    for ((pointer, value), expected) in cases.into_iter().zip(pointers) {
        let doc = with(minimal(), pointer, value);
        assert_err(&profile_err(&doc), ProfileErrorCode::UnknownField, expected);
    }
}

#[test]
fn monitor_placement_objects_stay_open() {
    let doc = with(
        minimal(),
        "/monitors",
        json!({ "primary": { "panel": { "futureKey": { "nested": [1, 2] } } } }),
    );
    let profile = parse_profile(&doc.to_string()).unwrap();
    assert_eq!(
        profile.monitors["primary"]["panel"]["futureKey"]["nested"],
        json!([1, 2])
    );
}

#[test]
fn theme_references_are_checked() {
    let doc = with(
        minimal(),
        "/baseTheme",
        json!({ "id": "mono", "version": "1.0.0" }),
    );
    assert_err(
        &profile_err(&doc),
        ProfileErrorCode::InvalidPackageId,
        "/baseTheme/id",
    );
    for bad in ["1.0", "^1.0.0", ">=1.0.0", "v1.0.0", "latest"] {
        let doc = with(
            minimal(),
            "/baseTheme",
            json!({ "id": "official.mono", "version": bad }),
        );
        assert_err(
            &profile_err(&doc),
            ProfileErrorCode::InvalidVersion,
            "/baseTheme/version",
        );
    }
    let doc = with(minimal(), "/baseTheme", json!({ "id": "official.mono" }));
    assert_err(
        &profile_err(&doc),
        ProfileErrorCode::MissingField,
        "/baseTheme/version",
    );
    let doc = with(
        minimal(),
        "/surfaces",
        json!({ "dock": { "version": "1.0.0" } }),
    );
    assert_err(
        &profile_err(&doc),
        ProfileErrorCode::MissingField,
        "/surfaces/dock/theme",
    );
}

#[test]
fn surface_ids_are_checked() {
    let ok = with(
        minimal(),
        "/surfaces",
        json!({ "custom:dev.example.clock": { "theme": "dev.example.clock", "version": "0.1.0" } }),
    );
    assert!(parse_profile(&ok.to_string()).is_ok());
    for bad in ["Dock", "custom:", "custom:clock", "dock/x", "a~b"] {
        let doc = with(
            minimal(),
            "/surfaces",
            json!({ bad: { "theme": "a.b", "version": "1.0.0" } }),
        );
        let pointer = format!("/surfaces/{}", bad.replace('~', "~0").replace('/', "~1"));
        assert_err(
            &profile_err(&doc),
            ProfileErrorCode::InvalidSurface,
            &pointer,
        );
    }
    let doc = with(
        minimal(),
        "/monitors",
        json!({ "primary": { "Panel": {} } }),
    );
    assert_err(
        &profile_err(&doc),
        ProfileErrorCode::InvalidSurface,
        "/monitors/primary/Panel",
    );
    let doc = with(minimal(), "/monitors", json!({ "Primary": {} }));
    assert_err(
        &profile_err(&doc),
        ProfileErrorCode::InvalidValue,
        "/monitors/Primary",
    );
    let doc = with(
        minimal(),
        "/monitors",
        json!({ "primary": { "panel": true } }),
    );
    assert_err(
        &profile_err(&doc),
        ProfileErrorCode::InvalidType,
        "/monitors/primary/panel",
    );
}

#[test]
fn one_version_per_theme_in_a_profile() {
    let doc = with(
        with(
            minimal(),
            "/baseTheme",
            json!({ "id": "official.mono", "version": "1.0.0" }),
        ),
        "/surfaces",
        json!({ "dock": { "theme": "official.mono", "version": "1.1.0" } }),
    );
    assert_err(
        &profile_err(&doc),
        ProfileErrorCode::ConflictingVersions,
        "/surfaces/dock/version",
    );
    // The same version twice is fine.
    let doc = with(
        with(
            minimal(),
            "/baseTheme",
            json!({ "id": "official.mono", "version": "1.0.0" }),
        ),
        "/surfaces",
        json!({ "dock": { "theme": "official.mono", "version": "1.0.0" } }),
    );
    assert!(parse_profile(&doc.to_string()).is_ok());
}

#[test]
fn token_overrides_are_checked() {
    let doc = with(
        minimal(),
        "/tokenOverrides",
        json!({ "radius.panel": 8, "color.accentStrong": "#fff" }),
    );
    let profile = parse_profile(&doc.to_string()).unwrap();
    assert!(matches!(
        profile.token_overrides["radius.panel"],
        TokenValue::Number(_)
    ));
    for bad_name in ["", "1color", "color..x", "color.", "color x", "color/x"] {
        let doc = with(minimal(), "/tokenOverrides", json!({ bad_name: "#fff" }));
        let pointer = format!("/tokenOverrides/{}", bad_name.replace('/', "~1"));
        assert_err(&profile_err(&doc), ProfileErrorCode::InvalidValue, &pointer);
    }
    for (value, code) in [
        (json!(true), ProfileErrorCode::InvalidType),
        (json!(null), ProfileErrorCode::InvalidType),
        (json!({ "a": 1 }), ProfileErrorCode::InvalidType),
        (json!(""), ProfileErrorCode::InvalidValue),
        (json!("a\u{0007}"), ProfileErrorCode::InvalidValue),
        (json!("x".repeat(257)), ProfileErrorCode::InvalidValue),
    ] {
        let doc = with(minimal(), "/tokenOverrides", json!({ "color.x": value }));
        assert_err(&profile_err(&doc), code, "/tokenOverrides/color.x");
    }
}

#[test]
fn document_level_rules_apply() {
    let err = parse_profile("{").unwrap_err();
    assert_eq!(err.code(), ProfileErrorCode::InvalidJson);
    let err = parse_profile("[]").unwrap_err();
    assert_eq!(err.code(), ProfileErrorCode::NotAnObject);
    let err = parse_profile(r#"{"schemaVersion":1,"id":"a","id":"b","name":"x"}"#).unwrap_err();
    assert_eq!(err.code(), ProfileErrorCode::DuplicateKey);
    let big = format!(
        r#"{{"schemaVersion":1,"id":"a","name":"{}"}}"#,
        "x".repeat(glint_schema::MAX_PROFILE_BYTES)
    );
    assert_eq!(
        parse_profile(&big).unwrap_err().code(),
        ProfileErrorCode::TooLarge
    );
    let bom = format!("\u{feff}{}", minimal());
    assert!(parse_profile(&bom).is_ok());
}

// -------------------------------------------------------- lock errors

#[test]
fn every_lock_field_is_required() {
    for field in ["kind", "id", "version", "origin", "digest"] {
        let doc = without(lock_doc(), &format!("/packages/0/{field}"));
        assert_err(
            &lock_err(&doc),
            ProfileErrorCode::MissingField,
            &format!("/packages/0/{field}"),
        );
    }
    for field in ["profile", "packages"] {
        assert_err(
            &lock_err(&without(lock_doc(), &format!("/{field}"))),
            ProfileErrorCode::MissingField,
            &format!("/{field}"),
        );
    }
}

#[test]
fn empty_lock_is_valid() {
    let doc = with(lock_doc(), "/packages", json!([]));
    assert!(
        parse_profile_lock(&doc.to_string())
            .unwrap()
            .packages
            .is_empty()
    );
}

#[test]
fn lock_digests_must_be_canonical_sha256() {
    for bad in [
        json!(""),
        json!("sha256:"),
        json!("0000000000000000000000000000000000000000000000000000000000000001"),
        json!("sha256:000000000000000000000000000000000000000000000000000000000000000G"),
        json!("SHA256:0000000000000000000000000000000000000000000000000000000000000001"),
        json!("md5:00000000000000000000000000000001"),
    ] {
        let doc = with(lock_doc(), "/packages/0/digest", bad);
        assert_err(
            &lock_err(&doc),
            ProfileErrorCode::InvalidDigest,
            "/packages/0/digest",
        );
    }
    let doc = with(lock_doc(), "/packages/0/digest", json!(null));
    assert_err(
        &lock_err(&doc),
        ProfileErrorCode::InvalidType,
        "/packages/0/digest",
    );
}

#[test]
fn lock_origins_must_be_canonical_and_credential_free() {
    for bad in [
        "registry:https://user:token@registry.example/x",
        "registry:http://registry.example/x",
        "community",
        "git:https://git.example/r?ref=main",
    ] {
        let doc = with(lock_doc(), "/packages/0/origin", json!(bad));
        assert_err(
            &lock_err(&doc),
            ProfileErrorCode::InvalidOrigin,
            "/packages/0/origin",
        );
    }
    // Parsable but not canonical: a lock is machine-written, so refuse.
    let doc = with(
        lock_doc(),
        "/packages/0/origin",
        json!("registry:https://Registry.Example/glint-library/"),
    );
    assert_err(
        &lock_err(&doc),
        ProfileErrorCode::InvalidOrigin,
        "/packages/0/origin",
    );
}

#[test]
fn lock_versions_are_exact() {
    for bad in ["^1.0.0", "1", "1.0", "*"] {
        let doc = with(lock_doc(), "/packages/0/version", json!(bad));
        assert_err(
            &lock_err(&doc),
            ProfileErrorCode::InvalidVersion,
            "/packages/0/version",
        );
    }
}

#[test]
fn lock_kind_and_id_are_checked() {
    let doc = with(lock_doc(), "/packages/0/kind", json!("widget"));
    assert_err(
        &lock_err(&doc),
        ProfileErrorCode::InvalidValue,
        "/packages/0/kind",
    );
    let doc = with(lock_doc(), "/packages/0/id", json!("Official.Mono"));
    assert_err(
        &lock_err(&doc),
        ProfileErrorCode::InvalidPackageId,
        "/packages/0/id",
    );
    let doc = with(lock_doc(), "/profile", json!("Work Setup"));
    assert_err(
        &lock_err(&doc),
        ProfileErrorCode::InvalidProfileId,
        "/profile",
    );
}

#[test]
fn a_package_is_pinned_once() {
    let mut doc = lock_doc();
    let entry = doc["packages"][0].clone();
    let mut other_origin = entry.clone();
    other_origin["origin"] = json!("local");
    doc["packages"] = json!([entry, other_origin]);
    assert_err(
        &lock_err(&doc),
        ProfileErrorCode::DuplicatePackage,
        "/packages/1",
    );
}

#[test]
fn unknown_lock_fields_are_rejected() {
    let doc = with(lock_doc(), "/packages/0/branch", json!("main"));
    assert_err(
        &lock_err(&doc),
        ProfileErrorCode::UnknownField,
        "/packages/0/branch",
    );
    let doc = with(lock_doc(), "/generated", json!("2026-11-23"));
    assert_err(
        &lock_err(&doc),
        ProfileErrorCode::UnknownField,
        "/generated",
    );
}

// -------------------------------------------------------- verification

#[test]
fn verification_reports_every_disagreement() {
    let profile = example_profile();
    let mut lock_json: Value = serde_json::from_str(&fixture("custom-mix/profile.lock")).unwrap();
    lock_json["profile"] = json!("other");
    lock_json["packages"][0]["version"] = json!("1.0.1"); // official.mono
    let removed = lock_json["packages"].as_array_mut().unwrap().remove(1); // glassdock
    assert_eq!(removed["id"], json!("community.glassdock"));
    lock_json["packages"].as_array_mut().unwrap().push(json!({
        "kind": "theme",
        "id": "dev.example.stale",
        "version": "0.1.0",
        "origin": "local",
        "digest": DIGEST
    }));
    let lock = parse_profile_lock(&lock_json.to_string()).unwrap();
    let problems: Vec<(LockProblemCode, String)> = verify_profile_lock(&profile, &lock)
        .into_iter()
        .map(|p| (p.code, p.pointer))
        .collect();
    assert_eq!(
        problems,
        [
            (LockProblemCode::ProfileMismatch, "/profile".to_owned()),
            (
                LockProblemCode::VersionMismatch,
                "/baseTheme/version".to_owned()
            ),
            (LockProblemCode::Unlocked, "/surfaces/dock".to_owned()),
            (LockProblemCode::Unused, "/packages/2".to_owned()),
        ]
    );
}

#[test]
fn lock_problem_codes_are_stable() {
    assert_eq!(
        [
            LockProblemCode::ProfileMismatch,
            LockProblemCode::Unlocked,
            LockProblemCode::VersionMismatch,
            LockProblemCode::Unused
        ]
        .map(LockProblemCode::as_str),
        [
            "GLINT_PROFILE_LOCK_PROFILE_MISMATCH",
            "GLINT_PROFILE_LOCK_UNLOCKED",
            "GLINT_PROFILE_LOCK_VERSION_MISMATCH",
            "GLINT_PROFILE_LOCK_UNUSED"
        ]
    );
}

#[test]
fn error_codes_are_stable_and_distinct() {
    let codes: BTreeSet<&str> = ProfileErrorCode::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(codes.len(), ProfileErrorCode::ALL.len());
    assert!(codes.iter().all(|c| c.starts_with("GLINT_PROFILE_")));
    assert_eq!(
        ProfileErrorCode::MissingField.to_string(),
        "GLINT_PROFILE_MISSING_FIELD"
    );
}

// ------------------------------------------------------------ schemas

fn read_schema(name: &str) -> Value {
    let path = repo_root().join("schemas").join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).expect("schema is JSON")
}

fn keys(value: &Value) -> BTreeSet<String> {
    value.as_object().expect("object").keys().cloned().collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn profile_schema_matches_the_parser() {
    let schema = read_schema("glint-profile-v1.schema.json");
    assert_eq!(schema["x-glint-status"], json!("proposed"));
    assert_eq!(schema["additionalProperties"], json!(false));
    assert_eq!(schema["properties"]["schemaVersion"]["const"], json!(1));
    assert_eq!(
        keys(&schema["properties"]),
        set(&[
            "$schema",
            "schemaVersion",
            "id",
            "name",
            "description",
            "baseTheme",
            "surfaces",
            "wallpaper",
            "monitors",
            "tokenOverrides"
        ])
    );
    assert_eq!(schema["required"], json!(["schemaVersion", "id", "name"]));
    assert_eq!(
        keys(&schema["properties"]["baseTheme"]["properties"]),
        set(&["id", "version"])
    );
    assert_eq!(
        keys(&schema["properties"]["surfaces"]["additionalProperties"]["properties"]),
        set(&["theme", "version"])
    );
}

#[test]
fn lock_schema_matches_the_parser() {
    let schema = read_schema("glint-profile-lock-v1.schema.json");
    assert_eq!(schema["x-glint-status"], json!("proposed"));
    assert_eq!(schema["additionalProperties"], json!(false));
    assert_eq!(
        keys(&schema["properties"]),
        set(&["$schema", "schemaVersion", "profile", "packages"])
    );
    let entry = &schema["properties"]["packages"]["items"];
    assert_eq!(entry["additionalProperties"], json!(false));
    let fields = ["kind", "id", "version", "origin", "digest"];
    assert_eq!(keys(&entry["properties"]), set(&fields));
    assert_eq!(entry["required"], json!(fields));
    assert_eq!(
        entry["properties"]["digest"]["pattern"],
        json!("^sha256:[0-9a-f]{64}$")
    );
}
