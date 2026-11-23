//! Deterministic packer tests (PKG-003): pack twice gives identical bytes,
//! pack -> inspect -> pack gives the same digest, excluded paths are absent
//! and the output always passes the reader.
//!
//! Run with `cargo test -p glint-package --test roundtrip`.

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use glint_package::{
    ErrorCode as C, PackageDigest, PackageError, PackageLimits, inspect_archive_in, pack_directory,
    pack_directory_with_limits,
};
use sha2::{Digest, Sha256};
use support::{GOOD_HTML, GOOD_MANIFEST};

const THEME_FILES: &[(&str, &str)] = &[
    ("glint.json", GOOD_MANIFEST),
    ("theme/tokens.json", r##"{"color":"#000"}"##),
    ("dist/panel/index.html", GOOD_HTML),
    ("dist/panel/main.js", "import { x } from './dep.js'; x();"),
    ("dist/panel/dep.js", "export const x = () => 1;"),
    ("LICENSE", "MIT License (fixture)"),
];

struct Workspace {
    dir: tempfile::TempDir,
}

impl Workspace {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("out")).unwrap();
        fs::create_dir(dir.path().join("staging")).unwrap();
        Self { dir }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    /// Write a theme directory, files in the given order.
    fn theme(&self, name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = self.path(name);
        fs::create_dir(&root).unwrap();
        for (rel, content) in files {
            write(&root, rel, content.as_bytes());
        }
        root
    }

    fn staging(&self) -> PathBuf {
        self.path("staging")
    }

    fn out(&self, name: &str) -> PathBuf {
        self.path("out").join(name)
    }
}

fn write(root: &Path, rel: &str, content: &[u8]) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn pack_err(src: &Path, dest: &Path) -> PackageError {
    pack_directory(src, dest).expect_err("packing must fail")
}

#[test]
fn packing_twice_gives_byte_identical_output() {
    let ws = Workspace::new();
    let theme = ws.theme("theme", THEME_FILES);
    let first = pack_directory(&theme, &ws.out("a.glint")).unwrap();
    let second = pack_directory(&theme, &ws.out("b.glint")).unwrap();
    let a = fs::read(ws.out("a.glint")).unwrap();
    let b = fs::read(ws.out("b.glint")).unwrap();
    assert_eq!(a, b);
    assert_eq!(first, second);
    assert_eq!(first.sha256, <[u8; 32]>::from(Sha256::digest(&a)));
    assert_eq!(first.size, a.len() as u64);
    assert_eq!(first.entries, THEME_FILES.len());
    assert_eq!(first.sha256_hex().len(), 64);
}

#[test]
fn output_does_not_depend_on_creation_order_or_timestamps() {
    let ws = Workspace::new();
    let forward = ws.theme("forward", THEME_FILES);
    let reversed: Vec<_> = THEME_FILES.iter().rev().copied().collect();
    let backward = ws.theme("backward", &reversed);
    let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
    let file = fs::File::options()
        .write(true)
        .open(backward.join("LICENSE"))
        .unwrap();
    file.set_modified(old).unwrap();
    drop(file);
    let a = pack_directory(&forward, &ws.out("a.glint")).unwrap();
    let b = pack_directory(&backward, &ws.out("b.glint")).unwrap();
    assert_eq!(a.sha256, b.sha256);
}

#[test]
fn pack_inspect_pack_round_trip_keeps_the_digest() {
    let ws = Workspace::new();
    let theme = ws.theme("theme", THEME_FILES);
    let first = pack_directory(&theme, &ws.out("first.glint")).unwrap();

    let candidate = inspect_archive_in(
        &ws.out("first.glint"),
        &PackageLimits::default(),
        &ws.staging(),
    )
    .expect("packed output must pass the reader");
    assert_eq!(candidate.sha256, first.sha256);
    assert_eq!(candidate.manifest.id, "dev.example.fixture");
    let mut names: Vec<&str> = candidate.files.iter().map(|f| f.path.as_str()).collect();
    let mut expected: Vec<&str> = THEME_FILES.iter().map(|(rel, _)| *rel).collect();
    expected.sort_unstable();
    assert_eq!(names, expected, "entries are written in sorted order");
    names.sort_unstable();
    assert_eq!(names, expected);

    let second = pack_directory(&candidate.extracted_root, &ws.out("second.glint")).unwrap();
    assert_eq!(second.sha256, first.sha256);
    assert_eq!(
        fs::read(ws.out("first.glint")).unwrap(),
        fs::read(ws.out("second.glint")).unwrap()
    );
}

#[test]
fn entries_use_fixed_metadata() {
    let ws = Workspace::new();
    let theme = ws.theme("theme", THEME_FILES);
    pack_directory(&theme, &ws.out("t.glint")).unwrap();
    let bytes = fs::read(ws.out("t.glint")).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut previous = String::new();
    for index in 0..archive.len() {
        let entry = archive.by_index_raw(index).unwrap();
        let name = String::from_utf8(entry.name_raw().to_vec()).unwrap();
        assert!(name > previous, "sorted entries");
        previous = name;
        assert_eq!(entry.compression(), zip::CompressionMethod::Deflated);
        assert_eq!(entry.unix_mode(), Some(0o100_644));
        assert_eq!(entry.extra_data_fields().count(), 0);
        let time = entry.last_modified().unwrap();
        assert_eq!(
            (
                time.year(),
                time.month(),
                time.day(),
                time.hour(),
                time.minute()
            ),
            (1980, 1, 1, 0, 0)
        );
        assert!(!entry.is_dir());
    }
}

#[test]
fn excluded_paths_are_absent_and_reported() {
    let ws = Workspace::new();
    let clean = ws.theme("clean", THEME_FILES);
    let mut noisy_files = THEME_FILES.to_vec();
    noisy_files.extend([
        ("node_modules/react/index.js", "module.exports = {};"),
        (".git/config", "[core]"),
        (".env", "TOKEN=secret"),
        (".DS_Store", "junk"),
        ("Thumbs.db", "junk"),
        ("dist/panel/desktop.ini", "junk"),
        ("package.json", "{}"),
        ("pnpm-lock.yaml", "lockfileVersion: 9"),
        ("tsconfig.json", "{}"),
        ("vite.config.ts", "export default {};"),
        ("target/debug/build.txt", "x"),
    ]);
    let noisy = ws.theme("noisy", &noisy_files);
    let clean_digest = pack_directory(&clean, &ws.out("clean.glint")).unwrap();
    let noisy_digest = pack_directory(&noisy, &ws.out("noisy.glint")).unwrap();
    assert_eq!(noisy_digest.sha256, clean_digest.sha256);
    assert_eq!(
        noisy_digest.excluded,
        vec![
            ".DS_Store",
            ".env",
            ".git",
            "Thumbs.db",
            "dist/panel/desktop.ini",
            "node_modules",
            "package.json",
            "pnpm-lock.yaml",
            "target",
            "tsconfig.json",
            "vite.config.ts",
        ]
    );
    let candidate = inspect_archive_in(
        &ws.out("noisy.glint"),
        &PackageLimits::default(),
        &ws.staging(),
    )
    .unwrap();
    assert!(
        candidate
            .files
            .iter()
            .all(|f| !f.path.contains("node_modules") && !f.path.starts_with('.'))
    );
}

#[test]
fn missing_source_is_refused() {
    let ws = Workspace::new();
    let error = pack_err(&ws.path("nope"), &ws.out("x.glint"));
    assert_eq!(error.code(), C::PackSourceInvalid, "{error}");
}

#[test]
fn destination_inside_source_is_refused() {
    let ws = Workspace::new();
    let theme = ws.theme("theme", THEME_FILES);
    let error = pack_err(&theme, &theme.join("dist").join("self.glint"));
    assert_eq!(error.code(), C::PackDestinationInvalid, "{error}");
    assert!(!theme.join("dist").join("self.glint").exists());
}

#[test]
fn destination_without_parent_directory_is_refused() {
    let ws = Workspace::new();
    let theme = ws.theme("theme", THEME_FILES);
    let error = pack_err(&theme, &ws.path("missing-dir").join("x.glint"));
    assert_eq!(error.code(), C::PackDestinationInvalid, "{error}");
}

#[test]
fn failed_pack_leaves_destination_and_directory_untouched() {
    let ws = Workspace::new();
    let mut files = THEME_FILES.to_vec();
    files.push(("tools/install.ps1", "Remove-Item -Recurse C:\\"));
    let theme = ws.theme("theme", &files);
    fs::write(ws.out("existing.glint"), b"previous package").unwrap();
    let error = pack_err(&theme, &ws.out("existing.glint"));
    assert_eq!(error.code(), C::ExecutableContent, "{error}");
    assert_eq!(
        fs::read(ws.out("existing.glint")).unwrap(),
        b"previous package"
    );
    let leftovers: Vec<_> = fs::read_dir(ws.path("out"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(leftovers, vec![std::ffi::OsString::from("existing.glint")]);
}

#[test]
fn successful_pack_replaces_an_existing_destination() {
    let ws = Workspace::new();
    let theme = ws.theme("theme", THEME_FILES);
    fs::write(ws.out("t.glint"), b"stale").unwrap();
    let digest: PackageDigest = pack_directory(&theme, &ws.out("t.glint")).unwrap();
    assert_eq!(
        <[u8; 32]>::from(Sha256::digest(fs::read(ws.out("t.glint")).unwrap())),
        digest.sha256
    );
}

#[test]
fn missing_manifest_is_refused() {
    let ws = Workspace::new();
    let theme = ws.theme("theme", &THEME_FILES[1..]);
    assert_eq!(
        pack_err(&theme, &ws.out("x.glint")).code(),
        C::MissingManifest
    );
}

#[test]
fn invalid_manifest_is_refused_before_writing() {
    let ws = Workspace::new();
    let mut files = THEME_FILES.to_vec();
    files[0] = ("glint.json", r#"{ "schemaVersion": 1 }"#);
    let theme = ws.theme("theme", &files);
    let error = pack_err(&theme, &ws.out("x.glint"));
    assert_eq!(error.code(), C::InvalidManifest, "{error}");
    assert!(!ws.out("x.glint").exists());
}

#[test]
fn manifest_reference_to_an_excluded_file_is_refused() {
    let ws = Workspace::new();
    let manifest = GOOD_MANIFEST.replace("theme/tokens.json", "node_modules/tokens.json");
    let mut files = THEME_FILES.to_vec();
    files[0] = ("glint.json", &manifest);
    files.push(("node_modules/tokens.json", "{}"));
    let theme = ws.theme("theme", &files);
    let error = pack_err(&theme, &ws.out("x.glint"));
    assert_eq!(error.code(), C::MissingReferencedFile, "{error}");
}

#[test]
fn unknown_capability_is_refused() {
    let ws = Workspace::new();
    let manifest = GOOD_MANIFEST.replace("\"surface.panel\"", "\"system.shell.exec\"");
    let mut files = THEME_FILES.to_vec();
    files[0] = ("glint.json", &manifest);
    let theme = ws.theme("theme", &files);
    assert_eq!(
        pack_err(&theme, &ws.out("x.glint")).code(),
        C::UnknownCapability
    );
}

#[test]
fn executable_content_is_refused() {
    let ws = Workspace::new();
    let theme = ws.theme("theme", THEME_FILES);
    write(&theme, "assets/image.png", b"MZ\x90\0\x03");
    assert_eq!(
        pack_err(&theme, &ws.out("x.glint")).code(),
        C::ExecutableContent
    );
}

#[test]
fn remote_reference_is_refused() {
    let ws = Workspace::new();
    let mut files = THEME_FILES.to_vec();
    files[2] = (
        "dist/panel/index.html",
        r#"<script src="https://cdn.example.com/x.js"></script>"#,
    );
    let theme = ws.theme("theme", &files);
    assert_eq!(
        pack_err(&theme, &ws.out("x.glint")).code(),
        C::RemoteReference
    );
}

#[test]
fn unsafe_file_name_is_refused() {
    let ws = Workspace::new();
    let theme = ws.theme("theme", THEME_FILES);
    write(&theme, "assets/PROGRA~1.txt", b"x");
    let error = pack_err(&theme, &ws.out("x.glint"));
    assert_eq!(error.code(), C::UnsafePath, "{error}");
}

#[test]
fn compression_bomb_content_is_refused_so_output_always_passes_the_reader() {
    let ws = Workspace::new();
    let theme = ws.theme("theme", THEME_FILES);
    write(&theme, "assets/blank.bin", &vec![0u8; 8 * 1024 * 1024]);
    let error = pack_err(&theme, &ws.out("x.glint"));
    assert_eq!(error.code(), C::RatioExceeded, "{error}");
}

#[test]
fn custom_limits_apply_to_the_packer() {
    let ws = Workspace::new();
    let theme = ws.theme("theme", THEME_FILES);
    let limits = PackageLimits {
        max_entries: 2,
        ..PackageLimits::default()
    };
    let error = pack_directory_with_limits(&theme, &ws.out("x.glint"), &limits).unwrap_err();
    assert_eq!(error.code(), C::TooManyEntries, "{error}");
}

#[cfg(unix)]
#[test]
fn case_folded_collision_is_refused() {
    let ws = Workspace::new();
    let theme = ws.theme("theme", THEME_FILES);
    write(&theme, "assets/A.txt", b"a");
    write(&theme, "assets/a.txt", b"b");
    if fs::read_dir(theme.join("assets")).unwrap().count() < 2 {
        eprintln!("SKIPPED: case-insensitive file system; collision cannot be created");
        return;
    }
    assert_eq!(
        pack_err(&theme, &ws.out("x.glint")).code(),
        C::PathCollision
    );
}

#[cfg(unix)]
#[test]
fn symlink_to_outside_the_theme_is_refused() {
    let ws = Workspace::new();
    let theme = ws.theme("theme", THEME_FILES);
    fs::write(ws.path("secret.txt"), b"private").unwrap();
    std::os::unix::fs::symlink(ws.path("secret.txt"), theme.join("assets-link.txt")).unwrap();
    assert_eq!(pack_err(&theme, &ws.out("x.glint")).code(), C::Symlink);
}

#[cfg(windows)]
#[test]
fn junction_to_outside_the_theme_is_refused() {
    // Junctions need no privilege on Windows (symlinks need Developer Mode).
    let ws = Workspace::new();
    let theme = ws.theme("theme", THEME_FILES);
    let outside = ws.path("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("secret.txt"), b"private").unwrap();
    let link = theme.join("linked");
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&outside)
        .output()
        .unwrap();
    assert!(status.status.success(), "mklink /J failed: {status:?}");
    let error = pack_err(&theme, &ws.out("x.glint"));
    assert_eq!(error.code(), C::Symlink, "{error}");
    fs::remove_dir(&link).unwrap();
}

#[cfg(windows)]
#[test]
fn file_symlink_is_refused_when_the_host_allows_creating_one() {
    let ws = Workspace::new();
    let theme = ws.theme("theme", THEME_FILES);
    fs::write(ws.path("secret.txt"), b"private").unwrap();
    if let Err(error) =
        std::os::windows::fs::symlink_file(ws.path("secret.txt"), theme.join("link.txt"))
    {
        eprintln!("SKIPPED: cannot create a file symlink on this host ({error})");
        return;
    }
    assert_eq!(pack_err(&theme, &ws.out("x.glint")).code(), C::Symlink);
}
