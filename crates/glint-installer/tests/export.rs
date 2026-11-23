//! Export installed packages without private data (PKG-005).
//!
//! An installed release is exported back to a `.glint`, re-imported offline
//! and compared: a Glint-packed release exports byte-identically (same
//! digest, re-import is a no-op), a release from another ZIP writer exports
//! to a canonical form that stays stable, and nothing Glint recorded locally
//! (paths, user name, provenance, timestamps) appears in the archive.
//!
//! Run with `cargo test -p glint-installer --test export`.

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use glint_installer::{ErrorCode, InstalledPackage, Installer};
use glint_package::{PackageLimits, inspect_archive_in};
use support::Home;

const ID: &str = "dev.example.export";
/// A distinctive name for Glint's root, standing in for a private user path.
const PRIVATE_ROOT: &str = "private-home-of-jdoe-7f3a";

fn install(home: &Home, installer: &mut Installer, archive: PathBuf) -> InstalledPackage {
    let staged = home.stage_path(installer, archive);
    installer.install(staged.candidate, staged.origin).unwrap()
}

/// Every needle that must never appear in an exported archive.
fn private_needles(home: &Home, installed: &InstalledPackage) -> Vec<String> {
    let mut needles = vec![
        PRIVATE_ROOT.to_owned(),
        home.tmp.path().to_string_lossy().into_owned(),
        home.paths.data_dir().to_string_lossy().into_owned(),
        home.sources.to_string_lossy().into_owned(),
        "install.record".to_owned(),
        "source.glint".to_owned(),
        "local-archive".to_owned(),
        "retrieved_at".to_owned(),
    ];
    if let Some(local) = &installed.local_path {
        needles.push(local.clone());
    }
    for var in ["USERNAME", "USER"] {
        if let Ok(user) = std::env::var(var)
            && user.len() >= 3
        {
            needles.push(user);
        }
    }
    needles
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    let needle = needle.as_bytes();
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// Assert that neither the raw archive nor any extracted name or content
/// contains a needle. Returns the sorted entry names.
fn assert_no_private_data(archive: &Path, staging: &Path, needles: &[String]) -> Vec<String> {
    let raw = fs::read(archive).unwrap();
    let candidate = inspect_archive_in(archive, &PackageLimits::default(), staging).unwrap();
    let mut names = Vec::new();
    for file in &candidate.files {
        let content = fs::read(candidate.staging_dir().join("root").join(&file.path)).unwrap();
        for needle in needles {
            assert!(!contains(file.path.as_bytes(), needle), "{needle} in name");
            assert!(!contains(&content, needle), "{needle} in {}", file.path);
        }
        names.push(file.path.clone());
    }
    for needle in needles {
        assert!(!contains(&raw, needle), "{needle} in archive bytes");
    }
    names.sort();
    names
}

/// Rewrite every entry's MS-DOS timestamp (local and central headers) to
/// 2024-05-01 13:37, as another ZIP tool would write it. The archive stays
/// valid but is no longer Glint's canonical form.
fn stamp_timestamps(archive: &Path) {
    let mut bytes = fs::read(archive).unwrap();
    let eocd = bytes.len() - 22;
    assert_eq!(&bytes[eocd..eocd + 4], b"PK\x05\x06");
    let u16_at = |b: &[u8], at: usize| usize::from(u16::from_le_bytes([b[at], b[at + 1]]));
    let u32_at =
        |b: &[u8], at: usize| u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]) as usize;
    let count = u16_at(&bytes, eocd + 10);
    let mut at = u32_at(&bytes, eocd + 16);
    let time = (13u16 << 11 | 37 << 5).to_le_bytes();
    let date = ((2024u16 - 1980) << 9 | 5 << 5 | 1).to_le_bytes();
    for _ in 0..count {
        assert_eq!(&bytes[at..at + 4], b"PK\x01\x02");
        bytes[at + 12..at + 14].copy_from_slice(&time);
        bytes[at + 14..at + 16].copy_from_slice(&date);
        let local = u32_at(&bytes, at + 42);
        assert_eq!(&bytes[local..local + 4], b"PK\x03\x04");
        bytes[local + 10..local + 12].copy_from_slice(&time);
        bytes[local + 12..local + 14].copy_from_slice(&date);
        at += 46 + u16_at(&bytes, at + 28) + u16_at(&bytes, at + 30) + u16_at(&bytes, at + 32);
    }
    fs::write(archive, bytes).unwrap();
}

#[test]
fn glint_packed_release_exports_byte_identically_and_reimports_idempotently() {
    let home = Home::with_root_name(PRIVATE_ROOT);
    let mut installer = home.open();
    let source = home.archive(ID, "1.0.0");
    let installed = install(&home, &mut installer, source.clone());

    let out = home.tmp.path().join("out").join("exported.glint");
    fs::create_dir_all(out.parent().unwrap()).unwrap();
    let exported = installer.export(&installed.digest, &out).unwrap();
    assert!(exported.is_canonical(), "{exported:?}");
    assert_eq!(exported.digest, installed.digest);
    assert_eq!(
        (exported.id.as_str(), exported.version.as_str()),
        (ID, "1.0.0")
    );
    assert_eq!(exported.files, 4);
    assert_eq!(fs::read(&out).unwrap(), fs::read(&source).unwrap());
    assert_eq!(exported.size, fs::metadata(&out).unwrap().len());

    // Re-import into the same store: the same release, nothing added.
    let again = install(&home, &mut installer, out.clone());
    assert_eq!(again.digest, installed.digest);
    assert_eq!(installer.installed().unwrap().len(), 1);
    assert_eq!(
        home.store_entries(),
        std::slice::from_ref(&installed.digest)
    );

    // Re-import offline into a fresh home: the same digest.
    let other = Home::new();
    let mut fresh = other.open();
    let reimported = install(&other, &mut fresh, out.clone());
    assert_eq!(reimported.digest, installed.digest);
    assert_eq!(reimported.id, ID);

    // Exporting twice writes the same bytes.
    let second = home.tmp.path().join("out").join("second.glint");
    installer.export(&installed.digest, &second).unwrap();
    assert_eq!(fs::read(&second).unwrap(), fs::read(&out).unwrap());
}

#[test]
fn export_contains_no_private_data() {
    let home = Home::with_root_name(PRIVATE_ROOT);
    let mut installer = home.open();
    let installed = install(&home, &mut installer, home.archive(ID, "1.0.0"));
    // Local state the export must not capture: an active profile, a pin
    // and a grant row bound to the release.
    installer.activate("default", &installed.digest).unwrap();
    installer.pin("work", &installed.digest).unwrap();
    installer
        .storage()
        .connection()
        .execute(
            "INSERT INTO grants (digest, capability, granted_at) VALUES (?1, 'surface.panel', 1)",
            [&installed.digest],
        )
        .unwrap();

    let out = home.tmp.path().join("exported.glint");
    installer.export(&installed.digest, &out).unwrap();
    let staging = home.tmp.path().join("check-staging");
    fs::create_dir(&staging).unwrap();
    let mut needles = private_needles(&home, &installed);
    needles.extend(["granted_at".to_owned(), "\"work\"".to_owned()]);
    let names = assert_no_private_data(&out, &staging, &needles);
    assert_eq!(
        names,
        [
            "dist/panel/index.html",
            "dist/panel/main.js",
            "glint.json",
            "theme/tokens.json"
        ]
    );
    // The active release is still active and installed: export is read-only.
    assert_eq!(
        installer.active_theme().unwrap().unwrap().package_digest,
        installed.digest
    );
}

#[test]
fn foreign_archive_exports_to_a_stable_canonical_form() {
    let home = Home::with_root_name(PRIVATE_ROOT);
    let mut installer = home.open();
    let canonical = home.archive(ID, "2.0.0");
    let foreign = home.sources.join("foreign.glint");
    fs::copy(&canonical, &foreign).unwrap();
    stamp_timestamps(&foreign);
    let installed = install(&home, &mut installer, foreign.clone());
    assert_ne!(fs::read(&foreign).unwrap(), fs::read(&canonical).unwrap());

    let out = home.tmp.path().join("canonical.glint");
    let exported = installer.export(&installed.digest, &out).unwrap();
    assert!(!exported.is_canonical());
    // The timestamps are gone: the export equals Glint's own pack.
    assert_eq!(fs::read(&out).unwrap(), fs::read(&canonical).unwrap());

    // Same origin and version with other bytes is a digest conflict, never
    // a silent merge; the installed release is untouched.
    let staged = home.stage_path(&installer, out.clone());
    let error = installer
        .install(staged.candidate, staged.origin)
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::IdentityConflict, "{error}");
    assert_eq!(installer.installed().unwrap(), [installed]);

    // Elsewhere it imports with the export's digest, and exporting that
    // release again is byte-identical: the canonical form is a fixed point.
    let other = Home::new();
    let mut fresh = other.open();
    let reimported = install(&other, &mut fresh, out.clone());
    assert_eq!(reimported.digest, exported.digest);
    let again = other.tmp.path().join("again.glint");
    let re_exported = fresh.export(&reimported.digest, &again).unwrap();
    assert!(re_exported.is_canonical());
    assert_eq!(fs::read(&again).unwrap(), fs::read(&out).unwrap());
}

#[test]
fn export_refuses_unknown_and_damaged_releases_without_writing() {
    let home = Home::new();
    let mut installer = home.open();
    let out = home.tmp.path().join("out.glint");

    let missing = "0".repeat(64);
    let error = installer.export(&missing, &out).unwrap_err();
    assert_eq!(error.code(), ErrorCode::NotInstalled);

    let installed = install(&home, &mut installer, home.archive(ID, "1.0.0"));
    let error = installer
        .export(&installed.digest, home.tmp.path())
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::ExportDestinationInvalid);

    // Tamper with the stored archive: the export refuses to vouch for it.
    let mut permissions = fs::metadata(&installed.archive_path).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    fs::set_permissions(&installed.archive_path, permissions).unwrap();
    fs::write(&installed.archive_path, b"tampered").unwrap();
    let error = installer.export(&installed.digest, &out).unwrap_err();
    assert_eq!(error.code(), ErrorCode::StoreCorrupt, "{error}");

    assert!(!out.exists());
    let leftovers: Vec<_> = fs::read_dir(home.tmp.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".export-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    assert!(home.staging_entries().is_empty());
}
