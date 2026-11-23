//! Package identity, origins and install-collision classification
//! (SCH-004, **proposed** ADR 0006:
//! `docs/adr/0006-package-identity-and-origin-collisions.md`).
//!
//! Three levels of identity:
//!
//! | Level | Type | Fields |
//! |---|---|---|
//! | Package key | [`PackageKey`] | `kind`, `id` |
//! | Installation | key + [`PackageOrigin`] | where the package came from |
//! | Release | [`PackageRelease`] | installation + `version` + [`Sha256Digest`] |
//!
//! [`classify_install`] applies the one-origin-per-key policy: a package key
//! is bound to a single origin, versions are immutable per origin, and the
//! same `id` from another origin is never treated as a trusted duplicate.
//! Everything here is pure; nothing touches the file system or network.

use std::fmt;
use std::str::FromStr;

use semver::Version;

use crate::manifest::PackageKind;
use crate::parse::{is_valid_id, path_problem};

/// Longest accepted origin URL, in bytes.
const MAX_ORIGIN_URL_LEN: usize = 2048;

/// Why an identity value was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum IdentityErrorKind {
    /// The package id is not a lowercase reverse-DNS style identifier.
    InvalidId,
    /// The origin is not one of the canonical forms, or is unsafe to store.
    InvalidOrigin,
    /// The digest is not `sha256:` followed by 64 lowercase hex digits.
    InvalidDigest,
}

/// A refused identity value: what kind of value and why (the message is not
/// stable).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityError {
    kind: IdentityErrorKind,
    message: String,
}

impl IdentityError {
    fn new(kind: IdentityErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// Which kind of value was refused.
    pub fn kind(&self) -> IdentityErrorKind {
        self.kind
    }

    /// Human-readable detail (not stable).
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for IdentityError {}

// ---------------------------------------------------------------- key

/// The name of a package: (`kind`, `id`). The version is not part of it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageKey {
    kind: PackageKind,
    id: String,
}

impl PackageKey {
    /// A key for `id` of the given kind.
    ///
    /// # Errors
    ///
    /// [`IdentityErrorKind::InvalidId`] when `id` does not follow the
    /// `glint.json` id rules (at least two dot-separated lowercase segments).
    pub fn new(kind: PackageKind, id: &str) -> Result<Self, IdentityError> {
        if !is_valid_id(id) {
            return Err(IdentityError::new(
                IdentityErrorKind::InvalidId,
                format!("{id:?} is not a package id"),
            ));
        }
        Ok(Self {
            kind,
            id: id.to_owned(),
        })
    }

    /// The package kind.
    pub fn kind(&self) -> PackageKind {
        self.kind
    }

    /// The package id, e.g. `dev.example.midnight`.
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl fmt::Display for PackageKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind, self.id)
    }
}

// ------------------------------------------------------------- origin

/// The four origin forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum OriginKind {
    /// A registry, identified by its index URL (`registry:https://...`).
    Registry,
    /// A direct Git repository install (`git:https://...[#subpath]`).
    Git,
    /// A direct `https` artifact download (`artifact:https://...`).
    Artifact,
    /// A local `.glint` file or directory import (`local`).
    Local,
}

impl OriginKind {
    fn prefix(self) -> &'static str {
        match self {
            OriginKind::Registry => "registry",
            OriginKind::Git => "git",
            OriginKind::Artifact => "artifact",
            OriginKind::Local => "local",
        }
    }
}

/// Where a package came from, in canonical form.
///
/// Two origins are the same exactly when their canonical strings
/// ([`PackageOrigin::as_str`]) are equal. Canonicalization only merges
/// spellings that certainly name the same resource (scheme and host case, one
/// trailing `/`); path case, percent-encoding and a `.git` suffix are kept, so
/// doubtful pairs stay different.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageOrigin {
    kind: OriginKind,
    canonical: String,
}

impl PackageOrigin {
    /// The single origin shared by every local file or directory import.
    pub fn local() -> Self {
        Self {
            kind: OriginKind::Local,
            canonical: "local".to_owned(),
        }
    }

    /// A registry origin from the registry's index URL.
    ///
    /// # Errors
    ///
    /// [`IdentityErrorKind::InvalidOrigin`] when `url` is not a safe `https` URL.
    pub fn registry(url: &str) -> Result<Self, IdentityError> {
        Ok(Self::remote(OriginKind::Registry, canonical_url(url)?))
    }

    /// A Git origin from a repository URL and an optional package subpath.
    ///
    /// # Errors
    ///
    /// [`IdentityErrorKind::InvalidOrigin`] when `url` is not a safe `https`
    /// URL or `subpath` is not a safe relative path.
    pub fn git(url: &str, subpath: Option<&str>) -> Result<Self, IdentityError> {
        let mut canonical = canonical_url(url)?;
        if let Some(subpath) = subpath {
            if let Some(problem) = path_problem(subpath) {
                return Err(invalid_origin(format!(
                    "Git subpath {subpath:?} is not a safe relative path: {problem}"
                )));
            }
            canonical.push('#');
            canonical.push_str(subpath);
        }
        Ok(Self::remote(OriginKind::Git, canonical))
    }

    /// A direct artifact download origin.
    ///
    /// # Errors
    ///
    /// [`IdentityErrorKind::InvalidOrigin`] when `url` is not a safe `https` URL.
    pub fn artifact(url: &str) -> Result<Self, IdentityError> {
        Ok(Self::remote(OriginKind::Artifact, canonical_url(url)?))
    }

    fn remote(kind: OriginKind, location: String) -> Self {
        Self {
            kind,
            canonical: format!("{}:{location}", kind.prefix()),
        }
    }

    /// Parse the textual form written in lock files, e.g.
    /// `registry:https://library.example/index` or `local`. The input is
    /// canonicalized, so `git:https://Host.example/r/` yields
    /// `git:https://host.example/r`.
    ///
    /// # Errors
    ///
    /// [`IdentityErrorKind::InvalidOrigin`] for anything else.
    pub fn parse(text: &str) -> Result<Self, IdentityError> {
        if text == "local" {
            return Ok(Self::local());
        }
        let Some((prefix, rest)) = text.split_once(':') else {
            return Err(invalid_origin(format!(
                "{text:?} is not an origin: expected local, registry:<url>, git:<url>[#subpath] \
                 or artifact:<url>"
            )));
        };
        match prefix {
            "registry" => Self::registry(rest),
            "artifact" => Self::artifact(rest),
            "git" => match rest.split_once('#') {
                Some((url, subpath)) => Self::git(url, Some(subpath)),
                None => Self::git(rest, None),
            },
            _ => Err(invalid_origin(format!(
                "unknown origin form {prefix:?}; expected local, registry, git or artifact"
            ))),
        }
    }

    /// The origin form.
    pub fn kind(&self) -> OriginKind {
        self.kind
    }

    /// The canonical text, used for comparison and in lock files.
    pub fn as_str(&self) -> &str {
        &self.canonical
    }
}

impl fmt::Display for PackageOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical)
    }
}

impl FromStr for PackageOrigin {
    type Err = IdentityError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

fn invalid_origin(message: impl Into<String>) -> IdentityError {
    IdentityError::new(IdentityErrorKind::InvalidOrigin, message)
}

/// Canonical `https://host[:port]/path` or a refusal. Conservative: only the
/// scheme and host are lowercased and one trailing `/` is dropped.
fn canonical_url(url: &str) -> Result<String, IdentityError> {
    if url.len() > MAX_ORIGIN_URL_LEN {
        return Err(invalid_origin("origin URL is too long"));
    }
    if url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(invalid_origin(
            "origin URL must not contain whitespace or control characters",
        ));
    }
    let Some(scheme_end) = url.find("://") else {
        return Err(invalid_origin(format!("{url:?} is not an https:// URL")));
    };
    if !url[..scheme_end].eq_ignore_ascii_case("https") {
        return Err(invalid_origin(format!(
            "{url:?} uses an unsupported scheme; only https:// origins are accepted"
        )));
    }
    if url.contains('?') || url.contains('#') {
        return Err(invalid_origin(
            "origin URL must not contain a query or fragment",
        ));
    }
    let rest = &url[scheme_end + 3..];
    let (authority, path) = match rest.find('/') {
        Some(slash) => (&rest[..slash], &rest[slash..]),
        None => (rest, ""),
    };
    if authority.contains('@') {
        return Err(invalid_origin(
            "origin URL must not contain credentials; they are never stored in identity or locks",
        ));
    }
    if authority.is_empty() || authority.starts_with(':') || authority.ends_with(':') {
        return Err(invalid_origin(format!("{url:?} has no host")));
    }
    if path
        .split('/')
        .any(|segment| segment == "." || segment == "..")
    {
        return Err(invalid_origin(
            "origin URL must not contain `.` or `..` segments",
        ));
    }
    let path = path.strip_suffix('/').unwrap_or(path);
    if path.ends_with('/') {
        return Err(invalid_origin(
            "origin URL must not end with an empty segment",
        ));
    }
    Ok(format!("https://{}{path}", authority.to_ascii_lowercase()))
}

// ------------------------------------------------------------- digest

/// A SHA-256 content digest in the canonical text form
/// `sha256:<64 lowercase hex digits>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Sha256Digest(String);

impl Sha256Digest {
    const PREFIX: &'static str = "sha256:";

    /// Parse the canonical text form. Uppercase hex and other algorithms are
    /// refused rather than normalized: lock files are machine-written.
    ///
    /// # Errors
    ///
    /// [`IdentityErrorKind::InvalidDigest`] for anything else.
    pub fn parse(text: &str) -> Result<Self, IdentityError> {
        let valid = text.strip_prefix(Self::PREFIX).is_some_and(|hex| {
            hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        });
        if valid {
            Ok(Self(text.to_owned()))
        } else {
            Err(IdentityError::new(
                IdentityErrorKind::InvalidDigest,
                format!("{text:?} is not a digest: expected sha256: and 64 lowercase hex digits"),
            ))
        }
    }

    /// A digest from the 32 raw bytes of a SHA-256 hash.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        let mut text = String::with_capacity(Self::PREFIX.len() + 64);
        text.push_str(Self::PREFIX);
        for byte in bytes {
            text.push_str(&format!("{byte:02x}"));
        }
        Self(text)
    }

    /// The canonical text form.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Sha256Digest {
    type Err = IdentityError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

// ------------------------------------------------------------ release

/// One immutable release: a package key, its origin, version and the digest
/// of the verified archive. This is what a profile lock entry pins.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct PackageRelease {
    /// Package key.
    pub key: PackageKey,
    /// Where the release came from.
    pub origin: PackageOrigin,
    /// Exact release version.
    pub version: Version,
    /// Digest of the verified archive.
    pub digest: Sha256Digest,
}

impl PackageRelease {
    /// Assemble a release from validated parts.
    pub fn new(
        key: PackageKey,
        origin: PackageOrigin,
        version: Version,
        digest: Sha256Digest,
    ) -> Self {
        Self {
            key,
            origin,
            version,
            digest,
        }
    }
}

/// The result of comparing a candidate release with what is installed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstallOutcome {
    /// The package key is not installed.
    New,
    /// This exact release (origin, version and digest) is already installed;
    /// installing it again is a no-op.
    AlreadyInstalled,
    /// Another version from the same origin; installed side by side, older
    /// versions are kept.
    AdditionalVersion,
    /// The same origin already provided this version with a different digest.
    /// Refused: versions are immutable and the installed release is kept.
    DigestConflict {
        /// Digest of the installed release.
        installed: Sha256Digest,
    },
    /// The package key is installed from a different origin. Refused unless
    /// the user explicitly replaces the origin; never a trusted duplicate,
    /// even when the digests are equal.
    OriginCollision {
        /// Origin of the installed package.
        installed: PackageOrigin,
    },
}

impl InstallOutcome {
    /// The stable code, e.g. `GLINT_IDENTITY_ORIGIN_COLLISION`.
    pub fn code(&self) -> &'static str {
        match self {
            InstallOutcome::New => "GLINT_IDENTITY_NEW",
            InstallOutcome::AlreadyInstalled => "GLINT_IDENTITY_ALREADY_INSTALLED",
            InstallOutcome::AdditionalVersion => "GLINT_IDENTITY_ADDITIONAL_VERSION",
            InstallOutcome::DigestConflict { .. } => "GLINT_IDENTITY_DIGEST_CONFLICT",
            InstallOutcome::OriginCollision { .. } => "GLINT_IDENTITY_ORIGIN_COLLISION",
        }
    }

    /// Whether the installer may proceed without an explicit user decision.
    /// `false` for [`InstallOutcome::DigestConflict`] and
    /// [`InstallOutcome::OriginCollision`].
    pub fn may_proceed(&self) -> bool {
        matches!(
            self,
            InstallOutcome::New
                | InstallOutcome::AlreadyInstalled
                | InstallOutcome::AdditionalVersion
        )
    }
}

/// Classify installing `candidate` next to the `installed` releases.
///
/// Versions are compared by SemVer precedence, so build metadata never makes
/// a republished version look new (`1.0.0+a` and `1.0.0+b` are the same
/// version). Releases of other package keys are ignored. If the installed set already
/// violates the one-origin-per-key invariant, any foreign origin is reported
/// as a collision.
pub fn classify_install(
    installed: &[PackageRelease],
    candidate: &PackageRelease,
) -> InstallOutcome {
    let same_key: Vec<&PackageRelease> = installed
        .iter()
        .filter(|release| release.key == candidate.key)
        .collect();
    if let Some(foreign) = same_key
        .iter()
        .find(|release| release.origin != candidate.origin)
    {
        return InstallOutcome::OriginCollision {
            installed: foreign.origin.clone(),
        };
    }
    match same_key
        .iter()
        .find(|release| same_precedence(&release.version, &candidate.version))
    {
        Some(release) if release.digest == candidate.digest => InstallOutcome::AlreadyInstalled,
        Some(release) => InstallOutcome::DigestConflict {
            installed: release.digest.clone(),
        },
        None if same_key.is_empty() => InstallOutcome::New,
        None => InstallOutcome::AdditionalVersion,
    }
}

/// Equal SemVer precedence: build metadata is ignored.
fn same_precedence(a: &Version, b: &Version) -> bool {
    a.major == b.major && a.minor == b.minor && a.patch == b.patch && a.pre == b.pre
}
