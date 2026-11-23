//! Shared fixtures: synthetic themes generated in temporary directories.
//! No fixture is committed and nothing touches the network.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use glint_installer::Installer;
use glint_package::pack_directory;
use glint_sources::{LocalSource, PackageSource, StagedPackage, ThemeSource};
use glint_storage::GlintPaths;

/// A test home: a temporary directory with Glint's roots and a workspace
/// for source archives outside them.
pub struct Home {
    pub tmp: tempfile::TempDir,
    pub paths: GlintPaths,
    pub sources: PathBuf,
}

impl Home {
    pub fn new() -> Self {
        Self::with_root_name("glint")
    }

    pub fn with_root_name(name: &str) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let paths = GlintPaths::under_root(tmp.path().join(name)).unwrap();
        let sources = tmp.path().join("sources");
        fs::create_dir(&sources).unwrap();
        Self {
            tmp,
            paths,
            sources,
        }
    }

    pub fn open(&self) -> Installer {
        Installer::open(&self.paths).unwrap()
    }

    /// Pack a synthetic theme (`id`, `version`) into a `.glint` archive
    /// outside Glint's roots and return its path.
    pub fn archive(&self, id: &str, version: &str) -> PathBuf {
        let dir = self.sources.join(format!("{id}-{version}"));
        if !dir.exists() {
            write_theme(&dir, id, version);
        }
        let archive = self.sources.join(format!("{id}-{version}.glint"));
        if !archive.exists() {
            pack_directory(&dir, &archive).unwrap();
        }
        archive
    }

    /// Like [`Home::archive`] but with different token content, so the same
    /// ID and version produce a different digest.
    pub fn archive_variant(&self, id: &str, version: &str, variant: &str) -> PathBuf {
        let dir = self.sources.join(format!("{id}-{version}-{variant}"));
        if !dir.exists() {
            write_theme(&dir, id, version);
            fs::write(
                dir.join("theme/tokens.json"),
                format!(r##"{{"color":"#000000","variant":"{variant}"}}"##),
            )
            .unwrap();
        }
        let archive = self.sources.join(format!("{id}-{version}-{variant}.glint"));
        if !archive.exists() {
            pack_directory(&dir, &archive).unwrap();
        }
        archive
    }

    /// Stage any archive path for `installer`.
    pub fn stage_path(&self, installer: &Installer, path: PathBuf) -> StagedPackage {
        LocalSource::new()
            .stage(
                &ThemeSource::LocalArchive { path },
                &installer.staging_root(),
            )
            .unwrap()
    }

    /// Stage an archive for `installer`.
    pub fn stage(&self, installer: &Installer, id: &str, version: &str) -> StagedPackage {
        let source = ThemeSource::LocalArchive {
            path: self.archive(id, version),
        };
        LocalSource::new()
            .stage(&source, &installer.staging_root())
            .unwrap()
    }

    pub fn store_entries(&self) -> Vec<String> {
        names(&self.paths.store_dir())
    }

    pub fn staging_entries(&self) -> Vec<String> {
        names(&self.paths.staging_dir())
    }

    pub fn trash_entries(&self) -> Vec<String> {
        names(&self.paths.trash_dir())
    }
}

pub fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = match fs::read_dir(dir) {
        Ok(entries) => entries
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names
}

pub fn write_theme(dir: &Path, id: &str, version: &str) {
    let manifest = format!(
        r#"{{
  "schemaVersion": 1,
  "kind": "theme",
  "id": "{id}",
  "name": "Fixture {id}",
  "version": "{version}",
  "author": {{ "name": "Fixture Author" }},
  "license": "MIT",
  "compatibility": {{ "glint": ">=0.1.0", "platforms": ["windows", "linux", "macos"] }},
  "renderer": {{ "kind": "glint-web", "entrypointVersion": 1 }},
  "requiredCapabilities": ["surface.panel"],
  "tokens": "theme/tokens.json",
  "surfaces": {{ "panel": {{ "entry": "dist/panel/index.html" }} }}
}}"#
    );
    let files = [
        ("glint.json", manifest.as_str()),
        ("theme/tokens.json", r##"{"color":"#123456"}"##),
        (
            "dist/panel/index.html",
            r#"<!doctype html><html><head><script type="module" src="./main.js"></script></head><body></body></html>"#,
        ),
        ("dist/panel/main.js", "export const panel = 1;"),
    ];
    for (name, content) in files {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
}
