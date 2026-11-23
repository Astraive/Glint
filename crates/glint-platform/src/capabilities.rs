//! Capability identifiers, availability states and integration effects
//! (CAP-001).
//!
//! A capability is never a boolean: the host reports one of five
//! [`Availability`] states, and an unavailable capability always says why
//! ([`UnavailableReason`]).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{CapabilityError, CapabilityErrorCode};

/// Capability namespaces from
/// `docs/plan/02-architecture/capabilities-permissions-and-errors.md`
/// (*Capability namespaces*). The same five are accepted by the
/// `glint-schema` manifest parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Namespace {
    /// `surface.*`: Glint-owned surfaces (panel, launcher, overlays).
    Surface,
    /// `system.*`: system state such as audio, brightness, network, power.
    System,
    /// `desktop.*`: windows and workspaces.
    Desktop,
    /// `native.*`: changes to OS-managed appearance (wallpaper, taskbar).
    Native,
    /// `host.*`: host window mechanisms (layer shell, appbar, hotkeys).
    Host,
}

impl Namespace {
    /// Every namespace, in declaration order.
    pub const ALL: &'static [Namespace] = &[
        Namespace::Surface,
        Namespace::System,
        Namespace::Desktop,
        Namespace::Native,
        Namespace::Host,
    ];

    /// The namespace prefix, e.g. `system`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Namespace::Surface => "surface",
            Namespace::System => "system",
            Namespace::Desktop => "desktop",
            Namespace::Native => "native",
            Namespace::Host => "host",
        }
    }

    fn from_prefix(prefix: &str) -> Option<Self> {
        Namespace::ALL
            .iter()
            .copied()
            .find(|ns| ns.as_str() == prefix)
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a string is not a [`CapabilityId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CapabilityIdError {
    /// The string is empty.
    Empty,
    /// The string is longer than [`CapabilityId::MAX_LEN`] bytes.
    TooLong,
    /// The first segment is not one of the [`Namespace`]s.
    UnknownNamespace,
    /// Only a namespace was given (e.g. `surface`).
    MissingName,
    /// A name segment is empty, does not start with `a-z`, contains anything
    /// other than `a-z`, `0-9` and inner single hyphens, or starts or ends
    /// with a hyphen.
    InvalidSegment,
}

impl fmt::Display for CapabilityIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CapabilityIdError::Empty => "capability id is empty",
            CapabilityIdError::TooLong => "capability id is too long",
            CapabilityIdError::UnknownNamespace => {
                "capability id does not start with surface, system, desktop, native or host"
            }
            CapabilityIdError::MissingName => "capability id has no name after its namespace",
            CapabilityIdError::InvalidSegment => {
                "capability id segments must be lowercase a-z0-9 with inner hyphens, starting with a letter"
            }
        })
    }
}

impl std::error::Error for CapabilityIdError {}

/// A validated, namespaced capability identifier:
/// `<namespace>.<name>[.<name>...]`, e.g. `system.audio.write`.
///
/// The rules match the `glint-schema` manifest parser: one of the five
/// [`Namespace`]s, then at least one name segment of lowercase `a-z0-9` with
/// inner single hyphens, starting with a letter; at most
/// [`CapabilityId::MAX_LEN`] bytes. Whether an ID is in the frozen registry of
/// known capabilities is a separate question (CAP-005); an ID the host adapter
/// does not advertise is reported as unavailable, never assumed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CapabilityId(String);

impl CapabilityId {
    /// Maximum length in bytes (the manifest parser's limit).
    pub const MAX_LEN: usize = 128;

    /// Validates `raw` as a capability ID.
    pub fn parse(raw: &str) -> Result<Self, CapabilityIdError> {
        if raw.is_empty() {
            return Err(CapabilityIdError::Empty);
        }
        if raw.len() > Self::MAX_LEN {
            return Err(CapabilityIdError::TooLong);
        }
        let mut segments = raw.split('.');
        let prefix = segments.next().unwrap_or_default();
        if Namespace::from_prefix(prefix).is_none() {
            return Err(CapabilityIdError::UnknownNamespace);
        }
        let mut names = 0usize;
        for segment in segments {
            if !is_name_segment(segment) {
                return Err(CapabilityIdError::InvalidSegment);
            }
            names += 1;
        }
        if names == 0 {
            return Err(CapabilityIdError::MissingName);
        }
        Ok(Self(raw.to_owned()))
    }

    /// The ID as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The ID's namespace.
    pub fn namespace(&self) -> Namespace {
        let prefix = self.0.split('.').next().unwrap_or_default();
        // Validated on construction.
        Namespace::from_prefix(prefix).unwrap_or(Namespace::Surface)
    }
}

fn is_name_segment(segment: &str) -> bool {
    segment
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_lowercase)
        && !segment.ends_with('-')
        && !segment.contains("--")
        && segment
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

impl fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for CapabilityId {
    type Err = CapabilityIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<String> for CapabilityId {
    type Error = CapabilityIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<CapabilityId> for String {
    fn from(value: CapabilityId) -> Self {
        value.0
    }
}

impl AsRef<str> for CapabilityId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// The kind of reason a capability is [`Availability::Unavailable`]. Each kind
/// maps to one [`CapabilityErrorCode`] ([`UnavailableKind::code`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum UnavailableKind {
    /// The OS cannot provide it ([`CapabilityErrorCode::UnsupportedPlatform`]).
    UnsupportedPlatform,
    /// The session/compositor does not advertise it
    /// ([`CapabilityErrorCode::UnsupportedSession`]).
    UnsupportedSession,
    /// The device is absent or was removed
    /// ([`CapabilityErrorCode::HardwareUnavailable`]).
    HardwareUnavailable,
    /// The backing service is briefly gone
    /// ([`CapabilityErrorCode::TemporaryUnavailable`]).
    TemporaryUnavailable,
    /// Something else holds the resource ([`CapabilityErrorCode::Conflict`]).
    Conflict,
    /// The host adapter does not advertise this capability
    /// ([`CapabilityErrorCode::UnknownCapability`]).
    NotAdvertised,
}

impl UnavailableKind {
    /// Every kind, in declaration order.
    pub const ALL: &'static [UnavailableKind] = &[
        UnavailableKind::UnsupportedPlatform,
        UnavailableKind::UnsupportedSession,
        UnavailableKind::HardwareUnavailable,
        UnavailableKind::TemporaryUnavailable,
        UnavailableKind::Conflict,
        UnavailableKind::NotAdvertised,
    ];

    /// The error code a caller receives when it tries to use a capability
    /// that is unavailable for this reason.
    pub const fn code(self) -> CapabilityErrorCode {
        match self {
            UnavailableKind::UnsupportedPlatform => CapabilityErrorCode::UnsupportedPlatform,
            UnavailableKind::UnsupportedSession => CapabilityErrorCode::UnsupportedSession,
            UnavailableKind::HardwareUnavailable => CapabilityErrorCode::HardwareUnavailable,
            UnavailableKind::TemporaryUnavailable => CapabilityErrorCode::TemporaryUnavailable,
            UnavailableKind::Conflict => CapabilityErrorCode::Conflict,
            UnavailableKind::NotAdvertised => CapabilityErrorCode::UnknownCapability,
        }
    }
}

/// Returned when an [`UnavailableReason`] would have no detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyReason;

impl fmt::Display for EmptyReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an unavailable capability must carry a non-empty reason")
    }
}

impl std::error::Error for EmptyReason {}

/// Why a capability is unavailable: a typed [`UnavailableKind`] plus a
/// non-empty human-readable detail. Construction (including JSON
/// deserialization) refuses an empty or whitespace-only detail.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "RawReason")]
pub struct UnavailableReason {
    kind: UnavailableKind,
    detail: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawReason {
    kind: UnavailableKind,
    detail: String,
}

impl TryFrom<RawReason> for UnavailableReason {
    type Error = EmptyReason;

    fn try_from(raw: RawReason) -> Result<Self, Self::Error> {
        Self::new(raw.kind, raw.detail)
    }
}

impl UnavailableReason {
    /// A reason; `detail` must contain a non-whitespace character.
    pub fn new(kind: UnavailableKind, detail: impl Into<String>) -> Result<Self, EmptyReason> {
        let detail = detail.into();
        if detail.trim().is_empty() {
            return Err(EmptyReason);
        }
        Ok(Self { kind, detail })
    }

    /// The typed reason.
    pub fn kind(&self) -> UnavailableKind {
        self.kind
    }

    /// Human-readable detail.
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

/// What the host can do for one capability right now.
///
/// JSON form: `{"state": "supported"}`, `{"state": "read-only"}`,
/// `{"state": "permission-required"}`, `{"state": "experimental"}`, or
/// `{"state": "unavailable", "reason": {"kind": "...", "detail": "..."}}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Availability {
    /// Fully available.
    Supported,
    /// The value can be read but not changed.
    ReadOnly,
    /// Available once the user grants an OS permission; unusable until then.
    PermissionRequired,
    /// Available through an experimental or third-party integration; usable
    /// but must be labelled as such.
    Experimental,
    /// Not available, with the reason.
    Unavailable {
        /// Why the capability is unavailable.
        reason: UnavailableReason,
    },
}

impl Availability {
    /// `Unavailable` with a reason; fails when `detail` is blank.
    pub fn unavailable(
        kind: UnavailableKind,
        detail: impl Into<String>,
    ) -> Result<Self, EmptyReason> {
        Ok(Availability::Unavailable {
            reason: UnavailableReason::new(kind, detail)?,
        })
    }

    /// Whether the capability can be used at all (read or write):
    /// `Supported`, `Experimental` or `ReadOnly`.
    pub fn is_usable(&self) -> bool {
        matches!(
            self,
            Availability::Supported | Availability::Experimental | Availability::ReadOnly
        )
    }

    /// Whether the capability can be used to change state: `Supported` or
    /// `Experimental`.
    pub fn is_writable(&self) -> bool {
        matches!(self, Availability::Supported | Availability::Experimental)
    }

    /// `Ok(())` when the capability is usable ([`Self::is_usable`]),
    /// otherwise the typed error a caller must surface. Never reports success
    /// for `PermissionRequired` or `Unavailable`.
    pub fn check(&self, id: &CapabilityId) -> Result<(), CapabilityError> {
        match self {
            Availability::Supported | Availability::Experimental | Availability::ReadOnly => Ok(()),
            Availability::PermissionRequired => Err(CapabilityError::new(
                CapabilityErrorCode::PermissionRequired,
                Some(id.clone()),
                "an OS permission must be granted before this capability can be used",
            )),
            Availability::Unavailable { reason } => Err(CapabilityError::new(
                reason.kind().code(),
                Some(id.clone()),
                reason.detail(),
            )),
        }
    }

    /// Like [`Self::check`], but a `ReadOnly` capability is refused with
    /// [`CapabilityErrorCode::ReadOnly`].
    pub fn check_writable(&self, id: &CapabilityId) -> Result<(), CapabilityError> {
        self.check(id)?;
        if matches!(self, Availability::ReadOnly) {
            return Err(CapabilityError::new(
                CapabilityErrorCode::ReadOnly,
                Some(id.clone()),
                "only read access is available",
            ));
        }
        Ok(())
    }
}

/// Whether using a capability changes Glint-owned UI or an OS-managed
/// setting. A Glint control center is `GlintOwnedUi` even where the native
/// one cannot be replaced; setting the wallpaper is `OsManagedSetting`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum IntegrationEffect {
    /// Only affects UI that Glint draws and owns.
    GlintOwnedUi,
    /// Changes a setting the operating system owns and other apps observe.
    OsManagedSetting,
}

impl IntegrationEffect {
    /// Whether the effect reaches outside Glint into OS state.
    pub const fn changes_os_state(self) -> bool {
        matches!(self, IntegrationEffect::OsManagedSetting)
    }
}

/// One capability as reported by a host adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Capability {
    /// Which capability.
    pub id: CapabilityId,
    /// What the host can do for it right now.
    pub availability: Availability,
    /// Optional host-specific scope, e.g. a compositor or display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Whether using it affects Glint UI or an OS setting.
    pub effect: IntegrationEffect,
}

impl Capability {
    /// A capability without a scope.
    pub fn new(id: CapabilityId, availability: Availability, effect: IntegrationEffect) -> Self {
        Self {
            id,
            availability,
            scope: None,
            effect,
        }
    }

    /// Sets the host-specific scope.
    #[must_use]
    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = Some(scope.into());
        self
    }
}
