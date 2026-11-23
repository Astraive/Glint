//! Local archive and local directory resolver (SRC-001).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use glint_package::{
    PackageCandidate, PackageLimits, inspect_archive_in, pack_directory_with_limits,
};

use crate::{
    PackageSource, SigningStatus, SourceError, SourceKind, SourceOrigin, StagedPackage, ThemeSource,
};

/// Where remote backends are planned, quoted in `Unsupported` errors.
const REMOTE_BACKENDS_PLANNED: &str = "M4 (SRC-003 Git, SRC-005 artifact URL, REG-001 registry)";

/// Name of the temporary archive a local directory is packed into.
const PACKED_NAME: &str = "package.glint";

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Resolver for [`ThemeSource::LocalArchive`] and
/// [`ThemeSource::LocalDirectory`].
///
/// - A **local archive** is handed to `glint-package`'s
///   [`inspect_archive_in`], which copies it into private staging while
///   hashing it; the caller's file is never modified.
/// - A **local directory** is packed with the deterministic
///   [`pack_directory_with_limits`] into a temporary archive inside the
///   staging root, which is then inspected the same way and removed. The
///   packer's exclusion rules apply (hidden files, `node_modules`, build-tool
///   files), links and executables are refused, and nothing is ever written
///   into the source directory. No build step, package script or other
///   program is run, in any mode.
///
/// Every other kind returns [`SourceError::Unsupported`].
#[derive(Debug, Clone, Default)]
pub struct LocalSource {
    limits: PackageLimits,
}

impl LocalSource {
    /// A resolver using the default [`PackageLimits`].
    pub fn new() -> Self {
        Self::default()
    }

    /// A resolver using explicit package limits.
    pub fn with_limits(limits: PackageLimits) -> Self {
        Self { limits }
    }

    /// The package limits this resolver applies.
    pub fn limits(&self) -> &PackageLimits {
        &self.limits
    }

    fn stage_archive(
        &self,
        path: &Path,
        staging_root: &Path,
    ) -> Result<StagedPackage, SourceError> {
        let meta = fs::metadata(path).map_err(|error| SourceError::from_lookup(path, error))?;
        if !meta.is_file() {
            return Err(SourceError::NotAFile(path.to_path_buf()));
        }
        let local_path = canonical(path)?;
        let candidate = inspect_archive_in(path, &self.limits, staging_root)?;
        Ok(staged(SourceKind::LocalArchive, local_path, candidate))
    }

    fn stage_directory(
        &self,
        path: &Path,
        staging_root: &Path,
    ) -> Result<StagedPackage, SourceError> {
        let meta = fs::metadata(path).map_err(|error| SourceError::from_lookup(path, error))?;
        if !meta.is_dir() {
            return Err(SourceError::NotADirectory(path.to_path_buf()));
        }
        if !is_real_dir(staging_root) {
            return Err(SourceError::StagingRootInvalid(staging_root.to_path_buf()));
        }
        let local_path = canonical(path)?;
        let scratch = Scratch::create(staging_root)?;
        let packed = scratch.path.join(PACKED_NAME);
        pack_directory_with_limits(path, &packed, &self.limits)?;
        let candidate = inspect_archive_in(&packed, &self.limits, staging_root)?;
        drop(scratch);
        Ok(staged(SourceKind::LocalDirectory, local_path, candidate))
    }
}

impl PackageSource for LocalSource {
    fn supports(&self, kind: SourceKind) -> bool {
        kind.is_local()
    }

    fn stage(
        &self,
        source: &ThemeSource,
        staging_root: &Path,
    ) -> Result<StagedPackage, SourceError> {
        match source {
            ThemeSource::LocalArchive { path } => self.stage_archive(path, staging_root),
            ThemeSource::LocalDirectory { path } => self.stage_directory(path, staging_root),
            other => Err(SourceError::Unsupported {
                kind: other.kind(),
                planned: REMOTE_BACKENDS_PLANNED,
            }),
        }
    }
}

fn staged(kind: SourceKind, local_path: PathBuf, candidate: PackageCandidate) -> StagedPackage {
    let origin = SourceOrigin {
        kind,
        locator: local_path.display().to_string(),
        local_path: Some(local_path),
        registry: None,
        git_commit: None,
        archive_sha256: candidate.sha256,
        package_id: candidate.manifest.id.clone(),
        package_version: candidate.manifest.version.to_string(),
        retrieved_at: SystemTime::now(),
        signing: SigningStatus::Unsigned,
    };
    StagedPackage { candidate, origin }
}

/// Absolute, link-resolved form of `path` for provenance, without the
/// Windows `\\?\` prefix when the path is an ordinary drive path.
fn canonical(path: &Path) -> Result<PathBuf, SourceError> {
    let resolved = fs::canonicalize(path).map_err(|error| SourceError::from_lookup(path, error))?;
    Ok(simplify_verbatim(resolved))
}

fn simplify_verbatim(path: PathBuf) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path;
    };
    match text.strip_prefix(r"\\?\") {
        Some(rest)
            if rest.len() >= 3
                && rest.as_bytes()[0].is_ascii_alphabetic()
                && &rest[1..3] == r":\" =>
        {
            PathBuf::from(rest)
        }
        _ => path,
    }
}

fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir() && !meta.file_type().is_symlink())
}

/// A private scratch directory under the staging root, removed on drop.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn create(root: &Path) -> Result<Self, SourceError> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        for _ in 0..16 {
            let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = root.join(format!(
                "src-pack-{}-{nanos:x}-{counter}",
                std::process::id()
            ));
            match create_private_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(source) => {
                    return Err(SourceError::Io {
                        context: "create scratch directory",
                        source,
                    });
                }
            }
        }
        Err(SourceError::Io {
            context: "create scratch directory",
            source: io::Error::from(io::ErrorKind::AlreadyExists),
        })
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn create_private_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(path)
    }
    #[cfg(not(unix))]
    {
        fs::create_dir(path)
    }
}

#[cfg(test)]
mod tests {
    use super::simplify_verbatim;
    use std::path::PathBuf;

    #[test]
    fn verbatim_drive_prefix_is_removed() {
        assert_eq!(
            simplify_verbatim(PathBuf::from(r"\\?\C:\themes\a.glint")),
            PathBuf::from(r"C:\themes\a.glint")
        );
        assert_eq!(
            simplify_verbatim(PathBuf::from(r"\\?\UNC\server\share")),
            PathBuf::from(r"\\?\UNC\server\share")
        );
        assert_eq!(
            simplify_verbatim(PathBuf::from("/home/a")),
            PathBuf::from("/home/a")
        );
    }
}
