//! Manifest cross-checks against the package's file set (PKG-004).
//!
//! Shared by the reader and the packer, so packed output always passes the
//! reader. Checks run in a fixed order and stop at the first problem:
//!
//! 1. `glint.json` is UTF-8 and passes `glint-schema`
//!    (`GLINT_PACKAGE_MANIFEST_NOT_UTF8`, `GLINT_PACKAGE_INVALID_MANIFEST`).
//! 2. Every declared capability is known ([`is_known_capability`]); an
//!    unknown ID is refused, never ignored or granted
//!    (`GLINT_PACKAGE_UNKNOWN_CAPABILITY`).
//! 3. Every surface passes `glint_surfaces::surface_definitions`: standard or
//!    namespaced custom kind, placement and behaviour allowed for that kind,
//!    package-local `.html` entrypoint (`GLINT_PACKAGE_INVALID_SURFACE`, with
//!    the `GLINT_SURFACE_*` code and JSON Pointer).
//! 4. Every token, asset and layout path, then every surface HTML
//!    entrypoint, is a regular file in the archive
//!    (`GLINT_PACKAGE_MISSING_REFERENCED_FILE`): a theme missing a declared
//!    surface entrypoint is refused.
//!
//! Matching is exact (case-sensitive) on the normalized `/`-separated path,
//! so a reference that differs only in case is missing.

use std::collections::BTreeSet;

use glint_schema::ThemeManifest;
use glint_surfaces::{CATALOG, CUSTOM_SURFACE_CAPABILITY};

use crate::error::PackageError;

/// Provider capability IDs the reader accepts in addition to the ones the
/// surface catalog implies.
///
/// The accepted set is [`known_capabilities`]: this list plus every
/// `surface.*` capability and every optional provider capability in
/// `glint_surfaces::CATALOG` (and `surface.custom`), so a manifest that
/// declares catalog capabilities is never refused. Everything else is refused
/// (fail closed).
///
/// This is provisional. The frozen v1 capability registry is owned by the CAP
/// epic (CAP-005, not yet built); when it exists it replaces this list and the
/// catalog derivation.
pub const PROVIDER_CAPABILITIES: &[&str] = &[
    "system.audio.read",
    "system.audio.write",
    "desktop.workspaces.read",
    "desktop.workspaces.switch",
];

/// Every capability ID the reader accepts: [`PROVIDER_CAPABILITIES`] plus the
/// surface catalog's required and optional capabilities and
/// `surface.custom`.
pub fn known_capabilities() -> BTreeSet<&'static str> {
    let mut known: BTreeSet<&'static str> = PROVIDER_CAPABILITIES.iter().copied().collect();
    known.insert(CUSTOM_SURFACE_CAPABILITY);
    for entry in CATALOG {
        known.insert(entry.capability);
        known.extend(entry.optional_capabilities.iter().copied());
    }
    known
}

/// True when `id` is in [`known_capabilities`].
pub fn is_known_capability(id: &str) -> bool {
    PROVIDER_CAPABILITIES.contains(&id)
        || id == CUSTOM_SURFACE_CAPABILITY
        || CATALOG
            .iter()
            .any(|entry| entry.capability == id || entry.optional_capabilities.contains(&id))
}

/// Parse `glint.json` bytes and cross-check them against the package's
/// regular files (see the module docs for the order).
pub(crate) fn check_manifest(
    bytes: Vec<u8>,
    files: &BTreeSet<&str>,
) -> Result<ThemeManifest, PackageError> {
    let text = String::from_utf8(bytes).map_err(|_| PackageError::ManifestNotUtf8)?;
    let manifest =
        glint_schema::parse_theme_manifest(&text).map_err(PackageError::InvalidManifest)?;

    for id in manifest
        .required_capabilities
        .iter()
        .chain(&manifest.optional_capabilities)
    {
        if !is_known_capability(id) {
            return Err(PackageError::UnknownCapability(id.clone()));
        }
    }

    let surfaces =
        glint_surfaces::surface_definitions(&manifest).map_err(PackageError::InvalidSurface)?;

    let entrypoints = surfaces.iter().map(|s| s.entrypoint.as_str());
    for reference in file_references(&manifest).chain(entrypoints) {
        if !files.contains(reference) {
            return Err(PackageError::MissingReferencedFile(reference.to_owned()));
        }
    }
    Ok(manifest)
}

/// Token, asset and layout paths (surface entrypoints come from the
/// validated surface definitions).
fn file_references(manifest: &ThemeManifest) -> impl Iterator<Item = &str> {
    manifest
        .tokens
        .iter()
        .map(String::as_str)
        .chain(manifest.assets.values().map(String::as_str))
        .chain(manifest.layouts.values().map(String::as_str))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalog_capability_is_known() {
        let known = known_capabilities();
        assert!(known.contains(CUSTOM_SURFACE_CAPABILITY));
        for entry in CATALOG {
            assert!(
                is_known_capability(entry.capability),
                "{}",
                entry.capability
            );
            for id in entry.optional_capabilities {
                assert!(is_known_capability(id), "{id}");
            }
        }
        for id in &known {
            assert!(is_known_capability(id), "{id}");
        }
    }

    #[test]
    fn provider_capabilities_are_known_and_others_are_not() {
        for id in PROVIDER_CAPABILITIES {
            assert!(is_known_capability(id), "{id}");
        }
        for id in [
            "system.shell.exec",
            "system.files.write",
            "surface.taskbar",
            "",
        ] {
            assert!(!is_known_capability(id), "{id}");
        }
    }
}
