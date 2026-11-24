//! `glint doctor` and `glint status` (CLI-003): read-only diagnostics,
//! redacted by default, against a temporary `GLINT_HOME` whose path holds a
//! synthetic user name, with and without a real `glint-daemon`.
//!
//! Default output (JSON, text and the support bundle) must contain neither
//! the user name, the home or Glint paths, the computer name, nor any
//! environment value such as a token; `--include-sensitive` needs `--yes`
//! (or a terminal answer) and then shows them. Every `--json` document is
//! validated against the envelope and `$defs/data/doctor` or
//! `$defs/data/status`. Run with `cargo test -p glint-cli --test doctor`.

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use support::daemon::Daemon;
use support::schema;

const OK: i32 = 0;
const USAGE: i32 = 2;

/// A synthetic user name; it appears in every path of the fixture.
const USER: &str = "glintfixtureuser";
/// A synthetic computer name.
const HOST: &str = "GLINT-FIXTURE-HOST";
/// A token-like environment value that must never be printed.
const TOKEN: &str = "ghp_fixtureTOKENvalue0123456789";

fn output_schema() -> Value {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/glint-cli-output-v1.schema.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

struct Env {
    tmp: tempfile::TempDir,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join(USER).join("glint")).unwrap();
        fs::create_dir_all(tmp.path().join(USER).join("work")).unwrap();
        Self { tmp }
    }

    fn user_home(&self) -> PathBuf {
        self.tmp.path().join(USER)
    }

    fn home(&self) -> PathBuf {
        self.user_home().join("glint")
    }

    fn work(&self) -> PathBuf {
        self.user_home().join("work")
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_glint"))
            .args(args)
            .env("GLINT_HOME", self.home())
            .env("USERNAME", USER)
            .env("USER", USER)
            .env("USERPROFILE", self.user_home())
            .env("HOME", self.user_home())
            .env("COMPUTERNAME", HOST)
            .env("HOSTNAME", HOST)
            .env("GITHUB_TOKEN", TOKEN)
            .env("GLINT_FIXTURE_SSID", "FixtureHomeWifi")
            .current_dir(self.work())
            .output()
            .expect("run glint")
    }

    fn json(&self, args: &[&str], expected_exit: i32, shape: Option<&str>) -> (Value, String) {
        let mut all = args.to_vec();
        all.push("--json");
        let output = self.run(&all);
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert_eq!(
            output.status.code(),
            Some(expected_exit),
            "glint {args:?}\n{stdout}"
        );
        assert!(output.stderr.is_empty(), "--json keeps stderr empty");
        let envelope: Value = serde_json::from_str(&stdout).expect("one JSON document");
        let root = output_schema();
        let errors = schema::validate(&root, &envelope);
        assert!(errors.is_empty(), "glint {args:?}: {errors:#?}\n{stdout}");
        if let Some(shape) = shape {
            let errors =
                schema::validate_at(&root, &format!("#/$defs/data/{shape}"), &envelope["data"]);
            assert!(
                errors.is_empty(),
                "glint {args:?} {shape}: {errors:#?}\n{stdout}"
            );
        }
        (envelope, stdout)
    }
}

/// Fail if `text` contains any sensitive value, in any spelling a JSON
/// encoder or a path API could produce.
fn assert_redacted(env: &Env, text: &str) {
    let lower = text.to_lowercase();
    for needle in [USER, HOST, TOKEN, "FixtureHomeWifi"] {
        assert!(
            !lower.contains(&needle.to_lowercase()),
            "{needle} leaked:\n{text}"
        );
    }
    let tmp = env.tmp.path().to_string_lossy().to_lowercase();
    for spelling in [
        tmp.clone(),
        tmp.replace('\\', "/"),
        tmp.replace('\\', "\\\\"),
    ] {
        assert!(!lower.contains(&spelling), "temp path leaked:\n{text}");
    }
}

#[test]
fn offline_doctor_and_status_are_redacted_and_valid() {
    let env = Env::new();
    let (doctor, stdout) = env.json(&["doctor"], OK, Some("doctor"));
    assert_redacted(&env, &stdout);
    let data = &doctor["data"];
    assert_eq!(data["redacted"], true);
    assert_eq!(data["core"]["state"], "not-running");
    assert_eq!(data["paths"]["data"]["path"], "<glint-data>");
    assert_eq!(data["paths"]["source"], "glint-home");
    assert_eq!(data["storage"]["database"]["present"], false);
    assert!(
        data["privacy"]["notCollected"]
            .as_array()
            .unwrap()
            .contains(&Value::from("wifi-ssids"))
    );
    // Diagnostics never create or change state.
    assert!(
        !env.home().join("data").exists()
            || fs::read_dir(env.home().join("data"))
                .unwrap()
                .next()
                .is_none()
    );

    let (status, stdout) = env.json(&["status"], OK, Some("status"));
    assert_redacted(&env, &stdout);
    assert_eq!(status["data"]["core"]["state"], "not-running");
    assert_eq!(status["data"]["theme"]["active"], Value::Null);
    assert_eq!(status["data"]["dataDir"], Value::Null);

    // Human output is redacted too.
    for args in [&["doctor"][..], &["status"][..]] {
        let output = env.run(args);
        assert_eq!(output.status.code(), Some(OK));
        assert_redacted(&env, &String::from_utf8(output.stdout).unwrap());
        assert_redacted(&env, &String::from_utf8(output.stderr).unwrap());
    }
}

#[test]
fn sensitive_output_needs_confirmation() {
    let env = Env::new();
    // Installed state, so the database and the active record exist.
    let (_, _) = env.json(&["safe-mode"], OK, Some("themeActivation"));

    let (refused, _) = env.json(&["doctor", "--include-sensitive"], USAGE, None);
    assert_eq!(refused["error"]["code"], "GLINT_CLI_CONFIRMATION_REQUIRED");
    let (refused, _) = env.json(&["status", "--yes"], USAGE, None);
    assert_eq!(refused["error"]["code"], "GLINT_CLI_USAGE");

    let (doctor, _) = env.json(
        &["doctor", "--include-sensitive", "--yes"],
        OK,
        Some("doctor"),
    );
    let data = &doctor["data"];
    assert_eq!(data["redacted"], false);
    let shown = data["paths"]["data"]["path"].as_str().unwrap();
    assert!(shown.contains(USER), "{shown}");
    assert_eq!(data["storage"]["database"]["present"], true);
    assert_eq!(data["storage"]["database"]["quickCheck"], "ok");
    assert_eq!(data["storage"]["packages"], 1);
    assert_eq!(data["theme"]["active"]["profile"], "safe-mode");
    // Never collected, even when sensitive output is confirmed.
    let text = doctor.to_string();
    assert!(!text.contains(TOKEN));
    assert!(!text.contains("FixtureHomeWifi"));

    let (status, _) = env.json(
        &["status", "--include-sensitive", "--yes"],
        OK,
        Some("status"),
    );
    assert!(status["data"]["dataDir"].as_str().unwrap().contains(USER));
    assert_eq!(status["data"]["packages"]["installed"], 1);
}

#[test]
fn the_support_bundle_is_redacted() {
    let env = Env::new();
    let (doctor, stdout) = env.json(&["doctor", "-o", "bundle.json"], OK, Some("doctor"));
    assert_redacted(&env, &stdout);
    assert_eq!(doctor["data"]["bundle"]["path"], "bundle.json");
    let bundle_text = fs::read_to_string(env.work().join("bundle.json")).unwrap();
    assert_redacted(&env, &bundle_text);
    let bundle: Value = serde_json::from_str(&bundle_text).unwrap();
    assert_eq!(bundle["kind"], "glint-support-bundle");
    let errors = schema::validate_at(&output_schema(), "#/$defs/data/doctor", &bundle["report"]);
    assert!(errors.is_empty(), "{errors:#?}");
    assert_eq!(bundle["report"]["redacted"], true);
}

#[test]
fn doctor_and_status_read_a_running_core_over_ipc() {
    let env = Env::new();
    let daemon = Daemon::start(&env.home());
    // A theme imported from a path holding the user name: its locator and
    // the core's warnings must not leak it.
    let (_, _) = env.json(&["safe-mode"], OK, Some("themeActivation"));

    let (status, stdout) = env.json(&["status"], OK, Some("status"));
    assert_redacted(&env, &stdout);
    let core = &status["data"]["core"];
    assert_eq!(core["state"], "running");
    assert_eq!(core["pid"], daemon.pid());
    assert_eq!(core["lifecycle"], "ready");
    assert_eq!(core["apiVersion"], 1);
    let presented = &status["data"]["theme"]["presented"];
    assert_eq!(presented["themeId"], "official.default");
    assert_eq!(presented["profile"], "safe-mode");
    assert_eq!(status["data"]["packages"]["installed"], 1);

    let (doctor, stdout) = env.json(&["doctor"], OK, Some("doctor"));
    assert_redacted(&env, &stdout);
    let data = &doctor["data"];
    assert_eq!(data["capabilities"]["adapter"], "headless");
    assert!(
        data["versions"]["core"]
            .as_str()
            .unwrap()
            .starts_with("glint-daemon/")
    );
    assert!(!data["warnings"].as_array().unwrap().is_empty());
    assert_eq!(data["storage"]["database"]["readable"], true);

    let (code, out) = daemon.stop();
    assert_eq!(code, Some(0), "{out}");
}
