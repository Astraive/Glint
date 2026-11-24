//! Offline theme workflow (CLI-002): import, list, export, re-import,
//! remove, `theme use` and `safe-mode` through the real binary with a
//! temporary `GLINT_HOME`. Every `--json` document is validated against the
//! envelope and its `$defs/data` shape in
//! `schemas/glint-cli-output-v1.schema.json`.
//!
//! Run with `cargo test -p glint-cli --test cli_roundtrip`.

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use support::schema;

const OK: i32 = 0;
const USAGE: i32 = 2;
const INVALID_PACKAGE: i32 = 4;
const FAILURE: i32 = 1;
const NOT_FOUND: i32 = 7;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn output_schema() -> Value {
    let path = repo_root().join("schemas/glint-cli-output-v1.schema.json");
    serde_json::from_str(&fs::read_to_string(path).expect("read schema")).expect("schema JSON")
}

/// A temporary Glint home plus a workspace for source and exported files.
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

    /// Run `glint ARGS --json` in the workspace with `GLINT_HOME` set, check
    /// the exit code, the empty stderr and the envelope schema; when `shape`
    /// is given, also validate `data` against `#/$defs/data/<shape>`.
    fn json(&self, args: &[&str], expected_exit: i32, shape: Option<&str>) -> Value {
        let output = Command::new(env!("CARGO_BIN_EXE_glint"))
            .args(args)
            .arg("--json")
            .env("GLINT_HOME", self.home())
            .current_dir(self.work())
            .output()
            .expect("run glint");
        let stdout = String::from_utf8(output.stdout).expect("UTF-8 stdout");
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
            let reference = format!("#/$defs/data/{shape}");
            let errors = schema::validate_at(&root, &reference, &envelope["data"]);
            assert!(
                errors.is_empty(),
                "glint {args:?} {shape}: {errors:#?}\n{stdout}"
            );
        }
        envelope
    }

    /// Human mode: exit code and stdout.
    fn plain(&self, args: &[&str]) -> (i32, String, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_glint"))
            .args(args)
            .env("GLINT_HOME", self.home())
            .current_dir(self.work())
            .output()
            .expect("run glint");
        (
            output.status.code().unwrap(),
            String::from_utf8(output.stdout).unwrap(),
            String::from_utf8(output.stderr).unwrap(),
        )
    }

    fn active_digest(&self) -> Value {
        self.json(&["theme", "list"], OK, Some("themeList"))["data"]["active"]["digest"].clone()
    }

    /// Write a synthetic theme directory in the workspace and pack it into a
    /// `.glint` with the deterministic `glint-package` writer.
    fn theme_archive(&self, id: &str, version: &str) -> PathBuf {
        let dir = self.work().join(format!("{id}-{version}-src"));
        write_theme(&dir, id, version);
        let archive = self.work().join(format!("{id}-{version}.glint"));
        glint_package::pack_directory(&dir, &archive).unwrap();
        archive
    }
}

fn write_theme(dir: &Path, id: &str, version: &str) {
    let manifest = format!(
        r#"{{
  "schemaVersion": 1,
  "kind": "theme",
  "id": "{id}",
  "name": "Fixture {id}",
  "version": "{version}",
  "author": {{ "name": "Fixture Author" }},
  "license": "MIT",
  "compatibility": {{ "glint": ">=0.1.0", "platforms": ["windows", "linux", "macos"] }},
  "renderer": {{ "kind": "glint-web", "entrypointVersion": 1 }},
  "requiredCapabilities": ["surface.panel"],
  "tokens": "theme/tokens.json",
  "surfaces": {{ "panel": {{ "entry": "dist/panel/index.html" }} }}
}}"#
    );
    for (name, content) in [
        ("glint.json", manifest.as_str()),
        ("theme/tokens.json", r##"{"color":"#123456"}"##),
        (
            "dist/panel/index.html",
            "<!doctype html><html><head></head><body>panel</body></html>",
        ),
    ] {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
}

#[test]
fn import_list_export_reimport_preserves_manifest_and_hash() {
    let env = Env::new();
    let archive = env.theme_archive("dev.example.midnight", "1.2.0");
    let source_bytes = fs::read(&archive).unwrap();

    // An empty store lists nothing and has no active theme.
    let list = env.json(&["theme", "list"], OK, Some("themeList"));
    assert_eq!(list["data"]["packages"], Value::Array(Vec::new()));
    assert_eq!(list["data"]["active"], Value::Null);

    let imported = env.json(
        &["theme", "import", archive.to_str().unwrap()],
        OK,
        Some("themeImport"),
    );
    let package = &imported["data"]["package"];
    assert_eq!(package["id"], "dev.example.midnight");
    assert_eq!(package["version"], "1.2.0");
    assert_eq!(package["origin"], "local");
    assert_eq!(package["active"], false, "import never activates");
    assert_eq!(imported["data"]["alreadyInstalled"], false);
    let digest = package["digest"].as_str().unwrap().to_owned();

    let list = env.json(&["theme", "list"], OK, Some("themeList"));
    assert_eq!(list["data"]["packages"].as_array().unwrap().len(), 1);
    assert_eq!(list["data"]["packages"][0]["digest"], digest.as_str());

    // Export to the default name in the working directory, and with -o.
    let exported = env.json(
        &["theme", "export", "dev.example.midnight"],
        OK,
        Some("themeExport"),
    );
    assert_eq!(exported["data"]["canonical"], true);
    assert_eq!(exported["data"]["archive"]["sha256"], digest.as_str());
    let default_out = env.work().join("dev.example.midnight-1.2.0.glint");
    assert_eq!(fs::read(&default_out).unwrap(), source_bytes);

    let out = env.work().join("out.glint");
    let exported = env.json(
        &[
            "theme",
            "export",
            "-o",
            out.to_str().unwrap(),
            "dev.example.midnight@1.2.0",
        ],
        OK,
        Some("themeExport"),
    );
    assert_eq!(exported["data"]["package"]["digest"], digest.as_str());
    assert_eq!(fs::read(&out).unwrap(), source_bytes);
    // No local path of the home or workspace inside the archive.
    let bytes = fs::read(&out).unwrap();
    for needle in [
        env.tmp.path().to_string_lossy().into_owned(),
        "install.record".into(),
    ] {
        assert!(
            !bytes.windows(needle.len()).any(|w| w == needle.as_bytes()),
            "{needle} leaked into the export"
        );
    }

    // Re-import is idempotent: same digest, nothing added.
    let again = env.json(
        &["theme", "import", out.to_str().unwrap()],
        OK,
        Some("themeImport"),
    );
    assert_eq!(again["data"]["alreadyInstalled"], true);
    assert_eq!(again["data"]["package"]["digest"], digest.as_str());
    let list = env.json(&["theme", "list"], OK, Some("themeList"));
    assert_eq!(list["data"]["packages"].as_array().unwrap().len(), 1);

    // Re-import into another home: the same manifest and hash.
    let other = Env::new();
    let reimported = other.json(
        &["theme", "import", out.to_str().unwrap()],
        OK,
        Some("themeImport"),
    );
    assert_eq!(reimported["data"]["package"]["digest"], digest.as_str());
    assert_eq!(reimported["data"]["package"]["id"], "dev.example.midnight");
    assert_eq!(reimported["data"]["package"]["version"], "1.2.0");

    // Use, then remove is refused while active, allowed afterwards.
    let used = env.json(
        &["theme", "use", "dev.example.midnight"],
        OK,
        Some("themeActivation"),
    );
    assert_eq!(used["data"]["active"]["profile"], "default");
    assert_eq!(used["data"]["active"]["digest"], digest.as_str());
    assert_eq!(used["data"]["previous"], Value::Null);
    assert_eq!(used["data"]["safeMode"], false);

    let refused = env.json(&["theme", "remove", "dev.example.midnight"], FAILURE, None);
    assert_eq!(refused["error"]["code"], "GLINT_INSTALL_IN_USE");
    assert_eq!(refused["error"]["details"]["profiles"][0], "default");

    env.json(&["theme", "use", "default"], OK, Some("themeActivation"));
    let removed = env.json(
        &["theme", "remove", &format!("sha256:{digest}")],
        OK,
        Some("themeRemove"),
    );
    assert_eq!(removed["data"]["removed"]["id"], "dev.example.midnight");
    let missing = env.json(
        &["theme", "export", "dev.example.midnight"],
        NOT_FOUND,
        None,
    );
    assert_eq!(missing["error"]["kind"], "not-found");
    assert_eq!(missing["error"]["code"], "GLINT_INSTALL_NOT_INSTALLED");
}

#[test]
fn invalid_archive_leaves_the_active_package_unchanged() {
    let env = Env::new();
    let archive = env.theme_archive("dev.example.keep", "1.0.0");
    env.json(&["theme", "import", archive.to_str().unwrap()], OK, None);
    env.json(&["theme", "use", "dev.example.keep"], OK, None);
    let before_active = env.active_digest();
    let before = env.json(&["theme", "list"], OK, Some("themeList"))["data"].clone();

    let broken = env.work().join("broken.glint");
    fs::write(&broken, b"definitely not a zip archive").unwrap();
    let error = env.json(
        &["theme", "import", broken.to_str().unwrap()],
        INVALID_PACKAGE,
        None,
    );
    assert_eq!(error["error"]["code"], "GLINT_PACKAGE_NOT_ZIP");

    // A truncated copy of a real package is refused too.
    let truncated = env.work().join("truncated.glint");
    let bytes = fs::read(&archive).unwrap();
    fs::write(&truncated, &bytes[..bytes.len() / 2]).unwrap();
    env.json(
        &["theme", "import", truncated.to_str().unwrap()],
        INVALID_PACKAGE,
        None,
    );

    // A package that tries to take the bundled default theme's ID.
    let impostor = env.theme_archive("official.default", "9.9.9");
    let error = env.json(
        &["theme", "import", impostor.to_str().unwrap()],
        FAILURE,
        None,
    );
    assert_eq!(error["error"]["code"], "GLINT_INSTALL_RESERVED_ID");

    let missing = env.work().join("missing.glint");
    env.json(
        &["theme", "import", missing.to_str().unwrap()],
        NOT_FOUND,
        None,
    );

    assert_eq!(env.active_digest(), before_active);
    let after = env.json(&["theme", "list"], OK, Some("themeList"))["data"].clone();
    assert_eq!(after, before, "inventory and active theme unchanged");
    let staging = env.home().join("data").join("staging");
    assert_eq!(fs::read_dir(staging).unwrap().count(), 0, "staging cleaned");
}

#[test]
fn safe_mode_always_selects_the_bundled_default_theme() {
    // Empty store.
    let env = Env::new();
    let safe = env.json(&["safe-mode"], OK, Some("themeActivation"));
    assert_eq!(safe["data"]["package"]["id"], "official.default");
    assert_eq!(safe["data"]["package"]["bundled"], true);
    assert_eq!(safe["data"]["active"]["profile"], "safe-mode");
    assert_eq!(safe["data"]["safeMode"], true);
    let bundled = safe["data"]["package"]["digest"]
        .as_str()
        .unwrap()
        .to_owned();

    // After a user theme, safe mode switches back without touching the
    // user's profile; `theme use` then targets the default profile again.
    let archive = env.theme_archive("dev.example.user", "1.0.0");
    env.json(&["theme", "import", archive.to_str().unwrap()], OK, None);
    let used = env.json(&["theme", "use", "dev.example.user"], OK, None);
    assert_eq!(used["data"]["active"]["profile"], "default");
    let user_digest = used["data"]["active"]["digest"].clone();
    let safe = env.json(&["safe-mode"], OK, Some("themeActivation"));
    assert_eq!(safe["data"]["active"]["digest"], bundled.as_str());
    assert_eq!(safe["data"]["previous"]["digest"], user_digest);

    // Damaged store entry: the bundled theme is repaired from the binary.
    let entry = env.home().join("data").join("store").join(&bundled);
    make_writable(&entry);
    fs::remove_dir_all(entry.join("root")).unwrap();
    let safe = env.json(&["safe-mode"], OK, Some("themeActivation"));
    assert_eq!(safe["data"]["active"]["digest"], bundled.as_str());
    assert!(entry.join("root").join("glint.json").is_file());

    // Corrupt database: quarantined and recreated on open; safe mode works.
    let db = env.home().join("data").join("glint.sqlite3");
    fs::write(
        &db,
        b"this is not a sqlite database at all, just junk bytes",
    )
    .unwrap();
    let safe = env.json(&["safe-mode"], OK, Some("themeActivation"));
    assert_eq!(safe["data"]["active"]["digest"], bundled.as_str());

    // Exporting the default theme and re-importing it is a no-op.
    let out = env.work().join("default.glint");
    let exported = env.json(
        &["theme", "export", "-o", out.to_str().unwrap(), "default"],
        OK,
        Some("themeExport"),
    );
    assert_eq!(exported["data"]["canonical"], true);
    let again = env.json(
        &["theme", "import", out.to_str().unwrap()],
        OK,
        Some("themeImport"),
    );
    assert_eq!(again["data"]["alreadyInstalled"], true);
    assert_eq!(again["data"]["package"]["bundled"], true);
}

#[test]
fn theme_use_default_works_on_an_empty_store() {
    let env = Env::new();
    let used = env.json(&["theme", "use", "default"], OK, Some("themeActivation"));
    assert_eq!(used["data"]["package"]["id"], "official.default");
    assert_eq!(used["data"]["active"]["profile"], "default");
    assert_eq!(used["data"]["safeMode"], false);
}

#[test]
fn lookups_report_ambiguity_and_absence() {
    let env = Env::new();
    for version in ["1.0.0", "2.0.0"] {
        let archive = env.theme_archive("dev.example.multi", version);
        env.json(&["theme", "import", archive.to_str().unwrap()], OK, None);
    }
    let error = env.json(&["theme", "use", "dev.example.multi"], USAGE, None);
    assert_eq!(error["error"]["code"], "GLINT_INSTALL_AMBIGUOUS");
    assert_eq!(
        error["error"]["details"]["candidates"],
        serde_json::json!(["dev.example.multi@1.0.0", "dev.example.multi@2.0.0"])
    );
    env.json(&["theme", "use", "dev.example.multi@2.0.0"], OK, None);
    let error = env.json(&["theme", "remove", "dev.example.none"], NOT_FOUND, None);
    assert_eq!(error["error"]["kind"], "not-found");
    let error = env.json(
        &[
            "theme",
            "export",
            "-o",
            env.work().to_str().unwrap(),
            "dev.example.multi@1.0.0",
        ],
        USAGE,
        None,
    );
    assert_eq!(
        error["error"]["code"],
        "GLINT_INSTALL_EXPORT_DESTINATION_INVALID"
    );
}

#[test]
fn human_output_and_errors() {
    let env = Env::new();
    let (code, stdout, _) = env.plain(&["theme", "list"]);
    assert_eq!(code, OK);
    assert!(stdout.contains("No themes installed"), "{stdout}");
    let (code, stdout, _) = env.plain(&["safe-mode"]);
    assert_eq!(code, OK);
    assert!(
        stdout.starts_with("safe mode: official.default"),
        "{stdout}"
    );
    let (code, stdout, stderr) = env.plain(&["theme", "remove", "dev.example.none"]);
    assert_eq!(code, NOT_FOUND);
    assert!(stdout.is_empty());
    assert!(stderr.starts_with("error[not-found]:"), "{stderr}");
}

#[cfg_attr(windows, allow(clippy::permissions_set_readonly_false))]
fn make_writable(path: &Path) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_readonly(false);
    fs::set_permissions(path, permissions).unwrap();
    if path.is_dir() {
        for entry in fs::read_dir(path).unwrap() {
            make_writable(&entry.unwrap().path());
        }
    }
}
