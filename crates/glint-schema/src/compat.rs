//! Version negotiation between a theme manifest and the running host
//! (SCH-003). The rules are specified in
//! `docs/architecture/manifest-compatibility.md`.
//!
//! Four axes are compared **independently** and every failing axis is
//! reported, in this fixed order:
//!
//! | Axis | Manifest field | Host value |
//! |---|---|---|
//! | Package format | `schemaVersion` | [`HostVersions::schema_versions`] |
//! | Glint application | `compatibility.glint` | [`HostVersions::glint`] |
//! | `@glint/sdk` | `compatibility.sdk` (optional) | [`HostVersions::sdk`] |
//! | Renderer entrypoint contract | `renderer.entrypointVersion` (optional) | [`HostVersions::entrypoint_versions`] |
//!
//! Nothing here executes, loads or migrates package content: a manifest
//! that does not match is reported as `theme-incompatible`, never upgraded.

use std::fmt;

use semver::{Version, VersionReq};
use serde_json::Value;

use crate::error::{ErrorCode, ManifestError};
use crate::manifest::{ParsedManifest, ThemeManifest};
use crate::parse::{self, Obj};
use crate::{SUPPORTED_ENTRYPOINT_VERSION, SUPPORTED_SCHEMA_VERSION};

/// The capability-error category every [`IncompatibilityReport`] belongs to,
/// from `docs/plan/02-architecture/capabilities-permissions-and-errors.md`:
/// block activation and keep the current theme.
pub const THEME_INCOMPATIBLE: &str = "theme-incompatible";

/// What the running host provides on each negotiated axis.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct HostVersions {
    /// Version of the running Glint application.
    pub glint: Version,
    /// Version of the bundled `@glint/sdk` runtime.
    pub sdk: Version,
    /// Package format versions (`schemaVersion`) the host can read.
    pub schema_versions: Vec<u32>,
    /// Renderer entrypoint contract versions the host can load.
    pub entrypoint_versions: Vec<u32>,
}

impl HostVersions {
    /// A host at the given Glint and SDK versions that supports exactly the
    /// format and entrypoint versions this crate understands
    /// ([`SUPPORTED_SCHEMA_VERSION`], [`SUPPORTED_ENTRYPOINT_VERSION`]).
    pub fn new(glint: Version, sdk: Version) -> Self {
        Self {
            glint,
            sdk,
            schema_versions: vec![SUPPORTED_SCHEMA_VERSION],
            entrypoint_versions: vec![SUPPORTED_ENTRYPOINT_VERSION],
        }
    }
}

/// The version-negotiation envelope of a manifest: the fields every format
/// version keeps at the same place with the same meaning, so that a host can
/// explain why it cannot load a manifest written for a newer format.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ManifestVersions {
    /// `schemaVersion`.
    pub schema_version: u64,
    /// `compatibility.glint`.
    pub glint: VersionReq,
    /// `compatibility.sdk`; `None` for declarative themes that use no SDK.
    pub sdk: Option<VersionReq>,
    /// `renderer.entrypointVersion`; `None` when no renderer is declared.
    pub entrypoint_version: Option<u64>,
}

/// Stable, machine-readable reasons a theme is incompatible. Codes are only
/// ever added, never renamed or reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum IncompatibilityCode {
    /// The host cannot read this `schemaVersion`.
    SchemaVersion,
    /// The Glint version is outside `compatibility.glint`.
    GlintVersion,
    /// The SDK version is outside `compatibility.sdk`.
    SdkVersion,
    /// The host cannot load this `renderer.entrypointVersion`.
    EntrypointVersion,
}

impl IncompatibilityCode {
    /// Every code, in the order axes are checked.
    pub const ALL: &'static [IncompatibilityCode] = &[
        IncompatibilityCode::SchemaVersion,
        IncompatibilityCode::GlintVersion,
        IncompatibilityCode::SdkVersion,
        IncompatibilityCode::EntrypointVersion,
    ];

    /// The stable string form, e.g. `GLINT_COMPAT_SDK_VERSION`.
    pub const fn as_str(self) -> &'static str {
        match self {
            IncompatibilityCode::SchemaVersion => "GLINT_COMPAT_SCHEMA_VERSION",
            IncompatibilityCode::GlintVersion => "GLINT_COMPAT_GLINT_VERSION",
            IncompatibilityCode::SdkVersion => "GLINT_COMPAT_SDK_VERSION",
            IncompatibilityCode::EntrypointVersion => "GLINT_COMPAT_ENTRYPOINT_VERSION",
        }
    }

    const fn pointer(self) -> &'static str {
        match self {
            IncompatibilityCode::SchemaVersion => "/schemaVersion",
            IncompatibilityCode::GlintVersion => "/compatibility/glint",
            IncompatibilityCode::SdkVersion => "/compatibility/sdk",
            IncompatibilityCode::EntrypointVersion => "/renderer/entrypointVersion",
        }
    }

    const fn subject(self) -> &'static str {
        match self {
            IncompatibilityCode::SchemaVersion => "package format schemaVersion",
            IncompatibilityCode::GlintVersion => "Glint",
            IncompatibilityCode::SdkVersion => "@glint/sdk",
            IncompatibilityCode::EntrypointVersion => "renderer entrypointVersion",
        }
    }
}

impl fmt::Display for IncompatibilityCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One failing axis: what the theme requires and what the host provides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incompatibility {
    code: IncompatibilityCode,
    required: String,
    available: String,
}

impl Incompatibility {
    fn new(code: IncompatibilityCode, required: String, available: String) -> Self {
        Self {
            code,
            required,
            available,
        }
    }

    /// The stable reason code.
    pub fn code(&self) -> IncompatibilityCode {
        self.code
    }

    /// JSON Pointer of the manifest field that does not match.
    pub fn pointer(&self) -> &str {
        self.code.pointer()
    }

    /// What the theme declares, verbatim in canonical form (e.g. `>=0.3.0`, `2`).
    pub fn required(&self) -> &str {
        &self.required
    }

    /// What the host provides (a version, or a comma-separated list).
    pub fn available(&self) -> &str {
        &self.available
    }

    /// Human-readable detail (not stable).
    pub fn message(&self) -> String {
        format!(
            "theme requires {} {}; this Glint provides {}",
            self.code.subject(),
            self.required,
            if self.available.is_empty() {
                "none"
            } else {
                &self.available
            }
        )
    }
}

impl fmt::Display for Incompatibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}: {}", self.code, self.pointer(), self.message())
    }
}

/// A `theme-incompatible` verdict listing every failing axis (never empty).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncompatibilityReport {
    reasons: Vec<Incompatibility>,
}

impl IncompatibilityReport {
    /// Always [`THEME_INCOMPATIBLE`].
    pub fn category(&self) -> &'static str {
        THEME_INCOMPATIBLE
    }

    /// Failing axes in check order (schema, glint, sdk, entrypoint).
    pub fn reasons(&self) -> &[Incompatibility] {
        &self.reasons
    }
}

impl fmt::Display for IncompatibilityReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{THEME_INCOMPATIBLE}: ")?;
        for (index, reason) in self.reasons.iter().enumerate() {
            if index > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{reason}")?;
        }
        Ok(())
    }
}

impl std::error::Error for IncompatibilityReport {}

/// Why [`negotiate_theme_manifest`] refused a manifest.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum NegotiationError {
    /// The manifest is malformed or breaks a v1 rule.
    Invalid(ManifestError),
    /// The manifest is well-formed but targets versions this host lacks.
    Incompatible(IncompatibilityReport),
}

impl fmt::Display for NegotiationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NegotiationError::Invalid(err) => err.fmt(f),
            NegotiationError::Incompatible(report) => report.fmt(f),
        }
    }
}

impl std::error::Error for NegotiationError {}

fn list(versions: &[u32]) -> String {
    versions
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn supports(versions: &[u32], wanted: u64) -> bool {
    versions.iter().any(|&v| u64::from(v) == wanted)
}

/// Compare a manifest's version envelope with the host, axis by axis.
///
/// # Errors
///
/// Returns an [`IncompatibilityReport`] naming every failing axis.
pub fn check_versions(
    manifest: &ManifestVersions,
    host: &HostVersions,
) -> Result<(), IncompatibilityReport> {
    let mut reasons = Vec::new();
    if !supports(&host.schema_versions, manifest.schema_version) {
        reasons.push(Incompatibility::new(
            IncompatibilityCode::SchemaVersion,
            manifest.schema_version.to_string(),
            list(&host.schema_versions),
        ));
    }
    if !manifest.glint.matches(&host.glint) {
        reasons.push(Incompatibility::new(
            IncompatibilityCode::GlintVersion,
            manifest.glint.to_string(),
            host.glint.to_string(),
        ));
    }
    if let Some(sdk) = &manifest.sdk
        && !sdk.matches(&host.sdk)
    {
        reasons.push(Incompatibility::new(
            IncompatibilityCode::SdkVersion,
            sdk.to_string(),
            host.sdk.to_string(),
        ));
    }
    if let Some(entrypoint) = manifest.entrypoint_version
        && !supports(&host.entrypoint_versions, entrypoint)
    {
        reasons.push(Incompatibility::new(
            IncompatibilityCode::EntrypointVersion,
            entrypoint.to_string(),
            list(&host.entrypoint_versions),
        ));
    }
    if reasons.is_empty() {
        Ok(())
    } else {
        Err(IncompatibilityReport { reasons })
    }
}

/// [`check_versions`] for an already parsed manifest.
///
/// # Errors
///
/// Returns an [`IncompatibilityReport`] naming every failing axis.
pub fn check_compatibility(
    manifest: &ThemeManifest,
    host: &HostVersions,
) -> Result<(), IncompatibilityReport> {
    check_versions(&manifest.versions(), host)
}

/// Read only the version envelope of a manifest of **any** `schemaVersion`.
///
/// Document-level rules (size, strict JSON, duplicate keys, root object)
/// still apply; every other field, including unknown ones, is left for the
/// full parser.
///
/// # Errors
///
/// Returns a [`ManifestError`] when the envelope itself is missing or
/// malformed, so negotiation cannot even start.
pub fn probe_manifest_versions(text: &str) -> Result<ManifestVersions, ManifestError> {
    let document = parse::load_document(text)?;
    let root = Obj::root(&document);

    let schema_version = match root.get("schemaVersion") {
        None => {
            return Err(parse::invalid(
                ErrorCode::MissingSchemaVersion,
                "/schemaVersion",
                "required field `schemaVersion` is missing",
            ));
        }
        Some(value) => {
            non_negative_integer(value, "/schemaVersion", ErrorCode::UnsupportedSchemaVersion)?
        }
    };

    let compatibility = root.required_object("compatibility")?;
    let glint = parse::version_req(
        compatibility.required("glint")?,
        &compatibility.child("glint"),
    )?;
    let sdk = compatibility
        .get("sdk")
        .map(|v| parse::version_req(v, &compatibility.child("sdk")))
        .transpose()?;

    let entrypoint_version = match root.optional_object("renderer")? {
        None => None,
        Some(renderer) => renderer
            .get("entrypointVersion")
            .map(|v| {
                non_negative_integer(
                    v,
                    &renderer.child("entrypointVersion"),
                    ErrorCode::UnsupportedRenderer,
                )
            })
            .transpose()?,
    };

    Ok(ManifestVersions {
        schema_version,
        glint,
        sdk,
        entrypoint_version,
    })
}

fn non_negative_integer(
    value: &Value,
    pointer: &str,
    code: ErrorCode,
) -> Result<u64, ManifestError> {
    let Value::Number(number) = value else {
        return Err(parse::wrong_type(value, pointer, "an integer"));
    };
    number.as_u64().ok_or_else(|| {
        parse::invalid(
            code,
            pointer,
            format!("{number} is not a non-negative integer version"),
        )
    })
}

/// Negotiate, then fully parse: the order a host uses before activating a
/// theme.
///
/// 1. [`probe_manifest_versions`]: a malformed envelope is
///    [`NegotiationError::Invalid`].
/// 2. [`check_versions`]: any failing axis is
///    [`NegotiationError::Incompatible`], even when the rest of the document
///    uses syntax this parser does not know (it may be valid in that newer
///    format).
/// 3. [`crate::parse_theme_manifest_with_diagnostics`]: any v1 rule
///    violation is [`NegotiationError::Invalid`].
///
/// # Errors
///
/// See the steps above.
pub fn negotiate_theme_manifest(
    text: &str,
    host: &HostVersions,
) -> Result<ParsedManifest, NegotiationError> {
    let versions = probe_manifest_versions(text).map_err(NegotiationError::Invalid)?;
    check_versions(&versions, host).map_err(NegotiationError::Incompatible)?;
    parse::parse_theme_manifest(text).map_err(NegotiationError::Invalid)
}
