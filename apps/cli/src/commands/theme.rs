//! `glint theme import/list/export/remove/use` and `glint safe-mode`
//! (CLI-002, CLI-006). Glint-state only: no native OS setting is touched.
//!
//! Each command first decides its route ([`crate::ipc::route`]): with a
//! core running it sends the typed IPC operation and the core runs it on
//! its own installer (the single writer); with none it opens the installer
//! itself through `glint-sources` (staging) and `glint-installer` (store,
//! activation, export, bundled default theme). Both paths build the same
//! reply values (`glint_ipc::ThemeListReply`, ...) and render them the same
//! way, so the JSON `data` differs only in `route`.

use std::fs;
use std::path::{Path, PathBuf};

use glint_installer::{
    Activated, ErrorCode as InstallCode, InstallError, InstalledPackage, Installer, is_bundled,
};
use glint_ipc::{
    ActiveInfo, GlintRequest, PackageInfo, ResponseBody, ThemeActivationReply, ThemeExportReply,
    ThemeImportReply, ThemeListReply, ThemeRemoveReply,
};
use glint_sources::{LocalSource, PackageSource, ThemeSource};
use glint_storage::{ActiveTheme, ErrorCategory, GlintPaths, StorageError};
use serde_json::{Value, json};

use super::source_error;
use crate::default_theme::DEFAULT_THEME;
use crate::error::{CliError, ErrorKind, codes};
use crate::ipc::{self, Route};
use crate::output::Success;

/// The resolved Glint home (`GLINT_HOME` or the OS directories).
pub fn paths() -> Result<GlintPaths, CliError> {
    GlintPaths::resolve().map_err(|error| storage_error(&error))
}

/// Open the installer for the offline route. Opening recovers interrupted
/// operations and a corrupt database before any command runs. A core that
/// started after routing holds the lock: that is exit 6, never a silent
/// retry.
fn open(paths: &GlintPaths) -> Result<Installer, CliError> {
    Installer::open(paths).map_err(|error| {
        if error.code() == InstallCode::Busy && ipc::core_running(paths) {
            ipc::core_unreachable("it started while this command was running")
                .with_detail("installCode", InstallCode::Busy.as_str())
        } else {
            install_error(&error)
        }
    })
}

/// Make `path` absolute against the CLI's working directory: the core's
/// working directory is not the caller's.
fn absolute(path: &Path) -> Result<PathBuf, CliError> {
    std::path::absolute(path).map_err(|error| {
        CliError::new(
            ErrorKind::Failure,
            codes::IO,
            format!("could not resolve {}: {error}", path.display()),
        )
    })
}

/// `glint theme import PATH`: stage a `.glint` (or a theme directory) through
/// the strict reader and install it without activating. A rejected package
/// changes nothing, so the active theme is kept.
pub fn import(path: &Path) -> Result<Success, CliError> {
    let paths = paths()?;
    let route = ipc::route(&paths, true)?;
    let route_name = route.name();
    let reply = match route {
        Route::Daemon(mut client) => {
            let request = GlintRequest::ThemeImport {
                path: absolute(path)?.to_string_lossy().into_owned(),
            };
            match ipc::request(&mut client, request)? {
                ResponseBody::ThemeImported(reply) => *reply,
                other => return Err(ipc::unexpected(&other)),
            }
        }
        Route::Offline => {
            let mut installer = open(&paths)?;
            let source = if path.is_dir() {
                ThemeSource::LocalDirectory {
                    path: path.to_path_buf(),
                }
            } else {
                ThemeSource::LocalArchive {
                    path: path.to_path_buf(),
                }
            };
            let staged = LocalSource::new()
                .stage(&source, &installer.staging_root())
                .map_err(|error| source_error(&error).with_detail("route", "offline"))?;
            let imported = installer
                .import(staged.candidate, staged.origin, &DEFAULT_THEME)
                .map_err(|error| install_error(&error))?;
            let active = installer
                .active_theme()
                .map_err(|error| install_error(&error))?;
            ThemeImportReply::new(
                package_info(&imported.package),
                imported.already_installed,
                active.as_ref().map(active_info),
            )
        }
    };
    let package = &reply.package;
    let text = format!(
        "{} {} {} ({})\n  sha256   {}\n  Not activated; run `glint theme use {}` to use it.",
        if reply.already_installed {
            "already installed:"
        } else {
            "imported:"
        },
        package.id,
        package.version,
        package.origin,
        package.digest,
        package.id,
    );
    Ok(Success {
        data: json!({
            "package": package_json(package, reply.active.as_ref()),
            "alreadyInstalled": reply.already_installed,
            "activated": false,
            "active": active_json(reply.active.as_ref()),
            "route": route_name,
        }),
        text,
    })
}

/// `glint theme list`: every installed release and the active theme.
pub fn list() -> Result<Success, CliError> {
    let paths = paths()?;
    let route = ipc::route(&paths, false)?;
    let route_name = route.name();
    let reply = match route {
        Route::Daemon(mut client) => match ipc::request(&mut client, GlintRequest::ThemeList)? {
            ResponseBody::ThemeList(reply) => reply,
            other => return Err(ipc::unexpected(&other)),
        },
        Route::Offline => {
            let installer = open(&paths)?;
            let packages = installer
                .installed()
                .map_err(|error| install_error(&error))?;
            let active = installer
                .active_theme()
                .map_err(|error| install_error(&error))?;
            ThemeListReply::new(
                packages.iter().map(package_info).collect(),
                active.as_ref().map(active_info),
            )
        }
    };
    let active = reply.active.as_ref();
    let mut text = String::new();
    if reply.packages.is_empty() {
        text.push_str(
            "No themes installed. `glint theme import PATH` adds one; \
             `glint safe-mode` installs the bundled default theme.",
        );
    }
    for package in &reply.packages {
        let marker = if is_active(package, active) { "*" } else { " " };
        let bundled = if package.bundled { "  (bundled)" } else { "" };
        text.push_str(&format!(
            "{marker} {} {}  {}  {}{bundled}\n",
            package.id,
            package.version,
            &package.digest[..12.min(package.digest.len())],
            package.origin,
        ));
    }
    if let Some(active) = active {
        text.push_str(&format!("\nactive profile: {}", active.profile_id));
    }
    Ok(Success {
        data: json!({
            "packages": reply
                .packages
                .iter()
                .map(|p| package_json(p, active))
                .collect::<Vec<_>>(),
            "active": active_json(active),
            "route": route_name,
        }),
        text,
    })
}

/// `glint theme export [-o PATH] ID`: write an installed release back to a
/// `.glint` without local data (default `./<id>-<version>.glint`).
pub fn export(query: &str, output: Option<&str>) -> Result<Success, CliError> {
    let paths = paths()?;
    let route = ipc::route(&paths, true)?;
    let route_name = route.name();
    let (reply, dest) = match route {
        Route::Daemon(mut client) => export_via_core(&mut client, query, output)?,
        Route::Offline => {
            let installer = open(&paths)?;
            let package = installer
                .find(query)
                .map_err(|error| install_error(&error))?;
            let dest = output.map_or_else(
                || PathBuf::from(format!("{}-{}.glint", package.id, package.version)),
                PathBuf::from,
            );
            let exported = installer
                .export(&package.digest, &dest)
                .map_err(|error| install_error(&error))?;
            let reply = ThemeExportReply::new(
                exported.id,
                exported.version,
                exported.installed_digest,
                exported.path.display().to_string(),
                exported.digest,
                exported.size,
                u64::try_from(exported.files).unwrap_or(u64::MAX),
            );
            (reply, dest)
        }
    };
    let note = if reply.is_canonical() {
        "byte-identical to the installed archive"
    } else {
        "canonical form of a non-Glint archive: same files, new digest"
    };
    let text = format!(
        "exported: {} {} -> {}\n  sha256   {}\n  {note}; no local paths, provenance or profile data included.",
        reply.id,
        reply.version,
        dest.display(),
        reply.digest,
    );
    Ok(Success {
        data: json!({
            "package": {
                "id": reply.id,
                "version": reply.version,
                "digest": reply.installed_digest,
            },
            "archive": {
                "path": dest.display().to_string(),
                "sha256": reply.digest,
                "size": reply.size,
                "files": reply.files,
            },
            "canonical": reply.is_canonical(),
            "route": route_name,
        }),
        text,
    })
}

/// Export through the core. Without `-o` the name depends on the release
/// the core resolves, so the core writes a temporary file in the working
/// directory and the CLI renames it to `<id>-<version>.glint`.
fn export_via_core(
    client: &mut glint_ipc::Client,
    query: &str,
    output: Option<&str>,
) -> Result<(ThemeExportReply, PathBuf), CliError> {
    let target = match output {
        Some(output) => absolute(Path::new(output))?,
        None => absolute(Path::new(&format!(
            ".glint-export-{}.tmp",
            std::process::id()
        )))?,
    };
    let request = GlintRequest::ThemeExport {
        theme: query.to_owned(),
        destination: target.to_string_lossy().into_owned(),
    };
    let reply = match ipc::request(client, request)? {
        ResponseBody::ThemeExported(reply) => reply,
        other => return Err(ipc::unexpected(&other)),
    };
    let Some(output) = output else {
        let dest = PathBuf::from(format!("{}-{}.glint", reply.id, reply.version));
        if let Err(error) = fs::rename(&target, &dest) {
            let _ = fs::remove_file(&target);
            return Err(
                super::io_error(&error, "move the exported archive into place")
                    .with_detail("path", dest.display().to_string())
                    .with_detail("route", "daemon"),
            );
        }
        return Ok((reply, dest));
    };
    Ok((reply, PathBuf::from(output)))
}

/// `glint theme remove ID`: remove an installed release that no profile
/// uses.
pub fn remove(query: &str) -> Result<Success, CliError> {
    let paths = paths()?;
    let route = ipc::route(&paths, true)?;
    let route_name = route.name();
    let removed = match route {
        Route::Daemon(mut client) => {
            let request = GlintRequest::ThemeRemove {
                theme: query.to_owned(),
            };
            match ipc::request(&mut client, request)? {
                ResponseBody::ThemeRemoved(reply) => reply,
                other => return Err(ipc::unexpected(&other)),
            }
        }
        Route::Offline => {
            let mut installer = open(&paths)?;
            let package = installer
                .find(query)
                .map_err(|error| install_error(&error))?;
            installer
                .remove(&package.digest)
                .map_err(|error| install_error(&error))?;
            ThemeRemoveReply::new(package.id, package.version, package.digest)
        }
    };
    Ok(Success {
        text: format!(
            "removed: {} {} ({})",
            removed.id, removed.version, removed.digest
        ),
        data: json!({
            "removed": {
                "id": removed.id,
                "version": removed.version,
                "digest": removed.digest,
            },
            "route": route_name,
        }),
    })
}

/// `glint theme use ID|default`: activate an installed release (the bundled
/// default theme is installed or repaired first for `default`).
pub fn use_theme(query: &str) -> Result<Success, CliError> {
    let paths = paths()?;
    let route = ipc::route(&paths, true)?;
    let route_name = route.name();
    let reply = match route {
        Route::Daemon(mut client) => {
            let request = GlintRequest::ThemeUse {
                theme: query.to_owned(),
            };
            activation_reply(ipc::request(&mut client, request)?)?
        }
        Route::Offline => {
            let mut installer = open(&paths)?;
            let wants_default = query == glint_installer::DEFAULT_ALIAS
                || query == glint_installer::DEFAULT_THEME_ID;
            let digest = if wants_default {
                installer
                    .ensure_bundled(&DEFAULT_THEME)
                    .map_err(|error| install_error(&error))?
                    .digest
            } else {
                installer
                    .find(query)
                    .map_err(|error| install_error(&error))?
                    .digest
            };
            let activated = installer
                .use_theme(&digest)
                .map_err(|error| install_error(&error))?;
            offline_activation(&activated, false)
        }
    };
    Ok(activation(&reply, route_name))
}

/// `glint safe-mode`: activate the bundled default theme in the safe-mode
/// profile, installing or repairing it from the embedded archive.
pub fn safe_mode() -> Result<Success, CliError> {
    let paths = paths()?;
    let route = ipc::route(&paths, true)?;
    let route_name = route.name();
    let reply = match route {
        Route::Daemon(mut client) => {
            activation_reply(ipc::request(&mut client, GlintRequest::SafeMode)?)?
        }
        Route::Offline => {
            let mut installer = open(&paths)?;
            let activated = installer
                .safe_mode(&DEFAULT_THEME)
                .map_err(|error| install_error(&error))?;
            offline_activation(&activated, true)
        }
    };
    Ok(activation(&reply, route_name))
}

fn activation_reply(body: ResponseBody) -> Result<ThemeActivationReply, CliError> {
    match body {
        ResponseBody::ThemeActivated(reply) => Ok(*reply),
        other => Err(ipc::unexpected(&other)),
    }
}

fn offline_activation(activated: &Activated, safe_mode: bool) -> ThemeActivationReply {
    ThemeActivationReply::new(
        package_info(&activated.package),
        active_info(&activated.active),
        activated.previous.as_ref().map(active_info),
        safe_mode,
    )
}

fn activation(reply: &ThemeActivationReply, route: &str) -> Success {
    let package = &reply.package;
    let mut text = format!(
        "{}: {} {} (profile {})",
        if reply.safe_mode {
            "safe mode"
        } else {
            "active"
        },
        package.id,
        package.version,
        reply.active.profile_id,
    );
    text.push_str(if route == "daemon" {
        "\n  Applied by the running Glint core, which presents it now; \
         no native setting was changed."
    } else {
        "\n  Glint state only: surfaces pick it up when the shell runs; \
         no native setting was changed."
    });
    Success {
        data: json!({
            "package": package_json(package, Some(&reply.active)),
            "active": active_json(Some(&reply.active)),
            "previous": active_json(reply.previous.as_ref()),
            "safeMode": reply.safe_mode,
            "route": route,
        }),
        text,
    }
}

/// The wire form of an installed release (shared by both routes).
pub fn package_info(package: &InstalledPackage) -> PackageInfo {
    PackageInfo::new(
        package.id.clone(),
        package.version.clone(),
        package.digest.clone(),
        package.origin.clone(),
        package.source_kind.clone(),
        package.source_locator.clone(),
        package.archive_size,
        package.installed_at,
        is_bundled(package),
    )
}

/// The wire form of the active profile.
pub fn active_info(active: &ActiveTheme) -> ActiveInfo {
    ActiveInfo::new(
        active.profile_id.clone(),
        active.package_digest.clone(),
        active.revision,
    )
}

fn is_active(package: &PackageInfo, active: Option<&ActiveInfo>) -> bool {
    active.is_some_and(|a| a.digest == package.digest)
}

fn package_json(package: &PackageInfo, active: Option<&ActiveInfo>) -> Value {
    json!({
        "id": package.id,
        "version": package.version,
        "digest": package.digest,
        "origin": package.origin,
        "source": {
            "kind": package.source_kind,
            "locator": package.source_locator,
        },
        "size": package.size,
        "installedAt": package.installed_at,
        "bundled": package.bundled,
        "active": is_active(package, active),
    })
}

/// `{profile, digest, revision}` or `null`.
pub fn active_json(active: Option<&ActiveInfo>) -> Value {
    active.map_or(Value::Null, |a| {
        json!({
            "profile": a.profile_id,
            "digest": a.digest,
            "revision": a.revision,
        })
    })
}

fn category_kind(category: ErrorCategory) -> ErrorKind {
    match category {
        ErrorCategory::PermissionDenied => ErrorKind::PermissionDenied,
        _ => ErrorKind::Failure,
    }
}

/// Strip the library's own `CODE: ` prefix from a display string.
fn message(display: &str, code: &str) -> String {
    display
        .strip_prefix(code)
        .and_then(|rest| rest.strip_prefix(": "))
        .unwrap_or(display)
        .to_owned()
}

/// Map a storage error to a CLI error.
pub fn storage_error(error: &StorageError) -> CliError {
    let code = error.code().as_str();
    CliError::new(
        category_kind(error.category()),
        code,
        message(&error.to_string(), code),
    )
    .with_detail("storageCode", code)
}

/// Map an installer error to a CLI error, keeping the most specific code
/// (`GLINT_IDENTITY_*`, `GLINT_PACKAGE_*`, `GLINT_STORAGE_*`) in `code` and
/// the installer code in `details.installCode`.
pub fn install_error(error: &InstallError) -> CliError {
    let install_code = error.code();
    let kind = match install_code {
        InstallCode::NotInstalled => ErrorKind::NotFound,
        InstallCode::Ambiguous | InstallCode::ExportDestinationInvalid => ErrorKind::Usage,
        _ => category_kind(error.category()),
    };
    let display = error.to_string();
    let mut cli = CliError::new(
        kind,
        install_code.as_str(),
        message(&display, install_code.as_str()),
    )
    .with_detail("installCode", install_code.as_str())
    .with_detail("route", "offline");
    if let Some(outcome) = error.identity_outcome() {
        cli.code = outcome.code().to_owned();
        cli = cli.with_detail("identityCode", outcome.code());
    }
    if let Some(package) = error.package_error() {
        cli.code = package.code().as_str().to_owned();
        cli = cli.with_detail("packageCode", package.code().as_str());
    }
    if let Some(storage) = error.storage_error() {
        cli.code = storage.code().as_str().to_owned();
        cli = cli.with_detail("storageCode", storage.code().as_str());
    }
    match error {
        InstallError::Ambiguous { candidates, .. } => {
            cli = cli.with_detail("candidates", candidates.clone());
        }
        InstallError::InUse { profiles, .. } => {
            cli = cli.with_detail("profiles", profiles.clone());
        }
        _ => {}
    }
    cli
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::PathBuf;

    use glint_installer::InstallError;

    use super::install_error;
    use crate::error::ErrorKind;

    #[test]
    fn installer_errors_map_to_exit_code_kinds() {
        let cases = [
            (InstallError::NotInstalled("x".into()), ErrorKind::NotFound),
            (
                InstallError::Ambiguous {
                    query: "x".into(),
                    candidates: vec!["x@1.0.0".into(), "x@2.0.0".into()],
                },
                ErrorKind::Usage,
            ),
            (
                InstallError::InUse {
                    digest: "d".into(),
                    profiles: vec!["default".into()],
                },
                ErrorKind::Failure,
            ),
            (
                InstallError::Io {
                    context: "read",
                    path: PathBuf::from("x"),
                    source: io::Error::from(io::ErrorKind::PermissionDenied),
                },
                ErrorKind::PermissionDenied,
            ),
            (
                InstallError::ReservedId("official.default".into()),
                ErrorKind::Failure,
            ),
        ];
        for (error, kind) in cases {
            let cli = install_error(&error);
            assert_eq!(cli.kind, kind, "{error}");
            assert!(cli.code.starts_with("GLINT_"), "{}", cli.code);
            assert!(!cli.message.starts_with("GLINT_"), "{}", cli.message);
        }
        let ambiguous = install_error(&InstallError::Ambiguous {
            query: "x".into(),
            candidates: vec!["x@1.0.0".into()],
        });
        assert_eq!(ambiguous.code, "GLINT_INSTALL_AMBIGUOUS");
        assert_eq!(ambiguous.details["candidates"][0], "x@1.0.0");
    }
}
