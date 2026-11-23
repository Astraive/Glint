//! Strict, bounded reader for `.glint` theme packages (PKG-001).
//!
//! A `.glint` file is a ZIP archive with a `glint.json` manifest at its root.
//! Packages are untrusted input: [`inspect_archive`] (or
//! [`inspect_archive_in`] with an explicit staging root) validates one and
//! extracts it into an immutable, Glint-owned staging directory, returning a
//! [`PackageCandidate`]. Nothing is activated or installed here; that is the
//! installer's job (INS).
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
//! ([`PackageError::manifest_error`]).
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
mod limits;
mod path;
mod reader;
mod scan;
mod staging;

pub use error::{ErrorCode, PackageError, PathRejection};
pub use limits::PackageLimits;
pub use path::{
    EXECUTABLE_EXTENSIONS, MANIFEST_NAME, is_executable_name, is_reserved_device_name,
    validate_entry_name,
};
pub use reader::{
    KNOWN_CAPABILITIES, PackageCandidate, PackageFile, default_staging_root, inspect_archive,
    inspect_archive_in,
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
