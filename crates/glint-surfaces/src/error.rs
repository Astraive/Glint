//! Stable surface definition diagnostics.

use std::fmt;

/// Stable, machine-readable surface definition error codes.
///
/// The string form returned by [`SurfaceErrorCode::as_str`] is the stable
/// contract: tools, tests, documentation and the generated TypeScript types
/// match on it. Codes are only ever added, never renamed or reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum SurfaceErrorCode {
    /// The surface ID is not a lowercase name or a `custom:` ID.
    InvalidId,
    /// The surface ID is well formed but is not a standard surface kind.
    UnknownKind,
    /// A `custom:` ID lacks a dotted namespace (`custom:clock`) or is malformed.
    InvalidCustomId,
    /// A required field is absent; the pointer names it.
    MissingField,
    /// A field is not part of the surface definition format.
    UnknownField,
    /// A field has the wrong JSON type.
    InvalidType,
    /// A field has the right type but a value outside its vocabulary.
    InvalidValue,
    /// A number is outside its allowed range.
    OutOfRange,
    /// The entrypoint is not a package-local `.html` document (remote URL,
    /// `file://`, absolute, escaping or otherwise unsafe path).
    InvalidEntrypoint,
    /// The placement kind is not allowed for this surface kind.
    UnsupportedPlacement,
    /// Two fields contradict each other (e.g. `width` on a top edge,
    /// `minWidth` above `maxWidth`, `reserveWorkArea` on a floating surface).
    Conflict,
}

impl SurfaceErrorCode {
    /// Every code, in declaration order.
    pub const ALL: &'static [SurfaceErrorCode] = &[
        SurfaceErrorCode::InvalidId,
        SurfaceErrorCode::UnknownKind,
        SurfaceErrorCode::InvalidCustomId,
        SurfaceErrorCode::MissingField,
        SurfaceErrorCode::UnknownField,
        SurfaceErrorCode::InvalidType,
        SurfaceErrorCode::InvalidValue,
        SurfaceErrorCode::OutOfRange,
        SurfaceErrorCode::InvalidEntrypoint,
        SurfaceErrorCode::UnsupportedPlacement,
        SurfaceErrorCode::Conflict,
    ];

    /// The stable string form, e.g. `GLINT_SURFACE_UNKNOWN_KIND`.
    pub const fn as_str(self) -> &'static str {
        match self {
            SurfaceErrorCode::InvalidId => "GLINT_SURFACE_INVALID_ID",
            SurfaceErrorCode::UnknownKind => "GLINT_SURFACE_UNKNOWN_KIND",
            SurfaceErrorCode::InvalidCustomId => "GLINT_SURFACE_INVALID_CUSTOM_ID",
            SurfaceErrorCode::MissingField => "GLINT_SURFACE_MISSING_FIELD",
            SurfaceErrorCode::UnknownField => "GLINT_SURFACE_UNKNOWN_FIELD",
            SurfaceErrorCode::InvalidType => "GLINT_SURFACE_INVALID_TYPE",
            SurfaceErrorCode::InvalidValue => "GLINT_SURFACE_INVALID_VALUE",
            SurfaceErrorCode::OutOfRange => "GLINT_SURFACE_OUT_OF_RANGE",
            SurfaceErrorCode::InvalidEntrypoint => "GLINT_SURFACE_INVALID_ENTRYPOINT",
            SurfaceErrorCode::UnsupportedPlacement => "GLINT_SURFACE_UNSUPPORTED_PLACEMENT",
            SurfaceErrorCode::Conflict => "GLINT_SURFACE_CONFLICT",
        }
    }
}

impl fmt::Display for SurfaceErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A surface definition rejection: a stable [`SurfaceErrorCode`], the
/// RFC 6901 JSON Pointer of the offending value relative to the manifest root
/// (e.g. `/surfaces/panel/placement/height`) and a human-readable message.
/// Only the code and pointer are stable; the message wording may change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceError {
    code: SurfaceErrorCode,
    pointer: String,
    message: String,
}

impl SurfaceError {
    pub(crate) fn new(
        code: SurfaceErrorCode,
        pointer: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            pointer: pointer.into(),
            message: message.into(),
        }
    }

    /// The stable error code.
    pub fn code(&self) -> SurfaceErrorCode {
        self.code
    }

    /// JSON Pointer (RFC 6901) to the offending value, relative to the
    /// manifest root.
    pub fn pointer(&self) -> &str {
        &self.pointer
    }

    /// Human-readable detail (not stable).
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for SurfaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}: {}", self.code, self.pointer, self.message)
    }
}

impl std::error::Error for SurfaceError {}

/// RFC 6901 child pointer: `~` becomes `~0` and `/` becomes `~1`.
pub(crate) fn child_pointer(parent: &str, key: &str) -> String {
    format!("{parent}/{}", key.replace('~', "~0").replace('/', "~1"))
}
