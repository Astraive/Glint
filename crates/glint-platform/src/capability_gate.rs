//! Capability validation before theme activation (CAP-003).
//!
//! A theme's `requiredCapabilities` are hard blockers unless a usable
//! fallback is declared; its `optionalCapabilities` are runtime-gated. The
//! gate compares them against a [`CapabilitySnapshot`] and either
//!
//! - refuses activation with an [`ActivationRefusal`]
//!   (`theme-incompatible`) that lists **every** unusable required
//!   capability with its typed [`CapabilityError`] (permission-gated,
//!   unavailable, not advertised, ...), so the installer can present each
//!   one and the current theme stays active; or
//! - allows it with an [`ActivationReport`] whose degraded entries say which
//!   optional controls must be disabled ([`CheckOutcome::Disabled`]) and
//!   which capabilities are usable only with limits
//!   ([`CheckOutcome::Limited`]: `ReadOnly` or `Experimental`) or through a
//!   fallback.
//!
//! The gate is a pure function of its inputs and changes no state.
//! [`gate_activation`] runs the caller's activation step only after the gate
//! passes, so a refused theme can never reach it. Declaring a capability does
//! not grant it: permission enforcement is the permission engine's job
//! (CAP-004).
//!
//! The v1 manifest has no field for fallbacks (`integrations` is not
//! accepted by `glint-schema`), so [`CapabilityRequirements::from_manifest`]
//! declares none; [`CapabilityRequirements::with_fallback`] is the hook for
//! the format version or profile resolver that adds them.

use std::fmt;

use glint_schema::ThemeManifest;
use serde::{Deserialize, Serialize};

use crate::capabilities::{Availability, CapabilityId};
use crate::error::{CapabilityError, CapabilityErrorCode};
use crate::service::CapabilitySnapshot;

/// Whether the theme declared a capability as required or optional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityRole {
    /// From `requiredCapabilities`: blocks activation when unusable.
    Required,
    /// From `optionalCapabilities`: disables controls when unusable.
    Optional,
}

/// A required capability and the fallbacks that may stand in for it, tried
/// in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RequiredCapability {
    /// The capability the theme requires.
    pub id: CapabilityId,
    /// Declared fallbacks, in preference order.
    pub fallbacks: Vec<CapabilityId>,
}

/// What a theme needs from the host, in manifest order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityRequirements {
    theme_id: String,
    required: Vec<RequiredCapability>,
    optional: Vec<CapabilityId>,
}

impl CapabilityRequirements {
    /// The requirements a validated manifest declares, with no fallbacks.
    ///
    /// `glint-schema` already enforces the capability ID syntax; should an ID
    /// still fail [`CapabilityId::parse`] the gate fails closed and refuses
    /// activation, listing each such ID as `unknown-capability`.
    pub fn from_manifest(manifest: &ThemeManifest) -> Result<Self, ActivationRefusal> {
        let mut invalid = Vec::new();
        let mut parse_all = |raw: &[String]| -> Vec<CapabilityId> {
            raw.iter()
                .filter_map(|s| match CapabilityId::parse(s) {
                    Ok(id) => Some(id),
                    Err(e) => {
                        invalid.push(Blocker {
                            capability: s.clone(),
                            error: CapabilityError::new(
                                CapabilityErrorCode::UnknownCapability,
                                None,
                                format!("{s:?} is not a capability id: {e}"),
                            ),
                            fallbacks: Vec::new(),
                        });
                        None
                    }
                })
                .collect()
        };
        let required = parse_all(&manifest.required_capabilities);
        let optional = parse_all(&manifest.optional_capabilities);
        if !invalid.is_empty() {
            return Err(ActivationRefusal {
                code: CapabilityErrorCode::ThemeIncompatible,
                theme_id: manifest.id.clone(),
                snapshot_generation: 0,
                blockers: invalid,
                checks: Vec::new(),
            });
        }
        Ok(Self {
            theme_id: manifest.id.clone(),
            required: required
                .into_iter()
                .map(|id| RequiredCapability {
                    id,
                    fallbacks: Vec::new(),
                })
                .collect(),
            optional,
        })
    }

    /// Declares `fallback` as a stand-in for the required capability
    /// `required`. Ignored when `required` is not a required capability.
    #[must_use]
    pub fn with_fallback(mut self, required: &CapabilityId, fallback: CapabilityId) -> Self {
        if let Some(entry) = self.required.iter_mut().find(|r| &r.id == required)
            && !entry.fallbacks.contains(&fallback)
        {
            entry.fallbacks.push(fallback);
        }
        self
    }

    /// The theme's package ID.
    pub fn theme_id(&self) -> &str {
        &self.theme_id
    }

    /// Required capabilities, in manifest order.
    pub fn required(&self) -> &[RequiredCapability] {
        &self.required
    }

    /// Optional capabilities, in manifest order.
    pub fn optional(&self) -> &[CapabilityId] {
        &self.optional
    }
}

/// The verdict for one declared capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum CheckOutcome {
    /// `Supported`: usable without limits.
    Available,
    /// `ReadOnly` or `Experimental`: usable, but controls must show the
    /// limit (read-only control, experimental label).
    Limited,
    /// Required capability unusable, satisfied by a declared fallback.
    Fallback {
        /// The fallback that is used instead.
        fallback: CapabilityId,
    },
    /// Optional capability unusable: its controls are disabled.
    Disabled {
        /// Why, with UI guidance through its code.
        error: CapabilityError,
    },
    /// Required capability unusable and no usable fallback: activation is
    /// refused.
    Blocking {
        /// Why.
        error: CapabilityError,
    },
}

/// One declared capability, its host availability and the verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CapabilityCheck {
    /// The declared capability.
    pub id: CapabilityId,
    /// Required or optional.
    pub role: CapabilityRole,
    /// The host's availability at evaluation time.
    pub availability: Availability,
    /// The verdict.
    pub outcome: CheckOutcome,
}

/// Activation is allowed; what the theme gets and what is degraded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivationReport {
    theme_id: String,
    snapshot_generation: u64,
    checks: Vec<CapabilityCheck>,
}

impl ActivationReport {
    /// The theme's package ID.
    pub fn theme_id(&self) -> &str {
        &self.theme_id
    }

    /// Generation of the snapshot the verdict is based on. A later snapshot
    /// may change it; re-evaluate on capability change events.
    pub fn snapshot_generation(&self) -> u64 {
        self.snapshot_generation
    }

    /// Every declared capability: required first, then optional, each in
    /// manifest order.
    pub fn checks(&self) -> &[CapabilityCheck] {
        &self.checks
    }

    /// Whether anything is not plainly available.
    pub fn is_degraded(&self) -> bool {
        self.checks
            .iter()
            .any(|c| c.outcome != CheckOutcome::Available)
    }

    /// Optional capabilities whose controls must be disabled.
    pub fn disabled(&self) -> impl Iterator<Item = &CapabilityCheck> {
        self.checks
            .iter()
            .filter(|c| matches!(c.outcome, CheckOutcome::Disabled { .. }))
    }

    /// Capabilities usable only with limits (`ReadOnly`, `Experimental`).
    pub fn limited(&self) -> impl Iterator<Item = &CapabilityCheck> {
        self.checks
            .iter()
            .filter(|c| c.outcome == CheckOutcome::Limited)
    }

    /// Capabilities waiting for an OS permission, for the installer to
    /// present.
    pub fn permission_gated(&self) -> impl Iterator<Item = &CapabilityCheck> {
        self.checks
            .iter()
            .filter(|c| c.availability == Availability::PermissionRequired)
    }
}

/// One required capability that blocks activation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Blocker {
    /// The capability as declared in the manifest.
    pub capability: String,
    /// Why it is unusable.
    pub error: CapabilityError,
    /// Declared fallbacks that were tried and were also unusable.
    pub fallbacks: Vec<CapabilityId>,
}

/// Activation is refused (`theme-incompatible`): the current theme is kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivationRefusal {
    code: CapabilityErrorCode,
    theme_id: String,
    snapshot_generation: u64,
    blockers: Vec<Blocker>,
    checks: Vec<CapabilityCheck>,
}

impl ActivationRefusal {
    /// Always [`CapabilityErrorCode::ThemeIncompatible`].
    pub fn code(&self) -> CapabilityErrorCode {
        self.code
    }

    /// The refused theme's package ID.
    pub fn theme_id(&self) -> &str {
        &self.theme_id
    }

    /// Generation of the snapshot the verdict is based on.
    pub fn snapshot_generation(&self) -> u64 {
        self.snapshot_generation
    }

    /// Every unusable required capability, in manifest order.
    pub fn blockers(&self) -> &[Blocker] {
        &self.blockers
    }

    /// Every declared capability with its verdict, for the installer to
    /// present alongside the blockers.
    pub fn checks(&self) -> &[CapabilityCheck] {
        &self.checks
    }
}

impl fmt::Display for ActivationRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: theme {} cannot be activated; missing required capabilities:",
            self.code, self.theme_id
        )?;
        for blocker in &self.blockers {
            write!(f, " {} ({});", blocker.capability, blocker.error.code())?;
        }
        Ok(())
    }
}

impl std::error::Error for ActivationRefusal {}

fn usable_outcome(availability: &Availability) -> Option<CheckOutcome> {
    match availability {
        Availability::Supported => Some(CheckOutcome::Available),
        Availability::ReadOnly | Availability::Experimental => Some(CheckOutcome::Limited),
        Availability::PermissionRequired | Availability::Unavailable { .. } => None,
    }
}

fn unusable_error(id: &CapabilityId, availability: &Availability) -> CapabilityError {
    availability.check(id).err().unwrap_or_else(|| {
        CapabilityError::new(
            CapabilityErrorCode::InvalidState,
            Some(id.clone()),
            "capability reported unusable without a reason",
        )
    })
}

/// Validates `requirements` against `snapshot`. Pure: no state changes.
pub fn evaluate(
    requirements: &CapabilityRequirements,
    snapshot: &CapabilitySnapshot,
) -> Result<ActivationReport, ActivationRefusal> {
    let mut checks = Vec::new();
    let mut blockers = Vec::new();

    for required in &requirements.required {
        let availability = snapshot.availability(&required.id);
        let outcome = match usable_outcome(&availability) {
            Some(outcome) => outcome,
            None => {
                let fallback = required
                    .fallbacks
                    .iter()
                    .find(|f| snapshot.availability(f).is_usable());
                match fallback {
                    Some(fallback) => CheckOutcome::Fallback {
                        fallback: fallback.clone(),
                    },
                    None => {
                        let error = unusable_error(&required.id, &availability);
                        blockers.push(Blocker {
                            capability: required.id.to_string(),
                            error: error.clone(),
                            fallbacks: required.fallbacks.clone(),
                        });
                        CheckOutcome::Blocking { error }
                    }
                }
            }
        };
        checks.push(CapabilityCheck {
            id: required.id.clone(),
            role: CapabilityRole::Required,
            availability,
            outcome,
        });
    }

    for id in &requirements.optional {
        let availability = snapshot.availability(id);
        let outcome = usable_outcome(&availability).unwrap_or_else(|| CheckOutcome::Disabled {
            error: unusable_error(id, &availability),
        });
        checks.push(CapabilityCheck {
            id: id.clone(),
            role: CapabilityRole::Optional,
            availability,
            outcome,
        });
    }

    if blockers.is_empty() {
        Ok(ActivationReport {
            theme_id: requirements.theme_id.clone(),
            snapshot_generation: snapshot.generation(),
            checks,
        })
    } else {
        Err(ActivationRefusal {
            code: CapabilityErrorCode::ThemeIncompatible,
            theme_id: requirements.theme_id.clone(),
            snapshot_generation: snapshot.generation(),
            blockers,
            checks,
        })
    }
}

/// [`evaluate`] for a manifest's declared requirements (no fallbacks).
pub fn evaluate_manifest(
    manifest: &ThemeManifest,
    snapshot: &CapabilitySnapshot,
) -> Result<ActivationReport, ActivationRefusal> {
    evaluate(&CapabilityRequirements::from_manifest(manifest)?, snapshot)
}

/// Runs `activate` only when the manifest's capabilities pass the gate.
///
/// On refusal `activate` is never called, so whatever state it would change
/// (the active theme, profile, surfaces) is left untouched.
pub fn gate_activation<T>(
    manifest: &ThemeManifest,
    snapshot: &CapabilitySnapshot,
    activate: impl FnOnce(&ActivationReport) -> T,
) -> Result<T, ActivationRefusal> {
    let report = evaluate_manifest(manifest, snapshot)?;
    Ok(activate(&report))
}
