//! SPK-007 throwaway spike: a bounded `.glint` (ZIP) validator.
//!
//! This crate is feasibility code. It is never shipped and never imported by
//! product code; PKG-001 builds the real `crates/glint-package` reader from the
//! findings recorded in `docs/research/spikes/package-security.md`.
//!
//! Design: the archive is opened once, hashed, and every entry is validated in
//! two passes (metadata, then bounded decompression with SHA-256) **before** a
//! single byte is written. [`stage_archive`] writes only into a staging
//! directory it creates itself, and only after [`inspect`] succeeded.

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use zip::read::ArchiveOffset;
use zip::read::Config;
use zip::{CompressionMethod, ExtraField, ZipArchive};

mod scan;

pub use scan::find_remote_reference;

/// Resource budgets applied while reading an untrusted archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageLimits {
    /// Maximum size of the `.glint` file on disk, checked before parsing.
    pub max_archive_bytes: u64,
    /// Maximum number of central-directory entries (files + directories).
    pub max_entries: usize,
    /// Maximum sum of uncompressed file sizes (declared and actual).
    pub max_total_uncompressed: u64,
    /// Maximum uncompressed size of one file (declared and actual).
    pub max_file_uncompressed: u64,
    /// Maximum uncompressed:compressed ratio for one entry and for the archive.
    pub max_ratio: u64,
    /// Ratio checks apply only to entries/totals at least this large, so tiny
    /// highly compressible files (a 2 KiB JSON of spaces) do not trip them.
    pub ratio_min_bytes: u64,
    /// Maximum number of path components (`a/b/c.txt` has depth 3).
    pub max_depth: usize,
    /// Maximum entry name length in bytes.
    pub max_path_len: usize,
    /// Maximum single component length in bytes.
    pub max_component_len: usize,
    /// When true (default), entry names must be portable ASCII. This removes
    /// Unicode normalization and Unicode case-folding collisions entirely.
    pub ascii_paths_only: bool,
    /// Maximum size of a text file kept in memory for manifest parsing and
    /// remote-reference scanning.
    pub max_scanned_text_bytes: u64,
}

impl Default for PackageLimits {
    fn default() -> Self {
        const MIB: u64 = 1024 * 1024;
        Self {
            max_archive_bytes: 256 * MIB,
            max_entries: 10_000,
            max_total_uncompressed: 512 * MIB,
            max_file_uncompressed: 128 * MIB,
            max_ratio: 100,
            ratio_min_bytes: MIB,
            max_depth: 16,
            max_path_len: 240,
            max_component_len: 128,
            ascii_paths_only: true,
            max_scanned_text_bytes: 4 * MIB,
        }
    }
}

/// Why an entry name was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathRejection {
    Empty,
    TooLong,
    TooDeep,
    ComponentTooLong,
    NotUtf8,
    NonAscii,
    ControlChar,
    /// `\\server\share`, `//server/share`, `\\.\device`, `\\?\C:\x`.
    UncOrDevicePath,
    /// Leading `/` or `\`.
    Absolute,
    /// `C:` drive prefix, or any `:` (also blocks NTFS alternate data streams).
    DriveOrColon,
    /// `\` used as a separator; Windows would split there, POSIX would not.
    Backslash,
    ParentDir,
    CurDir,
    /// `a//b`.
    EmptyComponent,
    /// `CON`, `NUL`, `COM1`, `LPT1`, `CONIN$` ... (any extension).
    ReservedDeviceName,
    /// Win32 strips trailing `.` and ` `, creating collisions.
    TrailingDotOrSpace,
    /// `< > " | ? * ~` (`~` blocks 8.3 short-name aliasing such as `PROGRA~1`).
    ForbiddenChar,
}

/// Every way an archive can be rejected. Variants are typed so callers can
/// map them to the installer error taxonomy (`invalid-archive`, ...).
#[derive(Debug)]
pub enum PackageError {
    Io(io::Error),
    /// The file does not start with a ZIP local file header (`PK\x03\x04`).
    NotZip,
    ArchiveTooLarge {
        size: u64,
        limit: u64,
    },
    /// The `zip` crate refused to parse the archive.
    Malformed(String),
    TooManyEntries {
        count: usize,
        limit: usize,
    },
    /// Two central-directory records share one exact name.
    DuplicateName,
    /// Two names collide after case folding, or a name is both file and directory.
    PathCollision {
        first: String,
        second: String,
    },
    UnsafePath {
        name: String,
        reason: PathRejection,
    },
    Symlink {
        name: String,
    },
    /// Unix file type other than regular file/directory (device, FIFO, socket).
    NotRegularFile {
        name: String,
        mode: u32,
    },
    Encrypted {
        name: String,
    },
    UnsupportedCompression {
        name: String,
        method: String,
    },
    ForbiddenExtraField {
        name: String,
        header_id: u16,
    },
    /// Compressed data of one entry is shared with another (overlapping-file bomb).
    OverlappingEntries,
    EntryTooLarge {
        name: String,
        size: u64,
        limit: u64,
    },
    TotalTooLarge {
        limit: u64,
    },
    RatioExceeded {
        name: String,
        ratio: u64,
        limit: u64,
    },
    /// Declared and actual uncompressed sizes disagree.
    SizeMismatch {
        name: String,
        declared: u64,
        actual: u64,
    },
    /// Native executable or install script (by extension or magic bytes).
    ExecutableContent {
        name: String,
    },
    MissingManifest,
    InvalidManifest(String),
    UnknownCapability(String),
    MissingReferencedFile(String),
    RemoteReference {
        name: String,
        reference: String,
    },
    /// The staging directory already exists; staging never reuses a directory.
    StagingExists(PathBuf),
    /// Content changed between validation and staging.
    StagingHashMismatch {
        name: String,
    },
}

impl fmt::Display for PackageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for PackageError {}

impl From<io::Error> for PackageError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<zip::result::ZipError> for PackageError {
    fn from(error: zip::result::ZipError) -> Self {
        Self::Malformed(error.to_string())
    }
}

/// One validated regular file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateEntry {
    /// Normalized `/`-separated relative path.
    pub path: String,
    /// Index in the central directory.
    pub index: usize,
    pub size: u64,
    pub sha256: [u8; 32],
}

/// A fully validated archive. Nothing has been written to disk.
#[derive(Debug, Clone)]
pub struct PackageCandidate {
    pub archive_sha256: [u8; 32],
    pub files: Vec<CandidateEntry>,
    pub directories: Vec<String>,
    pub total_uncompressed: u64,
    pub manifest: serde_json::Value,
}

/// Result of a successful [`stage_archive`].
#[derive(Debug, Clone)]
pub struct StagedPackage {
    pub root: PathBuf,
    pub candidate: PackageCandidate,
}

/// Capabilities this spike knows about (from the SDK `CapabilityId` proposal).
pub const KNOWN_CAPABILITIES: &[&str] = &[
    "system.audio.read",
    "system.audio.write",
    "desktop.workspaces.read",
    "desktop.workspaces.switch",
    "surface.panel",
    "surface.launcher",
    "surface.control-center",
];

const MANIFEST_NAME: &str = "glint.json";
const ZIP_LOCAL_MAGIC: &[u8; 4] = b"PK\x03\x04";
const S_IFMT: u32 = 0o170_000;
const S_IFREG: u32 = 0o100_000;
const S_IFDIR: u32 = 0o040_000;
const S_IFLNK: u32 = 0o120_000;

/// Extra fields that are inert for extraction. Everything else is rejected:
/// `0x7075` (Info-ZIP Unicode Path) silently renames entries inside the `zip`
/// crate, `0x000d` (PKWARE Unix) can carry symlink/hardlink targets, `0x9901`
/// is AES encryption.
const ALLOWED_CUSTOM_EXTRA_IDS: &[u16] = &[
    0x7875, // Info-ZIP Unix UID/GID ("ux")
];

const EXECUTABLE_EXTENSIONS: &[&str] = &[
    "exe", "dll", "sys", "com", "scr", "msi", "msix", "appx", "bat", "cmd", "ps1", "psm1", "vbs",
    "vbe", "wsf", "wsh", "hta", "lnk", "reg", "cpl", "jar", "sh", "bash", "zsh", "so", "dylib",
    "app", "pkg", "deb", "rpm", "appimage", "command", "node",
];

/// Inspect a `.glint` file without writing anything. See [`inspect`].
pub fn inspect_archive(
    path: &Path,
    limits: &PackageLimits,
) -> Result<PackageCandidate, PackageError> {
    let file = open_untrusted(path)?;
    inspect(file, limits).map(|(candidate, _)| candidate)
}

/// Validate an archive from any seekable reader.
///
/// Returns the candidate and the still-open archive so staging reuses the same
/// handle (no re-open TOCTOU window for the central directory).
pub fn inspect<R: Read + Seek>(
    mut reader: R,
    limits: &PackageLimits,
) -> Result<(PackageCandidate, ZipArchive<R>), PackageError> {
    let archive_sha256 = hash_and_check_envelope(&mut reader, limits)?;

    // Known(0): do not let the crate "detect" an offset, which would accept
    // data prepended before the archive (polyglot EXE+ZIP files).
    let config = Config {
        archive_offset: ArchiveOffset::Known(0),
    };
    let mut archive = ZipArchive::with_config(config, reader)?;

    // `len()` counts unique names only; duplicates are detected separately.
    if archive.has_duplicate_names() {
        return Err(PackageError::DuplicateName);
    }
    if archive.len() > limits.max_entries {
        return Err(PackageError::TooManyEntries {
            count: archive.len(),
            limit: limits.max_entries,
        });
    }
    if archive.has_overlapping_files()? {
        return Err(PackageError::OverlappingEntries);
    }
    // Not enforced by default in `zip` 9.8.1: local headers must match the
    // central directory and no local header may be missing from it. Without
    // this, a streaming extractor and this reader can see different files.
    archive
        .validate_header_consistency()
        .map_err(|error| PackageError::Malformed(format!("header consistency: {error}")))?;

    // Pass 1: metadata only.
    let mut metas = Vec::with_capacity(archive.len());
    let mut folded = BTreeMap::<String, String>::new();
    let mut declared_total: u64 = 0;
    let mut compressed_total: u64 = 0;
    for index in 0..archive.len() {
        let entry = archive.by_index_raw(index)?;
        let meta = check_entry_metadata(&entry, index, limits)?;
        if !meta.is_dir {
            declared_total = declared_total
                .checked_add(meta.declared_size)
                .filter(|total| *total <= limits.max_total_uncompressed)
                .ok_or(PackageError::TotalTooLarge {
                    limit: limits.max_total_uncompressed,
                })?;
            compressed_total = compressed_total.saturating_add(meta.compressed_size);
        }
        register_path(&mut folded, &meta, limits)?;
        metas.push(meta);
    }
    check_ratio("<archive>", declared_total, compressed_total, limits)?;
    check_file_directory_conflicts(&metas)?;

    // Pass 2: bounded decompression + hashing. Nothing is written.
    let mut files = Vec::new();
    let mut directories = Vec::new();
    let mut texts = BTreeMap::<String, Vec<u8>>::new();
    let mut actual_total: u64 = 0;
    for meta in &metas {
        if meta.is_dir {
            directories.push(meta.path.clone());
            continue;
        }
        let keep_text =
            is_scanned_text(&meta.path) && meta.declared_size <= limits.max_scanned_text_bytes;
        let mut text = keep_text.then(Vec::new);
        let (size, sha256) = {
            let entry = archive.by_index(meta.index)?;
            bounded_copy(entry, meta, limits, text.as_mut(), &mut io::sink())?
        };
        actual_total = actual_total
            .checked_add(size)
            .filter(|total| *total <= limits.max_total_uncompressed)
            .ok_or(PackageError::TotalTooLarge {
                limit: limits.max_total_uncompressed,
            })?;
        if let Some(text) = text {
            texts.insert(meta.path.clone(), text);
        }
        files.push(CandidateEntry {
            path: meta.path.clone(),
            index: meta.index,
            size,
            sha256,
        });
    }

    let present: HashSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let manifest = validate_manifest(&texts, &present)?;
    for (name, bytes) in &texts {
        if let Some(reference) = find_remote_reference(name, bytes) {
            return Err(PackageError::RemoteReference {
                name: name.clone(),
                reference,
            });
        }
    }

    Ok((
        PackageCandidate {
            archive_sha256,
            files,
            directories,
            total_uncompressed: actual_total,
            manifest,
        },
        archive,
    ))
}

/// Validate, then extract into a **new** directory `staging`.
///
/// Nothing is written unless validation fully succeeds. Files are created with
/// `create_new` under paths built from validated components only; the zip
/// crate's own extraction helpers are not used. On any error during writing the
/// staging directory is removed.
pub fn stage_archive(
    path: &Path,
    staging: &Path,
    limits: &PackageLimits,
) -> Result<StagedPackage, PackageError> {
    let file = open_untrusted(path)?;
    let (candidate, mut archive) = inspect(file, limits)?;

    if staging.exists() {
        return Err(PackageError::StagingExists(staging.to_path_buf()));
    }
    fs::create_dir(staging)?;
    let result = write_staging(&mut archive, &candidate, staging, limits);
    match result {
        Ok(()) => Ok(StagedPackage {
            root: staging.to_path_buf(),
            candidate,
        }),
        Err(error) => {
            let _ = fs::remove_dir_all(staging);
            Err(error)
        }
    }
}

fn write_staging<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    candidate: &PackageCandidate,
    staging: &Path,
    limits: &PackageLimits,
) -> Result<(), PackageError> {
    for dir in &candidate.directories {
        fs::create_dir_all(join_validated(staging, dir))?;
    }
    for file in &candidate.files {
        let target = join_validated(staging, &file.path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        let meta = EntryMeta {
            index: file.index,
            path: file.path.clone(),
            is_dir: false,
            declared_size: file.size,
            compressed_size: 0,
        };
        let entry = archive.by_index(file.index)?;
        let (_, sha256) = bounded_copy(entry, &meta, limits, None, &mut out)?;
        out.flush()?;
        if sha256 != file.sha256 {
            return Err(PackageError::StagingHashMismatch {
                name: file.path.clone(),
            });
        }
    }
    Ok(())
}

/// Join a validated `/`-separated relative path component by component.
fn join_validated(root: &Path, relative: &str) -> PathBuf {
    let mut path = root.to_path_buf();
    for component in relative.split('/') {
        path.push(component);
    }
    path
}

fn open_untrusted(path: &Path) -> Result<File, PackageError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_SHARE_READ only: no other process may write or delete the
        // archive while it is validated and staged.
        options.share_mode(0x0000_0001);
    }
    Ok(options.open(path)?)
}

/// Check the size budget and magic bytes, and compute the archive SHA-256.
fn hash_and_check_envelope<R: Read + Seek>(
    reader: &mut R,
    limits: &PackageLimits,
) -> Result<[u8; 32], PackageError> {
    let size = reader.seek(SeekFrom::End(0))?;
    if size > limits.max_archive_bytes {
        return Err(PackageError::ArchiveTooLarge {
            size,
            limit: limits.max_archive_bytes,
        });
    }
    reader.seek(SeekFrom::Start(0))?;
    let mut magic = [0u8; 4];
    if reader.read_exact(&mut magic).is_err() || &magic != ZIP_LOCAL_MAGIC {
        return Err(PackageError::NotZip);
    }
    reader.seek(SeekFrom::Start(0))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut limited = reader.by_ref().take(limits.max_archive_bytes);
    loop {
        let n = limited.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    precheck_end_of_central_directory(reader, size, limits)?;
    reader.seek(SeekFrom::Start(0))?;
    Ok(hasher.finalize().into())
}

/// Read the End Of Central Directory record ourselves and apply the entry
/// budget **before** `ZipArchive` materializes every central record (it parses
/// all of them up front; see `central_directory_is_fully_parsed_before_any_count_check`).
/// ZIP64 is refused: no package within the default limits needs it.
fn precheck_end_of_central_directory<R: Read + Seek>(
    reader: &mut R,
    size: u64,
    limits: &PackageLimits,
) -> Result<(), PackageError> {
    const EOCD_LEN: u64 = 22;
    const MAX_COMMENT: u64 = u16::MAX as u64;
    if size < EOCD_LEN {
        return Err(PackageError::Malformed("archive shorter than EOCD".into()));
    }
    let tail_len = size.min(EOCD_LEN + MAX_COMMENT);
    reader.seek(SeekFrom::Start(size - tail_len))?;
    let mut tail = vec![0u8; usize::try_from(tail_len).unwrap_or(usize::MAX)];
    reader.read_exact(&mut tail)?;
    let eocd_start = (0..=tail.len() - EOCD_LEN as usize)
        .rev()
        .find(|&i| tail[i..].starts_with(b"PK\x05\x06"))
        .ok_or_else(|| PackageError::Malformed("no end of central directory".into()))?;
    let eocd = &tail[eocd_start..];
    let u16_at = |at: usize| u16::from_le_bytes([eocd[at], eocd[at + 1]]);
    let u32_at =
        |at: usize| u32::from_le_bytes([eocd[at], eocd[at + 1], eocd[at + 2], eocd[at + 3]]);
    let (disk, cd_disk) = (u16_at(4), u16_at(6));
    let (entries_disk, entries_total) = (u16_at(8), u16_at(10));
    let (cd_size, cd_offset) = (u32_at(12), u32_at(16));
    if entries_total == u16::MAX || cd_size == u32::MAX || cd_offset == u32::MAX {
        return Err(PackageError::Malformed(
            "ZIP64 archives are not supported".into(),
        ));
    }
    if disk != 0 || cd_disk != 0 || entries_disk != entries_total {
        return Err(PackageError::Malformed(
            "multi-disk archives are not supported".into(),
        ));
    }
    if usize::from(entries_total) > limits.max_entries {
        return Err(PackageError::TooManyEntries {
            count: usize::from(entries_total),
            limit: limits.max_entries,
        });
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct EntryMeta {
    index: usize,
    path: String,
    is_dir: bool,
    declared_size: u64,
    compressed_size: u64,
}

fn check_entry_metadata<R: Read>(
    entry: &zip::read::ZipFile<'_, R>,
    index: usize,
    limits: &PackageLimits,
) -> Result<EntryMeta, PackageError> {
    let display = String::from_utf8_lossy(entry.name_raw()).into_owned();

    // Extra fields first: the Unicode Path field (0x7075) has already replaced
    // `name_raw()` by the time we see it, so its presence must be fatal.
    for field in entry.extra_data_fields() {
        let header_id = match field {
            ExtraField::Zip64ExtendedInformation(_)
            | ExtraField::ExtendedTimestamp(_)
            | ExtraField::Ntfs(_)
            | ExtraField::DataStreamAlignment(_) => continue,
            ExtraField::Custom(custom) if ALLOWED_CUSTOM_EXTRA_IDS.contains(&custom.header_id) => {
                continue;
            }
            ExtraField::Custom(custom) => custom.header_id,
            ExtraField::UnicodePath(_) => 0x7075,
            ExtraField::UnicodeComment(_) => 0x6375,
            _ => 0xFFFF,
        };
        return Err(PackageError::ForbiddenExtraField {
            name: display,
            header_id,
        });
    }

    let (path, is_dir) = validate_entry_name(entry.name_raw(), limits).map_err(|reason| {
        PackageError::UnsafePath {
            name: display.clone(),
            reason,
        }
    })?;

    // Check the raw high 16 bits of the external attributes regardless of the
    // "version made by" host: the crate only trusts them for some hosts.
    let raw_type = (entry.external_attributes() >> 16) & S_IFMT;
    let crate_mode = entry.unix_mode().unwrap_or(0) & S_IFMT;
    for file_type in [raw_type, crate_mode] {
        match file_type {
            0 | S_IFREG if !is_dir => {}
            0 | S_IFDIR if is_dir => {}
            S_IFLNK => return Err(PackageError::Symlink { name: display }),
            mode => {
                return Err(PackageError::NotRegularFile {
                    name: display,
                    mode,
                });
            }
        }
    }
    if entry.is_symlink() {
        return Err(PackageError::Symlink { name: display });
    }
    if entry.encrypted() {
        return Err(PackageError::Encrypted { name: display });
    }
    let compression = entry.compression();
    if !matches!(
        compression,
        CompressionMethod::Stored | CompressionMethod::Deflated
    ) {
        return Err(PackageError::UnsupportedCompression {
            name: display,
            method: format!("{compression:?}"),
        });
    }

    let declared_size = entry.size();
    let compressed_size = entry.compressed_size();
    if is_dir {
        if declared_size != 0 || compressed_size > 2 {
            return Err(PackageError::SizeMismatch {
                name: display,
                declared: declared_size,
                actual: compressed_size,
            });
        }
    } else {
        if declared_size > limits.max_file_uncompressed {
            return Err(PackageError::EntryTooLarge {
                name: display,
                size: declared_size,
                limit: limits.max_file_uncompressed,
            });
        }
        if compression == CompressionMethod::Stored && declared_size != compressed_size {
            return Err(PackageError::SizeMismatch {
                name: display,
                declared: declared_size,
                actual: compressed_size,
            });
        }
        check_ratio(&display, declared_size, compressed_size, limits)?;
        if is_executable_name(&path) {
            return Err(PackageError::ExecutableContent { name: display });
        }
    }

    Ok(EntryMeta {
        index,
        path,
        is_dir,
        declared_size,
        compressed_size,
    })
}

fn check_ratio(
    name: &str,
    uncompressed: u64,
    compressed: u64,
    limits: &PackageLimits,
) -> Result<(), PackageError> {
    if uncompressed < limits.ratio_min_bytes {
        return Ok(());
    }
    let ratio = uncompressed / compressed.max(1);
    if ratio > limits.max_ratio {
        return Err(PackageError::RatioExceeded {
            name: name.to_owned(),
            ratio,
            limit: limits.max_ratio,
        });
    }
    Ok(())
}

/// Validate a raw entry name. Returns the normalized path (no trailing `/`)
/// and whether it names a directory.
pub fn validate_entry_name(
    raw: &[u8],
    limits: &PackageLimits,
) -> Result<(String, bool), PathRejection> {
    use PathRejection as P;
    if raw.is_empty() {
        return Err(P::Empty);
    }
    if raw.len() > limits.max_path_len {
        return Err(P::TooLong);
    }
    let name = std::str::from_utf8(raw).map_err(|_| P::NotUtf8)?;
    if name.chars().any(|c| c.is_control()) {
        return Err(P::ControlChar);
    }
    if name.starts_with("\\\\") || name.starts_with("//") {
        return Err(P::UncOrDevicePath);
    }
    if name.starts_with('/') || name.starts_with('\\') {
        return Err(P::Absolute);
    }
    if name.contains(':') {
        return Err(P::DriveOrColon);
    }
    if name.contains('\\') {
        return Err(P::Backslash);
    }
    if limits.ascii_paths_only && !name.is_ascii() {
        return Err(P::NonAscii);
    }
    let (body, is_dir) = match name.strip_suffix('/') {
        Some(body) => (body, true),
        None => (name, false),
    };
    if body.is_empty() {
        return Err(P::Empty);
    }
    let components: Vec<&str> = body.split('/').collect();
    if components.len() > limits.max_depth {
        return Err(P::TooDeep);
    }
    for component in &components {
        match *component {
            "" => return Err(P::EmptyComponent),
            "." => return Err(P::CurDir),
            ".." => return Err(P::ParentDir),
            _ => {}
        }
        if component.len() > limits.max_component_len {
            return Err(P::ComponentTooLong);
        }
        if component.contains(['<', '>', '"', '|', '?', '*', '~']) {
            return Err(P::ForbiddenChar);
        }
        if component.ends_with(['.', ' ']) || component.starts_with(' ') {
            return Err(P::TrailingDotOrSpace);
        }
        if is_reserved_device_name(component) {
            return Err(P::ReservedDeviceName);
        }
    }
    Ok((body.to_owned(), is_dir))
}

/// Windows reserved device names, matched on the part before the first `.`,
/// case-insensitively, including superscript-digit `COM¹` forms.
pub fn is_reserved_device_name(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$" | "CONFIG$"
    ) {
        return true;
    }
    let mut chars = stem.chars();
    let prefix: String = chars.by_ref().take(3).collect();
    if prefix == "COM" || prefix == "LPT" {
        let rest: Vec<char> = chars.collect();
        return matches!(rest.as_slice(), [c] if matches!(c, '0'..='9' | '¹' | '²' | '³'));
    }
    false
}

fn fold_key(path: &str, limits: &PackageLimits) -> String {
    if limits.ascii_paths_only {
        path.to_ascii_lowercase()
    } else {
        // Gap: no Unicode normalization (NFC/NFD) here; see the research doc.
        path.to_lowercase()
    }
}

fn register_path(
    folded: &mut BTreeMap<String, String>,
    meta: &EntryMeta,
    limits: &PackageLimits,
) -> Result<(), PackageError> {
    let key = fold_key(&meta.path, limits);
    if let Some(first) = folded.insert(key, meta.path.clone()) {
        return Err(PackageError::PathCollision {
            first,
            second: meta.path.clone(),
        });
    }
    Ok(())
}

/// A path cannot be a file and also the parent of another entry.
fn check_file_directory_conflicts(metas: &[EntryMeta]) -> Result<(), PackageError> {
    let files: BTreeMap<String, &str> = metas
        .iter()
        .filter(|m| !m.is_dir)
        .map(|m| (m.path.to_ascii_lowercase(), m.path.as_str()))
        .collect();
    for meta in metas {
        let mut prefix = String::new();
        let components: Vec<&str> = meta.path.split('/').collect();
        for component in &components[..components.len() - 1] {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(component);
            if let Some(file) = files.get(&prefix.to_ascii_lowercase()) {
                return Err(PackageError::PathCollision {
                    first: (*file).to_owned(),
                    second: meta.path.clone(),
                });
            }
        }
    }
    Ok(())
}

fn extension(path: &str) -> String {
    path.rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

fn is_executable_name(path: &str) -> bool {
    EXECUTABLE_EXTENSIONS.contains(&extension(path).as_str())
}

fn is_scanned_text(path: &str) -> bool {
    path == MANIFEST_NAME
        || matches!(
            extension(path).as_str(),
            "html" | "htm" | "xhtml" | "svg" | "js" | "mjs" | "cjs" | "css" | "json"
        )
}

fn is_executable_magic(head: &[u8]) -> bool {
    head.starts_with(b"MZ")
        || head.starts_with(b"\x7fELF")
        || head.starts_with(b"#!")
        || head.starts_with(&[0xCF, 0xFA, 0xED, 0xFE])
        || head.starts_with(&[0xCE, 0xFA, 0xED, 0xFE])
        || head.starts_with(&[0xFE, 0xED, 0xFA, 0xCF])
        || head.starts_with(&[0xCA, 0xFE, 0xBA, 0xBE])
}

/// Decompress one entry through a `take(declared + 1)` bound, hash it, and
/// optionally keep a copy (text scanning) and/or write it (`sink`).
fn bounded_copy<R: Read, W: Write>(
    entry: R,
    meta: &EntryMeta,
    limits: &PackageLimits,
    mut keep: Option<&mut Vec<u8>>,
    sink: &mut W,
) -> Result<(u64, [u8; 32]), PackageError> {
    let bound = meta
        .declared_size
        .min(limits.max_file_uncompressed)
        .saturating_add(1);
    let mut limited = entry.take(bound);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total: u64 = 0;
    loop {
        let n = match limited.read(&mut buf) {
            Ok(n) => n,
            Err(error) => {
                // The crate reports "larger than declared" and CRC failures as
                // InvalidData; both mean the entry lied about itself.
                return Err(PackageError::Malformed(format!("{}: {error}", meta.path)));
            }
        };
        if n == 0 {
            break;
        }
        if total == 0 && is_executable_magic(&buf[..n]) {
            return Err(PackageError::ExecutableContent {
                name: meta.path.clone(),
            });
        }
        total += n as u64;
        if total > meta.declared_size {
            return Err(PackageError::SizeMismatch {
                name: meta.path.clone(),
                declared: meta.declared_size,
                actual: total,
            });
        }
        hasher.update(&buf[..n]);
        if let Some(keep) = keep.as_deref_mut() {
            keep.extend_from_slice(&buf[..n]);
        }
        sink.write_all(&buf[..n])?;
    }
    if total != meta.declared_size {
        return Err(PackageError::SizeMismatch {
            name: meta.path.clone(),
            declared: meta.declared_size,
            actual: total,
        });
    }
    Ok((total, hasher.finalize().into()))
}

fn validate_manifest(
    texts: &BTreeMap<String, Vec<u8>>,
    present: &HashSet<&str>,
) -> Result<serde_json::Value, PackageError> {
    use serde_json::Value;
    let bytes = texts
        .get(MANIFEST_NAME)
        .ok_or(PackageError::MissingManifest)?;
    let manifest: Value = serde_json::from_slice(bytes)
        .map_err(|error| PackageError::InvalidManifest(error.to_string()))?;
    let object = manifest
        .as_object()
        .ok_or_else(|| PackageError::InvalidManifest("root is not an object".into()))?;
    if object.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
        return Err(PackageError::InvalidManifest(
            "unsupported schemaVersion".into(),
        ));
    }
    for field in ["kind", "id", "name", "version"] {
        if !object.get(field).is_some_and(Value::is_string) {
            return Err(PackageError::InvalidManifest(format!(
                "missing string field `{field}`"
            )));
        }
    }
    for field in ["requiredCapabilities", "optionalCapabilities"] {
        let Some(value) = object.get(field) else {
            continue;
        };
        let list = value
            .as_array()
            .ok_or_else(|| PackageError::InvalidManifest(format!("`{field}` is not an array")))?;
        for capability in list {
            let id = capability.as_str().ok_or_else(|| {
                PackageError::InvalidManifest(format!("`{field}` has a non-string item"))
            })?;
            if !KNOWN_CAPABILITIES.contains(&id) {
                return Err(PackageError::UnknownCapability(id.to_owned()));
            }
        }
    }
    let mut references = Vec::new();
    if let Some(tokens) = object.get("tokens").and_then(Value::as_str) {
        references.push(tokens);
    }
    if let Some(surfaces) = object.get("surfaces").and_then(Value::as_object) {
        for surface in surfaces.values() {
            match surface.get("entry").and_then(Value::as_str) {
                Some(entry) => references.push(entry),
                None => {
                    return Err(PackageError::InvalidManifest(
                        "surface without string `entry`".into(),
                    ));
                }
            }
        }
    }
    for section in ["assets", "layouts"] {
        if let Some(map) = object.get(section).and_then(Value::as_object) {
            references.extend(map.values().filter_map(Value::as_str));
        }
    }
    for reference in references {
        if scan::is_remote(reference) {
            return Err(PackageError::RemoteReference {
                name: MANIFEST_NAME.into(),
                reference: reference.to_owned(),
            });
        }
        if !present.contains(reference) {
            return Err(PackageError::MissingReferencedFile(reference.to_owned()));
        }
    }
    Ok(manifest)
}
