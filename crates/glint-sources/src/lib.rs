//! Source-agnostic theme source model (SRC-001).
//!
//! A theme can come from the optional official registry, an independent Git
//! repository, a local `.glint` archive, a local theme directory or a direct
//! artifact URL ([`ThemeSource`]). Every source is resolved by a
//! [`PackageSource`] into the same thing: a [`StagedPackage`], which is a
//! `glint-package` [`PackageCandidate`] (validated by the one strict reader)
//! plus a [`SourceOrigin`] provenance record for the installer (INS-004). The
//! rest of Glint never needs to know where a package came from.
//!
//! # What is implemented
//!
//! [`LocalSource`] stages [`ThemeSource::LocalArchive`] and
//! [`ThemeSource::LocalDirectory`]. A local directory is packed with the
//! deterministic `glint-package` packer into a temporary archive and then
//! inspected exactly like any other archive, so it passes the same rules.
//! **No build ever runs**: a directory without its built output is refused
//! as a package (`GLINT_PACKAGE_MISSING_REFERENCED_FILE`), never "fixed" by
//! running `npm`, a `package.json` script or anything else.
//!
//! Registry, Git and artifact URL backends are deferred to M4; staging those
//! kinds returns [`SourceErrorCode::Unsupported`], never a fake success.
//!
//! # Errors
//!
//! Every failure is a [`SourceError`] with a stable [`SourceErrorCode`]
//! whose string form is `GLINT_SOURCE_*`. Package verdicts carry the
//! underlying `GLINT_PACKAGE_*` error ([`SourceError::package_error`]).
//!
//! # Example
//!
//! ```no_run
//! use glint_sources::{LocalSource, PackageSource, ThemeSource};
//!
//! let source = ThemeSource::LocalArchive { path: "midnight.glint".into() };
//! let staged = LocalSource::new().stage(&source, "staging".as_ref())?;
//! println!("{} from {}", staged.origin.package_id, staged.origin.locator);
//! # Ok::<(), glint_sources::SourceError>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod error;
mod local;

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub use error::{SourceError, SourceErrorCode};
pub use glint_package::PackageCandidate;
pub use local::LocalSource;

/// Where a theme comes from.
///
/// Values are produced by callers or by the source string parser (SRC-002);
/// constructing one never touches the filesystem or the network.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ThemeSource {
    /// A package from a named registry index, e.g. `official/mono` or
    /// `community/carbon@1.4.2`.
    Registry {
        /// Registry name (`official` is the bundled official registry).
        registry: String,
        /// Package name inside that registry.
        package: String,
        /// Exact requested version; `None` means the newest the index lists.
        version: Option<String>,
    },
    /// An independent Git repository over HTTPS.
    Git {
        /// Canonical `https://` repository URL.
        url: String,
        /// Branch, tag or commit (*proposed* `#ref:subpath` syntax).
        reference: Option<String>,
        /// Theme directory inside the repository (*proposed* syntax).
        subpath: Option<String>,
    },
    /// A local `.glint` archive.
    LocalArchive {
        /// Path to the archive file.
        path: PathBuf,
    },
    /// A local theme directory (already built; never built by Glint).
    LocalDirectory {
        /// Path to the directory holding `glint.json`.
        path: PathBuf,
    },
    /// A direct `https://` link to a `.glint` archive.
    ArtifactUrl {
        /// Canonical `https://` URL.
        url: String,
        /// Expected lowercase hex SHA-256 of the archive, if known.
        sha256: Option<String>,
    },
}

impl ThemeSource {
    /// The kind of this source.
    pub fn kind(&self) -> SourceKind {
        match self {
            ThemeSource::Registry { .. } => SourceKind::Registry,
            ThemeSource::Git { .. } => SourceKind::Git,
            ThemeSource::LocalArchive { .. } => SourceKind::LocalArchive,
            ThemeSource::LocalDirectory { .. } => SourceKind::LocalDirectory,
            ThemeSource::ArtifactUrl { .. } => SourceKind::ArtifactUrl,
        }
    }

    /// The local path of a local source.
    pub fn local_path(&self) -> Option<&Path> {
        match self {
            ThemeSource::LocalArchive { path } | ThemeSource::LocalDirectory { path } => Some(path),
            _ => None,
        }
    }
}

/// The kind of a [`ThemeSource`], without its data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SourceKind {
    /// [`ThemeSource::Registry`].
    Registry,
    /// [`ThemeSource::Git`].
    Git,
    /// [`ThemeSource::LocalArchive`].
    LocalArchive,
    /// [`ThemeSource::LocalDirectory`].
    LocalDirectory,
    /// [`ThemeSource::ArtifactUrl`].
    ArtifactUrl,
}

impl SourceKind {
    /// Every kind, in declaration order.
    pub const ALL: &'static [SourceKind] = &[
        SourceKind::Registry,
        SourceKind::Git,
        SourceKind::LocalArchive,
        SourceKind::LocalDirectory,
        SourceKind::ArtifactUrl,
    ];

    /// The stable name recorded in provenance, e.g. `local-archive`.
    pub const fn as_str(self) -> &'static str {
        match self {
            SourceKind::Registry => "registry",
            SourceKind::Git => "git",
            SourceKind::LocalArchive => "local-archive",
            SourceKind::LocalDirectory => "local-directory",
            SourceKind::ArtifactUrl => "artifact-url",
        }
    }

    /// True for sources read from the local filesystem.
    pub const fn is_local(self) -> bool {
        matches!(self, SourceKind::LocalArchive | SourceKind::LocalDirectory)
    }
}

impl fmt::Display for SourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Signing status recorded in provenance. Package signing is designed in
/// SEC-005; until then every package is [`SigningStatus::Unsigned`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SigningStatus {
    /// No signature was checked.
    Unsigned,
}

/// Provenance of a staged package, for the installer's records (INS-004).
///
/// Follows the source provenance list in the distribution design: source
/// kind, location, Git commit or registry identifier, package ID and
/// version, retrieval time, verified archive digest and signing status. The
/// install path is added by the installer.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SourceOrigin {
    /// The source kind.
    pub kind: SourceKind,
    /// Human-readable canonical location: the absolute local path, or the
    /// canonical URL or `registry/package` for remote kinds.
    pub locator: String,
    /// Canonical absolute path of a local source.
    pub local_path: Option<PathBuf>,
    /// Registry name for registry sources.
    pub registry: Option<String>,
    /// Pinned Git commit SHA for Git sources.
    pub git_commit: Option<String>,
    /// SHA-256 of the archive bytes that were validated (for a local
    /// directory, of the deterministic archive packed from it).
    pub archive_sha256: [u8; 32],
    /// `id` from the validated manifest.
    pub package_id: String,
    /// `version` from the validated manifest.
    pub package_version: String,
    /// When the package was read and staged.
    pub retrieved_at: SystemTime,
    /// Signature verification result.
    pub signing: SigningStatus,
}

impl SourceOrigin {
    /// Lowercase hexadecimal form of [`archive_sha256`](Self::archive_sha256).
    pub fn archive_sha256_hex(&self) -> String {
        to_hex(&self.archive_sha256)
    }
}

/// A validated package in private staging plus where it came from.
///
/// Dropping it removes the staging directory unless
/// [`PackageCandidate::keep_staging`] was called on [`candidate`](Self::candidate).
#[derive(Debug)]
#[non_exhaustive]
pub struct StagedPackage {
    /// The validated, extracted package.
    pub candidate: PackageCandidate,
    /// Provenance of the package.
    pub origin: SourceOrigin,
}

/// A resolver that turns a [`ThemeSource`] into a [`StagedPackage`].
///
/// The trait is source-agnostic: local, Git, HTTP and registry backends all
/// implement it, and every implementation must hand its bytes to the
/// `glint-package` reader so one rule set applies to every source.
pub trait PackageSource {
    /// Whether this resolver implements `kind`. [`stage`](Self::stage)
    /// returns [`SourceErrorCode::Unsupported`] for kinds it does not.
    fn supports(&self, kind: SourceKind) -> bool;

    /// Read `source` and stage a validated package in a new private
    /// directory under `staging_root`, which must already exist and be a real
    /// directory only Glint can write to.
    ///
    /// # Errors
    ///
    /// A [`SourceError`] with a stable `GLINT_SOURCE_*` code. On error nothing
    /// is left in `staging_root`.
    fn stage(
        &self,
        source: &ThemeSource,
        staging_root: &Path,
    ) -> Result<StagedPackage, SourceError>;
}

/// Lowercase hexadecimal encoding of a digest.
pub(crate) fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    out
}
