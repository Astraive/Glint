//! Deterministic `.glint` packer (PKG-003).
//!
//! [`pack_directory`] turns a theme directory into a `.glint` archive whose
//! bytes depend only on the packed file names and contents:
//!
//! - entries are sorted by their `/`-separated relative path (byte order);
//! - only regular files are written (no directory entries, so empty
//!   directories are dropped);
//! - every entry uses Deflate (fixed level 6), the UTF-8 name flag, the
//!   fixed MS-DOS timestamp 1980-01-01 00:00, "made by Unix" with mode
//!   `0100644`, and no extra fields, comments or data descriptors;
//! - the digest is the SHA-256 of the finished archive.
//!
//! Byte-identical output additionally assumes the same `flate2`/`zlib-rs`
//! versions (pinned by `Cargo.lock`); a compressor upgrade may change the
//! bytes but never the extracted content.
//!
//! The packer applies the reader's rules up front so its output always passes
//! [`crate::inspect_archive`] under the same limits: names, executables,
//! sizes, ratios, case-folded collisions, remote references and the manifest
//! (schema, known capabilities, referenced files). Symlinks (and Windows
//! junctions) anywhere in the tree are refused, so nothing outside the theme
//! root can be packed. Development junk is skipped and reported in
//! [`PackageDigest::excluded`]; see [`is_excluded_name`].

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use flate2::Compression;
use flate2::write::DeflateEncoder;
use sha2::{Digest, Sha256};

use crate::error::{PackageError, PathRejection};
use crate::limits::PackageLimits;
use crate::path::{
    MANIFEST_NAME, fold_key, is_executable_magic, is_executable_name, is_scanned_text,
    validate_entry_name,
};
use crate::reader::check_ratio;
use crate::scan;
use crate::staging;
use crate::validate::check_manifest;

/// Directory names skipped at any depth: dependency and build caches.
pub const EXCLUDED_DIR_NAMES: &[&str] =
    &["node_modules", "bower_components", "target", "__pycache__"];

/// File names skipped at any depth: OS metadata, package-manager lockfiles
/// and build-tool configuration.
pub const EXCLUDED_FILE_NAMES: &[&str] = &[
    "Thumbs.db",
    "desktop.ini",
    "package.json",
    "package-lock.json",
    "npm-shrinkwrap.json",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    "yarn.lock",
    "bun.lockb",
    "tsconfig.json",
    "jsconfig.json",
];

/// File-name prefixes of build-tool configuration skipped at any depth
/// (`vite.config.ts`, `rollup.config.mjs`, ...).
pub const EXCLUDED_FILE_PREFIXES: &[&str] = &[
    "vite.config.",
    "webpack.config.",
    "rollup.config.",
    "esbuild.config.",
    "postcss.config.",
    "tailwind.config.",
    "babel.config.",
];

/// DOS date for 1980-01-01 (the earliest representable date); time is 00:00.
const FIXED_DOS_DATE: u16 = 0x0021;
const FIXED_DOS_TIME: u16 = 0;
const FIXED_UNIX_MODE: u32 = 0o100_644;
const VERSION: u16 = 20;
const MADE_BY_UNIX: u16 = (3 << 8) | VERSION;
const FLAG_UTF8: u16 = 0x0800;
const METHOD_DEFLATE: u16 = 8;
const DEFLATE_LEVEL: u32 = 6;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The result of a successful [`pack_directory`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PackageDigest {
    /// SHA-256 of the archive bytes.
    pub sha256: [u8; 32],
    /// Archive size in bytes.
    pub size: u64,
    /// Number of files packed.
    pub entries: usize,
    /// Relative paths that were skipped by the exclusion rules, sorted.
    pub excluded: Vec<String>,
}

impl PackageDigest {
    /// Lowercase hexadecimal form of [`sha256`](Self::sha256).
    pub fn sha256_hex(&self) -> String {
        crate::to_hex(&self.sha256)
    }
}

/// True when a directory entry named `name` is skipped by the packer: any
/// hidden name (leading `.`, such as `.git`, `.env`, `.DS_Store`), the
/// [`EXCLUDED_DIR_NAMES`] (directories) and the [`EXCLUDED_FILE_NAMES`] and
/// [`EXCLUDED_FILE_PREFIXES`] (files).
pub fn is_excluded_name(name: &str, is_dir: bool) -> bool {
    if name.starts_with('.') {
        return true;
    }
    if is_dir {
        EXCLUDED_DIR_NAMES.contains(&name)
    } else {
        EXCLUDED_FILE_NAMES.contains(&name)
            || EXCLUDED_FILE_PREFIXES
                .iter()
                .any(|prefix| name.starts_with(prefix))
    }
}

/// Pack the theme directory `src` into the archive `dest` with the default
/// [`PackageLimits`].
///
/// `dest` is written through a temporary file in its parent directory and
/// then renamed into place, replacing an existing file. It must not be inside
/// `src`.
///
/// # Errors
///
/// Returns a [`PackageError`] with a stable `GLINT_PACKAGE_*` code. On error
/// `dest` is left untouched and no temporary file remains.
pub fn pack_directory(src: &Path, dest: &Path) -> Result<PackageDigest, PackageError> {
    pack_directory_with_limits(src, dest, &PackageLimits::default())
}

/// [`pack_directory`] with explicit limits. The output passes
/// [`crate::inspect_archive`] under the same limits.
///
/// # Errors
///
/// As [`pack_directory`].
pub fn pack_directory_with_limits(
    src: &Path,
    dest: &Path,
    limits: &PackageLimits,
) -> Result<PackageDigest, PackageError> {
    if !staging::is_real_dir(src) {
        return Err(PackageError::PackSourceInvalid(src.to_path_buf()));
    }
    let source_root =
        fs::canonicalize(src).map_err(|_| PackageError::PackSourceInvalid(src.to_path_buf()))?;
    let invalid_dest = || PackageError::PackDestinationInvalid(dest.to_path_buf());
    let file_name = dest.file_name().ok_or_else(invalid_dest)?;
    let parent = match dest.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let parent_canonical = fs::canonicalize(parent).map_err(|_| invalid_dest())?;
    if parent_canonical.starts_with(&source_root) {
        return Err(invalid_dest());
    }

    let mut files = Vec::new();
    let mut excluded = Vec::new();
    collect(src, "", 0, limits, &mut files, &mut excluded)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    excluded.sort();
    check_file_set(&files, limits)?;

    let temp = parent.join(format!(
        ".{}.tmp-{}-{}",
        file_name.to_string_lossy(),
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let written = write_archive(&temp, &files, limits)
        .and_then(|()| digest_file(&temp))
        .and_then(|digest| {
            fs::rename(&temp, dest).map_err(PackageError::io("move package into place"))?;
            Ok(digest)
        });
    match written {
        Ok((sha256, size)) => Ok(PackageDigest {
            sha256,
            size,
            entries: files.len(),
            excluded,
        }),
        Err(error) => {
            let _ = fs::remove_file(&temp);
            Err(error)
        }
    }
}

/// Walk `dir` (relative path `rel`), refusing links and special files and
/// skipping excluded names.
fn collect(
    dir: &Path,
    rel: &str,
    depth: usize,
    limits: &PackageLimits,
    files: &mut Vec<(String, PathBuf)>,
    excluded: &mut Vec<String>,
) -> Result<(), PackageError> {
    let entries = fs::read_dir(dir).map_err(PackageError::io("read source directory"))?;
    for entry in entries {
        let entry = entry.map_err(PackageError::io("read source directory"))?;
        let os_name = entry.file_name();
        let lossy = os_name.to_string_lossy().into_owned();
        let child_rel = if rel.is_empty() {
            lossy.clone()
        } else {
            format!("{rel}/{lossy}")
        };
        let Some(name) = os_name.to_str() else {
            return Err(PackageError::UnsafePath {
                name: child_rel,
                reason: PathRejection::NotUtf8,
            });
        };
        let meta =
            fs::symlink_metadata(entry.path()).map_err(PackageError::io("inspect source entry"))?;
        let file_type = meta.file_type();
        if file_type.is_symlink() {
            return Err(PackageError::Symlink { name: child_rel });
        }
        if is_excluded_name(name, meta.is_dir()) {
            excluded.push(child_rel);
            continue;
        }
        if meta.is_dir() {
            if depth + 1 >= limits.max_depth {
                return Err(PackageError::UnsafePath {
                    name: child_rel,
                    reason: PathRejection::TooDeep,
                });
            }
            collect(
                &entry.path(),
                &child_rel,
                depth + 1,
                limits,
                files,
                excluded,
            )?;
        } else if meta.is_file() {
            validate_entry_name(child_rel.as_bytes(), limits).map_err(|reason| {
                PackageError::UnsafePath {
                    name: child_rel.clone(),
                    reason,
                }
            })?;
            if is_executable_name(&child_rel) {
                return Err(PackageError::ExecutableContent { name: child_rel });
            }
            if meta.len() > limits.max_file_uncompressed {
                return Err(PackageError::EntryTooLarge {
                    name: child_rel,
                    size: meta.len(),
                    limit: limits.max_file_uncompressed,
                });
            }
            if is_scanned_text(&child_rel) && meta.len() > limits.max_scanned_text_bytes {
                return Err(PackageError::TextTooLarge {
                    name: child_rel,
                    size: meta.len(),
                    limit: limits.max_scanned_text_bytes,
                });
            }
            files.push((child_rel, entry.path()));
        } else {
            return Err(PackageError::NotRegularFile {
                name: child_rel,
                mode: 0,
            });
        }
    }
    Ok(())
}

/// Entry count, case-folded collisions and the manifest, before writing.
fn check_file_set(files: &[(String, PathBuf)], limits: &PackageLimits) -> Result<(), PackageError> {
    if files.len() > limits.max_entries {
        return Err(PackageError::TooManyEntries {
            count: files.len(),
            limit: limits.max_entries,
        });
    }
    let mut folded = BTreeMap::<String, &str>::new();
    for (rel, _) in files {
        if let Some(first) = folded.insert(fold_key(rel, limits), rel) {
            return Err(PackageError::PathCollision {
                first: first.to_owned(),
                second: rel.clone(),
            });
        }
    }
    let manifest_path = files
        .iter()
        .find(|(rel, _)| rel == MANIFEST_NAME)
        .map(|(_, path)| path)
        .ok_or(PackageError::MissingManifest)?;
    let bytes = read_bounded(manifest_path, MANIFEST_NAME, limits.max_scanned_text_bytes)?;
    let names: BTreeSet<&str> = files.iter().map(|(rel, _)| rel.as_str()).collect();
    check_manifest(bytes, &names)?;
    Ok(())
}

/// Read a whole regular file, refusing more than `limit` bytes.
fn read_bounded(path: &Path, rel: &str, limit: u64) -> Result<Vec<u8>, PackageError> {
    let file = File::open(path).map_err(PackageError::io("open source file"))?;
    let meta = file
        .metadata()
        .map_err(PackageError::io("inspect source file"))?;
    if !meta.is_file() {
        return Err(PackageError::NotRegularFile {
            name: rel.to_owned(),
            mode: 0,
        });
    }
    let mut data = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut data)
        .map_err(PackageError::io("read source file"))?;
    if data.len() as u64 > limit {
        return Err(PackageError::EntryTooLarge {
            name: rel.to_owned(),
            size: data.len() as u64,
            limit,
        });
    }
    Ok(data)
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn fits_u32(value: u64, limits: &PackageLimits) -> Result<u32, PackageError> {
    u32::try_from(value).map_err(|_| PackageError::ArchiveTooLarge {
        size: value,
        limit: limits.max_archive_bytes,
    })
}

fn fits_u16(value: usize, rel: &str) -> Result<u16, PackageError> {
    u16::try_from(value).map_err(|_| PackageError::UnsafePath {
        name: rel.to_owned(),
        reason: PathRejection::TooLong,
    })
}

fn write_archive(
    temp: &Path,
    files: &[(String, PathBuf)],
    limits: &PackageLimits,
) -> Result<(), PackageError> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp)
        .map_err(PackageError::io("create temporary package"))?;
    let mut out = BufWriter::new(file);
    let mut central = Vec::new();
    let mut offset: u64 = 0;
    let mut total: u64 = 0;
    for (rel, path) in files {
        let data = read_bounded(path, rel, limits.max_file_uncompressed)?;
        let size = data.len() as u64;
        if is_executable_magic(&data) {
            return Err(PackageError::ExecutableContent { name: rel.clone() });
        }
        if is_scanned_text(rel) {
            if size > limits.max_scanned_text_bytes {
                return Err(PackageError::TextTooLarge {
                    name: rel.clone(),
                    size,
                    limit: limits.max_scanned_text_bytes,
                });
            }
            if let Some(reference) = scan::find_remote_reference(rel, &data) {
                return Err(PackageError::RemoteReference {
                    name: rel.clone(),
                    reference,
                });
            }
            if let Some(construct) = scan::find_active_content(rel, &data) {
                return Err(PackageError::ActiveContent {
                    name: rel.clone(),
                    construct,
                });
            }
        }
        total = total
            .checked_add(size)
            .filter(|total| *total <= limits.max_total_uncompressed)
            .ok_or(PackageError::TotalTooLarge {
                limit: limits.max_total_uncompressed,
            })?;
        let compressed = deflate(&data)?;
        check_ratio(rel, size, compressed.len() as u64, limits)?;

        let crc = crc32fast::hash(&data);
        let name = rel.as_bytes();
        let name_len = fits_u16(name.len(), rel)?;
        let compressed_len = fits_u32(compressed.len() as u64, limits)?;
        let size32 = fits_u32(size, limits)?;
        let offset32 = fits_u32(offset, limits)?;

        let mut local = Vec::with_capacity(30 + name.len());
        put_u32(&mut local, 0x0403_4b50);
        put_u16(&mut local, VERSION);
        put_u16(&mut local, FLAG_UTF8);
        put_u16(&mut local, METHOD_DEFLATE);
        put_u16(&mut local, FIXED_DOS_TIME);
        put_u16(&mut local, FIXED_DOS_DATE);
        put_u32(&mut local, crc);
        put_u32(&mut local, compressed_len);
        put_u32(&mut local, size32);
        put_u16(&mut local, name_len);
        put_u16(&mut local, 0); // extra field length
        local.extend_from_slice(name);
        out.write_all(&local)
            .and_then(|()| out.write_all(&compressed))
            .map_err(PackageError::io("write temporary package"))?;

        put_u32(&mut central, 0x0201_4b50);
        put_u16(&mut central, MADE_BY_UNIX);
        put_u16(&mut central, VERSION);
        put_u16(&mut central, FLAG_UTF8);
        put_u16(&mut central, METHOD_DEFLATE);
        put_u16(&mut central, FIXED_DOS_TIME);
        put_u16(&mut central, FIXED_DOS_DATE);
        put_u32(&mut central, crc);
        put_u32(&mut central, compressed_len);
        put_u32(&mut central, size32);
        put_u16(&mut central, name_len);
        put_u16(&mut central, 0); // extra field length
        put_u16(&mut central, 0); // comment length
        put_u16(&mut central, 0); // disk number
        put_u16(&mut central, 0); // internal attributes
        put_u32(&mut central, FIXED_UNIX_MODE << 16);
        put_u32(&mut central, offset32);
        central.extend_from_slice(name);

        offset += local.len() as u64 + compressed.len() as u64;
    }

    let count = u16::try_from(files.len()).map_err(|_| PackageError::TooManyEntries {
        count: files.len(),
        limit: usize::from(u16::MAX - 1),
    })?;
    let cd_offset = fits_u32(offset, limits)?;
    let cd_size = fits_u32(central.len() as u64, limits)?;
    let archive_size = offset + central.len() as u64 + 22;
    if archive_size > limits.max_archive_bytes {
        return Err(PackageError::ArchiveTooLarge {
            size: archive_size,
            limit: limits.max_archive_bytes,
        });
    }
    let mut end = Vec::with_capacity(22);
    put_u32(&mut end, 0x0605_4b50);
    put_u16(&mut end, 0); // this disk
    put_u16(&mut end, 0); // central directory disk
    put_u16(&mut end, count);
    put_u16(&mut end, count);
    put_u32(&mut end, cd_size);
    put_u32(&mut end, cd_offset);
    put_u16(&mut end, 0); // comment length
    out.write_all(&central)
        .and_then(|()| out.write_all(&end))
        .map_err(PackageError::io("write temporary package"))?;
    let file = out.into_inner().map_err(|error| PackageError::Io {
        context: "flush temporary package",
        source: error.into_error(),
    })?;
    file.sync_all()
        .map_err(PackageError::io("flush temporary package"))
}

fn deflate(data: &[u8]) -> Result<Vec<u8>, PackageError> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::new(DEFLATE_LEVEL));
    encoder
        .write_all(data)
        .map_err(PackageError::io("compress file"))?;
    encoder.finish().map_err(PackageError::io("compress file"))
}

fn digest_file(path: &Path) -> Result<([u8; 32], u64), PackageError> {
    let mut file = File::open(path).map_err(PackageError::io("hash package"))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut size: u64 = 0;
    loop {
        let n = file
            .read(&mut buf)
            .map_err(PackageError::io("hash package"))?;
        if n == 0 {
            break;
        }
        size += n as u64;
        hasher.update(&buf[..n]);
    }
    Ok((hasher.finalize().into(), size))
}
