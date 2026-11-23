//! Typed package errors with stable codes.

use std::fmt;
use std::io;
use std::path::PathBuf;

use glint_schema::ManifestError;

/// Stable, machine-readable package error codes.
///
/// The string form returned by [`ErrorCode::as_str`] (`GLINT_PACKAGE_*`) is
/// the stable contract: tools, tests and documentation match on it. Codes are
/// only ever added, never renamed or reused for a different meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ErrorCode {
    /// An operating-system I/O operation failed (disk full, permission
    /// denied, file vanished). Not a verdict about the package.
    Io,
    /// The source path is not a regular file.
    SourceNotFile,
    /// The file does not start with a ZIP local file header (`PK\x03\x04`).
    NotZip,
    /// The archive file is larger than `max_archive_bytes`.
    ArchiveTooLarge,
    /// The ZIP structure is inconsistent, truncated or lies about itself.
    Malformed,
    /// A valid but deliberately unsupported ZIP feature: ZIP64, multiple
    /// disks, archive comments, data descriptors or masked headers.
    UnsupportedArchiveFeature,
    /// More entries than `max_entries`.
    TooManyEntries,
    /// Two central-directory records share one exact name.
    DuplicateName,
    /// Two names collide after case folding, or one path is both a file and
    /// a parent directory.
    PathCollision,
    /// An entry name is absolute, escapes the root or is otherwise unsafe;
    /// see [`PathRejection`].
    UnsafePath,
    /// An entry is a symbolic link.
    Symlink,
    /// An entry is a device, FIFO, socket or other non-regular file.
    NotRegularFile,
    /// An entry is encrypted.
    Encrypted,
    /// An entry uses a compression method other than Stored or Deflate.
    UnsupportedCompression,
    /// An entry carries an extra field outside the allow-list (Unicode Path,
    /// PKWARE Unix link targets, AES and unknown IDs).
    ForbiddenExtraField,
    /// Two entries share compressed data (overlapping-file bomb).
    OverlappingEntries,
    /// One file exceeds `max_file_uncompressed`.
    EntryTooLarge,
    /// The files together exceed `max_total_uncompressed`.
    TotalTooLarge,
    /// The compression ratio exceeds `max_ratio`.
    RatioExceeded,
    /// Declared and actual uncompressed sizes disagree.
    SizeMismatch,
    /// A native executable or script (by extension or magic bytes).
    ExecutableContent,
    /// A scanned text file (HTML, SVG, CSS, JS, JSON) exceeds
    /// `max_scanned_text_bytes`.
    TextTooLarge,
    /// There is no `glint.json` at the package root.
    MissingManifest,
    /// `glint.json` is not valid UTF-8.
    ManifestNotUtf8,
    /// `glint.json` fails `glint-schema` validation; see
    /// [`PackageError::manifest_error`] for the `GLINT_MANIFEST_*` code.
    InvalidManifest,
    /// The manifest lists a capability Glint does not know.
    UnknownCapability,
    /// A path the manifest references is not a file in the archive.
    MissingReferencedFile,
    /// A UI file loads content from the network or a share.
    RemoteReference,
    /// The staging root is missing, not a directory or a link.
    StagingRootInvalid,
    /// Something unexpected already exists, or appeared, inside the private
    /// staging directory.
    StagingConflict,
}

impl ErrorCode {
    /// Every code, in declaration order.
    pub const ALL: &'static [ErrorCode] = &[
        ErrorCode::Io,
        ErrorCode::SourceNotFile,
        ErrorCode::NotZip,
        ErrorCode::ArchiveTooLarge,
        ErrorCode::Malformed,
        ErrorCode::UnsupportedArchiveFeature,
        ErrorCode::TooManyEntries,
        ErrorCode::DuplicateName,
        ErrorCode::PathCollision,
        ErrorCode::UnsafePath,
        ErrorCode::Symlink,
        ErrorCode::NotRegularFile,
        ErrorCode::Encrypted,
        ErrorCode::UnsupportedCompression,
        ErrorCode::ForbiddenExtraField,
        ErrorCode::OverlappingEntries,
        ErrorCode::EntryTooLarge,
        ErrorCode::TotalTooLarge,
        ErrorCode::RatioExceeded,
        ErrorCode::SizeMismatch,
        ErrorCode::ExecutableContent,
        ErrorCode::TextTooLarge,
        ErrorCode::MissingManifest,
        ErrorCode::ManifestNotUtf8,
        ErrorCode::InvalidManifest,
        ErrorCode::UnknownCapability,
        ErrorCode::MissingReferencedFile,
        ErrorCode::RemoteReference,
        ErrorCode::StagingRootInvalid,
        ErrorCode::StagingConflict,
    ];

    /// The stable string form, e.g. `GLINT_PACKAGE_UNSAFE_PATH`.
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Io => "GLINT_PACKAGE_IO",
            ErrorCode::SourceNotFile => "GLINT_PACKAGE_SOURCE_NOT_FILE",
            ErrorCode::NotZip => "GLINT_PACKAGE_NOT_ZIP",
            ErrorCode::ArchiveTooLarge => "GLINT_PACKAGE_ARCHIVE_TOO_LARGE",
            ErrorCode::Malformed => "GLINT_PACKAGE_MALFORMED",
            ErrorCode::UnsupportedArchiveFeature => "GLINT_PACKAGE_UNSUPPORTED_ARCHIVE_FEATURE",
            ErrorCode::TooManyEntries => "GLINT_PACKAGE_TOO_MANY_ENTRIES",
            ErrorCode::DuplicateName => "GLINT_PACKAGE_DUPLICATE_NAME",
            ErrorCode::PathCollision => "GLINT_PACKAGE_PATH_COLLISION",
            ErrorCode::UnsafePath => "GLINT_PACKAGE_UNSAFE_PATH",
            ErrorCode::Symlink => "GLINT_PACKAGE_SYMLINK",
            ErrorCode::NotRegularFile => "GLINT_PACKAGE_NOT_REGULAR_FILE",
            ErrorCode::Encrypted => "GLINT_PACKAGE_ENCRYPTED",
            ErrorCode::UnsupportedCompression => "GLINT_PACKAGE_UNSUPPORTED_COMPRESSION",
            ErrorCode::ForbiddenExtraField => "GLINT_PACKAGE_FORBIDDEN_EXTRA_FIELD",
            ErrorCode::OverlappingEntries => "GLINT_PACKAGE_OVERLAPPING_ENTRIES",
            ErrorCode::EntryTooLarge => "GLINT_PACKAGE_ENTRY_TOO_LARGE",
            ErrorCode::TotalTooLarge => "GLINT_PACKAGE_TOTAL_TOO_LARGE",
            ErrorCode::RatioExceeded => "GLINT_PACKAGE_RATIO_EXCEEDED",
            ErrorCode::SizeMismatch => "GLINT_PACKAGE_SIZE_MISMATCH",
            ErrorCode::ExecutableContent => "GLINT_PACKAGE_EXECUTABLE_CONTENT",
            ErrorCode::TextTooLarge => "GLINT_PACKAGE_TEXT_TOO_LARGE",
            ErrorCode::MissingManifest => "GLINT_PACKAGE_MISSING_MANIFEST",
            ErrorCode::ManifestNotUtf8 => "GLINT_PACKAGE_MANIFEST_NOT_UTF8",
            ErrorCode::InvalidManifest => "GLINT_PACKAGE_INVALID_MANIFEST",
            ErrorCode::UnknownCapability => "GLINT_PACKAGE_UNKNOWN_CAPABILITY",
            ErrorCode::MissingReferencedFile => "GLINT_PACKAGE_MISSING_REFERENCED_FILE",
            ErrorCode::RemoteReference => "GLINT_PACKAGE_REMOTE_REFERENCE",
            ErrorCode::StagingRootInvalid => "GLINT_PACKAGE_STAGING_ROOT_INVALID",
            ErrorCode::StagingConflict => "GLINT_PACKAGE_STAGING_CONFLICT",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why an entry name was rejected ([`ErrorCode::UnsafePath`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PathRejection {
    /// Empty name.
    Empty,
    /// Longer than `max_path_len`.
    TooLong,
    /// More components than `max_depth`.
    TooDeep,
    /// A component longer than `max_component_len`.
    ComponentTooLong,
    /// Not valid UTF-8.
    NotUtf8,
    /// Non-ASCII while `ascii_paths_only` is set.
    NonAscii,
    /// A control character or NUL.
    ControlChar,
    /// `\\server\share`, `//server/share`, `\\.\device`, `\\?\C:\x`.
    UncOrDevicePath,
    /// Leading `/` or `\`.
    Absolute,
    /// Any `:` (drive letters, `C:x`, NTFS alternate data streams).
    DriveOrColon,
    /// `\` anywhere; Windows would split there, POSIX would not.
    Backslash,
    /// A `..` component.
    ParentDir,
    /// A `.` component.
    CurDir,
    /// An empty component (`a//b`).
    EmptyComponent,
    /// `CON`, `NUL`, `COM1`, `LPT1`, `CONIN$` ... with any extension.
    ReservedDeviceName,
    /// A component with a leading space or a trailing `.` or space; Win32
    /// strips these and creates aliases.
    TrailingDotOrSpace,
    /// `< > " | ? * ~` (`~` blocks 8.3 short-name aliases such as `PROGRA~1`).
    ForbiddenChar,
}

/// Every way reading or writing a package can fail.
///
/// Each variant maps to exactly one stable [`ErrorCode`] through
/// [`PackageError::code`]. Match on the code (or the variant), never on the
/// [`Display`](fmt::Display) text, which may change.
#[derive(Debug)]
#[non_exhaustive]
pub enum PackageError {
    /// An I/O operation failed. `context` names the step.
    Io {
        /// The step that failed, e.g. `"copy source into staging"`.
        context: &'static str,
        /// The underlying error.
        source: io::Error,
    },
    /// The source path is not a regular file.
    SourceNotFile(PathBuf),
    /// The file does not start with `PK\x03\x04`.
    NotZip,
    /// The archive is larger than the limit.
    ArchiveTooLarge {
        /// Observed size in bytes (a lower bound if the copy was cut short).
        size: u64,
        /// The limit.
        limit: u64,
    },
    /// Structural problem; the detail is not stable.
    Malformed(String),
    /// A deliberately unsupported ZIP feature.
    UnsupportedArchiveFeature(&'static str),
    /// Too many entries.
    TooManyEntries {
        /// Declared or parsed count.
        count: usize,
        /// The limit.
        limit: usize,
    },
    /// Two central-directory records share one exact name.
    DuplicateName,
    /// Two paths collide.
    PathCollision {
        /// The path seen first.
        first: String,
        /// The colliding path.
        second: String,
    },
    /// An unsafe entry name.
    UnsafePath {
        /// The raw name, lossily decoded.
        name: String,
        /// The rule it broke.
        reason: PathRejection,
    },
    /// A symbolic link entry.
    Symlink {
        /// Entry name.
        name: String,
    },
    /// A device, FIFO, socket or other non-regular entry.
    NotRegularFile {
        /// Entry name.
        name: String,
        /// The Unix file-type bits found.
        mode: u32,
    },
    /// An encrypted entry.
    Encrypted {
        /// Entry name.
        name: String,
    },
    /// A compression method other than Stored or Deflate.
    UnsupportedCompression {
        /// Entry name.
        name: String,
        /// The method as reported by the parser.
        method: String,
    },
    /// An extra field outside the allow-list.
    ForbiddenExtraField {
        /// Entry name.
        name: String,
        /// The extra-field header ID.
        header_id: u16,
    },
    /// Entries share compressed data.
    OverlappingEntries,
    /// One file is too large.
    EntryTooLarge {
        /// Entry name.
        name: String,
        /// Declared or actual size.
        size: u64,
        /// The limit.
        limit: u64,
    },
    /// The files together are too large.
    TotalTooLarge {
        /// The limit.
        limit: u64,
    },
    /// The compression ratio is too high.
    RatioExceeded {
        /// Entry name, or `<archive>` for the archive-wide ratio.
        name: String,
        /// Observed ratio (integer division).
        ratio: u64,
        /// The limit.
        limit: u64,
    },
    /// Declared and actual sizes disagree.
    SizeMismatch {
        /// Entry name.
        name: String,
        /// Declared size.
        declared: u64,
        /// Actual size (or compressed size for Stored entries).
        actual: u64,
    },
    /// A native executable or script.
    ExecutableContent {
        /// Entry name.
        name: String,
    },
    /// A scanned text file is too large to scan.
    TextTooLarge {
        /// Entry name.
        name: String,
        /// Declared size.
        size: u64,
        /// The limit.
        limit: u64,
    },
    /// No root `glint.json`.
    MissingManifest,
    /// `glint.json` is not UTF-8.
    ManifestNotUtf8,
    /// `glint.json` fails schema validation.
    InvalidManifest(ManifestError),
    /// An unknown capability ID.
    UnknownCapability(String),
    /// A manifest-referenced path is not a file in the archive.
    MissingReferencedFile(String),
    /// A remote load in a UI file.
    RemoteReference {
        /// The file containing it.
        name: String,
        /// The offending value.
        reference: String,
    },
    /// The staging root is unusable.
    StagingRootInvalid(PathBuf),
    /// Something unexpected exists inside private staging.
    StagingConflict(PathBuf),
}

impl PackageError {
    /// The stable error code for this error.
    pub fn code(&self) -> ErrorCode {
        match self {
            PackageError::Io { .. } => ErrorCode::Io,
            PackageError::SourceNotFile(_) => ErrorCode::SourceNotFile,
            PackageError::NotZip => ErrorCode::NotZip,
            PackageError::ArchiveTooLarge { .. } => ErrorCode::ArchiveTooLarge,
            PackageError::Malformed(_) => ErrorCode::Malformed,
            PackageError::UnsupportedArchiveFeature(_) => ErrorCode::UnsupportedArchiveFeature,
            PackageError::TooManyEntries { .. } => ErrorCode::TooManyEntries,
            PackageError::DuplicateName => ErrorCode::DuplicateName,
            PackageError::PathCollision { .. } => ErrorCode::PathCollision,
            PackageError::UnsafePath { .. } => ErrorCode::UnsafePath,
            PackageError::Symlink { .. } => ErrorCode::Symlink,
            PackageError::NotRegularFile { .. } => ErrorCode::NotRegularFile,
            PackageError::Encrypted { .. } => ErrorCode::Encrypted,
            PackageError::UnsupportedCompression { .. } => ErrorCode::UnsupportedCompression,
            PackageError::ForbiddenExtraField { .. } => ErrorCode::ForbiddenExtraField,
            PackageError::OverlappingEntries => ErrorCode::OverlappingEntries,
            PackageError::EntryTooLarge { .. } => ErrorCode::EntryTooLarge,
            PackageError::TotalTooLarge { .. } => ErrorCode::TotalTooLarge,
            PackageError::RatioExceeded { .. } => ErrorCode::RatioExceeded,
            PackageError::SizeMismatch { .. } => ErrorCode::SizeMismatch,
            PackageError::ExecutableContent { .. } => ErrorCode::ExecutableContent,
            PackageError::TextTooLarge { .. } => ErrorCode::TextTooLarge,
            PackageError::MissingManifest => ErrorCode::MissingManifest,
            PackageError::ManifestNotUtf8 => ErrorCode::ManifestNotUtf8,
            PackageError::InvalidManifest(_) => ErrorCode::InvalidManifest,
            PackageError::UnknownCapability(_) => ErrorCode::UnknownCapability,
            PackageError::MissingReferencedFile(_) => ErrorCode::MissingReferencedFile,
            PackageError::RemoteReference { .. } => ErrorCode::RemoteReference,
            PackageError::StagingRootInvalid(_) => ErrorCode::StagingRootInvalid,
            PackageError::StagingConflict(_) => ErrorCode::StagingConflict,
        }
    }

    /// The `glint-schema` diagnostic behind [`ErrorCode::InvalidManifest`].
    pub fn manifest_error(&self) -> Option<&ManifestError> {
        match self {
            PackageError::InvalidManifest(error) => Some(error),
            _ => None,
        }
    }

    /// The path rule behind [`ErrorCode::UnsafePath`].
    pub fn path_rejection(&self) -> Option<PathRejection> {
        match self {
            PackageError::UnsafePath { reason, .. } => Some(*reason),
            _ => None,
        }
    }

    pub(crate) fn io(context: &'static str) -> impl FnOnce(io::Error) -> PackageError {
        move |source| PackageError::Io { context, source }
    }
}

impl fmt::Display for PackageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: ", self.code())?;
        match self {
            PackageError::Io { context, source } => write!(f, "{context}: {source}"),
            PackageError::SourceNotFile(path) => {
                write!(f, "{} is not a regular file", path.display())
            }
            PackageError::NotZip => f.write_str("not a ZIP archive (magic bytes)"),
            PackageError::ArchiveTooLarge { size, limit } => {
                write!(f, "archive is at least {size} bytes; the limit is {limit}")
            }
            PackageError::Malformed(detail) => write!(f, "malformed archive: {detail}"),
            PackageError::UnsupportedArchiveFeature(feature) => {
                write!(f, "unsupported archive feature: {feature}")
            }
            PackageError::TooManyEntries { count, limit } => {
                write!(f, "{count} entries; the limit is {limit}")
            }
            PackageError::DuplicateName => f.write_str("two entries share one name"),
            PackageError::PathCollision { first, second } => {
                write!(f, "{second:?} collides with {first:?}")
            }
            PackageError::UnsafePath { name, reason } => {
                write!(f, "unsafe entry name {name:?} ({reason:?})")
            }
            PackageError::Symlink { name } => write!(f, "{name:?} is a symbolic link"),
            PackageError::NotRegularFile { name, mode } => {
                write!(f, "{name:?} is not a regular file (type {mode:o})")
            }
            PackageError::Encrypted { name } => write!(f, "{name:?} is encrypted"),
            PackageError::UnsupportedCompression { name, method } => {
                write!(f, "{name:?} uses unsupported compression {method}")
            }
            PackageError::ForbiddenExtraField { name, header_id } => {
                write!(f, "{name:?} has forbidden extra field 0x{header_id:04x}")
            }
            PackageError::OverlappingEntries => f.write_str("entries share compressed data"),
            PackageError::EntryTooLarge { name, size, limit } => {
                write!(f, "{name:?} is {size} bytes; the limit is {limit}")
            }
            PackageError::TotalTooLarge { limit } => {
                write!(f, "total uncompressed size exceeds {limit} bytes")
            }
            PackageError::RatioExceeded { name, ratio, limit } => {
                write!(f, "{name:?} compression ratio {ratio} exceeds {limit}")
            }
            PackageError::SizeMismatch {
                name,
                declared,
                actual,
            } => write!(f, "{name:?} declares {declared} bytes but has {actual}"),
            PackageError::ExecutableContent { name } => {
                write!(f, "{name:?} is an executable or script")
            }
            PackageError::TextTooLarge { name, size, limit } => write!(
                f,
                "{name:?} is {size} bytes; scanned text files are limited to {limit}"
            ),
            PackageError::MissingManifest => f.write_str("no glint.json at the package root"),
            PackageError::ManifestNotUtf8 => f.write_str("glint.json is not valid UTF-8"),
            PackageError::InvalidManifest(error) => write!(f, "invalid glint.json: {error}"),
            PackageError::UnknownCapability(id) => write!(f, "unknown capability {id:?}"),
            PackageError::MissingReferencedFile(path) => {
                write!(
                    f,
                    "glint.json references {path:?}, which is not in the archive"
                )
            }
            PackageError::RemoteReference { name, reference } => {
                write!(f, "{name:?} loads remote content {reference:?}")
            }
            PackageError::StagingRootInvalid(path) => {
                write!(
                    f,
                    "staging root {} is not a usable directory",
                    path.display()
                )
            }
            PackageError::StagingConflict(path) => {
                write!(f, "unexpected entry in staging: {}", path.display())
            }
        }
    }
}

impl std::error::Error for PackageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PackageError::Io { source, .. } => Some(source),
            PackageError::InvalidManifest(error) => Some(error),
            _ => None,
        }
    }
}
