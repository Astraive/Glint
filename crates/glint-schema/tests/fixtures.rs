//! Golden manifest corpus shared with the TypeScript validator (SCH-002).
//!
//! Run with `cargo test -p glint-schema --test fixtures`. The corpus lives in
//! `tests/package-fixtures/manifests/`; `packages/theme-schema` runs the same
//! files through its validator and must reach identical verdicts and codes.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use glint_schema::{
    ErrorCode, WarningCode, parse_theme_manifest, parse_theme_manifest_with_diagnostics,
};
use serde_json::{Map, Value, json};

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/package-fixtures/manifests")
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{} is not JSON: {e}", path.display()))
}

fn expectations() -> Value {
    read_json(&corpus().join("expectations.json"))
}

fn section<'a>(expectations: &'a Value, name: &str) -> &'a Map<String, Value> {
    expectations[name]
        .as_object()
        .unwrap_or_else(|| panic!("expectations.{name} is an object"))
}

fn files_in(dir: &str) -> BTreeSet<String> {
    std::fs::read_dir(corpus().join(dir))
        .unwrap_or_else(|e| panic!("cannot list {dir}: {e}"))
        .map(|entry| {
            entry
                .expect("directory entry")
                .file_name()
                .into_string()
                .expect("UTF-8 file name")
        })
        .collect()
}

fn read_fixture(dir: &str, name: &str) -> String {
    let path = corpus().join(dir).join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn strings(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|v| v.as_str().expect("string"))
        .collect()
}

#[test]
fn every_fixture_has_exactly_one_expectation() {
    let expectations = expectations();
    for dir in ["valid", "invalid"] {
        let expected: BTreeSet<String> = section(&expectations, dir).keys().cloned().collect();
        assert_eq!(
            files_in(dir),
            expected,
            "{dir}/ and expectations.{dir} differ"
        );
    }
}

#[test]
fn valid_fixtures_are_accepted_with_the_expected_warnings() {
    let expectations = expectations();
    for (name, expected) in section(&expectations, "valid") {
        let text = read_fixture("valid", name);
        let parsed = parse_theme_manifest_with_diagnostics(&text)
            .unwrap_or_else(|e| panic!("valid/{name} was rejected: {e}"));
        let actual: Vec<Value> = parsed
            .warnings
            .iter()
            .map(|w| json!({ "code": w.code().as_str(), "pointer": w.pointer() }))
            .collect();
        assert_eq!(
            &Value::Array(actual),
            &expected["warnings"],
            "valid/{name} warnings"
        );
        assert!(parse_theme_manifest(&text).is_ok(), "valid/{name}");
    }
}

#[test]
fn invalid_fixtures_are_rejected_with_the_expected_code_and_pointer() {
    let expectations = expectations();
    for (name, expected) in section(&expectations, "invalid") {
        let text = read_fixture("invalid", name);
        let err = match parse_theme_manifest_with_diagnostics(&text) {
            Ok(parsed) => panic!("invalid/{name} was accepted: {:?}", parsed.manifest),
            Err(err) => err,
        };
        assert_eq!(
            err.code().as_str(),
            expected["code"].as_str().expect("code"),
            "invalid/{name}: {err}"
        );
        assert_eq!(
            err.pointer(),
            expected["pointer"].as_str().expect("pointer"),
            "invalid/{name}: {err}"
        );
    }
}

#[test]
fn expectation_code_lists_match_the_parser() {
    let expectations = expectations();
    let errors: Vec<&str> = ErrorCode::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(strings(&expectations["errorCodes"]), errors);
    let warnings: Vec<&str> = WarningCode::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(strings(&expectations["warningCodes"]), warnings);
}

#[test]
fn corpus_covers_every_code() {
    let expectations = expectations();
    let mut covered: BTreeSet<&str> = section(&expectations, "invalid")
        .values()
        .map(|e| e["code"].as_str().expect("code"))
        .collect();
    covered.extend(
        section(&expectations, "notCoveredByFixtures")
            .keys()
            .map(String::as_str),
    );
    for code in ErrorCode::ALL {
        assert!(covered.contains(code.as_str()), "no fixture for {code}");
    }
    let warned: BTreeSet<&str> = section(&expectations, "valid")
        .values()
        .flat_map(|e| e["warnings"].as_array().expect("warnings"))
        .map(|w| w["code"].as_str().expect("code"))
        .collect();
    for code in WarningCode::ALL {
        assert!(warned.contains(code.as_str()), "no fixture for {code}");
    }
}

fn minimal_with(pointer: &str, value: &str) -> String {
    let mut doc = json!({
        "schemaVersion": 1,
        "kind": "theme",
        "id": "dev.example.minimal",
        "name": "Minimal",
        "version": "0.1.0",
        "author": { "name": "Glint Test Fixtures" },
        "license": "MIT",
        "compatibility": { "glint": ">=0.1.0", "platforms": ["windows"] }
    });
    *doc.pointer_mut(pointer).expect("pointer exists") = json!(value);
    doc.to_string()
}

#[test]
fn version_string_cases_match_the_parser() {
    let cases = read_json(&corpus().join("version-strings.json"));
    for (key, pointer, code) in [
        ("versions", "/version", ErrorCode::InvalidVersion),
        (
            "requirements",
            "/compatibility/glint",
            ErrorCode::InvalidVersionRequirement,
        ),
    ] {
        for text in strings(&cases[key]["valid"]) {
            let result = parse_theme_manifest(&minimal_with(pointer, text));
            assert!(result.is_ok(), "{key} {text:?}: {result:?}");
        }
        for text in strings(&cases[key]["invalid"]) {
            match parse_theme_manifest(&minimal_with(pointer, text)) {
                Ok(_) => panic!("{key} {text:?} was accepted"),
                Err(err) => {
                    assert_eq!(err.code(), code, "{key} {text:?}");
                    assert_eq!(err.pointer(), pointer, "{key} {text:?}");
                }
            }
        }
    }
}
