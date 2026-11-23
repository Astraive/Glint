//! Manifest cross-checks against the package's file set (PKG-004).
//!
//! Shared by the reader and the packer, so packed output always passes the
//! reader. Checks run in a fixed order and stop at the first problem:
//!
//! 1. `glint.json` is UTF-8 and passes `glint-schema`
//!    (`GLINT_PACKAGE_MANIFEST_NOT_UTF8`, `GLINT_PACKAGE_INVALID_MANIFEST`).
//! 2. Every declared capability, required or optional, is in the frozen v1
//!    capability registry ([`is_known_capability`]); an unknown ID is
//!    refused, never ignored or granted
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

use glint_platform::registry;
use glint_schema::ThemeManifest;

use crate::error::PackageError;

/// Every capability ID the reader accepts: the frozen v1 capability
/// registry (`glint_platform::registry`, CAP-005). Anything else is refused
/// (fail closed).
pub fn known_capabilities() -> BTreeSet<&'static str> {
    registry::registered_ids().collect()
}

/// True when `id` is in the frozen v1 capability registry.
pub fn is_known_capability(id: &str) -> bool {
    registry::is_registered(id)
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
    use glint_surfaces::{CATALOG, CUSTOM_SURFACE_CAPABILITY};

    use super::*;

    #[test]
    fn known_capabilities_are_exactly_the_registry() {
        let known = known_capabilities();
        assert_eq!(known.len(), registry::REGISTRY.len());
        for spec in registry::REGISTRY {
            assert!(is_known_capability(spec.id), "{}", spec.id);
        }
    }

    #[test]
    fn every_catalog_capability_is_known() {
        assert!(is_known_capability(CUSTOM_SURFACE_CAPABILITY));
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
    }

    #[test]
    fn unregistered_ids_are_not_known() {
        for id in [
            "system.shell.exec",
            "system.files.write",
            "surface.taskbar",
            "surface.notification-overlay",
            "desktop.windows.close",
            "",
        ] {
            assert!(!is_known_capability(id), "{id}");
        }
    }
}
