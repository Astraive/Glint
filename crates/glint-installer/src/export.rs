//! `Installer::export` (PKG-005): write an installed release back out as a
//! self-contained `.glint` archive without private data.
//!
//! # What goes in
//!
//! Exactly the files of the release's extracted, read-only `root/` tree:
//! the manifest and every bundled asset, token file and compiled surface the
//! reader accepted at install. They are re-packed with
//! [`glint_package::export_package_tree`], the deterministic writer, so the
//! archive carries no timestamps, owners, permissions, extra fields or
//! comments.
//!
//! # What stays out
//!
//! Everything Glint added locally lives outside `root/` and is never read:
//! the provenance record (`install.record`: source path, retrieval and
//! install times, origin), the state database (profiles, capability grants,
//! install locks, journals), the active-theme record, staging and cache.
//! The exported bytes therefore contain no user path, user name, credential
//! or token that the package did not already contain when it was imported.
//!
//! # Digest
//!
//! A release that was packed by Glint's writer (`glint theme pack`, the
//! bundled default theme, any archive this function wrote) re-packs to the
//! identical bytes, so the export has the installed digest and re-importing
//! it is an idempotent no-op ([`ExportedPackage::is_canonical`]). An archive
//! written by another ZIP tool re-packs to its canonical form: the same files
//! with a new digest, which then stays stable across further export and
//! re-import cycles. Re-importing such an export into the same store is a
//! digest conflict under ADR 0006 (same origin and version, other bytes) and
//! is refused, never silently merged.
//!
//! Before the archive is moved into place it is verified: the store copy
//! must still hash to its content address, and the new archive is read back
//! through the strict reader into private staging and must carry the same
//! package ID, version and file count.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use glint_package::{PackageLimits, export_package_tree, inspect_archive_in};

use crate::error::InstallError;
use crate::fsutil;
use crate::install::Installer;

static EXPORT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The result of [`Installer::export`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ExportedPackage {
    /// Manifest `id`.
    pub id: String,
    /// Manifest `version`.
    pub version: String,
    /// The installed release that was exported (its content address).
    pub installed_digest: String,
    /// Lowercase hex SHA-256 of the exported archive.
    pub digest: String,
    /// Exported archive size in bytes.
    pub size: u64,
    /// Number of files in the exported archive.
    pub files: usize,
    /// Where the archive was written.
    pub path: PathBuf,
}

impl ExportedPackage {
    /// True when the export is byte-identical to the installed archive, so
    /// re-importing it yields the same digest and changes nothing.
    pub fn is_canonical(&self) -> bool {
        self.digest == self.installed_digest
    }
}

impl Installer {
    /// Export the installed release `digest` to the archive `dest`,
    /// replacing an existing file. See the [module docs](self) for what is
    /// and is not included.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::NotInstalled`](crate::ErrorCode::NotInstalled),
    /// [`ErrorCode::StoreCorrupt`](crate::ErrorCode::StoreCorrupt) when the
    /// store entry no longer matches its digest or record,
    /// [`ErrorCode::ExportFailed`](crate::ErrorCode::ExportFailed) when the
    /// package cannot be re-packed or read back (see
    /// [`InstallError::package_error`]), and I/O errors. On every error
    /// `dest` is left untouched and no temporary file remains.
    pub fn export(&self, digest: &str, dest: &Path) -> Result<ExportedPackage, InstallError> {
        let package = self
            .package(digest)?
            .ok_or_else(|| InstallError::NotInstalled(digest.to_owned()))?;
        let actual = fsutil::sha256_file(&package.archive_path).map_err(InstallError::io(
            "hash stored archive",
            &package.archive_path,
        ))?;
        if actual != package.digest {
            return Err(InstallError::StoreCorrupt {
                digest: package.digest,
                reason: "the stored archive no longer matches its digest",
            });
        }
        if !glint_storage::is_real_dir(&package.path) {
            return Err(InstallError::StoreCorrupt {
                digest: package.digest,
                reason: "the extracted package tree is missing",
            });
        }

        let file_name = dest
            .file_name()
            .ok_or_else(|| InstallError::ExportDestinationInvalid(dest.to_path_buf()))?;
        let parent = match dest.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent,
            _ => Path::new("."),
        };
        if fs::metadata(dest).is_ok_and(|meta| meta.is_dir()) {
            return Err(InstallError::ExportDestinationInvalid(dest.to_path_buf()));
        }
        let temp = parent.join(format!(
            ".{}.export-{}-{}",
            file_name.to_string_lossy(),
            std::process::id(),
            EXPORT_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let result = self.write_verified(&package, &temp).and_then(|exported| {
            fs::rename(&temp, dest).map_err(InstallError::io("move export into place", dest))?;
            Ok(exported)
        });
        match result {
            Ok((digest, size, files)) => Ok(ExportedPackage {
                id: package.id,
                version: package.version,
                installed_digest: package.digest,
                digest,
                size,
                files,
                path: dest.to_path_buf(),
            }),
            Err(error) => {
                let _ = fs::remove_file(&temp);
                Err(error)
            }
        }
    }

    /// Re-pack the release into `temp` and read it back through the strict
    /// reader. Returns the digest, size and file count.
    fn write_verified(
        &self,
        package: &crate::InstalledPackage,
        temp: &Path,
    ) -> Result<(String, u64, usize), InstallError> {
        let limits = PackageLimits::default();
        let packed =
            export_package_tree(&package.path, temp, &limits).map_err(InstallError::Export)?;
        let candidate = inspect_archive_in(temp, &limits, &self.staging_root())
            .map_err(InstallError::Export)?;
        // `candidate` removes its staging copy when dropped.
        if candidate.manifest.id != package.id
            || candidate.manifest.version.to_string() != package.version
        {
            return Err(InstallError::StoreCorrupt {
                digest: package.digest.clone(),
                reason: "the extracted manifest does not match the installed record",
            });
        }
        if candidate.sha256 != packed.sha256 || candidate.files.len() != packed.entries {
            return Err(InstallError::StoreCorrupt {
                digest: package.digest.clone(),
                reason: "the exported archive did not read back identically",
            });
        }
        Ok((packed.sha256_hex(), packed.size, packed.entries))
    }
}
