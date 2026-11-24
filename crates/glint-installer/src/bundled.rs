//! The bundled default theme, local import, theme lookup and safe mode
//! (CLI-002): the library side of `glint theme import/list/export/remove/use`
//! and `glint safe-mode`, so front ends (CLI, later the manager) stay thin.
//!
//! # Bundled default theme
//!
//! The front end embeds the default theme's deterministic `.glint` archive
//! and hands it over as a [`BundledTheme`]. [`Installer::ensure_bundled`]
//! installs it through the same reader and store as any other package
//! (origin `local`, source locator [`BUNDLED_LOCATOR`]) and repairs a damaged
//! store entry from the embedded bytes, so safe mode works with an empty
//! store, a damaged entry or a recovered (recreated) database.
//!
//! The bundled ID is reserved: importing a package with
//! [`DEFAULT_THEME_ID`] but other bytes is refused
//! ([`ErrorCode::ReservedId`](crate::ErrorCode::ReservedId)), so nothing can
//! shadow the fallback. Re-importing the exported default theme (identical
//! bytes) is an idempotent no-op.
//!
//! # Profiles
//!
//! `theme use` changes the base theme of the active profile, or of
//! [`DEFAULT_PROFILE`] when no profile is active or safe mode is on. Safe
//! mode activates the separate [`SAFE_MODE_PROFILE`], so the user's profiles
//! keep their themes for a later `theme use` or `theme revert`.

use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

use glint_package::PackageCandidate;
use glint_sources::{LocalSource, PackageSource, SourceOrigin, ThemeSource};
use glint_storage::ActiveTheme;
use sha2::{Digest, Sha256};

use crate::error::InstallError;
use crate::fsutil;
use crate::install::{InstalledPackage, Installer};

/// Manifest ID of the bundled default theme.
pub const DEFAULT_THEME_ID: &str = "official.default";
/// The alias that names the bundled default theme in lookups
/// (`glint theme use default`).
pub const DEFAULT_ALIAS: &str = "default";
/// Source locator recorded for the bundled default theme.
pub const BUNDLED_LOCATOR: &str = "bundled:official.default";
/// Profile that `theme use` targets when no other profile is active.
pub const DEFAULT_PROFILE: &str = "default";
/// Profile that safe mode activates.
pub const SAFE_MODE_PROFILE: &str = "safe-mode";

static BUNDLED_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The embedded archive of the bundled default theme.
#[derive(Debug, Clone, Copy)]
pub struct BundledTheme {
    archive: &'static [u8],
}

impl BundledTheme {
    /// Wrap the embedded archive bytes.
    pub const fn new(archive: &'static [u8]) -> Self {
        Self { archive }
    }

    /// The archive bytes.
    pub fn archive(&self) -> &'static [u8] {
        self.archive
    }

    /// Lowercase hex SHA-256 of the archive (its store digest).
    pub fn digest(&self) -> String {
        fsutil::to_hex(&Sha256::digest(self.archive))
    }
}

/// The result of [`Installer::import`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Imported {
    /// The installed release.
    pub package: InstalledPackage,
    /// The digest was installed already; nothing changed.
    pub already_installed: bool,
}

/// The result of [`Installer::safe_mode`] and [`Installer::use_theme`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Activated {
    /// The release that is now the active base theme.
    pub package: InstalledPackage,
    /// The new active profile state.
    pub active: ActiveTheme,
    /// What was active before, if anything.
    pub previous: Option<ActiveTheme>,
}

impl Installer {
    /// Install the bundled default theme if it is missing, repair its store
    /// entry if it is damaged, and return it. Never activates.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::BundledInvalid`](crate::ErrorCode::BundledInvalid) when
    /// the embedded archive fails the reader (a build defect), I/O and
    /// storage errors, or
    /// [`ErrorCode::IdentityConflict`](crate::ErrorCode::IdentityConflict)
    /// when another release of the default theme's ID and version occupies
    /// the identity.
    pub fn ensure_bundled(
        &mut self,
        bundled: &BundledTheme,
    ) -> Result<InstalledPackage, InstallError> {
        let digest = bundled.digest();
        if let Some(existing) = self.package(&digest)? {
            if entry_is_intact(&existing) {
                return Ok(existing);
            }
            self.forget_damaged(&digest)?;
        }
        let cache = self.paths.cache_dir().to_path_buf();
        fs::create_dir_all(&cache).map_err(InstallError::io("create cache directory", &cache))?;
        let temp = cache.join(format!(
            "bundled-default-{}-{}.glint",
            std::process::id(),
            BUNDLED_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&temp, bundled.archive())
            .map_err(InstallError::io("write bundled archive", &temp))?;
        let staged = LocalSource::new().stage(
            &ThemeSource::LocalArchive { path: temp.clone() },
            &self.staging_root(),
        );
        let _ = fs::remove_file(&temp);
        let staged = staged.map_err(InstallError::Bundled)?;
        let mut origin = staged.origin;
        origin.locator = BUNDLED_LOCATOR.to_owned();
        origin.local_path = None;
        self.install(staged.candidate, origin)
    }

    /// Install a staged local package without activating it. The bundled
    /// default theme's ID is reserved: identical bytes are the bundled
    /// release (installed or repaired from `bundled`), other bytes are
    /// refused.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::ReservedId`](crate::ErrorCode::ReservedId), and the
    /// errors of [`Installer::install`]. On every error the previous state,
    /// including the active theme, is kept.
    pub fn import(
        &mut self,
        candidate: PackageCandidate,
        origin: SourceOrigin,
        bundled: &BundledTheme,
    ) -> Result<Imported, InstallError> {
        let digest = candidate.sha256_hex();
        if candidate.manifest.id == DEFAULT_THEME_ID {
            if digest != bundled.digest() {
                return Err(InstallError::ReservedId(candidate.manifest.id.clone()));
            }
            drop(candidate);
            let already_installed = self.package(&digest)?.is_some();
            let package = self.ensure_bundled(bundled)?;
            return Ok(Imported {
                package,
                already_installed,
            });
        }
        let already_installed = self.package(&digest)?.is_some();
        let package = self.install(candidate, origin)?;
        Ok(Imported {
            package,
            already_installed,
        })
    }

    /// Find one installed release by `ID`, `ID@VERSION`, digest (64 hex
    /// digits, optionally `sha256:`-prefixed) or [`DEFAULT_ALIAS`].
    ///
    /// # Errors
    ///
    /// [`ErrorCode::NotInstalled`](crate::ErrorCode::NotInstalled) when
    /// nothing matches, [`ErrorCode::Ambiguous`](crate::ErrorCode::Ambiguous)
    /// when an ID has several installed versions, storage errors.
    pub fn find(&self, query: &str) -> Result<InstalledPackage, InstallError> {
        let hex = query.strip_prefix("sha256:").unwrap_or(query);
        if glint_storage::validate_digest(hex).is_ok() {
            return self
                .package(hex)?
                .ok_or_else(|| InstallError::NotInstalled(query.to_owned()));
        }
        let (id, version) = match query.split_once('@') {
            Some((id, version)) => (id, Some(version)),
            None => (query, None),
        };
        let id = if id == DEFAULT_ALIAS {
            DEFAULT_THEME_ID
        } else {
            id
        };
        let mut matches: Vec<InstalledPackage> = self
            .installed()?
            .into_iter()
            .filter(|p| p.id == id && version.is_none_or(|v| p.version == v))
            .collect();
        match matches.len() {
            0 => Err(InstallError::NotInstalled(query.to_owned())),
            1 => Ok(matches.remove(0)),
            _ => Err(InstallError::Ambiguous {
                query: query.to_owned(),
                candidates: matches
                    .iter()
                    .map(|p| format!("{}@{}", p.id, p.version))
                    .collect(),
            }),
        }
    }

    /// Make an installed release the base theme of the active profile (or
    /// of [`DEFAULT_PROFILE`] when none is active or safe mode is on).
    /// Glint state only: no surfaces or native settings change here.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::NotInstalled`](crate::ErrorCode::NotInstalled) or storage
    /// errors; the previous active theme is kept.
    pub fn use_theme(&mut self, digest: &str) -> Result<Activated, InstallError> {
        let package = self
            .package(digest)?
            .ok_or_else(|| InstallError::NotInstalled(digest.to_owned()))?;
        let previous = self.active_theme()?;
        let profile = match &previous {
            Some(active) if active.profile_id != SAFE_MODE_PROFILE => active.profile_id.clone(),
            _ => DEFAULT_PROFILE.to_owned(),
        };
        let active = self.activate(&profile, digest)?;
        Ok(Activated {
            package,
            active,
            previous,
        })
    }

    /// Safe mode: install or repair the bundled default theme and activate
    /// it in [`SAFE_MODE_PROFILE`]. Works with an empty store, a damaged
    /// store entry and a database recovered on [`Installer::open`].
    ///
    /// # Errors
    ///
    /// The errors of [`Installer::ensure_bundled`] and storage errors.
    pub fn safe_mode(&mut self, bundled: &BundledTheme) -> Result<Activated, InstallError> {
        let package = self.ensure_bundled(bundled)?;
        let previous = self.active_theme()?;
        let active = self.activate(SAFE_MODE_PROFILE, &package.digest)?;
        Ok(Activated {
            package,
            active,
            previous,
        })
    }

    /// Drop the index row and the store entry of a damaged release so it
    /// can be reinstalled under the same digest. Profiles keep referencing
    /// the digest, which the reinstall restores byte for byte.
    fn forget_damaged(&mut self, digest: &str) -> Result<(), InstallError> {
        let entry = self.paths.store_dir().join(digest);
        fsutil::remove_tree(&entry)
            .map_err(InstallError::io("remove damaged store entry", &entry))?;
        self.storage
            .connection()
            .execute("DELETE FROM packages WHERE digest = ?1", [digest])
            .map_err(InstallError::db("forget damaged package"))?;
        Ok(())
    }
}

/// True when the store entry still holds an archive with its content address
/// and an extracted tree with a manifest.
fn entry_is_intact(package: &InstalledPackage) -> bool {
    glint_storage::is_real_dir(&package.path)
        && package.path.join(glint_package::MANIFEST_NAME).is_file()
        && fsutil::sha256_file(&package.archive_path).is_ok_and(|hex| hex == package.digest)
}

/// True when `package` is the bundled default theme.
pub fn is_bundled(package: &InstalledPackage) -> bool {
    package.id == DEFAULT_THEME_ID && package.source_locator == BUNDLED_LOCATOR
}

#[cfg(test)]
mod tests {
    use super::BundledTheme;

    #[test]
    fn digest_is_lowercase_sha256_hex() {
        static EMPTY: &[u8] = b"";
        assert_eq!(
            BundledTheme::new(EMPTY).digest(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
