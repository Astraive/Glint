//! The bundled default theme (THM-001) is a real package: its manifest
//! passes `glint-schema`, it packs deterministically and the packed archive
//! passes the same strict reader third-party themes go through.
//!
//! Run with `cargo test -p glint-package --test default_theme`.

use std::fs;
use std::path::{Path, PathBuf};

use glint_package::{PackageLimits, inspect_archive_in, pack_directory};
use glint_schema::{HostVersions, Platform, check_compatibility, parse_theme_manifest};
use semver::Version;

fn theme_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes/default")
}

/// Every file the package must contain, sorted.
const EXPECTED_FILES: &[&str] = &[
    "LICENSE",
    "README.md",
    "assets/preview.svg",
    "glint.json",
    "surfaces/panel/index.html",
    "surfaces/panel/panel.css",
    "theme/tokens.json",
];

#[test]
fn manifest_is_valid_and_needs_no_capability() {
    let text = fs::read_to_string(theme_dir().join("glint.json")).unwrap();
    let manifest = parse_theme_manifest(&text).expect("default manifest parses");
    assert_eq!(manifest.id, "official.default");
    assert_eq!(manifest.license, "MIT");
    assert!(
        manifest.required_capabilities.is_empty(),
        "fallback must always activate"
    );
    for platform in [Platform::Windows, Platform::Linux, Platform::Macos] {
        assert!(manifest.compatibility.platforms.contains(&platform));
    }
    // Compatible with every Glint build, including this pre-release one.
    for glint in ["0.0.0", env!("CARGO_PKG_VERSION"), "1.0.0"] {
        let host = HostVersions::new(Version::parse(glint).unwrap(), Version::new(0, 0, 0));
        check_compatibility(&manifest, &host).unwrap_or_else(|report| {
            panic!("default theme incompatible with Glint {glint}: {report:?}")
        });
    }
}

#[test]
fn packs_deterministically_and_passes_the_reader() {
    let work = tempfile::tempdir().unwrap();
    let staging = work.path().join("staging");
    fs::create_dir(&staging).unwrap();
    let first = work.path().join("first.glint");
    let second = work.path().join("second.glint");

    let digest = pack_directory(&theme_dir(), &first).expect("default theme packs");
    let again = pack_directory(&theme_dir(), &second).unwrap();
    assert_eq!(digest, again);
    assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
    assert!(
        digest.excluded.is_empty(),
        "unexpected excluded files: {:?}",
        digest.excluded
    );
    assert_eq!(digest.entries, EXPECTED_FILES.len());

    let candidate =
        inspect_archive_in(&first, &PackageLimits::default(), &staging).expect("reader accepts it");
    assert_eq!(candidate.sha256, digest.sha256);
    assert_eq!(candidate.manifest.id, "official.default");
    let mut files: Vec<&str> = candidate.files.iter().map(|f| f.path.as_str()).collect();
    files.sort_unstable();
    assert_eq!(files, EXPECTED_FILES);
}

/// Brand rule: neutral graphite, silver and white, never green. Every hex
/// colour in the theme's styling files must be a neutral grey.
#[test]
fn palette_is_neutral() {
    for file in [
        "theme/tokens.json",
        "surfaces/panel/panel.css",
        "assets/preview.svg",
    ] {
        let text = fs::read_to_string(theme_dir().join(file)).unwrap();
        let mut colours = 0;
        for (index, _) in text.match_indices('#') {
            let hex = &text[index + 1..];
            if hex.len() < 6 || !hex[..6].chars().all(|c| c.is_ascii_hexdigit()) {
                continue;
            }
            let channel = |i: usize| i32::from(u8::from_str_radix(&hex[i..i + 2], 16).unwrap());
            let (r, g, b) = (channel(0), channel(2), channel(4));
            let spread = r.max(g).max(b) - r.min(g).min(b);
            assert!(spread <= 16, "{file}: #{} is not a neutral grey", &hex[..6]);
            colours += 1;
        }
        assert!(colours > 0, "{file} declares no colours");
    }
}
