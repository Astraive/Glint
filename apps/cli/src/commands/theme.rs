//! `glint theme import/list/export/remove/use` and `glint safe-mode`
//! (CLI-002). Offline and Glint-state only: every handler is wiring onto
//! `glint-sources` (staging) and `glint-installer` (store, activation,
//! export, bundled default theme); no native OS setting is touched.

use std::path::{Path, PathBuf};

use glint_installer::{
    Activated, ErrorCode as InstallCode, InstallError, InstalledPackage, Installer, is_bundled,
};
use glint_sources::{LocalSource, PackageSource, ThemeSource};
use glint_storage::{ActiveTheme, ErrorCategory, GlintPaths, StorageError};
use serde_json::{Value, json};

use super::source_error;
use crate::default_theme::DEFAULT_THEME;
use crate::error::{CliError, ErrorKind};
use crate::output::Success;

/// Open the installer at the resolved Glint home (`GLINT_HOME` or the OS
/// directories). Opening recovers interrupted operations and a corrupt
/// database before any command runs.
fn open() -> Result<Installer, CliError> {
    let paths = GlintPaths::resolve().map_err(|error| storage_error(&error))?;
    Installer::open(&paths).map_err(|error| install_error(&error))
}

/// `glint theme import PATH`: stage a `.glint` (or a theme directory) through
/// the strict reader and install it without activating. A rejected package
/// changes nothing, so the active theme is kept.
pub fn import(path: &Path) -> Result<Success, CliError> {
    let mut installer = open()?;
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
        .map_err(|error| source_error(&error))?;
    let imported = installer
        .import(staged.candidate, staged.origin, &DEFAULT_THEME)
        .map_err(|error| install_error(&error))?;
    let active = installer
        .active_theme()
        .map_err(|error| install_error(&error))?;
    let package = &imported.package;
    let text = format!(
        "{} {} {} ({})\n  sha256   {}\n  Not activated; run `glint theme use {}` to use it.",
        if imported.already_installed {
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
            "package": package_json(package, active.as_ref()),
            "alreadyInstalled": imported.already_installed,
            "activated": false,
            "active": active_json(active.as_ref()),
        }),
        text,
    })
}

/// `glint theme list`: every installed release and the active theme.
pub fn list() -> Result<Success, CliError> {
    let installer = open()?;
    let packages = installer
        .installed()
        .map_err(|error| install_error(&error))?;
    let active = installer
        .active_theme()
        .map_err(|error| install_error(&error))?;
    let mut text = String::new();
    if packages.is_empty() {
        text.push_str(
            "No themes installed. `glint theme import PATH` adds one; \
             `glint safe-mode` installs the bundled default theme.",
        );
    }
    for package in &packages {
        let marker = if is_active(package, active.as_ref()) {
            "*"
        } else {
            " "
        };
        let bundled = if is_bundled(package) {
            "  (bundled)"
        } else {
            ""
        };
        text.push_str(&format!(
            "{marker} {} {}  {}  {}{bundled}\n",
            package.id,
            package.version,
            &package.digest[..12],
            package.origin,
        ));
    }
    if let Some(active) = &active {
        text.push_str(&format!("\nactive profile: {}", active.profile_id));
    }
    Ok(Success {
        data: json!({
            "packages": packages
                .iter()
                .map(|p| package_json(p, active.as_ref()))
                .collect::<Vec<_>>(),
            "active": active_json(active.as_ref()),
        }),
        text,
    })
}

/// `glint theme export [-o PATH] ID`: write an installed release back to a
/// `.glint` without local data (default `./<id>-<version>.glint`).
pub fn export(query: &str, output: Option<&str>) -> Result<Success, CliError> {
    let installer = open()?;
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
    let note = if exported.is_canonical() {
        "byte-identical to the installed archive"
    } else {
        "canonical form of a non-Glint archive: same files, new digest"
    };
    let text = format!(
        "exported: {} {} -> {}\n  sha256   {}\n  {note}; no local paths, provenance or profile data included.",
        exported.id,
        exported.version,
        exported.path.display(),
        exported.digest,
    );
    Ok(Success {
        data: json!({
            "package": {
                "id": exported.id,
                "version": exported.version,
                "digest": exported.installed_digest,
            },
            "archive": {
                "path": exported.path.display().to_string(),
                "sha256": exported.digest,
                "size": exported.size,
                "files": exported.files,
            },
            "canonical": exported.is_canonical(),
        }),
        text,
    })
}

/// `glint theme remove ID`: remove an installed release that no profile
/// uses.
pub fn remove(query: &str) -> Result<Success, CliError> {
    let mut installer = open()?;
    let package = installer
        .find(query)
        .map_err(|error| install_error(&error))?;
    installer
        .remove(&package.digest)
        .map_err(|error| install_error(&error))?;
    Ok(Success {
        data: json!({
            "removed": {
                "id": package.id,
                "version": package.version,
                "digest": package.digest,
            },
        }),
        text: format!(
            "removed: {} {} ({})",
            package.id, package.version, package.digest
        ),
    })
}

/// `glint theme use ID|default`: activate an installed release (the bundled
/// default theme is installed or repaired first for `default`).
pub fn use_theme(query: &str) -> Result<Success, CliError> {
    let mut installer = open()?;
    let wants_default =
        query == glint_installer::DEFAULT_ALIAS || query == glint_installer::DEFAULT_THEME_ID;
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
    Ok(activation(&activated, false))
}

/// `glint safe-mode`: activate the bundled default theme in the safe-mode
/// profile, installing or repairing it from the embedded archive.
pub fn safe_mode() -> Result<Success, CliError> {
    let mut installer = open()?;
    let activated = installer
        .safe_mode(&DEFAULT_THEME)
        .map_err(|error| install_error(&error))?;
    Ok(activation(&activated, true))
}

fn activation(activated: &Activated, safe_mode: bool) -> Success {
    let package = &activated.package;
    let mut text = format!(
        "{}: {} {} (profile {})",
        if safe_mode { "safe mode" } else { "active" },
        package.id,
        package.version,
        activated.active.profile_id,
    );
    text.push_str(
        "\n  Glint state only: surfaces pick it up when the shell runs; \
         no native setting was changed.",
    );
    Success {
        data: json!({
            "package": package_json(package, Some(&activated.active)),
            "active": active_json(Some(&activated.active)),
            "previous": active_json(activated.previous.as_ref()),
            "safeMode": safe_mode,
        }),
        text,
    }
}

fn is_active(package: &InstalledPackage, active: Option<&ActiveTheme>) -> bool {
    active.is_some_and(|a| a.package_digest == package.digest)
}

fn package_json(package: &InstalledPackage, active: Option<&ActiveTheme>) -> Value {
    json!({
        "id": package.id,
        "version": package.version,
        "digest": package.digest,
        "origin": package.origin,
        "source": {
            "kind": package.source_kind,
            "locator": package.source_locator,
        },
        "size": package.archive_size,
        "installedAt": package.installed_at,
        "bundled": is_bundled(package),
        "active": is_active(package, active),
    })
}

fn active_json(active: Option<&ActiveTheme>) -> Value {
    active.map_or(Value::Null, |a| {
        json!({
            "profile": a.profile_id,
            "digest": a.package_digest,
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

fn storage_error(error: &StorageError) -> CliError {
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
    .with_detail("installCode", install_code.as_str());
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
