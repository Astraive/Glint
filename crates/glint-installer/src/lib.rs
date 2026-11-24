//! Glint's transactional package installer (INS-003) and apply transaction
//! with crash rollback (INS-005).
//!
//! [`Installer::install`] takes a validated `glint-package`
//! [`PackageCandidate`](glint_package::PackageCandidate) and its
//! `glint-sources` [`SourceOrigin`](glint_sources::SourceOrigin) and moves
//! the staged package into an immutable, content-addressed store
//! (`<data>/store/<sha256>`) with a single rename, recording it in the
//! state database. Every step is journalled, so a crash at any point leaves
//! either the previous state or the committed new one, and
//! [`Installer::recover_pending`] (run on every [`Installer::open`]) cleans up
//! the rest. Installing does not activate unless asked
//! ([`InstallOptions::activate`]); [`Installer::remove`] refuses a release
//! that the active profile or any profile still references.
//! [`Installer::export`] (PKG-005) writes an installed release back out as a
//! canonical `.glint` without any locally added data.
//!
//! For front ends (CLI-002), [`Installer::import`], [`Installer::find`],
//! [`Installer::use_theme`], [`Installer::ensure_bundled`] and
//! [`Installer::safe_mode`] add local import with the reserved default
//! theme ID, theme lookup by ID, version or digest, activation and a safe
//! mode that always reaches the bundled default theme ([`BundledTheme`]).
//!
//! [`Installer::apply`] activates a profile together with native preference
//! changes under an apply journal with per-adapter snapshots
//! ([`RestoreSemantics`]); failures and crashes roll back to the previous
//! theme and report exactly which settings could not be restored
//! ([`RollbackReport`], `GLINT_APPLY_ROLLBACK_INCOMPLETE`).
//!
//! # Errors
//!
//! Every failure is an [`InstallError`] with a stable [`ErrorCode`]
//! (`GLINT_INSTALL_*`) and a category from `glint-storage`.
//!
//! # Example
//!
//! ```no_run
//! use glint_installer::Installer;
//! use glint_sources::{LocalSource, PackageSource, ThemeSource};
//! use glint_storage::GlintPaths;
//!
//! let mut installer = Installer::open(&GlintPaths::resolve()?)?;
//! let source = ThemeSource::LocalArchive { path: "midnight.glint".into() };
//! let staged = LocalSource::new().stage(&source, &installer.staging_root())?;
//! let installed = installer.install(staged.candidate, staged.origin)?;
//! println!("installed {} {} at {}", installed.id, installed.version, installed.path.display());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod bundled;
mod error;
mod export;
mod fault;
mod fsutil;
mod install;
mod provenance;
mod record;
mod recover;
mod rollback;

pub use bundled::{
    Activated, BUNDLED_LOCATOR, BundledTheme, DEFAULT_ALIAS, DEFAULT_PROFILE, DEFAULT_THEME_ID,
    Imported, SAFE_MODE_PROFILE, is_bundled,
};
pub use error::{ErrorCode, InstallError};
pub use export::ExportedPackage;
pub use fault::{FaultInjector, FaultPoint};
pub use install::{InstallOptions, InstalledPackage, Installer};
pub use provenance::package_origin;
pub use recover::RecoveryReport;
pub use rollback::{
    ApplyError, ApplyErrorCode, ApplyOutcome, ApplyPlan, NativeAdapter, NativeChange, NativeError,
    NativeErrorKind, PendingApply, PendingNative, RestoreSemantics, RollbackReport, SettingRef,
    SurfaceHost, Unrestored, UnrestoredReason,
};
