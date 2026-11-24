//! The bundled default theme, embedded in the binary (CLI-002).
//!
//! `build.rs` packs `themes/default` with the deterministic `glint-package`
//! writer at build time and this module embeds the archive. The installer
//! (`glint_installer::BundledTheme`) installs or repairs it from these bytes,
//! so `glint theme use default` and `glint safe-mode` work with an empty,
//! damaged or recovered store and without the repository.

use glint_installer::BundledTheme;

/// The embedded default theme archive.
pub const DEFAULT_THEME: BundledTheme = BundledTheme::new(include_bytes!(concat!(
    env!("OUT_DIR"),
    "/default-theme.glint"
)));

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::DEFAULT_THEME;

    /// The embedded bytes are exactly what `pack_directory` writes for the
    /// committed theme, so the build cannot drift from `themes/default`.
    #[test]
    fn embedded_archive_matches_a_fresh_pack() {
        let theme = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes/default");
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("default.glint");
        let digest = glint_package::pack_directory(&theme, &out).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), DEFAULT_THEME.archive());
        assert_eq!(digest.sha256_hex(), DEFAULT_THEME.digest());
    }
}
