//! Strict, bounded reader (PKG-001) and deterministic writer (PKG-003) for
//! `.glint` theme packages.
//!
//! A `.glint` file is a ZIP archive with a `glint.json` manifest at its root.
//! Packages are untrusted input: [`inspect_archive`] (or
//! [`inspect_archive_in`] with an explicit staging root) validates one and
//! extracts it into an immutable, Glint-owned staging directory, returning a
//! [`PackageCandidate`]. Nothing is activated or installed here; that is the
//! installer's job (INS). [`pack_directory`] writes a byte-for-byte
//! reproducible archive from a theme directory and returns its
//! [`PackageDigest`]; its output always passes the reader.
//! [`export_package_tree`] re-packs an extracted package tree the same way,
//! without the authoring exclusions, for exporting installed releases
//! (PKG-005).
//!
//! The design follows the SPK-007 spike
//! (`docs/research/spikes/package-security.md`): the archive is identified by
//! magic bytes, copied into private staging while hashed, its directory is
//! pre-read by hand, every entry is checked on its metadata before anything
//! is inflated, and inflation is bounded and re-checked. The `zip` crate is
//! used only to parse records and inflate data; its path helpers and
//! extraction functions are never used.
//!
//! # Errors
//!
//! Every failure is a [`PackageError`] with a stable [`ErrorCode`] whose
//! string form is `GLINT_PACKAGE_*`. Manifest problems carry the underlying
//! `GLINT_MANIFEST_*` diagnostic from `glint-schema`
//! ([`PackageError::manifest_error`]); surface problems carry the
//! `GLINT_SURFACE_*` diagnostic from `glint-surfaces`
//! ([`PackageError::surface_error`]).
//!
//! # Example
//!
//! ```no_run
//! use glint_package::{PackageLimits, inspect_archive};
//!
//! let candidate = inspect_archive("midnight.glint".as_ref(), &PackageLimits::default())?;
//! println!("{} {} sha256={}", candidate.manifest.id, candidate.manifest.version, candidate.sha256_hex());
//! # Ok::<(), glint_package::PackageError>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod error;
mod export;
mod limits;
mod path;
mod reader;
mod scan;
mod staging;
mod validate;
mod writer;

pub use error::{ErrorCode, PackageError, PathRejection};
pub use export::export_package_tree;
pub use limits::PackageLimits;
pub use path::{
    EXECUTABLE_EXTENSIONS, MANIFEST_NAME, is_executable_name, is_reserved_device_name,
    validate_entry_name,
};
pub use reader::{
    PackageCandidate, PackageFile, default_staging_root, inspect_archive, inspect_archive_in,
};
pub use validate::{is_known_capability, known_capabilities};
pub use writer::{
    EXCLUDED_DIR_NAMES, EXCLUDED_FILE_NAMES, EXCLUDED_FILE_PREFIXES, PackageDigest,
    is_excluded_name, pack_directory, pack_directory_with_limits,
};

/// Lowercase hexadecimal encoding of a digest.
pub(crate) fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn hex_encodes_lowercase() {
        assert_eq!(super::to_hex(&[0x00, 0xab, 0x0f]), "00ab0f");
    }
}
