//! The capability service (CAP-002): discovers host capabilities through a
//! [`CapabilityAdapter`], keeps the current [`CapabilitySnapshot`] and emits a
//! [`CapabilityChange`] for every availability transition.
//!
//! The service is synchronous and owns no thread. A host adapter learns about
//! changes out of band (a device removed, a permission revoked, a compositor
//! restarted) and its owner calls [`CapabilityService::refresh`]; the service
//! diffs the new discovery against the previous snapshot and notifies every
//! subscriber. Failures fail closed:
//!
//! - before the first successful discovery nothing is usable;
//! - a capability the adapter does not advertise is
//!   [`UnavailableKind::NotAdvertised`] (`unknown-capability`);
//! - when discovery fails (timeout, adapter failure, contradictory report)
//!   every known capability becomes [`UnavailableKind::TemporaryUnavailable`]
//!   until a later refresh succeeds: the last known `Supported` is never
//!   reported while the adapter cannot confirm it.

use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver, Sender};

use serde::{Deserialize, Serialize};

use crate::capabilities::{
    Availability, Capability, CapabilityId, UnavailableKind, UnavailableReason,
};
use crate::error::{CapabilityError, CapabilityErrorCode};

/// A source of host capabilities: a native OS adapter, or
/// [`crate::mock::MockAdapter`] in tests.
pub trait CapabilityAdapter {
    /// Short adapter name used in diagnostics, e.g. `windows` or `mock`.
    fn name(&self) -> &str;

    /// Reports every capability the host advertises, with its current
    /// availability. A capability must appear at most once. Errors should use
    /// [`CapabilityErrorCode::Timeout`] or
    /// [`CapabilityErrorCode::AdapterFailure`].
    fn discover(&self) -> Result<Vec<Capability>, CapabilityError>;
}

/// How a [`CapabilityChange`] affected the adapter's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ChangeKind {
    /// The adapter started advertising the capability.
    Added,
    /// The capability's availability changed.
    Changed,
    /// The adapter stopped advertising the capability.
    Removed,
}

/// One availability transition, delivered to every subscriber.
///
/// For [`ChangeKind::Added`] `previous` (and for [`ChangeKind::Removed`]
/// `current`) is the `NotAdvertised` unavailable state the service reports
/// for capabilities the adapter does not list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct CapabilityChange {
    /// Snapshot generation that contains this change.
    pub generation: u64,
    /// The capability that changed.
    pub id: CapabilityId,
    /// Availability before the change.
    pub previous: Availability,
    /// Availability after the change.
    pub current: Availability,
    kind: ChangeKind,
}

impl CapabilityChange {
    /// Whether the capability appeared, changed or disappeared.
    pub fn kind(&self) -> ChangeKind {
        self.kind
    }

    /// The capability was usable and no longer is (device removed,
    /// permission revoked, adapter down). UI must disable dependent controls.
    pub fn is_revocation(&self) -> bool {
        self.previous.is_usable() && !self.current.is_usable()
    }

    /// The capability was not usable and now is.
    pub fn became_usable(&self) -> bool {
        !self.previous.is_usable() && self.current.is_usable()
    }
}

/// The capabilities known at one moment, as last reported by the adapter.
///
/// JSON form: `{"generation": n, "adapter": "...", "capabilities": [...],
/// "adapterError": {...}}` with capabilities sorted by ID and
/// `adapterError` omitted when the last discovery succeeded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawSnapshot", into = "RawSnapshot")]
pub struct CapabilitySnapshot {
    generation: u64,
    adapter: String,
    capabilities: BTreeMap<CapabilityId, Capability>,
    adapter_error: Option<CapabilityError>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawSnapshot {
    generation: u64,
    adapter: String,
    capabilities: Vec<Capability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    adapter_error: Option<CapabilityError>,
}

impl TryFrom<RawSnapshot> for CapabilitySnapshot {
    type Error = CapabilityError;

    fn try_from(raw: RawSnapshot) -> Result<Self, Self::Error> {
        Ok(Self {
            generation: raw.generation,
            capabilities: index(&raw.adapter, raw.capabilities)?,
            adapter: raw.adapter,
            adapter_error: raw.adapter_error,
        })
    }
}

impl From<CapabilitySnapshot> for RawSnapshot {
    fn from(snapshot: CapabilitySnapshot) -> Self {
        Self {
            generation: snapshot.generation,
            adapter: snapshot.adapter,
            capabilities: snapshot.capabilities.into_values().collect(),
            adapter_error: snapshot.adapter_error,
        }
    }
}

fn index(
    adapter: &str,
    capabilities: Vec<Capability>,
) -> Result<BTreeMap<CapabilityId, Capability>, CapabilityError> {
    let mut map = BTreeMap::new();
    for capability in capabilities {
        let id = capability.id.clone();
        if map.insert(id.clone(), capability).is_some() {
            return Err(CapabilityError::new(
                CapabilityErrorCode::AdapterFailure,
                Some(id),
                format!("adapter {adapter} reported this capability more than once"),
            ));
        }
    }
    Ok(map)
}

impl CapabilitySnapshot {
    /// An empty snapshot (generation 0): nothing discovered, nothing usable.
    pub fn empty(adapter: impl Into<String>) -> Self {
        Self {
            generation: 0,
            adapter: adapter.into(),
            capabilities: BTreeMap::new(),
            adapter_error: None,
        }
    }

    /// Increases whenever the snapshot's content changes; 0 means no
    /// discovery has completed.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Name of the adapter that produced the snapshot.
    pub fn adapter(&self) -> &str {
        &self.adapter
    }

    /// The error of the last discovery, if it failed.
    pub fn adapter_error(&self) -> Option<&CapabilityError> {
        self.adapter_error.as_ref()
    }

    /// The capability as advertised, if the adapter lists it.
    pub fn get(&self, id: &CapabilityId) -> Option<&Capability> {
        self.capabilities.get(id)
    }

    /// Every advertised capability, sorted by ID.
    pub fn iter(&self) -> impl Iterator<Item = &Capability> {
        self.capabilities.values()
    }

    /// The capability's availability; a capability the adapter does not
    /// advertise is `Unavailable` with [`UnavailableKind::NotAdvertised`].
    pub fn availability(&self, id: &CapabilityId) -> Availability {
        match self.capabilities.get(id) {
            Some(capability) => capability.availability.clone(),
            None => self.not_advertised(),
        }
    }

    /// The capability when it is usable (`Supported`, `Experimental` or
    /// `ReadOnly`), otherwise the typed error to surface.
    pub fn require(&self, id: &CapabilityId) -> Result<Capability, CapabilityError> {
        self.availability(id).check(id)?;
        self.capabilities.get(id).cloned().ok_or_else(|| {
            CapabilityError::new(
                CapabilityErrorCode::UnknownCapability,
                Some(id.clone()),
                "not advertised",
            )
        })
    }

    /// Like [`Self::require`], but a `ReadOnly` capability is refused with
    /// [`CapabilityErrorCode::ReadOnly`].
    pub fn require_writable(&self, id: &CapabilityId) -> Result<Capability, CapabilityError> {
        let capability = self.require(id)?;
        capability.availability.check_writable(id)?;
        Ok(capability)
    }

    fn not_advertised(&self) -> Availability {
        let detail = if self.generation == 0 {
            format!(
                "no capability discovery has completed for adapter {}",
                self.adapter
            )
        } else {
            format!("not advertised by adapter {}", self.adapter)
        };
        Availability::Unavailable {
            reason: UnavailableReason::internal(UnavailableKind::NotAdvertised, detail),
        }
    }
}

/// Discovers capabilities through an adapter, keeps the current snapshot and
/// notifies subscribers of every availability transition.
pub struct CapabilityService<A> {
    adapter: A,
    snapshot: CapabilitySnapshot,
    subscribers: Vec<Sender<CapabilityChange>>,
}

impl<A: CapabilityAdapter> CapabilityService<A> {
    /// A service with an empty snapshot; call [`Self::refresh`] to discover.
    pub fn new(adapter: A) -> Self {
        let snapshot = CapabilitySnapshot::empty(adapter.name());
        Self {
            adapter,
            snapshot,
            subscribers: Vec::new(),
        }
    }

    /// The adapter.
    pub fn adapter(&self) -> &A {
        &self.adapter
    }

    /// The current snapshot.
    pub fn snapshot(&self) -> &CapabilitySnapshot {
        &self.snapshot
    }

    /// See [`CapabilitySnapshot::availability`].
    pub fn availability(&self, id: &CapabilityId) -> Availability {
        self.snapshot.availability(id)
    }

    /// See [`CapabilitySnapshot::require`].
    pub fn require(&self, id: &CapabilityId) -> Result<Capability, CapabilityError> {
        self.snapshot.require(id)
    }

    /// See [`CapabilitySnapshot::require_writable`].
    pub fn require_writable(&self, id: &CapabilityId) -> Result<Capability, CapabilityError> {
        self.snapshot.require_writable(id)
    }

    /// Receives every future [`CapabilityChange`]. Dropping the receiver
    /// unsubscribes.
    pub fn subscribe(&mut self) -> Receiver<CapabilityChange> {
        let (tx, rx) = mpsc::channel();
        self.subscribers.push(tx);
        rx
    }

    /// Live subscribers (dropped receivers are pruned on the next
    /// notification).
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }

    /// Re-runs discovery, replaces the snapshot and notifies subscribers of
    /// every availability transition, which are also returned.
    ///
    /// On a discovery error the error is returned, recorded in the snapshot,
    /// and every known capability becomes temporarily unavailable (with
    /// change events) until a later refresh succeeds.
    pub fn refresh(&mut self) -> Result<Vec<CapabilityChange>, CapabilityError> {
        let name = self.adapter.name().to_owned();
        let discovered = self
            .adapter
            .discover()
            .and_then(|capabilities| index(&name, capabilities));
        let (capabilities, error) = match discovered {
            Ok(capabilities) => (capabilities, None),
            Err(error) => (
                degrade(&name, &self.snapshot.capabilities, &error),
                Some(error),
            ),
        };

        let next = CapabilitySnapshot {
            generation: self.snapshot.generation,
            adapter: name,
            capabilities,
            adapter_error: error.clone(),
        };
        let changes = if next == self.snapshot {
            Vec::new()
        } else {
            let generation = self.snapshot.generation + 1;
            let changes = diff(&self.snapshot, &next, generation);
            self.snapshot = CapabilitySnapshot { generation, ..next };
            changes
        };
        self.notify(&changes);
        match error {
            Some(error) => Err(error),
            None => Ok(changes),
        }
    }

    fn notify(&mut self, changes: &[CapabilityChange]) {
        if changes.is_empty() {
            return;
        }
        self.subscribers
            .retain(|tx| changes.iter().all(|change| tx.send(change.clone()).is_ok()));
    }
}

fn degrade(
    adapter: &str,
    known: &BTreeMap<CapabilityId, Capability>,
    error: &CapabilityError,
) -> BTreeMap<CapabilityId, Capability> {
    let detail = format!(
        "adapter {adapter} is not reporting capabilities ({})",
        error.code()
    );
    known
        .iter()
        .map(|(id, capability)| {
            let mut capability = capability.clone();
            capability.availability = Availability::Unavailable {
                reason: UnavailableReason::internal(
                    UnavailableKind::TemporaryUnavailable,
                    detail.clone(),
                ),
            };
            (id.clone(), capability)
        })
        .collect()
}

fn diff(
    old: &CapabilitySnapshot,
    new: &CapabilitySnapshot,
    generation: u64,
) -> Vec<CapabilityChange> {
    let mut ids: Vec<&CapabilityId> = old.capabilities.keys().collect();
    ids.extend(new.capabilities.keys());
    ids.sort();
    ids.dedup();

    let mut changes = Vec::new();
    for id in ids {
        let kind = match (old.capabilities.get(id), new.capabilities.get(id)) {
            (None, Some(_)) => ChangeKind::Added,
            (Some(_), None) => ChangeKind::Removed,
            (Some(before), Some(after)) if before.availability != after.availability => {
                ChangeKind::Changed
            }
            _ => continue,
        };
        changes.push(CapabilityChange {
            generation,
            id: id.clone(),
            previous: old.availability(id),
            current: new.availability(id),
            kind,
        });
    }
    changes
}
