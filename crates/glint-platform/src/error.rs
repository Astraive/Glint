//! Typed capability errors with stable codes and UI guidance (CAP-001).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::capabilities::CapabilityId;

/// Stable, machine-readable capability error codes.
///
/// The kebab-case string returned by [`CapabilityErrorCode::as_str`] (and used
/// as the JSON form) is the stable contract shared with the SDK, the CLI and
/// the manager UI. Codes are only ever added, never renamed or reused for a
/// different meaning.
///
/// The first eight codes are the table in
/// `docs/plan/02-architecture/capabilities-permissions-and-errors.md`
/// (*Capability errors*); the rest separate the outcomes that table folds
/// together, so a caller can always tell *unsupported* from
/// *permission-denied*, *unavailable*, *timeout* and *failure*
/// ([`ErrorCategory`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum CapabilityErrorCode {
    /// The operating system cannot provide this capability at all
    /// (e.g. replacing the macOS notification center).
    UnsupportedPlatform,
    /// The current session or compositor does not advertise what is needed
    /// (e.g. no layer-shell protocol).
    UnsupportedSession,
    /// An OS permission must be granted first (e.g. accessibility API not
    /// approved yet). The user can still grant it.
    PermissionRequired,
    /// The hardware or device behind the capability is absent
    /// (e.g. brightness on an external display, a removed audio endpoint).
    HardwareUnavailable,
    /// The backing service is briefly gone (e.g. network manager restarting).
    TemporaryUnavailable,
    /// Something else holds the resource (e.g. an appbar on the requested edge).
    Conflict,
    /// The theme cannot run on this host (e.g. it needs a newer SDK, or a
    /// required capability is missing); activation is blocked and the current
    /// theme is kept.
    ThemeIncompatible,
    /// The action targets state that no longer exists (e.g. a removed workspace).
    InvalidState,
    /// The user or the OS explicitly refused the permission.
    PermissionDenied,
    /// Only read access exists; the requested change cannot be made.
    ReadOnly,
    /// The capability is not advertised by this host's adapter (or is not a
    /// capability Glint knows). Unknown capabilities fail closed.
    UnknownCapability,
    /// The adapter did not answer in time.
    Timeout,
    /// The adapter failed unexpectedly.
    AdapterFailure,
}

/// The failure family a [`CapabilityErrorCode`] belongs to. Callers branch on
/// this when the exact code does not matter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ErrorCategory {
    /// The host can never provide this here; retrying will not help.
    Unsupported,
    /// A permission is missing or was refused.
    Permission,
    /// The capability exists but cannot be used right now.
    Unavailable,
    /// The adapter did not answer in time.
    Timeout,
    /// The adapter or the requested action failed.
    Failure,
}

impl CapabilityErrorCode {
    /// Every code, in declaration order.
    pub const ALL: &'static [CapabilityErrorCode] = &[
        CapabilityErrorCode::UnsupportedPlatform,
        CapabilityErrorCode::UnsupportedSession,
        CapabilityErrorCode::PermissionRequired,
        CapabilityErrorCode::HardwareUnavailable,
        CapabilityErrorCode::TemporaryUnavailable,
        CapabilityErrorCode::Conflict,
        CapabilityErrorCode::ThemeIncompatible,
        CapabilityErrorCode::InvalidState,
        CapabilityErrorCode::PermissionDenied,
        CapabilityErrorCode::ReadOnly,
        CapabilityErrorCode::UnknownCapability,
        CapabilityErrorCode::Timeout,
        CapabilityErrorCode::AdapterFailure,
    ];

    /// The stable string form, e.g. `permission-required`.
    pub const fn as_str(self) -> &'static str {
        match self {
            CapabilityErrorCode::UnsupportedPlatform => "unsupported-platform",
            CapabilityErrorCode::UnsupportedSession => "unsupported-session",
            CapabilityErrorCode::PermissionRequired => "permission-required",
            CapabilityErrorCode::HardwareUnavailable => "hardware-unavailable",
            CapabilityErrorCode::TemporaryUnavailable => "temporary-unavailable",
            CapabilityErrorCode::Conflict => "conflict",
            CapabilityErrorCode::ThemeIncompatible => "theme-incompatible",
            CapabilityErrorCode::InvalidState => "invalid-state",
            CapabilityErrorCode::PermissionDenied => "permission-denied",
            CapabilityErrorCode::ReadOnly => "read-only",
            CapabilityErrorCode::UnknownCapability => "unknown-capability",
            CapabilityErrorCode::Timeout => "timeout",
            CapabilityErrorCode::AdapterFailure => "adapter-failure",
        }
    }

    /// The failure family of this code.
    pub const fn category(self) -> ErrorCategory {
        match self {
            CapabilityErrorCode::UnsupportedPlatform
            | CapabilityErrorCode::UnsupportedSession
            | CapabilityErrorCode::ThemeIncompatible
            | CapabilityErrorCode::ReadOnly
            | CapabilityErrorCode::UnknownCapability => ErrorCategory::Unsupported,
            CapabilityErrorCode::PermissionRequired | CapabilityErrorCode::PermissionDenied => {
                ErrorCategory::Permission
            }
            CapabilityErrorCode::HardwareUnavailable
            | CapabilityErrorCode::TemporaryUnavailable
            | CapabilityErrorCode::Conflict => ErrorCategory::Unavailable,
            CapabilityErrorCode::Timeout => ErrorCategory::Timeout,
            CapabilityErrorCode::InvalidState | CapabilityErrorCode::AdapterFailure => {
                ErrorCategory::Failure
            }
        }
    }

    /// Whether the same request may succeed later without user action.
    pub const fn is_retryable(self) -> bool {
        matches!(
            self,
            CapabilityErrorCode::TemporaryUnavailable
                | CapabilityErrorCode::Conflict
                | CapabilityErrorCode::Timeout
        )
    }

    /// What the UI should do about this error. The wording is guidance for UI
    /// authors, not a stable contract; match on the code instead.
    pub const fn guidance(self) -> &'static str {
        match self {
            CapabilityErrorCode::UnsupportedPlatform => {
                "Not possible on this operating system; fall back to a Glint overlay."
            }
            CapabilityErrorCode::UnsupportedSession => {
                "Not offered by this session; choose ordinary window mode or disable the surface."
            }
            CapabilityErrorCode::PermissionRequired => {
                "Explain the native OS permission flow and let the user grant access."
            }
            CapabilityErrorCode::HardwareUnavailable => {
                "Render the control disabled until the device is present."
            }
            CapabilityErrorCode::TemporaryUnavailable => {
                "Show the control as temporarily unavailable and retry after the next change event."
            }
            CapabilityErrorCode::Conflict => {
                "Offer an alternate placement or ask the user how to resolve the conflict."
            }
            CapabilityErrorCode::ThemeIncompatible => {
                "Block activation and keep the current theme; show what is missing."
            }
            CapabilityErrorCode::InvalidState => {
                "Refresh the affected list or state, then let the user try again."
            }
            CapabilityErrorCode::PermissionDenied => {
                "Show that access was refused and how to change it in OS settings; do not re-prompt."
            }
            CapabilityErrorCode::ReadOnly => "Show the value but render the control read-only.",
            CapabilityErrorCode::UnknownCapability => {
                "Treat as unsupported: hide or disable the feature; never assume it works."
            }
            CapabilityErrorCode::Timeout => "Show the control as not responding and offer a retry.",
            CapabilityErrorCode::AdapterFailure => {
                "Show the feature as failed, keep the rest of the theme running and record diagnostics."
            }
        }
    }
}

impl fmt::Display for CapabilityErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Returned when a string is not a known [`CapabilityErrorCode`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownErrorCode(pub String);

impl fmt::Display for UnknownErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown capability error code {:?}", self.0)
    }
}

impl std::error::Error for UnknownErrorCode {}

impl FromStr for CapabilityErrorCode {
    type Err = UnknownErrorCode;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        CapabilityErrorCode::ALL
            .iter()
            .copied()
            .find(|code| code.as_str() == s)
            .ok_or_else(|| UnknownErrorCode(s.to_owned()))
    }
}

/// A capability failure: a stable [`CapabilityErrorCode`], the capability it
/// concerns (when there is one) and a human-readable message. Only the code
/// and capability are stable; the message wording may change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityError {
    code: CapabilityErrorCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    capability: Option<CapabilityId>,
    message: String,
}

impl CapabilityError {
    /// A new error.
    pub fn new(
        code: CapabilityErrorCode,
        capability: Option<CapabilityId>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            capability,
            message: message.into(),
        }
    }

    /// The stable error code.
    pub fn code(&self) -> CapabilityErrorCode {
        self.code
    }

    /// The failure family of [`Self::code`].
    pub fn category(&self) -> ErrorCategory {
        self.code.category()
    }

    /// The capability concerned, if any.
    pub fn capability(&self) -> Option<&CapabilityId> {
        self.capability.as_ref()
    }

    /// Human-readable detail (not stable).
    pub fn message(&self) -> &str {
        &self.message
    }

    /// UI guidance for [`Self::code`].
    pub fn guidance(&self) -> &'static str {
        self.code.guidance()
    }
}

impl fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.capability {
            Some(id) => write!(f, "{} ({id}): {}", self.code, self.message),
            None => write!(f, "{}: {}", self.code, self.message),
        }
    }
}

impl std::error::Error for CapabilityError {}
