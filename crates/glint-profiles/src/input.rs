//! What the resolver reads: loaded themes, a provider that looks them up by
//! profile reference, and the host's restrictions.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use glint_schema::{
    ColorScheme, PackageOrigin, PackageRelease, Sha256Digest, ThemeManifest, ThemeRef, TokenFile,
};
use semver::Version;

/// A theme release as the resolver sees it: its identity, its validated
/// manifest and its parsed token file. The resolver only reads these
/// values; it never opens files, follows paths or runs anything a theme
/// ships.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ThemeInput {
    /// Identity of the installed release (key, origin, version, digest).
    pub release: PackageRelease,
    /// The release's validated `glint.json`.
    pub manifest: ThemeManifest,
    /// The release's parsed `tokens.json`, when the manifest names one.
    pub tokens: Option<TokenFile>,
}

/// [`ThemeInput::new`] was given a manifest that is not the release's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeInputError {
    message: String,
}

impl ThemeInputError {
    /// Human-readable detail.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ThemeInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ThemeInputError {}

impl ThemeInput {
    /// Bundle a release with its manifest and tokens.
    ///
    /// # Errors
    ///
    /// [`ThemeInputError`] when the manifest's `id` or `version` is not the
    /// release's.
    pub fn new(
        release: PackageRelease,
        manifest: ThemeManifest,
        tokens: Option<TokenFile>,
    ) -> Result<Self, ThemeInputError> {
        if manifest.id != release.key.id() || manifest.version != release.version {
            return Err(ThemeInputError {
                message: format!(
                    "manifest {}@{} does not belong to release {}@{}",
                    manifest.id,
                    manifest.version,
                    release.key.id(),
                    release.version
                ),
            });
        }
        Ok(Self {
            release,
            manifest,
            tokens,
        })
    }

    /// `id@version`, for messages.
    pub fn label(&self) -> String {
        format!("{}@{}", self.release.key.id(), self.release.version)
    }
}

/// Why a referenced theme cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Unavailable {
    /// No release with that id and version is installed.
    Missing,
    /// The release is installed but unusable (its store entry fails
    /// verification, its manifest or tokens no longer parse, ...).
    Corrupt {
        /// What the caller found wrong.
        detail: String,
    },
    /// The profile lock does not pin the theme (PRF-002).
    Unlocked,
    /// The profile lock pins another version of the theme.
    LockVersionMismatch {
        /// The pinned version.
        pinned: semver::Version,
    },
    /// The installed release has another digest than the lock pins.
    DigestMismatch {
        /// The pinned digest.
        pinned: Sha256Digest,
        /// The installed digest.
        found: Sha256Digest,
    },
    /// The installed release comes from another origin than the lock pins.
    OriginMismatch {
        /// The pinned origin.
        pinned: PackageOrigin,
        /// The installed origin.
        found: PackageOrigin,
    },
}

impl Unavailable {
    /// Stable kebab-case reason: `missing`, `corrupt`, `unlocked`,
    /// `lock-version-mismatch`, `digest-mismatch` or `origin-mismatch`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Unavailable::Missing => "missing",
            Unavailable::Corrupt { .. } => "corrupt",
            Unavailable::Unlocked => "unlocked",
            Unavailable::LockVersionMismatch { .. } => "lock-version-mismatch",
            Unavailable::DigestMismatch { .. } => "digest-mismatch",
            Unavailable::OriginMismatch { .. } => "origin-mismatch",
        }
    }
}

impl fmt::Display for Unavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unavailable::Missing => f.write_str("not installed"),
            Unavailable::Corrupt { detail } => write!(f, "corrupt: {detail}"),
            Unavailable::Unlocked => f.write_str("not pinned by the profile lock"),
            Unavailable::LockVersionMismatch { pinned } => {
                write!(f, "the profile lock pins version {pinned}")
            }
            Unavailable::DigestMismatch { pinned, found } => {
                write!(f, "installed digest {found} is not the pinned {pinned}")
            }
            Unavailable::OriginMismatch { pinned, found } => {
                write!(f, "installed origin {found} is not the pinned {pinned}")
            }
        }
    }
}

/// The outcome of looking a profile reference up.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ThemeLookup<'a> {
    /// The release is usable.
    Available(&'a ThemeInput),
    /// The release cannot be used.
    Unavailable(Unavailable),
}

/// Looks up the theme a profile reference names. Implementations decide
/// where themes come from (the daemon maps installed releases); the resolver
/// only asks.
pub trait ThemeProvider {
    /// The release for `theme` (exact id and version).
    fn lookup(&self, theme: &ThemeRef) -> ThemeLookup<'_>;
}

/// An in-memory [`ThemeProvider`]: loaded themes by id and version, plus
/// releases the caller found unusable.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThemeSet {
    themes: BTreeMap<(String, Version), ThemeInput>,
    corrupt: BTreeMap<(String, Version), String>,
}

impl ThemeSet {
    /// An empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add (or replace) a usable theme.
    pub fn insert(&mut self, theme: ThemeInput) {
        let key = (
            theme.release.key.id().to_owned(),
            theme.release.version.clone(),
        );
        self.corrupt.remove(&key);
        self.themes.insert(key, theme);
    }

    /// Builder form of [`ThemeSet::insert`].
    #[must_use]
    pub fn with(mut self, theme: ThemeInput) -> Self {
        self.insert(theme);
        self
    }

    /// Record that `id@version` is installed but unusable.
    pub fn mark_corrupt(&mut self, id: &str, version: Version, detail: impl Into<String>) {
        let key = (id.to_owned(), version);
        self.themes.remove(&key);
        self.corrupt.insert(key, detail.into());
    }

    /// The usable theme `id@version`, if any.
    pub fn get(&self, id: &str, version: &Version) -> Option<&ThemeInput> {
        self.themes.get(&(id.to_owned(), version.clone()))
    }

    /// Every usable theme, ordered by id and version.
    pub fn themes(&self) -> impl Iterator<Item = &ThemeInput> {
        self.themes.values()
    }
}

impl ThemeProvider for ThemeSet {
    fn lookup(&self, theme: &ThemeRef) -> ThemeLookup<'_> {
        let key = (theme.key.id().to_owned(), theme.version.clone());
        if let Some(detail) = self.corrupt.get(&key) {
            return ThemeLookup::Unavailable(Unavailable::Corrupt {
                detail: detail.clone(),
            });
        }
        match self.themes.get(&key) {
            Some(input) => ThemeLookup::Available(input),
            None => ThemeLookup::Unavailable(Unavailable::Missing),
        }
    }
}

/// Host accessibility and safety restrictions: the last layer, which always
/// wins over every theme and user value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct HostEnvironment {
    /// The host's preferred colour scheme; `None` uses the token source's
    /// default (dark when defined).
    pub color_scheme: Option<ColorScheme>,
    /// Reduced motion is on: every `motion.duration.*` token becomes `0ms`.
    pub reduced_motion: bool,
    /// Capability ids the host cannot offer (unsupported, denied, ...). A
    /// surface whose required capability is listed is blocked.
    pub unusable_capabilities: BTreeSet<String>,
}

impl HostEnvironment {
    /// No preference and no restriction.
    pub fn new() -> Self {
        Self::default()
    }

    /// With a preferred colour scheme.
    #[must_use]
    pub fn with_color_scheme(mut self, scheme: ColorScheme) -> Self {
        self.color_scheme = Some(scheme);
        self
    }

    /// With reduced motion on or off.
    #[must_use]
    pub fn with_reduced_motion(mut self, on: bool) -> Self {
        self.reduced_motion = on;
        self
    }

    /// With a capability the host cannot offer.
    #[must_use]
    pub fn with_unusable_capability(mut self, capability: impl Into<String>) -> Self {
        self.unusable_capabilities.insert(capability.into());
        self
    }
}
