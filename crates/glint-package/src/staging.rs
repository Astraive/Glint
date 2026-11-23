//! Glint-owned private staging directories.
//!
//! Every import gets a fresh, uniquely named directory under a caller-chosen
//! staging root. It holds the private copy of the archive and the extracted
//! tree, and is removed on drop unless the caller keeps it.
//!
//! # Creation discipline (no `cap-std`)
//!
//! Files and directories are created **component by component** from names
//! that already passed [`crate::path::validate_entry_name`]:
//!
//! - directories with `create_dir` (never `create_dir_all` on archive input),
//!   and each one is re-checked with `symlink_metadata` to be a real
//!   directory, not a symlink, junction or other reparse point;
//! - files with `create_new`, so an existing file is never opened or
//!   truncated;
//! - the per-import directory itself is created with `create_dir` under a
//!   unique name, so it can never be a pre-existing directory, and with mode
//!   `0700` on Unix.
//!
//! Handle-relative creation (`openat`, `NtCreateFile` with a root handle, or
//! the `cap-std` crate) would also close the remaining race in which a local
//! process running as the same user swaps a just-created directory for a
//! link between the check and the next `create`. That attacker can already
//! write to Glint's own data directory, so the residual risk is accepted for
//! the foundation stage; `cap-std` was not adopted to avoid adding its
//! dependency tree to the security-critical crate before an audit. Long paths
//! need no special handling: Rust's standard library adds the `\\?\` prefix
//! on Windows when a path exceeds `MAX_PATH`.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::PackageError;

/// Name of the private archive copy inside a staging directory.
pub(crate) const SOURCE_COPY_NAME: &str = "source.glint";
/// Name of the extracted tree inside a staging directory.
pub(crate) const EXTRACTED_DIR_NAME: &str = "root";

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A private, uniquely named staging directory removed on drop unless kept.
#[derive(Debug)]
pub(crate) struct StagingDir {
    path: PathBuf,
    armed: bool,
}

impl StagingDir {
    /// Create a new unique directory under `root`, which must already exist
    /// and be a real directory.
    pub(crate) fn create(root: &Path) -> Result<Self, PackageError> {
        if !is_real_dir(root) {
            return Err(PackageError::StagingRootInvalid(root.to_path_buf()));
        }
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        for _ in 0..16 {
            let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
            let name = format!("pkg-{}-{nanos:x}-{counter}", std::process::id());
            let path = root.join(name);
            match create_private_dir(&path) {
                Ok(()) => return Ok(Self { path, armed: true }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(source) => {
                    return Err(PackageError::Io {
                        context: "create staging directory",
                        source,
                    });
                }
            }
        }
        Err(PackageError::StagingConflict(root.to_path_buf()))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Stop removing the directory on drop. The caller now owns it.
    pub(crate) fn keep(&mut self) {
        self.armed = false;
    }
}

impl Drop for StagingDir {
    fn drop(&mut self) {
        if self.armed {
            remove_tree(&self.path);
        }
    }
}

/// Create a fresh directory (never reusing an existing one).
pub(crate) fn create_private_dir(path: &Path) -> io::Result<()> {
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

/// True when `path` is a directory and not a symlink, junction or other
/// reparse point.
pub(crate) fn is_real_dir(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(meta) => meta.is_dir() && !meta.file_type().is_symlink() && !is_reparse_point(&meta),
        Err(_) => false,
    }
}

#[cfg(windows)]
pub(crate) fn is_reparse_point(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
pub(crate) fn is_reparse_point(_meta: &fs::Metadata) -> bool {
    false
}

/// Ensure every directory in the validated relative path `dir` exists under
/// `base`, creating one component at a time.
pub(crate) fn ensure_dirs(base: &Path, dir: &str) -> Result<PathBuf, PackageError> {
    let mut current = base.to_path_buf();
    if dir.is_empty() {
        return Ok(current);
    }
    for component in dir.split('/') {
        current.push(component);
        match fs::create_dir(&current) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                return Err(PackageError::Io {
                    context: "create staged directory",
                    source,
                });
            }
        }
        if !is_real_dir(&current) {
            return Err(PackageError::StagingConflict(current));
        }
    }
    Ok(current)
}

/// Create a new file at the validated relative path `relative` under `base`.
pub(crate) fn create_file(base: &Path, relative: &str) -> Result<File, PackageError> {
    let (parent, name) = match relative.rsplit_once('/') {
        Some((parent, name)) => (parent, name),
        None => ("", relative),
    };
    let target = ensure_dirs(base, parent)?.join(name);
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|source| {
            if source.kind() == io::ErrorKind::AlreadyExists {
                PackageError::StagingConflict(target)
            } else {
                PackageError::Io {
                    context: "create staged file",
                    source,
                }
            }
        })
}

/// Mark a tree read-only: files on every host, directories on Unix too.
/// Links are never followed.
pub(crate) fn make_read_only(path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() || is_reparse_point(&meta) {
        return Err(io::Error::other("link inside staging"));
    }
    if meta.is_dir() {
        for entry in fs::read_dir(path)? {
            make_read_only(&entry?.path())?;
        }
    }
    set_read_only(path, &meta, true)
}

#[cfg_attr(windows, allow(clippy::permissions_set_readonly_false))]
fn set_read_only(path: &Path, meta: &fs::Metadata, read_only: bool) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = match (meta.is_dir(), read_only) {
            (true, true) => 0o555,
            (true, false) => 0o700,
            (false, true) => 0o444,
            (false, false) => 0o600,
        };
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        // Windows: only the file read-only attribute exists; clearing it is
        // not "world writable" (ACLs are unchanged), so the lint is moot.
        if meta.is_dir() {
            return Ok(());
        }
        let mut permissions = meta.permissions();
        permissions.set_readonly(read_only);
        fs::set_permissions(path, permissions)
    }
}

/// Best-effort removal of a staging tree, restoring write permission first.
/// Links are removed, never followed.
pub(crate) fn remove_tree(path: &Path) {
    fn make_writable(path: &Path) {
        let Ok(meta) = fs::symlink_metadata(path) else {
            return;
        };
        if meta.file_type().is_symlink() || is_reparse_point(&meta) {
            return;
        }
        let _ = set_read_only(path, &meta, false);
        if meta.is_dir()
            && let Ok(entries) = fs::read_dir(path)
        {
            for entry in entries.flatten() {
                make_writable(&entry.path());
            }
        }
    }
    make_writable(path);
    let _ = fs::remove_dir_all(path);
}
