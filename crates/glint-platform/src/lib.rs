//! OS-independent capability contracts for Glint.
//!
//! Themes declare the capabilities they need; the host reports what it can
//! actually do. This crate holds the shared vocabulary, with **no
//! OS-specific dependency**: native adapters (`glint-windows`, `glint-linux`,
//! `glint-macos`) implement it, everything else consumes it.
//!
//! - [`CapabilityId`]: a validated `<namespace>.<name>[.<name>...]` ID in one
//!   of the five [`Namespace`]s.
//! - [`Availability`]: `Supported`, `ReadOnly`, `PermissionRequired`,
//!   `Experimental` or `Unavailable { reason }`. A capability is never a
//!   boolean, and an unavailable one always carries an
//!   [`UnavailableReason`].
//! - [`Capability`]: an ID, its availability, an optional host scope and its
//!   [`IntegrationEffect`] (Glint-owned UI versus an OS-managed setting).
//! - [`CapabilityError`]: every failure has a stable kebab-case
//!   [`CapabilityErrorCode`] (`unsupported-platform`, `permission-required`,
//!   `timeout`, ...) with UI guidance, and an [`ErrorCategory`] that tells
//!   *unsupported*, *permission*, *unavailable*, *timeout* and *failure*
//!   apart. Feature unavailable or permission required never masquerades as
//!   success: [`Availability::check`] returns the typed error.
//!
//! - [`CapabilityService`] (CAP-002): discovers capabilities through a
//!   [`CapabilityAdapter`], keeps the current [`CapabilitySnapshot`] and
//!   sends a [`CapabilityChange`] to every subscriber on each availability
//!   transition (device removed, permission revoked, adapter down). It fails
//!   closed: undiscovered, unadvertised and stale capabilities are never
//!   reported usable. [`mock::MockAdapter`] reaches every state.
//!
//! - The activation gate (CAP-003, [`evaluate_manifest`],
//!   [`gate_activation`]): a theme's `requiredCapabilities` block
//!   activation unless usable (or covered by a declared fallback), and the
//!   [`ActivationRefusal`] lists every blocker with its typed error;
//!   `optionalCapabilities` that are unusable only disable controls
//!   ([`ActivationReport::disabled`]). The gate changes no state.
//!
//! - The frozen v1 capability registry (CAP-005, [`registry`]): every
//!   capability ID that exists in SDK v1 with its read/write/present
//!   [`registry::Access`], [`registry::Sensitivity`], integration effect and
//!   platform notes. Unregistered IDs fail closed everywhere: the service
//!   refuses an adapter report that contains one, a snapshot reports one as
//!   `unknown-capability`, and the activation gate refuses a theme that
//!   declares one.
//!
//! Every type round-trips through JSON (serde); the shapes are documented on
//! each type and are *proposed* until stabilized by an ADR.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod capabilities;
mod capability_gate;
mod error;
pub mod mock;
pub mod registry;
mod service;

pub use capabilities::{
    Availability, Capability, CapabilityId, CapabilityIdError, EmptyReason, IntegrationEffect,
    Namespace, UnavailableKind, UnavailableReason,
};
pub use capability_gate::{
    ActivationRefusal, ActivationReport, Blocker, CapabilityCheck, CapabilityRequirements,
    CapabilityRole, CheckOutcome, RequiredCapability, evaluate, evaluate_manifest, gate_activation,
};
pub use error::{CapabilityError, CapabilityErrorCode, ErrorCategory, UnknownErrorCode};
pub use service::{
    CapabilityAdapter, CapabilityChange, CapabilityService, CapabilitySnapshot, ChangeKind,
};
