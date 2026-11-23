//! A full-state mock [`CapabilityAdapter`] (CAP-002) for tests and for
//! running Glint without a native adapter.
//!
//! Every [`Availability`] state can be configured, changed live (device
//! removed, permission revoked, capability gone) and discovery can be made to
//! fail with a typed error. A [`MockAdapter`] is a cheap handle: clones share
//! state, so a test keeps one clone to drive transitions while the
//! [`crate::CapabilityService`] owns another. It reads no OS state.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::capabilities::{
    Availability, Capability, CapabilityId, IntegrationEffect, UnavailableKind, UnavailableReason,
};
use crate::error::{CapabilityError, CapabilityErrorCode};
use crate::service::CapabilityAdapter;

#[derive(Debug, Default)]
struct State {
    capabilities: BTreeMap<CapabilityId, Capability>,
    failure: Option<CapabilityError>,
    discover_calls: usize,
}

/// A scriptable capability adapter. See the module documentation.
#[derive(Debug, Clone)]
pub struct MockAdapter {
    name: Arc<str>,
    state: Arc<Mutex<State>>,
}

impl MockAdapter {
    /// An adapter called `name` that advertises nothing.
    pub fn new(name: &str) -> Self {
        Self {
            name: Arc::from(name),
            state: Arc::default(),
        }
    }

    /// An adapter called `mock` advertising one capability in every state:
    /// `Supported`, `ReadOnly`, `PermissionRequired`, `Experimental` and
    /// `Unavailable` for each [`UnavailableKind`].
    pub fn all_states() -> Self {
        let unavailable = |kind, detail: &str| Availability::Unavailable {
            reason: UnavailableReason::internal(kind, detail.to_owned()),
        };
        let rows = [
            (
                "surface.panel",
                Availability::Supported,
                IntegrationEffect::GlintOwnedUi,
            ),
            (
                "system.brightness.write",
                Availability::ReadOnly,
                IntegrationEffect::OsManagedSetting,
            ),
            (
                "desktop.windows.list",
                Availability::PermissionRequired,
                IntegrationEffect::GlintOwnedUi,
            ),
            (
                "desktop.workspaces.switch",
                Availability::Experimental,
                IntegrationEffect::OsManagedSetting,
            ),
            (
                "native.taskbar.modify",
                unavailable(
                    UnavailableKind::UnsupportedPlatform,
                    "the OS taskbar cannot be replaced",
                ),
                IntegrationEffect::OsManagedSetting,
            ),
            (
                "host.layer-shell",
                unavailable(
                    UnavailableKind::UnsupportedSession,
                    "compositor does not advertise layer shell",
                ),
                IntegrationEffect::GlintOwnedUi,
            ),
            (
                "system.brightness.read",
                unavailable(
                    UnavailableKind::HardwareUnavailable,
                    "external display has no backlight control",
                ),
                IntegrationEffect::GlintOwnedUi,
            ),
            (
                "system.network.read",
                unavailable(
                    UnavailableKind::TemporaryUnavailable,
                    "network service restarting",
                ),
                IntegrationEffect::GlintOwnedUi,
            ),
            (
                "host.appbar",
                unavailable(
                    UnavailableKind::Conflict,
                    "another appbar occupies the top edge",
                ),
                IntegrationEffect::GlintOwnedUi,
            ),
            (
                "system.bluetooth.read",
                unavailable(
                    UnavailableKind::NotAdvertised,
                    "no bluetooth backend in this mock",
                ),
                IntegrationEffect::GlintOwnedUi,
            ),
        ];
        let mock = Self::new("mock");
        for (id, availability, effect) in rows {
            let id = CapabilityId::parse(id).unwrap_or_else(|e| unreachable!("fixture id: {e}"));
            mock.set(Capability::new(id, availability, effect));
        }
        mock
    }

    /// Builder form of [`Self::set`].
    #[must_use]
    pub fn with(self, capability: Capability) -> Self {
        self.set(capability);
        self
    }

    /// Advertises `capability`, replacing any previous entry with its ID.
    pub fn set(&self, capability: Capability) {
        self.lock()
            .capabilities
            .insert(capability.id.clone(), capability);
    }

    /// Changes the availability of an advertised capability. Fails with
    /// [`CapabilityErrorCode::InvalidState`] when the capability is not
    /// advertised (use [`Self::set`] to add one).
    pub fn set_availability(
        &self,
        id: &CapabilityId,
        availability: Availability,
    ) -> Result<(), CapabilityError> {
        let mut state = self.lock();
        let capability = state.capabilities.get_mut(id).ok_or_else(|| {
            CapabilityError::new(
                CapabilityErrorCode::InvalidState,
                Some(id.clone()),
                "the mock adapter does not advertise this capability",
            )
        })?;
        capability.availability = availability;
        Ok(())
    }

    /// The OS revoked the permission: the capability becomes
    /// `PermissionRequired`.
    pub fn revoke_permission(&self, id: &CapabilityId) -> Result<(), CapabilityError> {
        self.set_availability(id, Availability::PermissionRequired)
    }

    /// The device behind the capability went away: it becomes `Unavailable`
    /// with [`UnavailableKind::HardwareUnavailable`]. A blank `detail` is
    /// replaced by `device removed`.
    pub fn remove_device(&self, id: &CapabilityId, detail: &str) -> Result<(), CapabilityError> {
        let detail = if detail.trim().is_empty() {
            "device removed"
        } else {
            detail
        };
        self.set_availability(
            id,
            Availability::Unavailable {
                reason: UnavailableReason::internal(
                    UnavailableKind::HardwareUnavailable,
                    detail.to_owned(),
                ),
            },
        )
    }

    /// Stops advertising the capability; returns what was advertised.
    pub fn remove(&self, id: &CapabilityId) -> Option<Capability> {
        self.lock().capabilities.remove(id)
    }

    /// Makes every discovery fail with `error` until called with `None`.
    pub fn fail_with(&self, error: Option<CapabilityError>) {
        self.lock().failure = error;
    }

    /// Every advertised capability, sorted by ID.
    pub fn capabilities(&self) -> Vec<Capability> {
        self.lock().capabilities.values().cloned().collect()
    }

    /// How many times [`CapabilityAdapter::discover`] has run.
    pub fn discover_calls(&self) -> usize {
        self.lock().discover_calls
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // The state is plain data; a panic elsewhere cannot leave it torn.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl CapabilityAdapter for MockAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn discover(&self) -> Result<Vec<Capability>, CapabilityError> {
        let mut state = self.lock();
        state.discover_calls += 1;
        if let Some(error) = &state.failure {
            return Err(error.clone());
        }
        Ok(state.capabilities.values().cloned().collect())
    }
}
