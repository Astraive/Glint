//! Stable manifest diagnostics.

use std::fmt;

/// Stable, machine-readable manifest error codes.
///
/// The string form returned by [`ErrorCode::as_str`] is the stable contract:
/// tools, tests and documentation match on it. Codes are only ever added,
/// never renamed or reused for a different meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ErrorCode {
    /// The document is not well-formed JSON (or exceeds the nesting limit).
    InvalidJson,
    /// The document is larger than [`crate::MAX_MANIFEST_BYTES`].
    TooLarge,
    /// An object repeats a key; parsers disagree on which value wins, so
    /// duplicates are refused outright.
    DuplicateKey,
    /// The document root is not a JSON object.
    NotAnObject,
    /// `schemaVersion` is absent.
    MissingSchemaVersion,
    /// `schemaVersion` is not a version this parser understands.
    UnsupportedSchemaVersion,
    /// `kind` is absent.
    MissingKind,
    /// `kind` is not `"theme"`.
    WrongKind,
    /// `id` is absent.
    MissingId,
    /// `id` is not a lowercase reverse-DNS style package identifier.
    InvalidId,
    /// Another required field is absent; the pointer names it.
    MissingField,
    /// A field is not part of the v1 format.
    UnknownField,
    /// A field has the wrong JSON type.
    InvalidType,
    /// A field has the right type but a disallowed value.
    InvalidValue,
    /// `version` is not a semantic version.
    InvalidVersion,
    /// A compatibility range is not a valid semver requirement.
    InvalidVersionRequirement,
    /// A package-relative path is absolute, escapes the package or is otherwise unsafe.
    InvalidPath,
    /// A capability identifier is malformed, outside a known namespace or repeated.
    InvalidCapability,
    /// The renderer kind or entrypoint version is not supported.
    UnsupportedRenderer,
}

impl ErrorCode {
    /// Every code, in declaration order.
    pub const ALL: &'static [ErrorCode] = &[
        ErrorCode::InvalidJson,
        ErrorCode::TooLarge,
        ErrorCode::DuplicateKey,
        ErrorCode::NotAnObject,
        ErrorCode::MissingSchemaVersion,
        ErrorCode::UnsupportedSchemaVersion,
        ErrorCode::MissingKind,
        ErrorCode::WrongKind,
        ErrorCode::MissingId,
        ErrorCode::InvalidId,
        ErrorCode::MissingField,
        ErrorCode::UnknownField,
        ErrorCode::InvalidType,
        ErrorCode::InvalidValue,
        ErrorCode::InvalidVersion,
        ErrorCode::InvalidVersionRequirement,
        ErrorCode::InvalidPath,
        ErrorCode::InvalidCapability,
        ErrorCode::UnsupportedRenderer,
    ];

    /// The stable string form, e.g. `GLINT_MANIFEST_MISSING_ID`.
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorCode::InvalidJson => "GLINT_MANIFEST_INVALID_JSON",
            ErrorCode::TooLarge => "GLINT_MANIFEST_TOO_LARGE",
            ErrorCode::DuplicateKey => "GLINT_MANIFEST_DUPLICATE_KEY",
            ErrorCode::NotAnObject => "GLINT_MANIFEST_NOT_AN_OBJECT",
            ErrorCode::MissingSchemaVersion => "GLINT_MANIFEST_MISSING_SCHEMA_VERSION",
            ErrorCode::UnsupportedSchemaVersion => "GLINT_MANIFEST_UNSUPPORTED_SCHEMA_VERSION",
            ErrorCode::MissingKind => "GLINT_MANIFEST_MISSING_KIND",
            ErrorCode::WrongKind => "GLINT_MANIFEST_WRONG_KIND",
            ErrorCode::MissingId => "GLINT_MANIFEST_MISSING_ID",
            ErrorCode::InvalidId => "GLINT_MANIFEST_INVALID_ID",
            ErrorCode::MissingField => "GLINT_MANIFEST_MISSING_FIELD",
            ErrorCode::UnknownField => "GLINT_MANIFEST_UNKNOWN_FIELD",
            ErrorCode::InvalidType => "GLINT_MANIFEST_INVALID_TYPE",
            ErrorCode::InvalidValue => "GLINT_MANIFEST_INVALID_VALUE",
            ErrorCode::InvalidVersion => "GLINT_MANIFEST_INVALID_VERSION",
            ErrorCode::InvalidVersionRequirement => "GLINT_MANIFEST_INVALID_VERSION_REQUIREMENT",
            ErrorCode::InvalidPath => "GLINT_MANIFEST_INVALID_PATH",
            ErrorCode::InvalidCapability => "GLINT_MANIFEST_INVALID_CAPABILITY",
            ErrorCode::UnsupportedRenderer => "GLINT_MANIFEST_UNSUPPORTED_RENDERER",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A manifest rejection: a stable [`ErrorCode`], the RFC 6901 JSON Pointer of
/// the offending location (empty for the document root or for syntax errors)
/// and a human-readable message. Only the code and pointer are stable; the
/// message wording may change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestError {
    code: ErrorCode,
    pointer: String,
    message: String,
}

impl ManifestError {
    pub(crate) fn new(
        code: ErrorCode,
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
    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// JSON Pointer (RFC 6901) to the offending value; `""` means the root.
    pub fn pointer(&self) -> &str {
        &self.pointer
    }

    /// Human-readable detail (not stable).
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let at = if self.pointer.is_empty() {
            "(root)"
        } else {
            self.pointer.as_str()
        };
        write!(f, "{} at {}: {}", self.code, at, self.message)
    }
}

impl std::error::Error for ManifestError {}
