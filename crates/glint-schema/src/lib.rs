//! Types and a strict parser for the **proposed** `glint.json` v1 theme
//! manifest (SCH-001).
//!
//! The format is labelled [`FORMAT_STATUS`] = `"proposed"`: it follows the
//! example root manifest in `docs/plan/04-themes/manifest-and-glint-package-spec.md`
//! and may change until it is stabilized by tests and an ADR. The matching
//! JSON Schema is published at `schemas/glint-theme-v1.schema.json`; the
//! parser in this crate is the authority when the two disagree, and tests
//! keep their field sets aligned.
//!
//! # Usage
//!
//! ```
//! let manifest = glint_schema::parse_theme_manifest(r#"{
//!     "schemaVersion": 1,
//!     "kind": "theme",
//!     "id": "dev.example.minimal",
//!     "name": "Minimal",
//!     "version": "0.1.0",
//!     "author": { "name": "Example Developer" },
//!     "license": "MIT",
//!     "compatibility": { "glint": ">=0.1.0", "platforms": ["windows"] }
//! }"#)?;
//! assert_eq!(manifest.id, "dev.example.minimal");
//! # Ok::<(), glint_schema::ManifestError>(())
//! ```
//!
//! # Diagnostics
//!
//! Every rejection is a [`ManifestError`] carrying a stable [`ErrorCode`]
//! (string form `GLINT_MANIFEST_*`) and an RFC 6901 JSON Pointer to the
//! offending value. The parser stops at the first problem. Checks run in a
//! fixed order: size, JSON syntax and duplicate keys, root type,
//! `schemaVersion`, `kind`, unknown top-level fields, `id`, then the
//! remaining fields.
//!
//! # Decisions where the plan is ambiguous
//!
//! These are conservative choices, recorded here until an ADR settles them:
//!
//! - **Capabilities** are the plan's two top-level arrays
//!   `requiredCapabilities` and `optionalCapabilities` (there is no
//!   `capabilities` object). IDs must be `<namespace>.<name>[.<name>...]` with
//!   namespace `surface`, `system`, `desktop`, `native` or `host`, lowercase
//!   segments, no duplicates, and an ID may not be both required and optional.
//! - **Unknown fields are rejected in every security-sensitive section**
//!   (root, `compatibility`, `renderer`, each surface): an older reader that
//!   ignored them could silently drop something a newer reader enforces or
//!   grants. `integrations`, `variants`, `dependencies`, `scripts` and
//!   `migrations` are therefore *not* accepted in v1. Unknown keys inside
//!   `author` (display metadata Glint never acts on) are ignored and reported
//!   as [`WarningCode::UnknownField`] by
//!   [`parse_theme_manifest_with_diagnostics`] (SCH-003). `$schema` is
//!   accepted as an editor hint and is never dereferenced. Surface
//!   `placement` is the one open object: its shape is owned by the
//!   surface/layout engine (SRF) and is kept as raw JSON. The full table is
//!   in `docs/architecture/manifest-compatibility.md`.
//! - **Version negotiation** ([`negotiate_theme_manifest`],
//!   [`check_compatibility`]) compares `schemaVersion`,
//!   `compatibility.glint`, `compatibility.sdk` and
//!   `renderer.entrypointVersion` independently and reports every failing
//!   axis as `theme-incompatible` with a stable [`IncompatibilityCode`].
//!   Packages are never migrated and package-supplied scripts never run.
//! - **Required fields**: `schemaVersion`, `kind`, `id`, `name`, `version`,
//!   `author` (object with `name`), `license` and `compatibility` (with
//!   `glint` and a non-empty `platforms` list). Everything else is optional,
//!   so a declarative theme needs no renderer or surfaces. `renderer` becomes
//!   required as soon as any surface is declared.
//! - **Only `kind: "theme"`**, `renderer.kind: "glint-web"` and
//!   `renderer.entrypointVersion: 1` are accepted; future kinds are rejected
//!   with [`ErrorCode::WrongKind`] by this theme parser.
//! - **`id`** is at least two dot-separated segments of `a-z`, `0-9` and inner
//!   hyphens, at most 128 characters (e.g. `dev.example.midnight`,
//!   `official.mono`).
//! - **`version`** is strict SemVer 2.0 (`1.0`, `v1.0.0` are rejected);
//!   `compatibility.glint`/`sdk` are Cargo-style semver requirements
//!   (`>=0.1.0`, `^0.1.0`).
//! - **Paths** (`tokens`, `assets.*`, `layouts.*`, `surfaces.*.entry`) must be
//!   relative, `/`-separated, with no empty, `.` or `..` segments, no `\`, no
//!   `:` (drive letters, URL schemes, NTFS streams), no Windows-reserved
//!   characters or device names, and no segment ending in `.` or space.
//!   Surface entries must be `.html`/`.htm` documents. Existence of the
//!   referenced files is the package reader's job (PKG).
//! - **Names** used as `assets`/`layouts`/`surfaces` keys are lowercase
//!   `a-z0-9` with inner hyphens; custom surfaces use `custom:<package-id>`.
//! - **Limits**: documents over [`MAX_MANIFEST_BYTES`] are refused; duplicate
//!   object keys are refused; a single leading UTF-8 byte-order mark is
//!   tolerated.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod compat;
mod error;
mod manifest;
mod parse;
mod strict_json;

pub use compat::{
    HostVersions, Incompatibility, IncompatibilityCode, IncompatibilityReport, ManifestVersions,
    NegotiationError, THEME_INCOMPATIBLE, check_compatibility, check_versions,
    negotiate_theme_manifest, probe_manifest_versions,
};
pub use error::{ErrorCode, ManifestError, ManifestWarning, WarningCode};
pub use manifest::{
    Author, Compatibility, PackageKind, ParsedManifest, Platform, Renderer, RendererKind, Surface,
    ThemeManifest,
};

/// Stability label of the v1 manifest format. It stays `"proposed"` until the
/// format is stabilized by tests and an ADR.
pub const FORMAT_STATUS: &str = "proposed";

/// The only `schemaVersion` this crate understands.
pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// The only `renderer.entrypointVersion` this crate understands. It is
/// versioned independently of [`SUPPORTED_SCHEMA_VERSION`].
pub const SUPPORTED_ENTRYPOINT_VERSION: u32 = 1;

/// Largest accepted manifest, in bytes (1 MiB).
pub const MAX_MANIFEST_BYTES: usize = 1024 * 1024;

/// Parse and validate a `glint.json` theme manifest.
///
/// # Errors
///
/// Returns a [`ManifestError`] with a stable [`ErrorCode`] and JSON Pointer
/// for the first problem found, for example
/// [`ErrorCode::MissingId`] (`GLINT_MANIFEST_MISSING_ID`),
/// [`ErrorCode::UnsupportedSchemaVersion`], [`ErrorCode::WrongKind`] or
/// [`ErrorCode::UnknownField`].
///
/// Warnings are discarded; use [`parse_theme_manifest_with_diagnostics`] to
/// see them. Both functions accept and reject exactly the same documents.
pub fn parse_theme_manifest(text: &str) -> Result<ThemeManifest, ManifestError> {
    parse::parse_theme_manifest(text).map(|parsed| parsed.manifest)
}

/// Parse and validate a `glint.json` theme manifest, also returning
/// non-fatal [`ManifestWarning`]s (stable [`WarningCode`]s) for content that
/// was ignored, such as unknown keys inside `author`.
///
/// # Errors
///
/// Exactly as [`parse_theme_manifest`].
pub fn parse_theme_manifest_with_diagnostics(text: &str) -> Result<ParsedManifest, ManifestError> {
    parse::parse_theme_manifest(text)
}
