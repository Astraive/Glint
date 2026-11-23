//! Typed view of a validated `glint.json` theme manifest.

use std::collections::BTreeMap;

use semver::{Version, VersionReq};
use serde_json::{Map, Value};

use crate::compat::ManifestVersions;
use crate::error::ManifestWarning;

/// An accepted manifest together with the non-fatal diagnostics found while
/// parsing it, as returned by [`crate::parse_theme_manifest_with_diagnostics`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ParsedManifest {
    /// The validated manifest.
    pub manifest: ThemeManifest,
    /// Ignored, non-security-sensitive content, in document key order.
    pub warnings: Vec<ManifestWarning>,
}

impl ThemeManifest {
    /// The version-negotiation envelope of this manifest, for
    /// [`crate::check_versions`].
    pub fn versions(&self) -> ManifestVersions {
        ManifestVersions {
            schema_version: u64::from(self.schema_version),
            glint: self.compatibility.glint.clone(),
            sdk: self.compatibility.sdk.clone(),
            entrypoint_version: self
                .renderer
                .as_ref()
                .map(|r| u64::from(r.entrypoint_version)),
        }
    }
}

/// A `glint.json` theme manifest that passed [`crate::parse_theme_manifest`].
///
/// Every path field is a validated package-relative path (see the crate docs);
/// whether the referenced file actually exists is checked by the package
/// reader, not here.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ThemeManifest {
    /// Package format version; always [`crate::SUPPORTED_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Package kind; always [`PackageKind::Theme`] for this parser.
    pub kind: PackageKind,
    /// Package identifier, e.g. `dev.example.midnight`.
    pub id: String,
    /// Semantic version of this package release.
    pub version: Version,
    /// Display name.
    pub name: String,
    /// Optional one-line description.
    pub description: Option<String>,
    /// Package author.
    pub author: Author,
    /// License expression (an SPDX identifier is expected; not verified here).
    pub license: String,
    /// Glint/SDK version ranges and supported platforms.
    pub compatibility: Compatibility,
    /// Renderer contract; required when any surface is declared.
    pub renderer: Option<Renderer>,
    /// Capabilities the theme cannot run without.
    pub required_capabilities: Vec<String>,
    /// Capabilities the theme uses when available.
    pub optional_capabilities: Vec<String>,
    /// Path to the design-token file.
    pub tokens: Option<String>,
    /// Named static assets (wallpaper, preview, ...) to package-relative paths.
    pub assets: BTreeMap<String, String>,
    /// Named layouts to package-relative paths.
    pub layouts: BTreeMap<String, String>,
    /// Surface ID (e.g. `panel`, `custom:dev.example.clock`) to surface definition.
    pub surfaces: BTreeMap<String, Surface>,
}

/// Package kinds. Only `theme` exists in v1; `widget`, `icon-pack`,
/// `wallpaper-pack` and `profile` are possible future kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum PackageKind {
    /// A theme package (`"theme"`).
    Theme,
}

impl PackageKind {
    /// Every kind, in declaration order.
    pub const ALL: &'static [PackageKind] = &[PackageKind::Theme];

    /// The JSON spelling, e.g. `"theme"`.
    pub const fn as_str(self) -> &'static str {
        match self {
            PackageKind::Theme => "theme",
        }
    }

    /// The kind spelled `name`, if this crate knows it.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.as_str() == name)
    }
}

impl std::fmt::Display for PackageKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Package author metadata. Never dereferenced or contacted by Glint.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Author {
    /// Author display name.
    pub name: String,
    /// Optional contact e-mail.
    pub email: Option<String>,
    /// Optional `https://` home page.
    pub url: Option<String>,
}

/// Compatibility declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Compatibility {
    /// Supported Glint application versions.
    pub glint: VersionReq,
    /// Supported `@glint/sdk` versions, for programmable themes.
    pub sdk: Option<VersionReq>,
    /// Operating systems the author supports (non-empty, no duplicates).
    pub platforms: Vec<Platform>,
}

/// Operating systems a theme may declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Platform {
    /// `"windows"`
    Windows,
    /// `"linux"`
    Linux,
    /// `"macos"`
    Macos,
}

impl Platform {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "windows" => Some(Platform::Windows),
            "linux" => Some(Platform::Linux),
            "macos" => Some(Platform::Macos),
            _ => None,
        }
    }
}

/// Renderer contract for programmable surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Renderer {
    /// Renderer kind.
    pub kind: RendererKind,
    /// Surface entrypoint contract version (independent of `schemaVersion`).
    pub entrypoint_version: u32,
}

/// Renderer kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RendererKind {
    /// Compiled static HTML/CSS/JS loaded into an isolated web view (`"glint-web"`).
    GlintWeb,
}

/// One surface the theme provides.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Surface {
    /// Package-relative `.html` entry document.
    pub entry: String,
    /// Placement hints. The shape is owned by the surface/layout engine (SRF)
    /// and is kept as raw JSON data here; `glint-surfaces` validates it.
    pub placement: Option<Map<String, Value>>,
    /// Focus, dismissal and work-area behaviour. Like `placement`, the shape
    /// is owned by the surface engine (SRF-001) and is kept as raw JSON data
    /// here; `glint-surfaces` validates it.
    pub behavior: Option<Map<String, Value>>,
}
