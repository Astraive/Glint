//! Exit code contract (CLI-001): every failure class the binary can reach
//! today maps to its documented exit code, and every `--json` output
//! validates against `schemas/glint-cli-output-v1.schema.json`.
//!
//! `permission-denied` (5) is mapped from typed source errors in the unit
//! tests (`commands::tests`); `daemon-unavailable` (6) and `timeout` (8) are
//! reserved: no command can produce them until the daemon (COR) exists.

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use support::schema;

const OK: i32 = 0;
const USAGE: i32 = 2;
const UNSUPPORTED: i32 = 3;
const INVALID_PACKAGE: i32 = 4;
const NOT_FOUND: i32 = 7;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn glint(args: &[&str]) -> Output {
    // `output()` gives the child a null stdin and piped stdout/stderr, so it
    // never sees a terminal.
    Command::new(env!("CARGO_BIN_EXE_glint"))
        .args(args)
        .output()
        .expect("run glint")
}

fn output_schema() -> Value {
    let path = repo_root().join("schemas/glint-cli-output-v1.schema.json");
    serde_json::from_str(&fs::read_to_string(path).expect("read schema")).expect("schema JSON")
}

/// Run with `--json`, check the exit code, the empty stderr and the schema,
/// and return the envelope.
fn json(args: &[&str], expected_exit: i32) -> Value {
    let mut all = args.to_vec();
    all.push("--json");
    let output = glint(&all);
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 stdout");
    assert_eq!(
        output.status.code(),
        Some(expected_exit),
        "glint {all:?}\n{stdout}"
    );
    assert!(
        output.stderr.is_empty(),
        "--json keeps stderr empty: {all:?}"
    );
    let envelope: Value = serde_json::from_str(&stdout).expect("stdout is one JSON document");
    let errors = schema::validate(&output_schema(), &envelope);
    assert!(errors.is_empty(), "glint {all:?}: {errors:#?}\n{stdout}");
    assert_eq!(envelope["exitCode"], expected_exit);
    envelope
}

fn error_kind(envelope: &Value) -> &str {
    envelope["error"]["kind"].as_str().expect("error.kind")
}

#[test]
fn version_and_help_succeed() {
    let envelope = json(&["--version"], OK);
    assert_eq!(envelope["data"]["version"], env!("CARGO_PKG_VERSION"));
    json(&["version"], OK);
    let help = json(&["--help"], OK);
    let kinds: Vec<(i64, &str)> = help["data"]["exitCodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["code"].as_i64().unwrap(), e["kind"].as_str().unwrap()))
        .collect();
    assert_eq!(
        kinds,
        [
            (0, "ok"),
            (1, "failure"),
            (2, "usage"),
            (3, "unsupported"),
            (4, "invalid-package"),
            (5, "permission-denied"),
            (6, "daemon-unavailable"),
            (7, "not-found"),
            (8, "timeout"),
        ]
    );

    let plain = glint(&["--version"]);
    assert_eq!(plain.status.code(), Some(OK));
    assert_eq!(
        String::from_utf8(plain.stdout).unwrap().trim(),
        format!("glint {}", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(glint(&[]).status.code(), Some(OK));
}

#[test]
fn usage_errors_exit_2() {
    for args in [
        &["frobnicate"][..],
        &["theme", "frobnicate"],
        &["registry"],
        &["theme", "import"],
        &["theme", "list", "extra"],
        &["theme", "list", "--bogus"],
    ] {
        let envelope = json(args, USAGE);
        assert_eq!(error_kind(&envelope), "usage", "{args:?}");
    }
    assert_eq!(json(&["frobnicate"], USAGE)["command"], Value::Null);
}

#[test]
fn interactive_chooser_needs_a_terminal() {
    let envelope = json(&["theme"], USAGE);
    assert_eq!(envelope["error"]["code"], "GLINT_CLI_MISSING_SUBCOMMAND");
    let plain = glint(&["theme"]);
    assert_eq!(plain.status.code(), Some(USAGE));
}

#[test]
fn every_unimplemented_command_exits_3_and_never_succeeds() {
    let help = json(&["--help"], OK);
    let mut checked = 0;
    for command in help["data"]["commands"].as_array().unwrap() {
        if command["status"] == "implemented" || command["name"] == "theme" {
            continue;
        }
        let usage = command["usage"].as_str().unwrap();
        // Fill required placeholders (bare UPPERCASE words) with a value.
        let args: Vec<&str> = usage
            .split_whitespace()
            .skip(1)
            .filter(|word| !word.starts_with('['))
            .filter(|word| !(word.ends_with(']')))
            .map(|word| {
                if word.chars().all(|c| c.is_ascii_uppercase()) {
                    "x"
                } else {
                    word
                }
            })
            .collect();
        let envelope = json(&args, UNSUPPORTED);
        assert_eq!(error_kind(&envelope), "unsupported", "{args:?}");
        assert_eq!(envelope["error"]["code"], "GLINT_CLI_NOT_IMPLEMENTED");
        assert_eq!(envelope["command"], command["name"]);
        assert_eq!(
            envelope["error"]["details"]["plannedIn"],
            command["plannedIn"]
        );
        checked += 1;
    }
    assert!(checked >= 20, "only {checked} commands checked");
}

#[test]
fn validate_accepts_a_valid_package() {
    let fixture = repo_root().join("tests/package-fixtures/valid-minimal");
    let envelope = json(&["theme", "validate", fixture.to_str().unwrap()], OK);
    assert_eq!(envelope["data"]["valid"], true);
    assert_eq!(envelope["data"]["package"]["id"], "dev.example.minimal");
    assert_eq!(envelope["data"]["source"]["kind"], "local-directory");
    assert_eq!(
        envelope["data"]["archive"]["sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
}

#[test]
fn invalid_packages_exit_4_with_the_underlying_code() {
    let dir = tempfile::tempdir().unwrap();

    let theme = dir.path().join("theme");
    fs::create_dir(&theme).unwrap();
    fs::write(
        theme.join("glint.json"),
        r#"{"schemaVersion":1,"kind":"theme","name":"No id"}"#,
    )
    .unwrap();
    let envelope = json(
        &["theme", "validate", theme.to_str().unwrap()],
        INVALID_PACKAGE,
    );
    assert_eq!(error_kind(&envelope), "invalid-package");
    assert_eq!(envelope["error"]["code"], "GLINT_MANIFEST_MISSING_ID");
    let details = &envelope["error"]["details"];
    assert_eq!(details["sourceCode"], "GLINT_SOURCE_PACKAGE_REJECTED");
    assert_eq!(details["packageCode"], "GLINT_PACKAGE_INVALID_MANIFEST");
    assert_eq!(details["manifestCode"], "GLINT_MANIFEST_MISSING_ID");

    let not_zip = dir.path().join("broken.glint");
    fs::write(&not_zip, b"definitely not a zip archive").unwrap();
    let envelope = json(
        &["theme", "validate", not_zip.to_str().unwrap()],
        INVALID_PACKAGE,
    );
    assert_eq!(envelope["error"]["code"], "GLINT_PACKAGE_NOT_ZIP");

    // Human mode: same exit code, message on stderr, nothing on stdout.
    let plain = glint(&["theme", "validate", not_zip.to_str().unwrap()]);
    assert_eq!(plain.status.code(), Some(INVALID_PACKAGE));
    assert!(plain.stdout.is_empty());
    let stderr = String::from_utf8(plain.stderr).unwrap();
    assert!(stderr.starts_with("error[invalid-package]:"), "{stderr}");
}

#[test]
fn missing_paths_exit_7() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.glint");
    let envelope = json(&["theme", "validate", missing.to_str().unwrap()], NOT_FOUND);
    assert_eq!(error_kind(&envelope), "not-found");
    assert_eq!(envelope["error"]["code"], "GLINT_SOURCE_NOT_FOUND");
}

#[test]
fn schema_checker_rejects_malformed_envelopes() {
    let schema = output_schema();
    let good = json(&["theme", "list"], UNSUPPORTED);
    let mut cases = Vec::new();
    let mut ok_with_error = good.clone();
    ok_with_error["ok"] = Value::Bool(true);
    cases.push(ok_with_error);
    let mut wrong_kind = good.clone();
    wrong_kind["error"]["kind"] = Value::from("oops");
    cases.push(wrong_kind);
    let mut extra = good.clone();
    extra["extra"] = Value::Null;
    cases.push(extra);
    let mut bad_code = good.clone();
    bad_code["error"]["code"] = Value::from("not-a-code");
    cases.push(bad_code);
    let mut zero_exit = good;
    zero_exit["exitCode"] = Value::from(0);
    cases.push(zero_exit);
    for case in cases {
        assert!(!schema::validate(&schema, &case).is_empty(), "{case}");
    }
}
