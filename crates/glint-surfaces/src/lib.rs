//! The portable surface catalog and the surface definition parser
//! (SRF-001).
//!
//! A **surface** is a Glint-controlled region or window with a declared
//! role, content entrypoint and geometry policy; themes render its content.
//! This crate has no OS-specific code. It provides:
//!
//! - the **catalog** ([`StandardSurface`], [`CATALOG`]): the fourteen
//!   standard surface kinds with their stable IDs, required and optional
//!   capability IDs and placement/behaviour defaults, plus namespaced
//!   `custom:<namespace>.<name>` kinds ([`SurfaceKind`], [`CustomSurfaceId`]);
//! - the **parser** ([`SurfaceDefinition::parse`], [`surface_definitions`]):
//!   validates a manifest surface `{entry, placement?, behavior?}` into a
//!   [`SurfaceDefinition`] `{id, kind, entrypoint, placement, behavior,
//!   capabilities}`, with stable `GLINT_SURFACE_*` codes
//!   ([`SurfaceErrorCode`]) and RFC 6901 JSON Pointers relative to the
//!   manifest root, in the style of `glint-schema`;
//! - **TypeScript types** ([`typescript_module`]) for
//!   `packages/types/src/surfaces.ts`, generated from the same definitions
//!   and drift-checked by `tests/typegen.rs`.
//!
//! The `placement` and `behavior` formats are [`FORMAT_STATUS`] =
//! `"proposed"` until stabilized by tests and an ADR. The full contract is in
//! `docs/architecture/surfaces.md`.
//!
//! # Usage
//!
//! ```
//! let manifest = glint_schema::parse_theme_manifest(r#"{
//!     "schemaVersion": 1, "kind": "theme", "id": "dev.example.bar",
//!     "name": "Bar", "version": "0.1.0", "author": { "name": "Example" },
//!     "license": "MIT",
//!     "compatibility": { "glint": ">=0.1.0", "platforms": ["windows"] },
//!     "renderer": { "kind": "glint-web", "entrypointVersion": 1 },
//!     "surfaces": {
//!         "panel": {
//!             "entry": "dist/panel/index.html",
//!             "placement": { "anchor": "top", "height": 44 },
//!             "behavior": { "reserveWorkArea": true }
//!         }
//!     }
//! }"#)?;
//! let surfaces = glint_surfaces::surface_definitions(&manifest)?;
//! assert_eq!(surfaces[0].kind.as_str(), "panel");
//! assert_eq!(surfaces[0].capabilities.required[0].as_str(), "surface.panel");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Out of scope
//!
//! Host lifecycle and which placements a host supports (SRF-002), resolving
//! placements against monitors and work areas (SRF-003) and fallback to
//! default surfaces (SRF-004) build on these types.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Declares a closed string vocabulary: a `Copy` enum with `ALL`, `as_str`,
/// `from_name`, `Display` and string serialization.
macro_rules! vocab {
    (
        $(#[$meta:meta])*
        $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident => $text:literal, )+
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[non_exhaustive]
        pub enum $name {
            $( $(#[$vmeta])* $variant, )+
        }

        impl $name {
            /// Every value, in declaration order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub(crate) const WORDS: &'static [(&'static str, $name)] =
                &[$(($text, $name::$variant)),+];

            /// The stable string form used in manifests.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $text,)+
                }
            }

            /// Looks a value up by its string form.
            pub fn from_name(text: &str) -> Option<Self> {
                Self::WORDS.iter().find(|(word, _)| *word == text).map(|(_, value)| *value)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> ::core::result::Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }
    };
}

mod behavior;
mod catalog;
mod definition;
mod entrypoint;
mod error;
mod json;
mod placement;
mod typegen;

pub use behavior::{
    BEHAVIOR_FIELDS, DismissTrigger, FocusPolicy, MAX_AUTO_HIDE_MS, MIN_AUTO_HIDE_MS,
    SurfaceBehavior,
};
pub use catalog::{
    CATALOG, CUSTOM_PREFIX, CUSTOM_SURFACE_CAPABILITY, CatalogEntry, CustomSurfaceId,
    MAX_SURFACE_ID_LEN, StandardSurface, SurfaceKind,
};
pub use definition::{
    DEFINITION_FIELDS, SurfaceCapabilities, SurfaceDefinition, parse_surface_id,
    surface_definitions,
};
pub use entrypoint::MAX_ENTRYPOINT_LEN;
pub use error::{SurfaceError, SurfaceErrorCode};
pub use placement::{
    Anchor, MAX_LOGICAL_PX, MAX_MARGIN_PX, MonitorTarget, PLACEMENT_FIELDS, PlacementKind,
    SurfacePlacement,
};
pub use typegen::{TYPESCRIPT_PATH, typescript_module};

/// Stability label of the surface definition format (`placement`,
/// `behavior`, catalog defaults). It stays `"proposed"` until stabilized by
/// tests and an ADR.
pub const FORMAT_STATUS: &str = "proposed";
