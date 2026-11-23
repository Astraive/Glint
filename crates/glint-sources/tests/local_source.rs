//! Local source staging (SRC-001): a local `.glint` archive and a local theme
//! directory both resolve to a `StagedPackage` through the same
//! `glint-package` reader; a missing path is `GLINT_SOURCE_NOT_FOUND`; remote
//! kinds report "unsupported" instead of a fake success.
//!
//! Every fixture is generated in a temporary directory at test time.
//!
//! Run with `cargo test -p glint-sources --test local_source`.

use std::fs;
use std::path::{Path, PathBuf};

use glint_package::{ErrorCode as PackageCode, pack_directory};
use glint_sources::{
    LocalSource, PackageSource, SigningStatus, SourceErrorCode as C, SourceKind, ThemeSource,
};

const MANIFEST: &str = r#"{
  "schemaVersion": 1,
  "kind": "theme",
  "id": "dev.example.fixture",
  "name": "Fixture",
  "version": "1.2.3",
  "author": { "name": "Fixture Author" },
  "license": "MIT",
  "compatibility": { "glint": ">=0.1.0", "platforms": ["windows", "linux", "macos"] },
  "renderer": { "kind": "glint-web", "entrypointVersion": 1 },
  "requiredCapabilities": ["surface.panel"],
  "tokens": "theme/tokens.json",
  "surfaces": { "panel": { "entry": "dist/panel/index.html" } }
}"#;

const HTML: &str = r#"<!doctype html><html><head><script type="module" src="./main.js"></script></head><body></body></html>"#;

const THEME_FILES: &[(&str, &str)] = &[
    ("glint.json", MANIFEST),
    ("theme/tokens.json", r##"{"color":"#000"}"##),
    ("dist/panel/index.html", HTML),
    ("dist/panel/main.js", "export const x = 1;"),
];

struct Workspace {
    dir: tempfile::TempDir,
}

impl Workspace {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("staging")).unwrap();
        fs::create_dir(dir.path().join("out")).unwrap();
        Self { dir }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    fn staging(&self) -> PathBuf {
        self.path("staging")
    }

    fn theme(&self, name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = self.path(name);
        fs::create_dir(&root).unwrap();
        for (rel, content) in files {
            let path = root.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        root
    }

    fn archive(&self, name: &str) -> (PathBuf, [u8; 32]) {
        let theme = self.theme(&format!("{name}-src"), THEME_FILES);
        let dest = self.path("out").join(format!("{name}.glint"));
        let digest = pack_directory(&theme, &dest).unwrap();
        (dest, digest.sha256)
    }
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn local_archive_resolves_to_a_staged_package() {
    let ws = Workspace::new();
    let (archive, sha256) = ws.archive("midnight");
    let source = ThemeSource::LocalArchive {
        path: archive.clone(),
    };

    let staged = LocalSource::new().stage(&source, &ws.staging()).unwrap();

    assert_eq!(staged.candidate.manifest.id, "dev.example.fixture");
    assert_eq!(staged.candidate.sha256, sha256);
    assert!(staged.candidate.extracted_root.join("glint.json").is_file());
    assert!(staged.candidate.staging_dir().starts_with(ws.staging()));

    let origin = &staged.origin;
    assert_eq!(origin.kind, SourceKind::LocalArchive);
    assert_eq!(origin.package_id, "dev.example.fixture");
    assert_eq!(origin.package_version, "1.2.3");
    assert_eq!(origin.archive_sha256, sha256);
    assert_eq!(origin.archive_sha256_hex().len(), 64);
    assert_eq!(origin.signing, SigningStatus::Unsigned);
    assert_eq!(origin.registry, None);
    assert_eq!(origin.git_commit, None);
    let recorded = origin.local_path.as_ref().unwrap();
    assert!(recorded.is_absolute());
    assert_eq!(
        fs::canonicalize(recorded).unwrap(),
        fs::canonicalize(&archive).unwrap()
    );
}

#[test]
fn staged_package_is_removed_on_drop_and_source_is_untouched() {
    let ws = Workspace::new();
    let (archive, _) = ws.archive("drop");
    let before = fs::read(&archive).unwrap();
    let staged = LocalSource::new()
        .stage(
            &ThemeSource::LocalArchive {
                path: archive.clone(),
            },
            &ws.staging(),
        )
        .unwrap();
    drop(staged);
    assert!(entries(&ws.staging()).is_empty());
    assert_eq!(fs::read(&archive).unwrap(), before);
}

#[test]
fn missing_archive_is_source_not_found() {
    let ws = Workspace::new();
    let source = ThemeSource::LocalArchive {
        path: ws.path("absent.glint"),
    };
    let error = LocalSource::new()
        .stage(&source, &ws.staging())
        .expect_err("missing path must fail");
    assert_eq!(error.code(), C::NotFound);
    assert_eq!(error.code().as_str(), "GLINT_SOURCE_NOT_FOUND");
    assert!(entries(&ws.staging()).is_empty());
}

#[test]
fn missing_directory_is_source_not_found() {
    let ws = Workspace::new();
    let source = ThemeSource::LocalDirectory {
        path: ws.path("absent-theme"),
    };
    let error = LocalSource::new()
        .stage(&source, &ws.staging())
        .unwrap_err();
    assert_eq!(error.code(), C::NotFound);
}

#[test]
fn archive_source_that_is_a_directory_is_refused() {
    let ws = Workspace::new();
    let theme = ws.theme("theme", THEME_FILES);
    let error = LocalSource::new()
        .stage(&ThemeSource::LocalArchive { path: theme }, &ws.staging())
        .unwrap_err();
    assert_eq!(error.code(), C::NotAFile);
    assert_eq!(error.code().as_str(), "GLINT_SOURCE_NOT_A_FILE");
}

#[test]
fn directory_source_that_is_a_file_is_refused() {
    let ws = Workspace::new();
    let (archive, _) = ws.archive("file");
    let error = LocalSource::new()
        .stage(
            &ThemeSource::LocalDirectory { path: archive },
            &ws.staging(),
        )
        .unwrap_err();
    assert_eq!(error.code(), C::NotADirectory);
    assert_eq!(error.code().as_str(), "GLINT_SOURCE_NOT_A_DIRECTORY");
}

#[test]
fn invalid_archive_surfaces_the_package_code() {
    let ws = Workspace::new();
    let bogus = ws.path("out").join("bogus.glint");
    fs::write(&bogus, b"not a zip at all").unwrap();
    let error = LocalSource::new()
        .stage(&ThemeSource::LocalArchive { path: bogus }, &ws.staging())
        .unwrap_err();
    assert_eq!(error.code(), C::PackageRejected);
    assert_eq!(error.package_error().unwrap().code(), PackageCode::NotZip);
    assert!(entries(&ws.staging()).is_empty());
}

#[test]
fn local_directory_is_packed_deterministically_then_inspected() {
    let ws = Workspace::new();
    let theme = ws.theme("theme", THEME_FILES);
    let reference = pack_directory(&theme, &ws.path("out").join("ref.glint")).unwrap();

    let source = ThemeSource::LocalDirectory {
        path: theme.clone(),
    };
    let first = LocalSource::new().stage(&source, &ws.staging()).unwrap();
    let second = LocalSource::new().stage(&source, &ws.staging()).unwrap();

    assert_eq!(first.candidate.manifest.id, "dev.example.fixture");
    // Same bytes as an explicit `pack_directory`, every time.
    assert_eq!(first.candidate.sha256, reference.sha256);
    assert_eq!(second.candidate.sha256, reference.sha256);
    assert_eq!(first.origin.kind, SourceKind::LocalDirectory);
    assert_eq!(first.origin.archive_sha256, reference.sha256);
    assert!(
        first
            .candidate
            .extracted_root
            .join("dist/panel/index.html")
            .is_file()
    );

    // Only the two per-import staging directories remain: the temporary
    // packed archive is cleaned up.
    drop(first);
    drop(second);
    assert!(entries(&ws.staging()).is_empty());
}

#[test]
fn local_directory_staging_never_writes_into_the_source() {
    let ws = Workspace::new();
    let theme = ws.theme("theme", THEME_FILES);
    let before = tree(&theme);
    let staged = LocalSource::new()
        .stage(
            &ThemeSource::LocalDirectory {
                path: theme.clone(),
            },
            &ws.staging(),
        )
        .unwrap();
    drop(staged);
    assert_eq!(tree(&theme), before);
}

/// A directory with a build script and no built output is refused as a
/// package (missing referenced file); the build is never run.
#[test]
fn local_directory_never_runs_a_build() {
    let ws = Workspace::new();
    let theme = ws.theme(
        "unbuilt",
        &[
            ("glint.json", MANIFEST),
            ("theme/tokens.json", "{}"),
            (
                "package.json",
                r#"{"scripts":{"build":"node build.mjs","prepare":"node build.mjs"}}"#,
            ),
            ("src/panel/index.html", HTML),
        ],
    );
    let error = LocalSource::new()
        .stage(
            &ThemeSource::LocalDirectory {
                path: theme.clone(),
            },
            &ws.staging(),
        )
        .unwrap_err();
    assert_eq!(error.code(), C::PackageRejected);
    assert_eq!(
        error.package_error().unwrap().code(),
        PackageCode::MissingReferencedFile
    );
    assert!(!theme.join("dist").exists(), "no build output may appear");
    assert!(entries(&ws.staging()).is_empty());
}

#[test]
fn remote_kinds_are_unsupported_not_faked() {
    let ws = Workspace::new();
    let sources = [
        ThemeSource::Registry {
            registry: "official".into(),
            package: "mono".into(),
            version: None,
        },
        ThemeSource::Git {
            url: "https://github.com/user/glint-midnight.git".into(),
            reference: None,
            subpath: None,
        },
        ThemeSource::ArtifactUrl {
            url: "https://example.org/midnight.glint".into(),
            sha256: None,
        },
    ];
    let local = LocalSource::new();
    for source in &sources {
        assert!(!local.supports(source.kind()));
        let error = local.stage(source, &ws.staging()).unwrap_err();
        assert_eq!(error.code(), C::Unsupported, "{source:?}");
        assert_eq!(error.code().as_str(), "GLINT_SOURCE_UNSUPPORTED");
        assert_eq!(error.unsupported_kind(), Some(source.kind()));
    }
    assert!(local.supports(SourceKind::LocalArchive));
    assert!(local.supports(SourceKind::LocalDirectory));
    assert!(entries(&ws.staging()).is_empty());
}

#[test]
fn invalid_staging_root_is_reported() {
    let ws = Workspace::new();
    let (archive, _) = ws.archive("root");
    for root in [ws.path("no-such-root"), archive.clone()] {
        let error = LocalSource::new()
            .stage(
                &ThemeSource::LocalArchive {
                    path: archive.clone(),
                },
                &root,
            )
            .unwrap_err();
        assert_eq!(error.code(), C::StagingRootInvalid, "{root:?}");
    }
    let theme = ws.theme("theme", THEME_FILES);
    let error = LocalSource::new()
        .stage(
            &ThemeSource::LocalDirectory { path: theme },
            &ws.path("no-such-root"),
        )
        .unwrap_err();
    assert_eq!(error.code(), C::StagingRootInvalid);
}

#[test]
fn every_error_code_is_distinct_and_prefixed() {
    let mut seen = std::collections::BTreeSet::new();
    for code in C::ALL {
        assert!(code.as_str().starts_with("GLINT_SOURCE_"), "{code:?}");
        assert!(seen.insert(code.as_str()), "duplicate {code:?}");
        assert_eq!(code.to_string(), code.as_str());
    }
}

#[test]
fn source_kinds_have_stable_names() {
    let names: Vec<&str> = SourceKind::ALL.iter().map(|k| k.as_str()).collect();
    assert_eq!(
        names,
        [
            "registry",
            "git",
            "local-archive",
            "local-directory",
            "artifact-url"
        ]
    );
    assert!(SourceKind::LocalArchive.is_local());
    assert!(!SourceKind::Git.is_local());
}

fn tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        if path.is_dir() {
            out.push((rel, Vec::new()));
            walk(root, &path, out);
        } else {
            out.push((rel, fs::read(&path).unwrap()));
        }
    }
}
