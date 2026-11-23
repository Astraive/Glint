//! Canonical re-packing of an extracted package tree (PKG-005).
//!
//! [`export_package_tree`] writes an extracted, already validated package
//! tree (the installer's read-only `root/`) back into a `.glint` archive with
//! the same deterministic writer as [`crate::pack_directory`], with one
//! difference: nothing is skipped. Every regular file of the tree is a file
//! of the package (the reader accepted it), so the authoring exclusions
//! (hidden files, `package.json`, ...) would silently drop package content.
//!
//! The archive bytes depend only on the file names and contents: no
//! timestamps, owners, permissions, extra fields, comments or paths of the
//! machine that exported it. A package that was itself packed by Glint's
//! writer therefore re-packs to the identical bytes and digest; an archive
//! from another ZIP tool re-packs to its canonical form (same files, new
//! digest).

use std::path::Path;

use crate::error::PackageError;
use crate::limits::PackageLimits;
use crate::writer::{Exclusions, PackageDigest, pack_tree};

/// Pack every regular file under `root` into `dest`, deterministically and
/// without the authoring exclusions of [`crate::pack_directory`].
///
/// Links, junctions and special files are refused, and the same reader rules
/// as [`crate::pack_directory_with_limits`] apply, so the output always
/// passes [`crate::inspect_archive`] under `limits`. `dest` is written
/// through a temporary file beside it and renamed into place; it must not be
/// inside `root`.
///
/// # Errors
///
/// A [`PackageError`] with a stable `GLINT_PACKAGE_*` code. On error `dest`
/// is left untouched and no temporary file remains.
pub fn export_package_tree(
    root: &Path,
    dest: &Path,
    limits: &PackageLimits,
) -> Result<PackageDigest, PackageError> {
    pack_tree(root, dest, limits, Exclusions::None)
}
