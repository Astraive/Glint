//! `glint doctor` and `glint status` (CLI-003): read-only diagnostics,
//! redacted by default.
//!
//! Nothing here changes state. The core is asked over IPC with read
//! requests only (`get-snapshot`); the state database is opened
//! read-only and only when it exists; the installer is never opened, so no
//! recovery runs and no lock is taken. Sensitive values (the user name,
//! home and Glint paths, the computer name) are replaced by placeholders
//! unless `--include-sensitive` is given and confirmed (see
//! [`crate::redact`]). SSIDs, window titles, tokens and the environment are
//! never collected at all.

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::time::Duration;

use glint_core::{InstanceProbe, probe_instance};
use glint_ipc::{Client, Endpoint, SUPPORTED_VERSIONS, SnapshotReply, Transport};
use glint_storage::GlintPaths;
use glint_storage::rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde_json::{Value, json};

use super::Terminal;
use super::theme::{active_info, active_json};
use crate::cli::Invocation;
use crate::error::{CliError, ErrorKind, codes};
use crate::ipc;
use crate::output::{OUTPUT_SCHEMA_VERSION, Success};
use crate::redact::Redactor;

/// What diagnostics never collect, so they cannot leak it.
const NOT_COLLECTED: &[&str] = &[
    "wifi-ssids",
    "window-titles",
    "access-tokens",
    "environment-variables",
    "package-contents",
];

/// Decide whether sensitive values may be shown. `--include-sensitive`
/// needs confirmation: `--yes`, or an answer on an interactive terminal.
fn include_sensitive(
    invocation: &Invocation,
    json: bool,
    terminal: Terminal,
) -> Result<bool, CliError> {
    let wanted = invocation.flag("include-sensitive");
    let confirmed = invocation.flag("yes");
    if !wanted {
        if confirmed {
            return Err(CliError::usage(
                "`--yes` only confirms `--include-sensitive`",
            ));
        }
        return Ok(false);
    }
    if confirmed {
        return Ok(true);
    }
    if json || !terminal.interactive {
        return Err(CliError::new(
            ErrorKind::Usage,
            codes::CONFIRMATION_REQUIRED,
            "`--include-sensitive` shows your user name, home and Glint paths and the \
             computer name; confirm with `--yes` (required with --json or without a terminal)",
        ));
    }
    let mut stderr = io::stderr();
    let _ = write!(
        stderr,
        "Include your user name, home and Glint paths and the computer name in the output? [y/N] "
    );
    let _ = stderr.flush();
    let mut answer = String::new();
    io::stdin()
        .lock()
        .read_line(&mut answer)
        .map_err(|error| super::io_error(&error, "read the confirmation"))?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "YES" | "Yes"))
}

/// Where the core stands, as seen from the CLI.
struct CoreProbe {
    /// `running`, `not-running` or `unreachable`.
    state: &'static str,
    /// The instance lock owner's pid, when recorded.
    pid: Option<u32>,
    snapshot: Option<SnapshotReply>,
    server: Option<(String, u32)>,
    /// A stale endpoint record was found (a core that crashed).
    stale_record: bool,
    error: Option<(String, String)>,
}

fn probe_core(paths: &GlintPaths) -> CoreProbe {
    let (running, pid) = match probe_instance(paths) {
        Ok(InstanceProbe::Running(owner)) => (true, owner.map(|o| o.pid)),
        _ => (false, None),
    };
    let mut probe = CoreProbe {
        state: if running {
            "unreachable"
        } else {
            "not-running"
        },
        pid,
        snapshot: None,
        server: None,
        stale_record: false,
        error: None,
    };
    let endpoint = match Endpoint::read(paths) {
        Ok(endpoint) => endpoint,
        Err(error) => {
            if running {
                probe.error = Some((error.code().as_str().to_owned(), error.message().to_owned()));
            }
            return probe;
        }
    };
    let mut client = match Client::connect_with(&endpoint, ipc::client_options(false)) {
        Ok(client) => client,
        Err(error) => {
            if running {
                probe.error = Some((error.code().as_str().to_owned(), error.message().to_owned()));
            } else {
                probe.stale_record = true;
            }
            return probe;
        }
    };
    probe.server = Some((client.welcome().server.clone(), client.api_version()));
    match client.snapshot() {
        Ok(snapshot) => {
            probe.state = "running";
            probe.pid = Some(snapshot.snapshot.pid);
            probe.snapshot = Some(snapshot);
        }
        Err(error) => {
            probe.state = "unreachable";
            probe.error = Some((error.code().as_str().to_owned(), error.message().to_owned()));
        }
    }
    probe
}

fn core_json(probe: &CoreProbe) -> Value {
    let snapshot = probe.snapshot.as_ref();
    json!({
        "state": probe.state,
        "pid": probe.pid,
        "lifecycle": snapshot.map(|s| s.snapshot.lifecycle.clone()),
        "daemon": snapshot.map(|s| s.snapshot.daemon.clone()),
        "sequence": snapshot.map(|s| s.sequence),
        "server": probe.server.as_ref().map(|(name, _)| name.clone()),
        "apiVersion": probe.server.as_ref().map(|(_, version)| *version),
        "staleEndpointRecord": probe.stale_record,
        "error": probe.error.as_ref().map(|(code, message)| json!({
            "code": code,
            "message": message,
        })),
    })
}

fn presented_json(probe: &CoreProbe) -> Value {
    probe
        .snapshot
        .as_ref()
        .and_then(|s| s.snapshot.theme.as_ref())
        .map_or(Value::Null, |theme| {
            json!({
                "themeId": theme.theme_id,
                "version": theme.version,
                "origin": theme.origin,
                "profile": theme.profile_id,
                "digest": theme.digest,
                "fallbackReason": theme.fallback_reason,
                "surfaces": theme.surfaces,
                "skippedSurfaces": theme.skipped,
            })
        })
}

/// What a read-only look at the state database found.
#[derive(Default)]
struct DbInspection {
    present: bool,
    readable: bool,
    schema_version: Option<u32>,
    quick_check: Option<String>,
    packages: Option<u64>,
    profiles: Option<u64>,
    pins: Option<u64>,
    pending_installs: Option<u64>,
    pending_applies: Option<u64>,
    error: Option<String>,
}

fn count_rows(conn: &Connection, table: &str) -> Option<u64> {
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get(0),
        )
        .ok()?;
    if !exists {
        return None;
    }
    // `table` is one of the fixed names below, never user input.
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get::<_, i64>(0)
    })
    .ok()
    .and_then(|n| u64::try_from(n).ok())
}

fn inspect_database(paths: &GlintPaths) -> DbInspection {
    let path = paths.database_file();
    let mut found = DbInspection {
        present: path.is_file(),
        ..DbInspection::default()
    };
    if !found.present {
        return found;
    }
    let conn = match Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(conn) => conn,
        Err(error) => {
            found.error = Some(error.to_string());
            return found;
        }
    };
    let _ = conn.busy_timeout(Duration::from_secs(2));
    match conn
        .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
        .optional()
    {
        Ok(check) => {
            found.readable = true;
            found.quick_check = check;
        }
        Err(error) => {
            found.error = Some(error.to_string());
            return found;
        }
    }
    found.schema_version = conn
        .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
        .ok();
    found.packages = count_rows(&conn, "packages");
    found.profiles = count_rows(&conn, "profiles");
    found.pins = count_rows(&conn, "install_locks");
    found.pending_installs = count_rows(&conn, "install_journal");
    found.pending_applies = count_rows(&conn, "apply_journal");
    found
}

fn entries(dir: &Path) -> Option<u64> {
    fs::read_dir(dir)
        .ok()
        .map(|entries| u64::try_from(entries.count()).unwrap_or(u64::MAX))
}

fn dir_json(path: &Path) -> Value {
    json!({
        "path": path.display().to_string(),
        "exists": path.exists(),
        "directory": glint_storage::is_real_dir(path),
    })
}

fn system_json() -> Value {
    let env = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    // Session and desktop hints only; values are reduced to a fixed
    // vocabulary or a short identifier, never echoed raw.
    let session = if cfg!(windows) {
        match env("SESSIONNAME") {
            Some(name) if name.eq_ignore_ascii_case("console") => "console",
            Some(name) if name.to_ascii_uppercase().starts_with("RDP") => "remote-desktop",
            Some(_) => "other",
            None => "unknown",
        }
    } else {
        match env("XDG_SESSION_TYPE").as_deref() {
            Some("wayland") => "wayland",
            Some("x11") => "x11",
            Some("tty") => "tty",
            Some(_) => "other",
            None if cfg!(target_os = "macos") => "aqua",
            None => "unknown",
        }
    };
    let compositor = if cfg!(windows) {
        Some("dwm".to_owned())
    } else if cfg!(target_os = "macos") {
        Some("quartz".to_owned())
    } else {
        env("XDG_CURRENT_DESKTOP").map(|desktop| {
            desktop
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_'))
                .take(64)
                .collect::<String>()
        })
    };
    json!({
        "os": std::env::consts::OS,
        "family": std::env::consts::FAMILY,
        "arch": std::env::consts::ARCH,
        "session": session,
        "compositor": compositor,
        "wayland": if cfg!(any(windows, target_os = "macos")) {
            "not-applicable"
        } else if env("WAYLAND_DISPLAY").is_some() {
            "detected"
        } else {
            "not-detected"
        },
    })
}

fn versions_json(probe: &CoreProbe) -> Value {
    json!({
        "glint": env!("CARGO_PKG_VERSION"),
        "cliOutputSchema": OUTPUT_SCHEMA_VERSION,
        "ipcApi": { "min": SUPPORTED_VERSIONS.min, "max": SUPPORTED_VERSIONS.max },
        "core": probe.server.as_ref().map(|(name, _)| name.clone()),
        "databaseSchema": glint_storage::latest_schema_version(),
        "manifestSchema": glint_schema::SUPPORTED_SCHEMA_VERSION,
        "entrypoint": glint_schema::SUPPORTED_ENTRYPOINT_VERSION,
    })
}

struct Check {
    id: &'static str,
    status: &'static str,
    message: String,
}

fn check(id: &'static str, status: &'static str, message: impl Into<String>) -> Check {
    Check {
        id,
        status,
        message: message.into(),
    }
}

fn rank(status: &str) -> u8 {
    match status {
        "fail" => 2,
        "warn" => 1,
        _ => 0,
    }
}

/// Apply redaction (unless sensitive output was confirmed) and build the
/// result.
fn finish(data: Value, text: &str, sensitive: bool, redactor: &Redactor) -> Success {
    if sensitive {
        Success {
            data,
            text: text.to_owned(),
        }
    } else {
        Success {
            data: redactor.value(data),
            text: redactor.text(text),
        }
    }
}

fn privacy_json(sensitive: bool, redactor: &Redactor) -> Value {
    json!({
        "redacted": !sensitive,
        "notCollected": NOT_COLLECTED,
        "notRedacted": redactor.skipped(),
    })
}

/// `glint doctor [-o PATH] [--include-sensitive [--yes]]`.
pub fn doctor(
    invocation: &Invocation,
    json: bool,
    terminal: Terminal,
) -> Result<Success, CliError> {
    let sensitive = include_sensitive(invocation, json, terminal)?;
    let resolved = GlintPaths::resolve();
    let redactor = Redactor::for_user(resolved.as_ref().ok());
    let mut checks = Vec::new();

    let paths = match &resolved {
        Ok(paths) => paths,
        Err(error) => {
            checks.push(check(
                "paths",
                "fail",
                format!("Glint's directories cannot be resolved: {error}"),
            ));
            let data = json!({
                "redacted": !sensitive,
                "overall": "fail",
                "checks": checks_json(&checks),
                "system": system_json(),
                "versions": versions_json(&CoreProbe {
                    state: "not-running",
                    pid: None,
                    snapshot: None,
                    server: None,
                    stale_record: false,
                    error: None,
                }),
                "paths": Value::Null,
                "storage": Value::Null,
                "installer": Value::Null,
                "core": Value::Null,
                "capabilities": Value::Null,
                "renderer": renderer_json(),
                "protocols": protocols_json(),
                "theme": Value::Null,
                "warnings": [],
                "privacy": privacy_json(sensitive, &redactor),
            });
            return write_bundle(
                invocation,
                data,
                "doctor: Glint's directories cannot be resolved",
                sensitive,
                &redactor,
            );
        }
    };

    // Paths.
    let data_exists = glint_storage::is_real_dir(paths.data_dir());
    checks.push(if data_exists {
        check("paths", "ok", "Glint's directories exist")
    } else {
        check(
            "paths",
            "warn",
            "the data directory does not exist yet; the first state command or core start creates it",
        )
    });

    // Storage (read-only).
    let db = inspect_database(paths);
    let latest = glint_storage::latest_schema_version();
    checks.push(match (&db.error, db.present, db.schema_version) {
        (Some(error), _, _) => check(
            "database",
            "fail",
            format!("the state database cannot be read: {error}; the next start quarantines and recreates it"),
        ),
        (None, false, _) => check("database", "ok", "no state database yet (nothing installed)"),
        (None, true, _) if db.quick_check.as_deref() != Some("ok") => check(
            "database",
            "fail",
            format!(
                "quick_check reported {}; the next start quarantines and recreates it",
                db.quick_check.as_deref().unwrap_or("nothing")
            ),
        ),
        (None, true, Some(version)) if version > latest => check(
            "database",
            "fail",
            format!("schema {version} is newer than this build's {latest}; it is never touched"),
        ),
        (None, true, Some(version)) if version < latest => check(
            "database",
            "warn",
            format!("schema {version}; migrations to {latest} run at the next start"),
        ),
        _ => check("database", "ok", format!("schema {latest}, quick_check ok")),
    });

    // Installer recovery state.
    let staging = entries(&paths.staging_dir());
    let trash = entries(&paths.trash_dir());
    let pending = db.pending_installs.unwrap_or(0)
        + db.pending_applies.unwrap_or(0)
        + staging.unwrap_or(0)
        + trash.unwrap_or(0);
    checks.push(if pending == 0 {
        check("recovery", "ok", "no interrupted install, removal or apply")
    } else {
        check(
            "recovery",
            "warn",
            format!(
                "{} journalled install(s)/removal(s), {} interrupted apply(s), {} staging and {} trash entr(ies); recovered at the next start",
                db.pending_installs.unwrap_or(0),
                db.pending_applies.unwrap_or(0),
                staging.unwrap_or(0),
                trash.unwrap_or(0)
            ),
        )
    });

    // The core.
    let probe = probe_core(paths);
    checks.push(match probe.state {
        "running" => check(
            "core",
            "ok",
            format!("core running (pid {})", probe.pid.unwrap_or(0)),
        ),
        "not-running" => check(
            "core",
            "warn",
            "no core is running: state commands work offline, nothing is presented",
        ),
        _ => check(
            "core",
            "fail",
            format!(
                "a core holds the instance lock but does not answer ({})",
                probe
                    .error
                    .as_ref()
                    .map_or("no endpoint", |(code, _)| code.as_str())
            ),
        ),
    });
    let snapshot = probe.snapshot.as_ref().map(|s| &s.snapshot);
    let capabilities = snapshot.and_then(|s| s.capabilities.as_ref());
    checks.push(match capabilities {
        None => check("capabilities", "warn", "unknown: no core is answering"),
        Some(caps) if caps.discovery_error.is_some() => check(
            "capabilities",
            "fail",
            format!(
                "discovery failed ({}); nothing is usable",
                caps.discovery_error.as_deref().unwrap_or("")
            ),
        ),
        Some(caps) if caps.usable == 0 => check(
            "capabilities",
            "warn",
            format!(
                "adapter {} advertises no usable capability; surfaces are skipped",
                caps.adapter
            ),
        ),
        Some(caps) => check(
            "capabilities",
            "ok",
            format!(
                "adapter {}: {} of {} usable",
                caps.adapter, caps.usable, caps.advertised
            ),
        ),
    });
    let presented = snapshot.and_then(|s| s.theme.as_ref());
    if let Some(theme) = presented
        && let Some(reason) = &theme.fallback_reason
    {
        checks.push(check(
            "theme",
            "warn",
            format!("the bundled default theme is presented ({reason})"),
        ));
    } else if presented.is_some() {
        checks.push(check("theme", "ok", "the active theme is presented"));
    }

    let active = glint_storage::read_active_theme(paths);
    let overall = checks
        .iter()
        .map(|c| c.status)
        .max_by_key(|s| rank(s))
        .unwrap_or("ok");
    let warnings: Vec<Value> = snapshot.map_or_else(Vec::new, |s| {
        s.warnings
            .iter()
            .map(|w| json!({ "stage": w.stage, "code": w.code, "message": w.message }))
            .collect()
    });
    let data = json!({
        "redacted": !sensitive,
        "overall": overall,
        "checks": checks_json(&checks),
        "system": system_json(),
        "versions": versions_json(&probe),
        "paths": {
            "source": if std::env::var_os(glint_storage::HOME_OVERRIDE_ENV).is_some_and(|v| !v.is_empty()) {
                "glint-home"
            } else {
                "os"
            },
            "config": dir_json(paths.config_dir()),
            "data": dir_json(paths.data_dir()),
            "cache": dir_json(paths.cache_dir()),
        },
        "storage": {
            "database": {
                "present": db.present,
                "readable": db.readable,
                "schemaVersion": db.schema_version,
                "latestSchemaVersion": latest,
                "quickCheck": db.quick_check,
                "error": db.error,
            },
            "activeRecord": paths.active_record_file().is_file(),
            "packages": db.packages,
            "profiles": db.profiles,
            "pins": db.pins,
        },
        "installer": {
            "pendingInstalls": db.pending_installs,
            "pendingApplies": db.pending_applies,
            "stagingEntries": staging,
            "trashEntries": trash,
        },
        "core": core_json(&probe),
        "capabilities": capabilities.map_or(Value::Null, |caps| json!({
            "adapter": caps.adapter,
            "generation": caps.generation,
            "advertised": caps.advertised,
            "usable": caps.usable,
            "discoveryError": caps.discovery_error,
        })),
        "renderer": renderer_json(),
        "protocols": protocols_json(),
        "theme": {
            "active": active_json(active.as_ref().map(active_info).as_ref()),
            "presented": presented_json(&probe),
        },
        "warnings": warnings,
        "privacy": privacy_json(sensitive, &redactor),
    });

    let mut text = format!("glint doctor: {overall}\n");
    for c in &checks {
        text.push_str(&format!("  [{:<4}] {:<12} {}\n", c.status, c.id, c.message));
    }
    text.push_str(&format!("  data dir {}\n", paths.data_dir().display()));
    if sensitive {
        text.push_str("Sensitive values are included; share this output with care.");
    } else {
        text.push_str(
            "Redacted: user name, home and Glint paths and computer name are replaced; \
             SSIDs, window titles and tokens are never collected.",
        );
    }
    write_bundle(invocation, data, &text, sensitive, &redactor)
}

fn checks_json(checks: &[Check]) -> Value {
    Value::Array(
        checks
            .iter()
            .map(|c| json!({ "id": c.id, "status": c.status, "message": c.message }))
            .collect(),
    )
}

fn renderer_json() -> Value {
    // The shipped core runs the headless surface backend; webview hosts
    // arrive with the native host and renderer epics.
    json!({ "backend": "headless", "webview": "not-implemented" })
}

fn protocols_json() -> Value {
    json!({
        "ipc": {
            "transport": Transport::current().as_str(),
            "apiVersions": { "min": SUPPORTED_VERSIONS.min, "max": SUPPORTED_VERSIONS.max },
        },
        "wayland": if cfg!(any(windows, target_os = "macos")) {
            "not-applicable"
        } else {
            "not-checked"
        },
    })
}

/// Finish the report and, with `-o PATH`, also write it as a support
/// bundle (redacted exactly like the output).
fn write_bundle(
    invocation: &Invocation,
    mut data: Value,
    text: &str,
    sensitive: bool,
    redactor: &Redactor,
) -> Result<Success, CliError> {
    let Some(output) = invocation.option("output") else {
        data["bundle"] = Value::Null;
        return Ok(finish(data, text, sensitive, redactor));
    };
    let path = Path::new(output);
    data["bundle"] = json!({ "path": path.display().to_string() });
    let success = finish(data, text, sensitive, redactor);
    let bundle = json!({
        "kind": "glint-support-bundle",
        "bundleVersion": 1,
        "glintVersion": env!("CARGO_PKG_VERSION"),
        "report": success.data,
    });
    let mut bytes = serde_json::to_vec_pretty(&bundle).unwrap_or_default();
    bytes.push(b'\n');
    fs::write(path, &bytes).map_err(|error| {
        super::io_error(&error, "write the support bundle").with_detail(
            "path",
            if sensitive {
                output.to_owned()
            } else {
                redactor.text(output)
            },
        )
    })?;
    let mut text = success.text;
    text.push_str(&format!(
        "\nSupport bundle written to {} ({}).",
        if sensitive {
            output.to_owned()
        } else {
            redactor.text(output)
        },
        if sensitive {
            "includes sensitive values"
        } else {
            "redacted"
        }
    ));
    Ok(Success {
        data: success.data,
        text,
    })
}

/// `glint status [--include-sensitive [--yes]]`: the core, the active and
/// presented theme and the installed count.
pub fn status(
    invocation: &Invocation,
    json: bool,
    terminal: Terminal,
) -> Result<Success, CliError> {
    let sensitive = include_sensitive(invocation, json, terminal)?;
    let paths = super::theme::paths()?;
    let redactor = Redactor::for_user(Some(&paths));
    let probe = probe_core(&paths);
    let db = inspect_database(&paths);
    let active = glint_storage::read_active_theme(&paths);
    let installed = probe
        .snapshot
        .as_ref()
        .and_then(|s| s.snapshot.state.as_ref())
        .map(|s| s.installed_packages)
        .or(db.packages);
    let warnings = probe
        .snapshot
        .as_ref()
        .map_or(0, |s| s.snapshot.warnings.len());
    let data = json!({
        "redacted": !sensitive,
        "core": core_json(&probe),
        "theme": {
            "active": active_json(active.as_ref().map(active_info).as_ref()),
            "presented": presented_json(&probe),
        },
        "packages": { "installed": installed },
        "warnings": warnings,
        "dataDir": if sensitive {
            Value::from(paths.data_dir().display().to_string())
        } else {
            Value::Null
        },
    });
    let mut text = match probe.state {
        "running" => format!(
            "core: running (pid {}, {})\n",
            probe.pid.unwrap_or(0),
            probe
                .snapshot
                .as_ref()
                .map_or("unknown", |s| s.snapshot.lifecycle.as_str())
        ),
        "not-running" => "core: not running (state commands work offline)\n".to_owned(),
        _ => format!(
            "core: unreachable ({})\n",
            probe
                .error
                .as_ref()
                .map_or("no endpoint", |(code, _)| code.as_str())
        ),
    };
    match &active {
        Some(active) => text.push_str(&format!(
            "active: profile {} release {}\n",
            active.profile_id,
            &active.package_digest[..12.min(active.package_digest.len())]
        )),
        None => text.push_str("active: none\n"),
    }
    if let Some(theme) = probe
        .snapshot
        .as_ref()
        .and_then(|s| s.snapshot.theme.as_ref())
    {
        text.push_str(&format!(
            "presented: {} {} ({})\n",
            theme.theme_id, theme.version, theme.origin
        ));
    }
    text.push_str(&format!(
        "installed: {}",
        installed.map_or_else(|| "unknown".to_owned(), |n| n.to_string())
    ));
    if warnings > 0 {
        text.push_str(&format!("\nwarnings: {warnings} (see `glint doctor`)"));
    }
    Ok(finish(data, &text, sensitive, &redactor))
}
