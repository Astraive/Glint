//! The **proposed** profile composition resolver (PRF-001) and profile
//! locks with per-surface fallback (PRF-002).
//!
//! A profile ([`glint_schema::Profile`]) mixes themes: a base theme, other
//! themes for single surfaces, token overrides, a wallpaper and per-monitor
//! placement. [`resolve`] turns it into a [`ResolvedPlan`]: one value for
//! every token, surface and the wallpaper, each tagged with the [`Layer`]
//! that won, plus typed [`Diagnostic`]s with stable `GLINT_RESOLVE_*` codes
//! ([`ResolveCode`]) for everything it could not honour.
//!
//! The crate is pure and OS-free: no files, no storage, no installer. The
//! caller (the daemon, the only writer of profile state) loads the releases
//! and passes them in through a [`ThemeProvider`] such as [`ThemeSet`].
//!
//! # Precedence
//!
//! Lowest first; a later layer wins (`docs/architecture/profile-format.md`):
//!
//! 1. [`Layer::Default`]: the bundled default safe theme.
//! 2. [`Layer::BaseTheme`]: tokens, wallpaper and surfaces of the base theme.
//! 3. [`Layer::SurfaceOverride`]: an override theme replaces only the named
//!    surface. It contributes nothing else (no tokens, no other surfaces).
//! 4. [`Layer::User`]: token overrides (validated against the closed token
//!    registry) and the wallpaper choice.
//! 5. [`Layer::Monitor`]: per-monitor placement and visibility, validated by
//!    `glint-surfaces` on top of the theme's own placement.
//! 6. [`Layer::HostRestriction`]: reduced motion, contrast safety and
//!    capability enforcement. Always wins.
//!
//! An unavailable layer (missing or corrupt theme, undeclared or invalid
//! surface, invalid override) contributes nothing and is reported; the next
//! lower layer applies.
//!
//! # Locks and fallback (PRF-002)
//!
//! [`generate_lock`] pins every referenced theme to the exact installed
//! release (written deterministically by
//! [`glint_schema::ProfileLock::to_json_string`]); [`check_lock`] reports
//! drift between profile, lock and installed releases with stable
//! `GLINT_LOCK_*` codes ([`LockCode`]). Resolving through [`LockedThemes`]
//! only hands out pinned releases. When a referenced theme is missing,
//! corrupt or not the pinned release, each affected surface falls back to
//! the base theme, then the bundled default, and records a [`Fallback`]
//! with a `GLINT_RESOLVE_FALLBACK_TO_*` diagnostic: never silently.
//!
//! # Usage
//!
//! ```
//! use glint_profiles::{HostEnvironment, ThemeInput, ThemeSet, resolve};
//! use glint_schema::{PackageKey, PackageKind, PackageOrigin, PackageRelease, Sha256Digest};
//!
//! let manifest = glint_schema::parse_theme_manifest(r#"{
//!     "schemaVersion": 1, "kind": "theme", "id": "official.default",
//!     "name": "Default", "version": "0.1.0", "author": { "name": "Example" },
//!     "license": "MIT",
//!     "compatibility": { "glint": ">=0.0.0", "platforms": ["windows"] },
//!     "renderer": { "kind": "glint-web", "entrypointVersion": 1 },
//!     "surfaces": { "panel": { "entry": "surfaces/panel/index.html" } }
//! }"#)?;
//! let release = PackageRelease::new(
//!     PackageKey::new(PackageKind::Theme, "official.default")?,
//!     PackageOrigin::local(),
//!     manifest.version.clone(),
//!     Sha256Digest::from_bytes([0; 32]),
//! );
//! let default_theme = ThemeInput::new(release, manifest, None)?;
//! let profile = glint_schema::parse_profile(
//!     r#"{ "schemaVersion": 1, "id": "default", "name": "Default" }"#,
//! )?;
//! let plan = resolve(&profile, &ThemeSet::new(), &default_theme, &HostEnvironment::new());
//! assert_eq!(plan.surfaces["panel"].layer.as_str(), "default");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod diagnostic;
mod input;
mod lock;
mod plan;
mod resolve;

pub use diagnostic::{Diagnostic, ResolveCode, Severity};
pub use input::{
    HostEnvironment, ThemeInput, ThemeInputError, ThemeLookup, ThemeProvider, ThemeSet, Unavailable,
};
pub use lock::{LockCode, LockIssue, LockReport, LockedThemes, check_lock, generate_lock};
pub use plan::{
    Fallback, Layer, MonitorOverride, ResolvedPlan, ResolvedSurface, ResolvedToken,
    ResolvedWallpaper, SurfaceStatus, WallpaperSource,
};
pub use resolve::{REDUCED_MOTION_DURATION, WALLPAPER_ASSET, resolve};

/// Stability label of the resolver's precedence and plan shape. It stays
/// `"proposed"` until stabilized by tests and an ADR.
pub const FORMAT_STATUS: &str = "proposed";
