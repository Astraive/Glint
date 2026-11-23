//! Hostile fixture generator, ported from the SPK-007 spike
//! (`spikes/package-security/tests/support/mod.rs`).
//!
//! Every fixture is generated in memory at test time and written only into a
//! per-test temporary directory. No archive (and certainly no compression
//! bomb) is committed to the repository. Provenance: all bytes below are
//! synthesized by this file; license MIT (repository license).
//!
//! The raw builder writes ZIP records by hand so tests can produce archives
//! the `zip` crate's writer refuses to create (duplicate names, lying sizes,
//! overlapping entries, mismatched local/central names, forged attributes).

#![allow(dead_code)]

use std::io::Write;
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::write::DeflateEncoder;

pub const HOST_DOS: u8 = 0;
pub const HOST_UNIX: u8 = 3;
pub const HOST_NTFS: u8 = 10;
pub const S_IFLNK: u32 = 0o120_000;
pub const S_IFREG: u32 = 0o100_000;
pub const S_IFCHR: u32 = 0o020_000;
pub const S_IFIFO: u32 = 0o010_000;
pub const FLAG_ENCRYPTED: u16 = 0x0001;
pub const FLAG_DATA_DESCRIPTOR: u16 = 0x0008;
pub const FLAG_STRONG_ENCRYPTION: u16 = 0x0040;
pub const FLAG_UTF8: u16 = 0x0800;
pub const FLAG_MASKED_HEADERS: u16 = 0x2000;
pub const MIB: usize = 1024 * 1024;

/// One raw ZIP entry. Defaults describe an honest stored file.
#[derive(Clone, Debug)]
pub struct RawEntry {
    pub name: Vec<u8>,
    pub data: Vec<u8>,
    pub deflate: bool,
    /// Override the declared uncompressed size (both headers).
    pub declared_size: Option<u32>,
    pub made_by_host: u8,
    pub external_attrs: u32,
    pub flags: u16,
    pub method_override: Option<u16>,
    /// Extra field bytes written to both local and central headers.
    pub extra: Vec<u8>,
    /// Different name in the local header (central keeps `name`).
    pub local_name: Option<Vec<u8>>,
    /// Point this central record at an earlier entry's local header.
    pub reuse_local_of: Option<usize>,
    /// Write the local header and data but omit the central record (a file
    /// only streaming extractors would see).
    pub central_skip: bool,
}

impl RawEntry {
    pub fn stored(name: impl AsRef<[u8]>, data: impl AsRef<[u8]>) -> Self {
        Self {
            name: name.as_ref().to_vec(),
            data: data.as_ref().to_vec(),
            deflate: false,
            declared_size: None,
            made_by_host: HOST_DOS,
            external_attrs: 0,
            flags: FLAG_UTF8,
            method_override: None,
            extra: Vec::new(),
            local_name: None,
            reuse_local_of: None,
            central_skip: false,
        }
    }

    pub fn deflated(name: impl AsRef<[u8]>, data: impl AsRef<[u8]>) -> Self {
        Self {
            deflate: true,
            ..Self::stored(name, data)
        }
    }

    pub fn unix_mode(mut self, mode: u32) -> Self {
        self.made_by_host = HOST_UNIX;
        self.external_attrs = mode << 16;
        self
    }

    pub fn host(mut self, host: u8) -> Self {
        self.made_by_host = host;
        self
    }

    pub fn external(mut self, attrs: u32) -> Self {
        self.external_attrs = attrs;
        self
    }

    pub fn flags(mut self, flags: u16) -> Self {
        self.flags = flags;
        self
    }

    pub fn method(mut self, method: u16) -> Self {
        self.method_override = Some(method);
        self
    }

    pub fn declared(mut self, size: u32) -> Self {
        self.declared_size = Some(size);
        self
    }

    pub fn extra_field(mut self, header_id: u16, payload: &[u8]) -> Self {
        self.extra.extend_from_slice(&header_id.to_le_bytes());
        self.extra
            .extend_from_slice(&u16::try_from(payload.len()).unwrap().to_le_bytes());
        self.extra.extend_from_slice(payload);
        self
    }

    pub fn local_name(mut self, name: impl AsRef<[u8]>) -> Self {
        self.local_name = Some(name.as_ref().to_vec());
        self
    }

    pub fn hidden_from_central(mut self) -> Self {
        self.central_skip = true;
        self
    }

    pub fn reuse_local_of(mut self, index: usize) -> Self {
        self.reuse_local_of = Some(index);
        self
    }
}

fn u16le(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn u32le(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn len16(len: usize) -> u16 {
    u16::try_from(len).expect("field fits u16")
}

fn len32(len: usize) -> u32 {
    u32::try_from(len).expect("fixture stays below 4 GiB")
}

/// Build a ZIP archive byte-for-byte.
pub fn build_zip(entries: &[RawEntry]) -> Vec<u8> {
    struct Written {
        offset: u32,
        crc: u32,
        compressed: u32,
        declared: u32,
        method: u16,
    }
    let mut out = Vec::new();
    let mut written: Vec<Written> = Vec::new();
    for entry in entries {
        if let Some(source) = entry.reuse_local_of {
            let src = &written[source];
            written.push(Written {
                offset: src.offset,
                crc: src.crc,
                compressed: src.compressed,
                declared: src.declared,
                method: src.method,
            });
            continue;
        }
        let payload = if entry.deflate {
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::best());
            encoder.write_all(&entry.data).unwrap();
            encoder.finish().unwrap()
        } else {
            entry.data.clone()
        };
        let method = entry
            .method_override
            .unwrap_or(if entry.deflate { 8 } else { 0 });
        let crc = crc32fast::hash(&entry.data);
        let declared = entry.declared_size.unwrap_or(len32(entry.data.len()));
        let offset = len32(out.len());
        let local_name = entry.local_name.as_ref().unwrap_or(&entry.name);
        u32le(&mut out, 0x0403_4b50);
        u16le(&mut out, 20);
        u16le(&mut out, entry.flags);
        u16le(&mut out, method);
        u16le(&mut out, 0); // time
        u16le(&mut out, 0x21); // 1980-01-01
        u32le(&mut out, crc);
        u32le(&mut out, len32(payload.len()));
        u32le(&mut out, declared);
        u16le(&mut out, len16(local_name.len()));
        u16le(&mut out, len16(entry.extra.len()));
        out.extend_from_slice(local_name);
        out.extend_from_slice(&entry.extra);
        out.extend_from_slice(&payload);
        written.push(Written {
            offset,
            crc,
            compressed: len32(payload.len()),
            declared,
            method,
        });
    }
    let cd_start = len32(out.len());
    let mut central_count = 0usize;
    for (entry, w) in entries.iter().zip(&written) {
        if entry.central_skip {
            continue;
        }
        central_count += 1;
        u32le(&mut out, 0x0201_4b50);
        u16le(&mut out, (u16::from(entry.made_by_host) << 8) | 20);
        u16le(&mut out, 20);
        u16le(&mut out, entry.flags);
        u16le(&mut out, w.method);
        u16le(&mut out, 0);
        u16le(&mut out, 0x21);
        u32le(&mut out, w.crc);
        u32le(&mut out, w.compressed);
        u32le(&mut out, w.declared);
        u16le(&mut out, len16(entry.name.len()));
        u16le(&mut out, len16(entry.extra.len()));
        u16le(&mut out, 0); // comment
        u16le(&mut out, 0); // disk
        u16le(&mut out, 0); // internal attrs
        u32le(&mut out, entry.external_attrs);
        u32le(&mut out, w.offset);
        out.extend_from_slice(&entry.name);
        out.extend_from_slice(&entry.extra);
    }
    let cd_size = len32(out.len()) - cd_start;
    u32le(&mut out, 0x0605_4b50);
    u16le(&mut out, 0);
    u16le(&mut out, 0);
    u16le(&mut out, len16(central_count));
    u16le(&mut out, len16(central_count));
    u32le(&mut out, cd_size);
    u32le(&mut out, cd_start);
    u16le(&mut out, 0);
    out
}

/// Offset of the end-of-central-directory record in a `build_zip` archive.
pub fn eocd_offset(bytes: &[u8]) -> usize {
    bytes.len() - 22
}

/// A manifest `glint-schema` accepts, referencing every baseline file.
pub const GOOD_MANIFEST: &str = r#"{
  "schemaVersion": 1,
  "kind": "theme",
  "id": "dev.example.fixture",
  "name": "Fixture",
  "version": "1.0.0",
  "author": { "name": "Fixture Author" },
  "license": "MIT",
  "compatibility": { "glint": ">=0.1.0", "platforms": ["windows", "linux", "macos"] },
  "renderer": { "kind": "glint-web", "entrypointVersion": 1 },
  "requiredCapabilities": ["surface.panel"],
  "optionalCapabilities": ["system.audio.read"],
  "tokens": "theme/tokens.json",
  "surfaces": { "panel": { "entry": "dist/panel/index.html" } }
}"#;

pub const GOOD_HTML: &str = r#"<!doctype html><html><head><script type="module" src="./main.js"></script></head><body><a href="https://example.org">site</a></body></html>"#;

/// The honest baseline package every hostile fixture is derived from.
pub fn good_entries() -> Vec<RawEntry> {
    vec![
        RawEntry::deflated("glint.json", GOOD_MANIFEST),
        RawEntry::stored("theme/", ""),
        RawEntry::deflated("theme/tokens.json", r##"{"color":"#000"}"##),
        RawEntry::deflated("dist/panel/index.html", GOOD_HTML),
        RawEntry::deflated("dist/panel/main.js", "import { x } from './dep.js'; x();"),
        RawEntry::deflated("dist/panel/dep.js", "export const x = () => 1;"),
    ]
}

/// Baseline plus extra entries.
pub fn good_plus(extra: Vec<RawEntry>) -> Vec<u8> {
    let mut entries = good_entries();
    entries.extend(extra);
    build_zip(&entries)
}

/// Baseline with `glint.json` replaced.
pub fn with_manifest(manifest: &str) -> Vec<u8> {
    let mut entries = good_entries();
    entries[0] = RawEntry::deflated("glint.json", manifest);
    build_zip(&entries)
}

/// Baseline with one file's content replaced.
pub fn with_file(name: &str, content: &str) -> Vec<u8> {
    let mut entries = good_entries();
    entries.retain(|e| e.name != name.as_bytes());
    entries.push(RawEntry::deflated(name, content));
    build_zip(&entries)
}

/// Deterministic incompressible bytes (xorshift), so size limits are hit
/// without tripping the ratio check first.
pub fn noise(len: usize) -> Vec<u8> {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state.to_le_bytes()[0]
        })
        .collect()
}

/// Temporary sandbox: `<root>/input/fixture.glint` and the staging root
/// `<root>/staging`. Nothing may ever appear anywhere else.
pub struct Sandbox {
    pub dir: tempfile::TempDir,
    pub archive: PathBuf,
    pub staging_root: PathBuf,
}

impl Sandbox {
    pub fn new(bytes: &[u8]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("input")).unwrap();
        std::fs::create_dir(dir.path().join("staging")).unwrap();
        let archive = dir.path().join("input").join("fixture.glint");
        std::fs::write(&archive, bytes).unwrap();
        let staging_root = dir.path().join("staging");
        Self {
            dir,
            archive,
            staging_root,
        }
    }

    /// Every file and directory under the sandbox root, relative, sorted.
    /// Links are listed, never followed.
    pub fn snapshot(&self) -> Vec<String> {
        fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                out.push(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
                if entry.file_type().unwrap().is_dir() {
                    walk(root, &path, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(self.dir.path(), self.dir.path(), &mut out);
        out.sort();
        out
    }
}
