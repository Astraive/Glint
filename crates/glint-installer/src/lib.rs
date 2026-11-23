//! Glint's transactional package installer (INS-003).
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

mod error;
mod fault;
mod fsutil;
mod install;
mod record;
mod recover;

pub use error::{ErrorCode, InstallError};
pub use fault::{FaultInjector, FaultPoint};
pub use install::{InstallOptions, InstalledPackage, Installer};
pub use recover::RecoveryReport;
