//! Typed theme operations on the daemon's installer (CLI-006).
//!
//! While the core runs, the daemon holds the installer lock, so a client
//! (the CLI, the manager) cannot open the store itself. It asks the core
//! over IPC instead, and the core runs the request here, on the only
//! holder of the installer: the single-writer rule holds for every client.
//! Each operation is exactly what the offline CLI does with its own
//! installer (`apps/cli/src/commands/theme.rs`), so both paths produce the
//! same state.

use std::path::{Path, PathBuf};

use glint_installer::{
    Activated, BundledTheme, ExportedPackage, Imported, InstalledPackage, Installer,
};
use glint_sources::{LocalSource, PackageSource, ThemeSource};
use glint_storage::ActiveTheme;

use crate::error::DaemonError;

/// The bundled default theme archive, packed from `themes/default` at build
/// time (`build.rs`), for `theme-use default` and `safe-mode`.
pub const BUNDLED_DEFAULT_THEME: BundledTheme = BundledTheme::new(include_bytes!(concat!(
    env!("OUT_DIR"),
    "/default-theme.glint"
)));

/// One theme operation a client asked the core for.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ThemeOperation {
    /// Every installed release and the active profile.
    List,
    /// Stage the archive or directory at `path` (absolute) and install it
    /// without activating.
    Import {
        /// Absolute path of a `.glint` or a theme directory.
        path: PathBuf,
    },
    /// Activate an installed release (`default`: the bundled theme).
    Use {
        /// `ID`, `ID@VERSION`, a digest or `default`.
        theme: String,
    },
    /// Remove a release no profile uses.
    Remove {
        /// `ID`, `ID@VERSION` or a digest.
        theme: String,
    },
    /// Export a release to `destination` (absolute).
    Export {
        /// `ID`, `ID@VERSION` or a digest.
        theme: String,
        /// Absolute path of the archive to write.
        destination: PathBuf,
    },
    /// Activate the bundled default theme in the `safe-mode` profile.
    SafeMode,
}

impl ThemeOperation {
    /// A short stable name (`theme-use`, `safe-mode`, ...).
    pub const fn name(&self) -> &'static str {
        match self {
            ThemeOperation::List => "theme-list",
            ThemeOperation::Import { .. } => "theme-import",
            ThemeOperation::Use { .. } => "theme-use",
            ThemeOperation::Remove { .. } => "theme-remove",
            ThemeOperation::Export { .. } => "theme-export",
            ThemeOperation::SafeMode => "safe-mode",
        }
    }

    /// True when the operation can change the store or the active theme.
    pub const fn changes_state(&self) -> bool {
        matches!(
            self,
            ThemeOperation::Import { .. }
                | ThemeOperation::Use { .. }
                | ThemeOperation::Remove { .. }
                | ThemeOperation::SafeMode
        )
    }
}

/// What a theme operation did.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ThemeOutcome {
    /// For [`ThemeOperation::List`].
    Listed {
        /// Installed releases.
        packages: Vec<InstalledPackage>,
        /// The active profile.
        active: Option<ActiveTheme>,
    },
    /// For [`ThemeOperation::Import`].
    Imported {
        /// The installed release.
        imported: Imported,
        /// The active profile (unchanged by an import).
        active: Option<ActiveTheme>,
    },
    /// For [`ThemeOperation::Use`] and [`ThemeOperation::SafeMode`].
    Activated {
        /// The activation.
        activated: Activated,
        /// Activated in the `safe-mode` profile.
        safe_mode: bool,
    },
    /// For [`ThemeOperation::Remove`].
    Removed {
        /// The removed release.
        package: InstalledPackage,
    },
    /// For [`ThemeOperation::Export`].
    Exported {
        /// The written archive.
        exported: ExportedPackage,
    },
}

fn absolute(path: &Path, what: &'static str) -> Result<(), DaemonError> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err(DaemonError::InvalidRequest(format!(
            "{what} must be an absolute path, got {}",
            path.display()
        )))
    }
}

/// Run `operation` on `installer` with `bundled` as the default theme.
pub(crate) fn run(
    installer: &mut Installer,
    bundled: &BundledTheme,
    operation: ThemeOperation,
) -> Result<ThemeOutcome, DaemonError> {
    match operation {
        ThemeOperation::List => Ok(ThemeOutcome::Listed {
            packages: installer.installed()?,
            active: installer.active_theme()?,
        }),
        ThemeOperation::Import { path } => {
            absolute(&path, "the package path")?;
            let source = if path.is_dir() {
                ThemeSource::LocalDirectory { path }
            } else {
                ThemeSource::LocalArchive { path }
            };
            let staged = LocalSource::new()
                .stage(&source, &installer.staging_root())
                .map_err(DaemonError::Source)?;
            let imported = installer.import(staged.candidate, staged.origin, bundled)?;
            Ok(ThemeOutcome::Imported {
                imported,
                active: installer.active_theme()?,
            })
        }
        ThemeOperation::Use { theme } => {
            let wants_default = theme == glint_installer::DEFAULT_ALIAS
                || theme == glint_installer::DEFAULT_THEME_ID;
            let digest = if wants_default {
                installer.ensure_bundled(bundled)?.digest
            } else {
                installer.find(&theme)?.digest
            };
            Ok(ThemeOutcome::Activated {
                activated: installer.use_theme(&digest)?,
                safe_mode: false,
            })
        }
        ThemeOperation::Remove { theme } => {
            let package = installer.find(&theme)?;
            installer.remove(&package.digest)?;
            Ok(ThemeOutcome::Removed { package })
        }
        ThemeOperation::Export { theme, destination } => {
            absolute(&destination, "the export destination")?;
            let package = installer.find(&theme)?;
            Ok(ThemeOutcome::Exported {
                exported: installer.export(&package.digest, &destination)?,
            })
        }
        ThemeOperation::SafeMode => Ok(ThemeOutcome::Activated {
            activated: installer.safe_mode(bundled)?,
            safe_mode: true,
        }),
    }
}
