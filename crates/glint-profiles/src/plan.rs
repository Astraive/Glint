//! The resolved plan: one value per token, surface and wallpaper, each with
//! the layer that won.

use std::collections::BTreeMap;
use std::fmt;

use glint_schema::{ColorScheme, PackageRelease};
use glint_surfaces::{SurfaceDefinition, SurfacePlacement};
use serde_json::{Map, Value, json};

use crate::diagnostic::{Diagnostic, Severity};

/// The precedence layers, lowest first. A later layer wins over every
/// earlier one; `HostRestriction` always wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Layer {
    /// 1. The bundled default safe theme (`official.default`).
    Default,
    /// 2. The profile's base theme: tokens, wallpaper and surfaces.
    BaseTheme,
    /// 3. A per-surface override theme: replaces only the named surface.
    SurfaceOverride,
    /// 4. The user's token overrides and wallpaper choice.
    User,
    /// 5. Per-monitor placement and visibility overrides.
    Monitor,
    /// 6. Host accessibility and safety restrictions (reduced motion,
    ///    contrast, capability enforcement).
    HostRestriction,
}

impl Layer {
    /// Every layer, lowest precedence first.
    pub const ALL: &'static [Layer] = &[
        Layer::Default,
        Layer::BaseTheme,
        Layer::SurfaceOverride,
        Layer::User,
        Layer::Monitor,
        Layer::HostRestriction,
    ];

    /// The stable kebab-case name, e.g. `surface-override`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Layer::Default => "default",
            Layer::BaseTheme => "base-theme",
            Layer::SurfaceOverride => "surface-override",
            Layer::User => "user",
            Layer::Monitor => "monitor",
            Layer::HostRestriction => "host-restriction",
        }
    }
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One resolved token value.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResolvedToken {
    /// Normalised CSS text.
    pub value: String,
    /// The layer that supplied the value.
    pub layer: Layer,
    /// The theme release that supplied it, for `Default` and `BaseTheme`.
    pub theme: Option<PackageRelease>,
}

/// Whether a resolved surface may be shown.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SurfaceStatus {
    /// Nothing prevents it.
    Active,
    /// A host restriction blocks it: the host cannot offer `capability`,
    /// which the surface requires. No user or monitor override can show it.
    Blocked {
        /// The unusable required capability.
        capability: String,
    },
}

/// A per-monitor refinement of one surface.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MonitorOverride {
    /// Whether the surface is shown on that monitor (before host
    /// restrictions; see [`ResolvedSurface::status`]).
    pub visible: bool,
    /// The placement on that monitor, validated by `glint-surfaces`.
    pub placement: SurfacePlacement,
    /// `Monitor` when the override changed anything, else the surface's
    /// own layer.
    pub layer: Layer,
}

/// A surface that could not come from the layer the profile asked for and
/// fell back to a lower one (PRF-002). Always reported as a
/// `GLINT_RESOLVE_FALLBACK_TO_*` diagnostic too.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Fallback {
    /// The layer the profile asked for: `SurfaceOverride` or `BaseTheme`.
    pub requested: Layer,
    /// The theme that layer named, `id@version`.
    pub theme: String,
    /// Why it could not be used: an [`crate::Unavailable::as_str`] reason,
    /// `mismatch` (provider returned another release), `not-provided` (the
    /// theme does not declare the surface) or `invalid` (its definition
    /// fails validation).
    pub reason: String,
    /// The layer that supplied the surface instead: `BaseTheme` or
    /// `Default`.
    pub to: Layer,
}

/// One surface of the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResolvedSurface {
    /// Surface id, e.g. `dock` or `custom:dev.example.clock`.
    pub id: String,
    /// The layer whose theme implements the surface: `Default`, `BaseTheme`
    /// or `SurfaceOverride`.
    pub layer: Layer,
    /// The release that implements it. Its entrypoint is a path inside this
    /// release only; nothing is imported from any other package.
    pub theme: PackageRelease,
    /// The validated definition from that release's manifest.
    pub definition: SurfaceDefinition,
    /// Per-monitor refinements, by monitor key.
    pub monitors: BTreeMap<String, MonitorOverride>,
    /// Host restriction outcome.
    pub status: SurfaceStatus,
    /// Set when the surface fell back from the layer the profile asked for.
    pub fallback: Option<Fallback>,
}

/// Where the wallpaper comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum WallpaperSource {
    /// A private user file, relative to the profile directory.
    UserFile {
        /// Profile-relative path.
        path: String,
    },
    /// A named asset of a theme release.
    ThemeAsset {
        /// The release.
        theme: PackageRelease,
        /// Asset name.
        name: String,
        /// Package-relative path of the asset.
        path: String,
    },
}

/// The resolved wallpaper.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResolvedWallpaper {
    /// Where it comes from.
    pub source: WallpaperSource,
    /// The layer that chose it.
    pub layer: Layer,
}

/// The deterministic output of [`crate::resolve`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResolvedPlan {
    /// The profile id.
    pub profile: String,
    /// The colour scheme the token values are for.
    pub color_scheme: ColorScheme,
    /// Every resolved token, by name.
    pub tokens: BTreeMap<String, ResolvedToken>,
    /// Every resolved surface, by id.
    pub surfaces: BTreeMap<String, ResolvedSurface>,
    /// The wallpaper, if any layer provides one.
    pub wallpaper: Option<ResolvedWallpaper>,
    /// Findings in a fixed order: base theme, surfaces, tokens, wallpaper,
    /// monitors, restrictions.
    pub diagnostics: Vec<Diagnostic>,
}

fn release_json(release: &PackageRelease) -> Value {
    json!({
        "id": release.key.id(),
        "version": release.version.to_string(),
        "origin": release.origin.as_str(),
        "digest": release.digest.as_str(),
    })
}

impl ResolvedPlan {
    /// Whether any diagnostic has [`Severity::Error`].
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    /// The plan as JSON with a fixed shape and key order (object keys are
    /// sorted), for inspectors, golden fixtures and IPC. Proposed shape.
    pub fn to_json(&self) -> Value {
        let tokens: Map<String, Value> = self
            .tokens
            .iter()
            .map(|(name, token)| {
                let mut entry = json!({ "value": token.value, "layer": token.layer.as_str() });
                if let Some(theme) = &token.theme {
                    entry["theme"] = Value::String(theme.key.id().to_owned());
                }
                (name.clone(), entry)
            })
            .collect();
        let surfaces: Map<String, Value> = self
            .surfaces
            .iter()
            .map(|(id, surface)| {
                let monitors: Map<String, Value> = surface
                    .monitors
                    .iter()
                    .map(|(key, monitor)| {
                        (
                            key.clone(),
                            json!({
                                "visible": monitor.visible,
                                "layer": monitor.layer.as_str(),
                                "placement": serde_json::to_value(&monitor.placement)
                                    .unwrap_or(Value::Null),
                            }),
                        )
                    })
                    .collect();
                let mut entry = json!({
                    "layer": surface.layer.as_str(),
                    "theme": release_json(&surface.theme),
                    "entrypoint": surface.definition.entrypoint,
                    "placement": serde_json::to_value(&surface.definition.placement)
                        .unwrap_or(Value::Null),
                    "behavior": serde_json::to_value(&surface.definition.behavior)
                        .unwrap_or(Value::Null),
                    "monitors": monitors,
                });
                entry["status"] = match &surface.status {
                    SurfaceStatus::Active => json!({ "state": "active" }),
                    SurfaceStatus::Blocked { capability } => json!({
                        "state": "blocked",
                        "layer": Layer::HostRestriction.as_str(),
                        "capability": capability,
                    }),
                };
                if let Some(fallback) = &surface.fallback {
                    entry["fallback"] = json!({
                        "requested": fallback.requested.as_str(),
                        "theme": fallback.theme,
                        "reason": fallback.reason,
                        "to": fallback.to.as_str(),
                    });
                }
                (id.clone(), entry)
            })
            .collect();
        let wallpaper = match &self.wallpaper {
            None => Value::Null,
            Some(w) => match &w.source {
                WallpaperSource::UserFile { path } => {
                    json!({ "layer": w.layer.as_str(), "path": path })
                }
                WallpaperSource::ThemeAsset { theme, name, path } => json!({
                    "layer": w.layer.as_str(),
                    "theme": theme.key.id(),
                    "asset": name,
                    "path": path,
                }),
            },
        };
        let diagnostics: Vec<Value> = self
            .diagnostics
            .iter()
            .map(|d| {
                json!({
                    "code": d.code.as_str(),
                    "severity": d.severity.as_str(),
                    "pointer": d.pointer,
                    "surface": d.surface,
                })
            })
            .collect();
        json!({
            "profile": self.profile,
            "colorScheme": self.color_scheme.as_str(),
            "tokens": tokens,
            "surfaces": surfaces,
            "wallpaper": wallpaper,
            "diagnostics": diagnostics,
        })
    }
}
