//! Strict, bounded `.glint` reader (PKG-001).
//!
//! Order of operations (each step must pass before the next one runs):
//!
//! 1. **Source envelope**, read from the caller's file without writing
//!    anything: it must be a regular file no larger than `max_archive_bytes`
//!    that starts with `PK\x03\x04`.
//! 2. **Private copy**: the source is streamed into a fresh Glint-owned
//!    staging directory while it is hashed (SHA-256). Every later step reads
//!    only this copy, so the caller's file can change without effect.
//! 3. **Directory pre-read**: the end-of-central-directory record and every
//!    central record are parsed by hand. ZIP64, multiple disks, archive
//!    comments, trailing data, data descriptors and masked headers are
//!    refused, and the entry budget is enforced *before* the `zip` crate
//!    materializes the directory.
//! 4. **Structure and metadata**: duplicate names, overlapping data, local
//!    and central header disagreement, extra fields, names, file types,
//!    encryption, methods, declared sizes, ratios, executables and
//!    case-folded collisions. Nothing is inflated.
//! 5. **Manifest**: `glint.json` is inflated (bounded), parsed with
//!    `glint-schema`, its capabilities are checked against
//!    [`KNOWN_CAPABILITIES`] and every path it references must be a file in
//!    the archive.
//! 6. **Extraction** into `<staging>/root`, one bounded entry at a time, with
//!    a SHA-256 per file and checks on the bytes actually produced (size,
//!    total, executable magic, remote references).
//! 7. The extracted tree and the private copy are made read-only.
//!
//! Any failure removes the whole staging directory, so a rejected archive
//! leaves nothing behind.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use glint_schema::ThemeManifest;
use sha2::{Digest, Sha256};
use zip::read::{ArchiveOffset, Config};
use zip::{CompressionMethod, ExtraField, ZipArchive};

use crate::error::PackageError;
use crate::limits::PackageLimits;
use crate::path::{
    MANIFEST_NAME, fold_key, is_executable_magic, is_executable_name, is_scanned_text,
    validate_entry_name,
};
use crate::scan;
use crate::staging::{
    self, EXTRACTED_DIR_NAME, SOURCE_COPY_NAME, StagingDir, create_private_dir, ensure_dirs,
};

/// Capability IDs Glint currently knows, from the SDK `CapabilityId`
/// proposal. An unknown capability is refused instead of being silently
/// ignored or granted. The capability registry is owned by the CAP epic; this
/// list is the reader's provisional copy until that registry exists.
pub const KNOWN_CAPABILITIES: &[&str] = &[
    "surface.panel",
    "surface.launcher",
    "surface.control-center",
    "system.audio.read",
    "system.audio.write",
    "desktop.workspaces.read",
    "desktop.workspaces.switch",
];

const ZIP_LOCAL_MAGIC: &[u8; 4] = b"PK\x03\x04";
const S_IFMT: u32 = 0o170_000;
const S_IFREG: u32 = 0o100_000;
const S_IFDIR: u32 = 0o040_000;
const S_IFLNK: u32 = 0o120_000;
const COPY_BUFFER: usize = 64 * 1024;

/// Extra fields that are inert for extraction, besides the ones the `zip`
/// crate parses itself (ZIP64, NTFS, extended timestamp, alignment).
/// Everything else is refused: `0x7075` (Info-ZIP Unicode Path) silently
/// renames entries inside the crate, `0x000d` (PKWARE Unix) can carry
/// symlink or hardlink targets, `0x9901` is AES encryption.
const ALLOWED_CUSTOM_EXTRA_IDS: &[u16] = &[
    0x7875, // Info-ZIP Unix UID/GID ("ux")
];

/// One extracted regular file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PackageFile {
    /// Normalized `/`-separated path relative to the package root.
    pub path: String,
    /// Uncompressed size in bytes.
    pub size: u64,
    /// SHA-256 of the uncompressed content.
    pub sha256: [u8; 32],
}

/// A fully validated package, extracted into an immutable private staging
/// directory.
///
/// The staging directory (private archive copy plus extracted tree) is
/// removed when the candidate is dropped, unless [`keep_staging`] was called;
/// the installer (INS) then takes ownership and moves it into the store.
///
/// [`keep_staging`]: PackageCandidate::keep_staging
#[derive(Debug)]
#[non_exhaustive]
pub struct PackageCandidate {
    /// The validated manifest.
    pub manifest: ThemeManifest,
    /// Root of the extracted, read-only package tree.
    pub extracted_root: PathBuf,
    /// SHA-256 of the archive bytes (of the private copy, which is what was
    /// validated).
    pub sha256: [u8; 32],
    /// Archive size in bytes.
    pub archive_size: u64,
    /// The private, read-only copy of the archive that was validated.
    pub source_copy: PathBuf,
    /// Every regular file, in central-directory order.
    pub files: Vec<PackageFile>,
    /// Explicit directory entries, in central-directory order.
    pub directories: Vec<String>,
    /// Sum of the files' uncompressed sizes.
    pub total_uncompressed: u64,
    staging: StagingDir,
}

impl PackageCandidate {
    /// Lowercase hexadecimal form of [`sha256`](Self::sha256).
    pub fn sha256_hex(&self) -> String {
        crate::to_hex(&self.sha256)
    }

    /// The staging directory holding [`source_copy`](Self::source_copy) and
    /// [`extracted_root`](Self::extracted_root).
    pub fn staging_dir(&self) -> &Path {
        self.staging.path()
    }

    /// Keep the staging directory after the candidate is dropped. The caller
    /// becomes responsible for moving or removing it.
    pub fn keep_staging(&mut self) {
        self.staging.keep();
    }
}

/// The staging root [`inspect_archive`] uses: `<temp>/glint-staging`.
pub fn default_staging_root() -> PathBuf {
    std::env::temp_dir().join("glint-staging")
}

/// Inspect a `.glint` archive and extract it into a private staging
/// directory under [`default_staging_root`].
///
/// Production callers (the installer) should prefer [`inspect_archive_in`]
/// with a directory inside Glint's own data directory.
///
/// # Errors
///
/// Returns a [`PackageError`] with a stable `GLINT_PACKAGE_*` code; on error
/// nothing is left in the staging root.
pub fn inspect_archive(
    path: &Path,
    limits: &PackageLimits,
) -> Result<PackageCandidate, PackageError> {
    let root = default_staging_root();
    match fs::create_dir(&root) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(source) => {
            return Err(PackageError::Io {
                context: "create default staging root",
                source,
            });
        }
    }
    inspect_archive_in(path, limits, &root)
}

/// Inspect a `.glint` archive and extract it into a new private directory
/// under `staging_root`, which must already exist and be a real directory
/// (not a link) that only Glint can write to.
///
/// # Errors
///
/// Returns a [`PackageError`] with a stable `GLINT_PACKAGE_*` code; on error
/// nothing is left in `staging_root` and nothing is written anywhere else.
pub fn inspect_archive_in(
    path: &Path,
    limits: &PackageLimits,
    staging_root: &Path,
) -> Result<PackageCandidate, PackageError> {
    check_staging_root(staging_root)?;
    let source = open_source(path, limits)?;
    let staging = StagingDir::create(staging_root)?;
    match stage(source, staging.path(), limits) {
        Ok(staged) => Ok(PackageCandidate {
            manifest: staged.manifest,
            extracted_root: staging.path().join(EXTRACTED_DIR_NAME),
            sha256: staged.sha256,
            archive_size: staged.archive_size,
            source_copy: staging.path().join(SOURCE_COPY_NAME),
            files: staged.files,
            directories: staged.directories,
            total_uncompressed: staged.total_uncompressed,
            staging,
        }),
        // Every handle opened by `stage` is closed by now, so the staging
        // directory can be removed (Windows refuses to delete open files).
        Err(error) => {
            drop(staging);
            Err(error)
        }
    }
}

fn check_staging_root(root: &Path) -> Result<(), PackageError> {
    if !staging::is_real_dir(root) {
        return Err(PackageError::StagingRootInvalid(root.to_path_buf()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // A group- or world-writable root (a shared /tmp directory planted by
        // another user) would let someone else tamper with staged files.
        let mode = fs::symlink_metadata(root)
            .map_err(PackageError::io("inspect staging root"))?
            .permissions()
            .mode();
        if mode & 0o022 != 0 {
            return Err(PackageError::StagingRootInvalid(root.to_path_buf()));
        }
    }
    Ok(())
}

/// Step 1: open the caller's file and check the envelope without writing.
fn open_source(path: &Path, limits: &PackageLimits) -> Result<File, PackageError> {
    let mut file = File::open(path).map_err(PackageError::io("open source archive"))?;
    let meta = file
        .metadata()
        .map_err(PackageError::io("inspect source archive"))?;
    if !meta.is_file() {
        return Err(PackageError::SourceNotFile(path.to_path_buf()));
    }
    if meta.len() > limits.max_archive_bytes {
        return Err(PackageError::ArchiveTooLarge {
            size: meta.len(),
            limit: limits.max_archive_bytes,
        });
    }
    let mut magic = [0u8; 4];
    if file.read_exact(&mut magic).is_err() || &magic != ZIP_LOCAL_MAGIC {
        return Err(PackageError::NotZip);
    }
    file.seek(SeekFrom::Start(0))
        .map_err(PackageError::io("rewind source archive"))?;
    Ok(file)
}

struct Staged {
    manifest: ThemeManifest,
    sha256: [u8; 32],
    archive_size: u64,
    files: Vec<PackageFile>,
    directories: Vec<String>,
    total_uncompressed: u64,
}

/// Steps 2-7. Owns every file handle so they are closed when it returns.
fn stage(source: File, staging: &Path, limits: &PackageLimits) -> Result<Staged, PackageError> {
    let (sha256, archive_size) = copy_source(source, staging, limits)?;
    let copy_path = staging.join(SOURCE_COPY_NAME);
    let mut copy = open_private_copy(&copy_path)?;
    precheck_directory(&mut copy, archive_size, limits)?;

    // Known(0): never let the crate "detect" an offset, which would accept
    // data prepended before the archive (polyglot EXE+ZIP files).
    let config = Config {
        archive_offset: ArchiveOffset::Known(0),
    };
    let mut archive = ZipArchive::with_config(config, BufReader::new(copy)).map_err(malformed)?;
    let metas = check_structure(&mut archive, limits)?;
    let manifest = read_manifest(&mut archive, &metas, limits)?;

    let root = staging.join(EXTRACTED_DIR_NAME);
    create_private_dir(&root).map_err(PackageError::io("create extraction root"))?;
    let (files, directories, total_uncompressed) = extract(&mut archive, &metas, &root, limits)?;
    drop(archive);
    staging::make_read_only(&root).map_err(PackageError::io("seal extracted tree"))?;

    Ok(Staged {
        manifest,
        sha256,
        archive_size,
        files,
        directories,
        total_uncompressed,
    })
}

/// Step 2: stream the source into the private copy while hashing it.
fn copy_source(
    source: File,
    staging: &Path,
    limits: &PackageLimits,
) -> Result<([u8; 32], u64), PackageError> {
    let out = staging::create_file(staging, SOURCE_COPY_NAME)?;
    let mut out = BufWriter::new(out);
    let mut limited = source.take(limits.max_archive_bytes.saturating_add(1));
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; COPY_BUFFER];
    let mut head = Vec::with_capacity(4);
    let mut total: u64 = 0;
    loop {
        let n = limited
            .read(&mut buf)
            .map_err(PackageError::io("read source archive"))?;
        if n == 0 {
            break;
        }
        total = total.saturating_add(n as u64);
        if total > limits.max_archive_bytes {
            return Err(PackageError::ArchiveTooLarge {
                size: total,
                limit: limits.max_archive_bytes,
            });
        }
        if head.len() < 4 {
            let take = (4 - head.len()).min(n);
            head.extend_from_slice(&buf[..take]);
        }
        hasher.update(&buf[..n]);
        out.write_all(&buf[..n])
            .map_err(PackageError::io("write private archive copy"))?;
    }
    // The source may have changed after the envelope check; the copy is
    // what counts.
    if head.as_slice() != ZIP_LOCAL_MAGIC {
        return Err(PackageError::NotZip);
    }
    let file = out.into_inner().map_err(|error| PackageError::Io {
        context: "flush private archive copy",
        source: error.into_error(),
    })?;
    file.sync_all()
        .map_err(PackageError::io("flush private archive copy"))?;
    drop(file);
    staging::make_read_only(&staging.join(SOURCE_COPY_NAME))
        .map_err(PackageError::io("seal private archive copy"))?;
    Ok((hasher.finalize().into(), total))
}

fn open_private_copy(path: &Path) -> Result<File, PackageError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_SHARE_READ only: nothing else may write or delete the copy
        // while it is validated and extracted.
        options.share_mode(0x0000_0001);
    }
    options
        .open(path)
        .map_err(PackageError::io("open private archive copy"))
}

fn malformed(error: zip::result::ZipError) -> PackageError {
    PackageError::Malformed(error.to_string())
}

fn read_u16(bytes: &[u8], at: usize) -> u16 {
    match bytes.get(at..at + 2) {
        Some(&[a, b]) => u16::from_le_bytes([a, b]),
        _ => 0,
    }
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    match bytes.get(at..at + 4) {
        Some(&[a, b, c, d]) => u32::from_le_bytes([a, b, c, d]),
        _ => 0,
    }
}

/// Step 3: parse the end-of-central-directory record and walk every central
/// record by hand, before `ZipArchive` parses (and allocates for) them.
fn precheck_directory<R: Read + Seek>(
    reader: &mut R,
    size: u64,
    limits: &PackageLimits,
) -> Result<(), PackageError> {
    const EOCD_LEN: usize = 22;
    const CENTRAL_LEN: usize = 46;
    const MAX_COMMENT: u64 = u16::MAX as u64;

    if size < EOCD_LEN as u64 {
        return Err(PackageError::Malformed("shorter than an end record".into()));
    }
    let tail_len = size.min(EOCD_LEN as u64 + MAX_COMMENT);
    let tail_start = size - tail_len;
    reader
        .seek(SeekFrom::Start(tail_start))
        .map_err(PackageError::io("read central directory"))?;
    let mut tail = vec![0u8; usize::try_from(tail_len).unwrap_or(EOCD_LEN)];
    reader
        .read_exact(&mut tail)
        .map_err(PackageError::io("read central directory"))?;
    let eocd_at = (0..=tail.len() - EOCD_LEN)
        .rev()
        .find(|&i| tail[i..].starts_with(b"PK\x05\x06"))
        .ok_or_else(|| PackageError::Malformed("no end of central directory record".into()))?;
    let eocd = &tail[eocd_at..];
    let eocd_pos = tail_start + eocd_at as u64;
    let comment_len = u64::from(read_u16(eocd, 20));
    if eocd_pos + EOCD_LEN as u64 + comment_len != size {
        return Err(PackageError::Malformed(
            "data after the end of central directory record".into(),
        ));
    }
    if comment_len != 0 {
        return Err(PackageError::UnsupportedArchiveFeature("archive comment"));
    }
    let (disk, cd_disk) = (read_u16(eocd, 4), read_u16(eocd, 6));
    let (entries_disk, entries_total) = (read_u16(eocd, 8), read_u16(eocd, 10));
    let (cd_size, cd_offset) = (read_u32(eocd, 12), read_u32(eocd, 16));
    let zip64_locator = eocd_at >= 20 && tail[eocd_at - 20..].starts_with(b"PK\x06\x07");
    if zip64_locator
        || entries_disk == u16::MAX
        || entries_total == u16::MAX
        || cd_size == u32::MAX
        || cd_offset == u32::MAX
    {
        return Err(PackageError::UnsupportedArchiveFeature("ZIP64"));
    }
    if disk != 0 || cd_disk != 0 || entries_disk != entries_total {
        return Err(PackageError::UnsupportedArchiveFeature("multiple disks"));
    }
    if usize::from(entries_total) > limits.max_entries {
        return Err(PackageError::TooManyEntries {
            count: usize::from(entries_total),
            limit: limits.max_entries,
        });
    }
    if u64::from(cd_offset) + u64::from(cd_size) != eocd_pos {
        return Err(PackageError::Malformed(
            "central directory does not end at the end record".into(),
        ));
    }

    reader
        .seek(SeekFrom::Start(u64::from(cd_offset)))
        .map_err(PackageError::io("read central directory"))?;
    let mut central = BufReader::new(reader.by_ref().take(u64::from(cd_size)));
    let mut consumed: u64 = 0;
    let mut header = [0u8; CENTRAL_LEN];
    for _ in 0..entries_total {
        central
            .read_exact(&mut header)
            .map_err(|_| PackageError::Malformed("truncated central directory".into()))?;
        if read_u32(&header, 0) != 0x0201_4b50 {
            return Err(PackageError::Malformed(
                "bad central directory signature".into(),
            ));
        }
        let flags = read_u16(&header, 8);
        let name_len = usize::from(read_u16(&header, 28));
        let rest = u64::from(read_u16(&header, 30)) + u64::from(read_u16(&header, 32));
        let mut name = vec![0u8; name_len];
        central
            .read_exact(&mut name)
            .map_err(|_| PackageError::Malformed("truncated central directory".into()))?;
        let skipped = io::copy(&mut central.by_ref().take(rest), &mut io::sink())
            .map_err(PackageError::io("read central directory"))?;
        if skipped != rest {
            return Err(PackageError::Malformed(
                "truncated central directory".into(),
            ));
        }
        consumed += (CENTRAL_LEN + name_len) as u64 + rest;
        let name = String::from_utf8_lossy(&name).into_owned();
        if flags & 0x0040 != 0 {
            return Err(PackageError::Encrypted { name });
        }
        if flags & 0x0008 != 0 {
            return Err(PackageError::UnsupportedArchiveFeature("data descriptor"));
        }
        if flags & 0x2000 != 0 {
            return Err(PackageError::UnsupportedArchiveFeature(
                "masked local headers",
            ));
        }
    }
    if consumed != u64::from(cd_size) {
        return Err(PackageError::Malformed(
            "central directory size disagrees with its records".into(),
        ));
    }
    drop(central);
    reader
        .seek(SeekFrom::Start(0))
        .map_err(PackageError::io("rewind private archive copy"))?;
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

/// Step 4: structure and per-entry metadata. Nothing is inflated.
fn check_structure<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    limits: &PackageLimits,
) -> Result<Vec<EntryMeta>, PackageError> {
    // `len()` counts unique names only; duplicates are flagged separately and
    // the crate keeps the *last* record for a duplicated name.
    if archive.has_duplicate_names() {
        return Err(PackageError::DuplicateName);
    }
    if archive.len() > limits.max_entries {
        return Err(PackageError::TooManyEntries {
            count: archive.len(),
            limit: limits.max_entries,
        });
    }
    if archive.has_overlapping_files().map_err(malformed)? {
        return Err(PackageError::OverlappingEntries);
    }
    // Not enforced by default in zip 9.8.1: local headers must match the
    // central directory and no local entry may be missing from it, or a
    // streaming extractor and this reader would see different files.
    archive
        .validate_header_consistency()
        .map_err(|error| PackageError::Malformed(format!("header consistency: {error}")))?;

    let mut metas = Vec::with_capacity(archive.len());
    let mut folded = BTreeMap::<String, String>::new();
    let mut declared_total: u64 = 0;
    let mut compressed_total: u64 = 0;
    for index in 0..archive.len() {
        let entry = archive.by_index_raw(index).map_err(malformed)?;
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
        let key = fold_key(&meta.path, limits);
        if let Some(first) = folded.insert(key, meta.path.clone()) {
            return Err(PackageError::PathCollision {
                first,
                second: meta.path,
            });
        }
        metas.push(meta);
    }
    check_ratio("<archive>", declared_total, compressed_total, limits)?;
    check_file_directory_conflicts(&metas, limits)?;
    Ok(metas)
}

fn check_entry_metadata<R: Read>(
    entry: &zip::read::ZipFile<'_, R>,
    index: usize,
    limits: &PackageLimits,
) -> Result<EntryMeta, PackageError> {
    let display = String::from_utf8_lossy(entry.name_raw()).into_owned();

    // Extra fields first: a Unicode Path field (0x7075) has already replaced
    // `name_raw()` by the time it is visible, so its presence must be fatal.
    if let Some(header_id) = entry.extra_data_fields().find_map(forbidden_extra_id) {
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

    // Check the raw high 16 bits of the external attributes whatever the
    // "version made by" host says: the crate trusts them only for some hosts.
    let raw_type = (entry.external_attributes() >> 16) & S_IFMT;
    let crate_type = entry.unix_mode().unwrap_or(0) & S_IFMT;
    for file_type in [raw_type, crate_type] {
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
        // An empty Deflate stream is two bytes.
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
        if is_scanned_text(&path) && declared_size > limits.max_scanned_text_bytes {
            return Err(PackageError::TextTooLarge {
                name: display,
                size: declared_size,
                limit: limits.max_scanned_text_bytes,
            });
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

fn forbidden_extra_id(field: &ExtraField) -> Option<u16> {
    match field {
        ExtraField::Zip64ExtendedInformation(_)
        | ExtraField::ExtendedTimestamp(_)
        | ExtraField::Ntfs(_)
        | ExtraField::DataStreamAlignment(_) => None,
        ExtraField::Custom(custom) if ALLOWED_CUSTOM_EXTRA_IDS.contains(&custom.header_id) => None,
        ExtraField::Custom(custom) => Some(custom.header_id),
        ExtraField::UnicodePath(_) => Some(0x7075),
        ExtraField::UnicodeComment(_) => Some(0x6375),
        _ => Some(0xFFFF),
    }
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

/// A path cannot be a file and also the parent of another entry.
fn check_file_directory_conflicts(
    metas: &[EntryMeta],
    limits: &PackageLimits,
) -> Result<(), PackageError> {
    let files: BTreeMap<String, &str> = metas
        .iter()
        .filter(|m| !m.is_dir)
        .map(|m| (fold_key(&m.path, limits), m.path.as_str()))
        .collect();
    for meta in metas {
        let mut end = 0;
        while let Some(offset) = meta.path[end..].find('/') {
            end += offset;
            if let Some(file) = files.get(&fold_key(&meta.path[..end], limits)) {
                return Err(PackageError::PathCollision {
                    first: (*file).to_owned(),
                    second: meta.path.clone(),
                });
            }
            end += 1;
        }
    }
    Ok(())
}

/// Step 5: read, parse and cross-check `glint.json`.
fn read_manifest<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    metas: &[EntryMeta],
    limits: &PackageLimits,
) -> Result<ThemeManifest, PackageError> {
    let meta = metas
        .iter()
        .find(|m| !m.is_dir && m.path == MANIFEST_NAME)
        .ok_or(PackageError::MissingManifest)?;
    let mut bytes = Vec::new();
    {
        let entry = archive.by_index(meta.index).map_err(malformed)?;
        bounded_copy(entry, meta, limits, Some(&mut bytes), &mut io::sink())?;
    }
    let text = String::from_utf8(bytes).map_err(|_| PackageError::ManifestNotUtf8)?;
    let manifest =
        glint_schema::parse_theme_manifest(&text).map_err(PackageError::InvalidManifest)?;

    for id in manifest
        .required_capabilities
        .iter()
        .chain(&manifest.optional_capabilities)
    {
        if !KNOWN_CAPABILITIES.contains(&id.as_str()) {
            return Err(PackageError::UnknownCapability(id.clone()));
        }
    }

    let files: BTreeSet<&str> = metas
        .iter()
        .filter(|m| !m.is_dir)
        .map(|m| m.path.as_str())
        .collect();
    for reference in manifest_references(&manifest) {
        if !files.contains(reference) {
            return Err(PackageError::MissingReferencedFile(reference.to_owned()));
        }
    }
    Ok(manifest)
}

/// Every package-relative path a manifest references.
pub(crate) fn manifest_references(manifest: &ThemeManifest) -> Vec<&str> {
    manifest
        .tokens
        .iter()
        .map(String::as_str)
        .chain(manifest.assets.values().map(String::as_str))
        .chain(manifest.layouts.values().map(String::as_str))
        .chain(manifest.surfaces.values().map(|s| s.entry.as_str()))
        .collect()
}

type Extracted = (Vec<PackageFile>, Vec<String>, u64);

/// Step 6: extract every entry, bounded, hashing as it goes.
fn extract<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    metas: &[EntryMeta],
    root: &Path,
    limits: &PackageLimits,
) -> Result<Extracted, PackageError> {
    let mut files = Vec::new();
    let mut directories = Vec::new();
    let mut actual_total: u64 = 0;
    for meta in metas {
        if meta.is_dir {
            ensure_dirs(root, &meta.path)?;
            directories.push(meta.path.clone());
            continue;
        }
        let mut text = is_scanned_text(&meta.path).then(Vec::new);
        let out = staging::create_file(root, &meta.path)?;
        let mut out = BufWriter::new(out);
        let (size, sha256) = {
            let entry = archive.by_index(meta.index).map_err(malformed)?;
            bounded_copy(entry, meta, limits, text.as_mut(), &mut out)?
        };
        out.flush().map_err(PackageError::io("write staged file"))?;
        drop(out);
        actual_total = actual_total
            .checked_add(size)
            .filter(|total| *total <= limits.max_total_uncompressed)
            .ok_or(PackageError::TotalTooLarge {
                limit: limits.max_total_uncompressed,
            })?;
        if let Some(text) = text
            && let Some(reference) = scan::find_remote_reference(&meta.path, &text)
        {
            return Err(PackageError::RemoteReference {
                name: meta.path.clone(),
                reference,
            });
        }
        files.push(PackageFile {
            path: meta.path.clone(),
            size,
            sha256,
        });
    }
    Ok((files, directories, actual_total))
}

/// Inflate one entry through a `take(declared + 1)` bound, hash it, and
/// optionally keep a copy and/or write it to `sink`.
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
    let mut buf = vec![0u8; COPY_BUFFER];
    let mut head = Vec::with_capacity(8);
    let mut total: u64 = 0;
    loop {
        // The crate reports "larger than declared" and CRC failures as
        // InvalidData; both mean the entry lied about itself.
        let n = limited
            .read(&mut buf)
            .map_err(|error| PackageError::Malformed(format!("{}: {error}", meta.path)))?;
        if n == 0 {
            break;
        }
        if head.len() < 8 {
            let take = (8 - head.len()).min(n);
            head.extend_from_slice(&buf[..take]);
            if head.len() >= 4 && is_executable_magic(&head) {
                return Err(PackageError::ExecutableContent {
                    name: meta.path.clone(),
                });
            }
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
        sink.write_all(&buf[..n])
            .map_err(PackageError::io("write staged file"))?;
    }
    if is_executable_magic(&head) {
        return Err(PackageError::ExecutableContent {
            name: meta.path.clone(),
        });
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
