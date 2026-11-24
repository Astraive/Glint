//! Typed theme operations (CLI-006): the wire types of the state-changing
//! requests `theme-import`, `theme-use`, `theme-remove`, `safe-mode` and the
//! store reads `theme-list` and `theme-export`.
//!
//! These are additive API version 1 request types (ADR 0010, evolution):
//! a server without them answers `GLINT_IPC_UNKNOWN_REQUEST`, one whose
//! host does not perform operations `GLINT_IPC_OPERATION_UNSUPPORTED`. The
//! server never touches state itself: it hands each request to the host's
//! [`Control::operate`](crate::Control::operate), and the core runs it on
//! the daemon, the only holder of the installer, so the single-writer rule
//! holds for every client (CLI, manager).
//!
//! A failed operation is an [`IpcError`](crate::IpcError) with code
//! `GLINT_IPC_OPERATION_FAILED` and an [`OperationFailure`] carrying the
//! component's own stable code (`GLINT_INSTALL_*`, `GLINT_PACKAGE_*`, ...)
//! and an [`OperationErrorKind`]; never a success body.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// One installed release, as `theme-list` and the operation replies report
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PackageInfo {
    /// Manifest `id`.
    pub id: String,
    /// Manifest `version`.
    pub version: String,
    /// Content digest (lowercase hex SHA-256 of the archive).
    pub digest: String,
    /// ADR 0006 origin (`local`, `registry:<url>`, ...).
    pub origin: String,
    /// Source kind recorded at install (`local-archive`, ...).
    pub source_kind: String,
    /// Canonical source location recorded at install.
    pub source_locator: String,
    /// Archive size in bytes.
    pub size: u64,
    /// Unix time of installation.
    pub installed_at: i64,
    /// Whether this is the bundled default theme.
    pub bundled: bool,
}

impl PackageInfo {
    /// A package description.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        version: impl Into<String>,
        digest: impl Into<String>,
        origin: impl Into<String>,
        source_kind: impl Into<String>,
        source_locator: impl Into<String>,
        size: u64,
        installed_at: i64,
        bundled: bool,
    ) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
            digest: digest.into(),
            origin: origin.into(),
            source_kind: source_kind.into(),
            source_locator: source_locator.into(),
            size,
            installed_at,
            bundled,
        }
    }
}

/// The active profile and the release its base theme uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ActiveInfo {
    /// The active profile.
    pub profile_id: String,
    /// The base theme's digest.
    pub digest: String,
    /// The profile revision.
    pub revision: u64,
}

impl ActiveInfo {
    /// An active profile.
    pub fn new(profile_id: impl Into<String>, digest: impl Into<String>, revision: u64) -> Self {
        Self {
            profile_id: profile_id.into(),
            digest: digest.into(),
            revision,
        }
    }
}

/// The answer to `theme-list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ThemeListReply {
    /// Every installed release, sorted by ID, version and digest.
    pub packages: Vec<PackageInfo>,
    /// The active profile, if any.
    pub active: Option<ActiveInfo>,
}

impl ThemeListReply {
    /// A reply.
    pub fn new(packages: Vec<PackageInfo>, active: Option<ActiveInfo>) -> Self {
        Self { packages, active }
    }
}

/// The answer to `theme-import`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ThemeImportReply {
    /// The installed release.
    pub package: PackageInfo,
    /// The digest was installed already; nothing changed.
    pub already_installed: bool,
    /// The active profile after the import (imports never activate).
    pub active: Option<ActiveInfo>,
}

impl ThemeImportReply {
    /// A reply.
    pub fn new(package: PackageInfo, already_installed: bool, active: Option<ActiveInfo>) -> Self {
        Self {
            package,
            already_installed,
            active,
        }
    }
}

/// The answer to `theme-use` and `safe-mode`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ThemeActivationReply {
    /// The release that is now the active base theme.
    pub package: PackageInfo,
    /// The new active profile.
    pub active: ActiveInfo,
    /// What was active before, if anything.
    pub previous: Option<ActiveInfo>,
    /// Activated in the `safe-mode` profile.
    pub safe_mode: bool,
}

impl ThemeActivationReply {
    /// A reply.
    pub fn new(
        package: PackageInfo,
        active: ActiveInfo,
        previous: Option<ActiveInfo>,
        safe_mode: bool,
    ) -> Self {
        Self {
            package,
            active,
            previous,
            safe_mode,
        }
    }
}

/// The answer to `theme-remove`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ThemeRemoveReply {
    /// Manifest `id`.
    pub id: String,
    /// Manifest `version`.
    pub version: String,
    /// The removed release's digest.
    pub digest: String,
}

impl ThemeRemoveReply {
    /// A reply.
    pub fn new(
        id: impl Into<String>,
        version: impl Into<String>,
        digest: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
            digest: digest.into(),
        }
    }
}

/// The answer to `theme-export`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ThemeExportReply {
    /// Manifest `id`.
    pub id: String,
    /// Manifest `version`.
    pub version: String,
    /// The exported release's digest.
    pub installed_digest: String,
    /// Where the archive was written (the absolute destination asked for).
    pub path: String,
    /// Lowercase hex SHA-256 of the written archive.
    pub digest: String,
    /// Its size in bytes.
    pub size: u64,
    /// Files in it.
    pub files: u64,
}

impl ThemeExportReply {
    /// A reply.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        version: impl Into<String>,
        installed_digest: impl Into<String>,
        path: impl Into<String>,
        digest: impl Into<String>,
        size: u64,
        files: u64,
    ) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
            installed_digest: installed_digest.into(),
            path: path.into(),
            digest: digest.into(),
            size,
            files,
        }
    }

    /// True when the export is byte-identical to the installed archive.
    pub fn is_canonical(&self) -> bool {
        self.digest == self.installed_digest
    }
}

/// What kind of failure an operation hit. Clients map it to their own
/// classes (the CLI to its exit codes) without parsing component codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum OperationErrorKind {
    /// A named theme, file or directory does not exist.
    NotFound,
    /// The request's arguments are unusable (an ambiguous theme ID, a
    /// relative path, an export destination that is a directory).
    InvalidRequest,
    /// A package or manifest failed validation.
    InvalidPackage,
    /// The OS refused access.
    PermissionDenied,
    /// The operation is not supported here.
    Unsupported,
    /// Anything else (a release in use, an identity conflict, I/O).
    Failure,
}

impl OperationErrorKind {
    /// A stable kebab-case name.
    pub const fn as_str(self) -> &'static str {
        match self {
            OperationErrorKind::NotFound => "not-found",
            OperationErrorKind::InvalidRequest => "invalid-request",
            OperationErrorKind::InvalidPackage => "invalid-package",
            OperationErrorKind::PermissionDenied => "permission-denied",
            OperationErrorKind::Unsupported => "unsupported",
            OperationErrorKind::Failure => "failure",
        }
    }
}

impl fmt::Display for OperationErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why an operation failed: the component's most specific stable code, its
/// kind and machine-readable context (the code chain, candidates, the
/// profiles using a release, a JSON Pointer).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OperationFailure {
    /// The most specific stable code (`GLINT_INSTALL_*`,
    /// `GLINT_IDENTITY_*`, `GLINT_PACKAGE_*`, `GLINT_MANIFEST_*`,
    /// `GLINT_SOURCE_*`, `GLINT_STORAGE_*`, `GLINT_DAEMON_*`).
    pub code: String,
    /// The failure class.
    pub kind: OperationErrorKind,
    /// Context: `installCode`, `packageCode`, `sourceCode`, `candidates`,
    /// `profiles`, `pointer`, ...
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, serde_json::Value>,
}

impl OperationFailure {
    /// A failure without details.
    pub fn new(code: impl Into<String>, kind: OperationErrorKind) -> Self {
        Self {
            code: code.into(),
            kind,
            details: BTreeMap::new(),
        }
    }

    /// Add one detail.
    #[must_use]
    pub fn with_detail(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }
}
