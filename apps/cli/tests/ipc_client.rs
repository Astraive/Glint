//! CLI routing through the core's IPC API (CLI-006), against a real
//! `glint-daemon` process with a temporary `GLINT_HOME`:
//!
//! - with a core running, state commands go through it (`route: daemon`),
//!   never take the installer lock themselves (no `GLINT_INSTALL_BUSY`),
//!   and their changes are visible to a second IPC client;
//! - with no core, the same commands run offline (`route: offline`);
//! - a core that holds the instance lock but does not serve IPC is exit 6
//!   and nothing changes; a stale endpoint record left by a crash is
//!   ignored; an operation that outlives the deadline is exit 8.
//!
//! Every `--json` document is validated against the envelope and its
//! `$defs/data` shape. Run with `cargo test -p glint-cli --test ipc_client`.

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use glint_ipc::{
    Client, Control, CoreSnapshot, Delta, GlintRequest, Hub, IpcError, ResponseBody, ServerOptions,
    Update,
};
use glint_storage::GlintPaths;
use serde_json::Value;
use support::daemon::Daemon;
use support::schema;

const OK: i32 = 0;
const FAILURE: i32 = 1;
const DAEMON_UNAVAILABLE: i32 = 6;
const NOT_FOUND: i32 = 7;
const TIMEOUT: i32 = 8;

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
        fs::create_dir(tmp.path().join("home")).unwrap();
        fs::create_dir(tmp.path().join("work")).unwrap();
        Self { tmp }
    }

    fn home(&self) -> PathBuf {
        self.tmp.path().join("home")
    }

    fn work(&self) -> PathBuf {
        self.tmp.path().join("work")
    }

    fn paths(&self) -> GlintPaths {
        GlintPaths::under_root(self.home()).unwrap()
    }

    fn json_with(
        &self,
        args: &[&str],
        env: &[(&str, &str)],
        expected_exit: i32,
        shape: Option<&str>,
    ) -> Value {
        let output = Command::new(env!("CARGO_BIN_EXE_glint"))
            .args(args)
            .arg("--json")
            .env("GLINT_HOME", self.home())
            .envs(env.iter().copied())
            .current_dir(self.work())
            .output()
            .expect("run glint");
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
        envelope
    }

    fn json(&self, args: &[&str], expected_exit: i32, shape: Option<&str>) -> Value {
        self.json_with(args, &[], expected_exit, shape)
    }

    /// A packed synthetic theme whose panel capability is optional, so the
    /// headless core presents it as installed rather than falling back.
    fn theme_archive(&self, id: &str) -> PathBuf {
        let dir = self.work().join(format!("{id}-src"));
        let manifest = format!(
            r#"{{
  "schemaVersion": 1,
  "kind": "theme",
  "id": "{id}",
  "name": "Fixture {id}",
  "version": "1.0.0",
  "author": {{ "name": "Fixture Author" }},
  "license": "MIT",
  "compatibility": {{ "glint": ">=0.0.0", "platforms": ["windows", "linux", "macos"] }},
  "renderer": {{ "kind": "glint-web", "entrypointVersion": 1 }},
  "optionalCapabilities": ["surface.panel"],
  "surfaces": {{ "panel": {{ "entry": "dist/panel/index.html" }} }}
}}"#
        );
        for (name, content) in [
            ("glint.json", manifest.as_str()),
            (
                "dist/panel/index.html",
                "<!doctype html><html><head></head><body>panel</body></html>",
            ),
        ] {
            let path = dir.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        let archive = self.work().join(format!("{id}.glint"));
        glint_package::pack_directory(&dir, &archive).unwrap();
        archive
    }
}

fn route(envelope: &Value) -> &str {
    envelope["data"]["route"]
        .as_str()
        .or_else(|| envelope["error"]["details"]["route"].as_str())
        .unwrap_or("none")
}

#[test]
fn state_commands_go_through_a_running_core_and_reach_a_second_client() {
    let env = Env::new();
    let archive = env.theme_archive("dev.example.routed");
    let mut daemon = Daemon::start(&env.home());

    // A second IPC client, subscribed before the CLI changes anything.
    let mut watcher = Client::connect(&env.paths()).unwrap().subscribe().unwrap();
    assert_eq!(
        watcher.snapshot().theme.as_ref().unwrap().origin,
        "bundled-default"
    );

    let imported = env.json(
        &["theme", "import", archive.to_str().unwrap()],
        OK,
        Some("themeImport"),
    );
    assert_eq!(route(&imported), "daemon");
    let digest = imported["data"]["package"]["digest"]
        .as_str()
        .unwrap()
        .to_owned();

    // A relative path is resolved against the CLI's directory, not the
    // core's: re-importing by relative name is a no-op.
    let again = env.json(
        &["theme", "import", "dev.example.routed.glint"],
        OK,
        Some("themeImport"),
    );
    assert_eq!(again["data"]["alreadyInstalled"], true);

    let used = env.json(
        &["theme", "use", "dev.example.routed"],
        OK,
        Some("themeActivation"),
    );
    assert_eq!(route(&used), "daemon");
    assert_eq!(used["data"]["active"]["digest"], digest.as_str());
    daemon.wait_for("operation theme-use");

    // The second client sees the change: as a delta on its subscription...
    let mut presented = None;
    for _ in 0..64 {
        if let Update::Delta {
            delta: Delta::Theme { theme },
            ..
        } = watcher.next(Duration::from_secs(10)).unwrap()
        {
            presented = Some(theme);
            break;
        }
    }
    let presented = presented.expect("a theme delta");
    assert_eq!(presented.theme_id, "dev.example.routed");
    assert_eq!(presented.origin, "installed");
    assert_eq!(presented.digest.as_deref(), Some(digest.as_str()));
    // ...and in a fresh read.
    let mut reader = Client::connect(&env.paths()).unwrap();
    let ResponseBody::State(state) = reader.request(GlintRequest::GetState).unwrap() else {
        panic!("state");
    };
    assert_eq!(state.installed_packages, 1);

    let list = env.json(&["theme", "list"], OK, Some("themeList"));
    assert_eq!(route(&list), "daemon");
    assert_eq!(list["data"]["packages"][0]["active"], true);

    // Export without -o lands at ./<id>-<version>.glint, no temp left.
    let exported = env.json(
        &["theme", "export", "dev.example.routed"],
        OK,
        Some("themeExport"),
    );
    assert_eq!(route(&exported), "daemon");
    assert_eq!(exported["data"]["canonical"], true);
    let out = env.work().join("dev.example.routed-1.0.0.glint");
    assert_eq!(fs::read(&out).unwrap(), fs::read(&archive).unwrap());
    let leftovers: Vec<_> = fs::read_dir(env.work())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");

    // Typed failures keep their exit codes and component codes.
    let in_use = env.json(&["theme", "remove", "dev.example.routed"], FAILURE, None);
    assert_eq!(in_use["error"]["code"], "GLINT_INSTALL_IN_USE");
    assert_eq!(route(&in_use), "daemon");
    let missing = env.json(&["theme", "use", "dev.example.missing"], NOT_FOUND, None);
    assert_eq!(missing["error"]["code"], "GLINT_INSTALL_NOT_INSTALLED");
    assert_eq!(
        missing["error"]["details"]["ipcCode"],
        "GLINT_IPC_OPERATION_FAILED"
    );

    let safe = env.json(&["safe-mode"], OK, Some("themeActivation"));
    assert_eq!(route(&safe), "daemon");
    assert_eq!(safe["data"]["active"]["profile"], "safe-mode");

    drop(watcher);
    drop(reader);
    let (code, out) = daemon.stop();
    assert_eq!(code, Some(0), "{out}");
    assert!(!out.contains("GLINT_INSTALL_BUSY"), "{out}");

    // With the core stopped the same state is read offline.
    let list = env.json(&["theme", "list"], OK, Some("themeList"));
    assert_eq!(route(&list), "offline");
    assert_eq!(list["data"]["active"]["profile"], "safe-mode");
    assert_eq!(list["data"]["packages"].as_array().unwrap().len(), 2);
}

#[test]
fn a_core_that_holds_the_lock_without_ipc_is_exit_6_and_nothing_changes() {
    let env = Env::new();
    let paths = env.paths();
    paths.ensure_dirs().unwrap();
    // A core between taking its instance lock and serving IPC.
    let lock = glint_core::InstanceLock::acquire(&paths).unwrap();
    for args in [&["safe-mode"][..], &["theme", "list"][..]] {
        let refused = env.json(args, DAEMON_UNAVAILABLE, None);
        assert_eq!(refused["error"]["kind"], "daemon-unavailable");
        assert_eq!(refused["error"]["code"], "GLINT_CLI_DAEMON_UNAVAILABLE");
    }
    lock.release().unwrap();
    let list = env.json(&["theme", "list"], OK, Some("themeList"));
    assert_eq!(route(&list), "offline");
    assert_eq!(
        list["data"]["active"],
        Value::Null,
        "safe-mode changed nothing"
    );
    assert_eq!(list["data"]["packages"], Value::Array(Vec::new()));
}

struct Idle;

impl Control for Idle {
    fn shutdown(&self) -> Result<bool, IpcError> {
        Ok(true)
    }
}

struct Slow;

impl Control for Slow {
    fn shutdown(&self) -> Result<bool, IpcError> {
        Ok(true)
    }

    fn operate(&self, _: &GlintRequest) -> Result<ResponseBody, IpcError> {
        std::thread::sleep(Duration::from_secs(3));
        Err(IpcError::new(glint_ipc::ErrorCode::Internal, "too late"))
    }
}

#[test]
fn a_stale_endpoint_record_is_ignored_and_slow_operations_time_out() {
    let env = Env::new();
    let paths = env.paths();
    paths.ensure_dirs().unwrap();

    // A record left behind by a core that crashed: nothing listens and no
    // instance lock is held, so the CLI works offline.
    let hub = Arc::new(Hub::new(CoreSnapshot::new(1)));
    let server = glint_ipc::serve(
        &paths,
        Arc::clone(&hub),
        Arc::new(Idle),
        ServerOptions::default(),
    )
    .unwrap();
    let record = fs::read(paths.ipc_endpoint_file()).unwrap();
    server.stop();
    fs::write(paths.ipc_endpoint_file(), record).unwrap();
    let list = env.json(&["theme", "list"], OK, Some("themeList"));
    assert_eq!(route(&list), "offline");

    // A core that does not answer an operation in time: exit 8, typed.
    let hub = Arc::new(Hub::new(CoreSnapshot::new(1)));
    let server = glint_ipc::serve(
        &paths,
        Arc::clone(&hub),
        Arc::new(Slow),
        ServerOptions::default(),
    )
    .unwrap();
    let timed_out = env.json_with(
        &["safe-mode"],
        &[("GLINT_IPC_TIMEOUT_MS", "500")],
        TIMEOUT,
        None,
    );
    assert_eq!(timed_out["error"]["kind"], "timeout");
    assert_eq!(timed_out["error"]["code"], "GLINT_IPC_TIMEOUT");
    assert_eq!(route(&timed_out), "daemon");
    hub.close("stopped");
    server.stop();
}
