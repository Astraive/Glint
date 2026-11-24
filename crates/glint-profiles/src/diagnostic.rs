//! Typed resolution diagnostics with stable `GLINT_RESOLVE_*` codes.

use std::fmt;

/// Stable, machine-readable resolution codes. The string form is the
/// contract; codes are only ever added, never renamed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ResolveCode {
    /// A theme the profile references is not available (missing, corrupt,
    /// or, with a lock, not the pinned release); its layer contributes
    /// nothing.
    ThemeUnavailable,
    /// The theme provider returned a release whose id or version is not the
    /// one requested; it is treated as unavailable.
    ThemeMismatch,
    /// A surface override names a theme that does not declare that surface.
    SurfaceNotProvided,
    /// A theme's surface definition failed `glint-surfaces` validation.
    SurfaceInvalid,
    /// No layer provides a surface the profile asked for.
    SurfaceUnresolved,
    /// A host restriction (an unusable required capability) blocks a
    /// surface.
    SurfaceBlocked,
    /// A token override names a token outside the registry.
    TokenUnknown,
    /// A token override uses a Glint brand constant as its name.
    TokenBrandName,
    /// A token override value does not match the token's grammar.
    TokenInvalidValue,
    /// A token override would break a contrast requirement and was reverted.
    TokenLowContrast,
    /// Neither the base theme nor the default theme supplies tokens.
    TokensUnavailable,
    /// The host's colour scheme is not offered by the token source.
    SchemeUnavailable,
    /// A user override was superseded by a host accessibility or safety
    /// restriction.
    OverrideRestricted,
    /// A monitor override names a surface that is not in the plan.
    MonitorSurfaceUnknown,
    /// A monitor override is malformed or produces an invalid placement.
    MonitorOverrideInvalid,
    /// The selected wallpaper asset is not provided by the base theme.
    WallpaperAssetMissing,
    /// A surface fell back from an unusable override theme to the base
    /// theme.
    FallbackToBase,
    /// A surface, or the tokens and wallpaper, fell back to the bundled
    /// default theme.
    FallbackToDefault,
}

impl ResolveCode {
    /// Every code, in declaration order.
    pub const ALL: &'static [ResolveCode] = &[
        ResolveCode::ThemeUnavailable,
        ResolveCode::ThemeMismatch,
        ResolveCode::SurfaceNotProvided,
        ResolveCode::SurfaceInvalid,
        ResolveCode::SurfaceUnresolved,
        ResolveCode::SurfaceBlocked,
        ResolveCode::TokenUnknown,
        ResolveCode::TokenBrandName,
        ResolveCode::TokenInvalidValue,
        ResolveCode::TokenLowContrast,
        ResolveCode::TokensUnavailable,
        ResolveCode::SchemeUnavailable,
        ResolveCode::OverrideRestricted,
        ResolveCode::MonitorSurfaceUnknown,
        ResolveCode::MonitorOverrideInvalid,
        ResolveCode::WallpaperAssetMissing,
        ResolveCode::FallbackToBase,
        ResolveCode::FallbackToDefault,
    ];

    /// The stable string form, e.g. `GLINT_RESOLVE_THEME_UNAVAILABLE`.
    pub const fn as_str(self) -> &'static str {
        match self {
            ResolveCode::ThemeUnavailable => "GLINT_RESOLVE_THEME_UNAVAILABLE",
            ResolveCode::ThemeMismatch => "GLINT_RESOLVE_THEME_MISMATCH",
            ResolveCode::SurfaceNotProvided => "GLINT_RESOLVE_SURFACE_NOT_PROVIDED",
            ResolveCode::SurfaceInvalid => "GLINT_RESOLVE_SURFACE_INVALID",
            ResolveCode::SurfaceUnresolved => "GLINT_RESOLVE_SURFACE_UNRESOLVED",
            ResolveCode::SurfaceBlocked => "GLINT_RESOLVE_SURFACE_BLOCKED",
            ResolveCode::TokenUnknown => "GLINT_RESOLVE_TOKEN_UNKNOWN",
            ResolveCode::TokenBrandName => "GLINT_RESOLVE_TOKEN_BRAND_NAME",
            ResolveCode::TokenInvalidValue => "GLINT_RESOLVE_TOKEN_INVALID_VALUE",
            ResolveCode::TokenLowContrast => "GLINT_RESOLVE_TOKEN_LOW_CONTRAST",
            ResolveCode::TokensUnavailable => "GLINT_RESOLVE_TOKENS_UNAVAILABLE",
            ResolveCode::SchemeUnavailable => "GLINT_RESOLVE_SCHEME_UNAVAILABLE",
            ResolveCode::OverrideRestricted => "GLINT_RESOLVE_OVERRIDE_RESTRICTED",
            ResolveCode::MonitorSurfaceUnknown => "GLINT_RESOLVE_MONITOR_SURFACE_UNKNOWN",
            ResolveCode::MonitorOverrideInvalid => "GLINT_RESOLVE_MONITOR_OVERRIDE_INVALID",
            ResolveCode::WallpaperAssetMissing => "GLINT_RESOLVE_WALLPAPER_ASSET_MISSING",
            ResolveCode::FallbackToBase => "GLINT_RESOLVE_FALLBACK_TO_BASE",
            ResolveCode::FallbackToDefault => "GLINT_RESOLVE_FALLBACK_TO_DEFAULT",
        }
    }

    /// How serious the code is. Fixed per code.
    pub const fn severity(self) -> Severity {
        match self {
            ResolveCode::ThemeUnavailable
            | ResolveCode::ThemeMismatch
            | ResolveCode::SurfaceNotProvided
            | ResolveCode::SurfaceInvalid
            | ResolveCode::SurfaceUnresolved
            | ResolveCode::TokensUnavailable => Severity::Error,
            ResolveCode::SurfaceBlocked
            | ResolveCode::TokenUnknown
            | ResolveCode::TokenBrandName
            | ResolveCode::TokenInvalidValue
            | ResolveCode::TokenLowContrast
            | ResolveCode::MonitorSurfaceUnknown
            | ResolveCode::MonitorOverrideInvalid
            | ResolveCode::WallpaperAssetMissing
            | ResolveCode::FallbackToBase
            | ResolveCode::FallbackToDefault => Severity::Warning,
            ResolveCode::SchemeUnavailable | ResolveCode::OverrideRestricted => Severity::Info,
        }
    }
}

impl fmt::Display for ResolveCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How serious a diagnostic is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Severity {
    /// Expected behaviour worth showing, e.g. a host restriction winning.
    Info,
    /// Part of the profile was ignored or replaced; the plan is still safe.
    Warning,
    /// Something the profile asked for could not be honoured at all.
    Error,
}

impl Severity {
    /// The stable string form: `info`, `warning` or `error`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One resolution finding. Never fatal: the resolver always produces a plan
/// and reports what it could not honour here.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Diagnostic {
    /// Stable code.
    pub code: ResolveCode,
    /// [`ResolveCode::severity`] of the code.
    pub severity: Severity,
    /// RFC 6901 JSON Pointer into `profile.json` of the value responsible
    /// (`""` when the bundled default theme itself is at fault).
    pub pointer: String,
    /// The surface concerned, if any.
    pub surface: Option<String>,
    /// Human-readable detail (not stable).
    pub message: String,
}

impl Diagnostic {
    pub(crate) fn new(
        code: ResolveCode,
        pointer: impl Into<String>,
        surface: Option<&str>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity: code.severity(),
            pointer: pointer.into(),
            surface: surface.map(str::to_owned),
            message: message.into(),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.code, self.message)?;
        if !self.pointer.is_empty() {
            write!(f, " (at {})", self.pointer)?;
        }
        Ok(())
    }
}

/// `parent/<escaped key>` per RFC 6901.
pub(crate) fn child_pointer(parent: &str, key: &str) -> String {
    format!("{parent}/{}", key.replace('~', "~0").replace('/', "~1"))
}
