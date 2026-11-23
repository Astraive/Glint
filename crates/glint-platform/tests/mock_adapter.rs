//! CAP-002: the capability service driven by the full-state mock adapter.
//! Every availability state is observable, live transitions (device removed,
//! permission revoked) emit change events, and nothing unusable is ever
//! reported as success.

use std::sync::mpsc::Receiver;

use glint_platform::mock::MockAdapter;
use glint_platform::{
    Availability, Capability, CapabilityAdapter, CapabilityChange, CapabilityError,
    CapabilityErrorCode, CapabilityId, CapabilityService, CapabilitySnapshot, ChangeKind,
    IntegrationEffect, UnavailableKind,
};

fn id(s: &str) -> CapabilityId {
    CapabilityId::parse(s).unwrap()
}

fn cap(s: &str, availability: Availability) -> Capability {
    Capability::new(id(s), availability, IntegrationEffect::GlintOwnedUi)
}

fn drain(rx: &Receiver<CapabilityChange>) -> Vec<CapabilityChange> {
    rx.try_iter().collect()
}

fn audio_service() -> (MockAdapter, CapabilityService<MockAdapter>) {
    let mock = MockAdapter::new("mock")
        .with(cap("system.audio.write", Availability::Supported))
        .with(cap("system.brightness.read", Availability::Supported));
    let mut service = CapabilityService::new(mock.clone());
    service.refresh().unwrap();
    (mock, service)
}

#[test]
fn every_availability_state_is_observable_through_the_service() {
    let mock = MockAdapter::all_states();
    let expected: Vec<Capability> = mock.capabilities();
    // One capability per state, including every unavailable reason.
    assert_eq!(expected.len(), 4 + UnavailableKind::ALL.len());

    let mut service = CapabilityService::new(mock);
    service.refresh().unwrap();
    for capability in &expected {
        assert_eq!(
            service.availability(&capability.id),
            capability.availability,
            "{}",
            capability.id
        );
        assert_eq!(service.snapshot().get(&capability.id), Some(capability));
    }
    let states: Vec<&Availability> = expected.iter().map(|c| &c.availability).collect();
    assert!(states.contains(&&Availability::Supported));
    assert!(states.contains(&&Availability::ReadOnly));
    assert!(states.contains(&&Availability::PermissionRequired));
    assert!(states.contains(&&Availability::Experimental));
    for kind in UnavailableKind::ALL {
        assert!(states.iter().any(|a| matches!(
            a,
            Availability::Unavailable { reason } if reason.kind() == *kind
        )));
    }
}

#[test]
fn unusable_states_never_masquerade_as_success() {
    let mock = MockAdapter::all_states();
    let all = mock.capabilities();
    let mut service = CapabilityService::new(mock);
    service.refresh().unwrap();

    for capability in all {
        let result = service.require(&capability.id);
        match &capability.availability {
            Availability::Supported | Availability::Experimental | Availability::ReadOnly => {
                assert_eq!(result.unwrap(), capability);
            }
            Availability::PermissionRequired => {
                let err = result.unwrap_err();
                assert_eq!(err.code(), CapabilityErrorCode::PermissionRequired);
                assert_eq!(err.capability(), Some(&capability.id));
            }
            Availability::Unavailable { reason } => {
                let err = result.unwrap_err();
                assert_eq!(err.code(), reason.kind().code());
                assert_eq!(err.capability(), Some(&capability.id));
            }
            other => panic!("unexpected state {other:?}"),
        }

        let writable = service.require_writable(&capability.id);
        assert_eq!(writable.is_ok(), capability.availability.is_writable());
        if capability.availability == Availability::ReadOnly {
            assert_eq!(writable.unwrap_err().code(), CapabilityErrorCode::ReadOnly);
        }
    }
}

#[test]
fn unknown_and_unrefreshed_capabilities_fail_closed() {
    let mock = MockAdapter::new("mock").with(cap("surface.panel", Availability::Supported));
    let mut service = CapabilityService::new(mock);

    // Nothing has been discovered yet: nothing is usable.
    assert_eq!(service.snapshot().generation(), 0);
    let err = service.require(&id("surface.panel")).unwrap_err();
    assert_eq!(err.code(), CapabilityErrorCode::UnknownCapability);

    service.refresh().unwrap();
    assert!(service.require(&id("surface.panel")).is_ok());

    let unknown = id("system.bluetooth.toggle");
    match service.availability(&unknown) {
        Availability::Unavailable { reason } => {
            assert_eq!(reason.kind(), UnavailableKind::NotAdvertised);
            assert!(reason.detail().contains("mock"), "{}", reason.detail());
        }
        other => panic!("unknown capability reported as {other:?}"),
    }
    assert_eq!(
        service.require(&unknown).unwrap_err().code(),
        CapabilityErrorCode::UnknownCapability
    );
}

#[test]
fn permission_revocation_emits_a_change_event() {
    let (mock, mut service) = audio_service();
    let rx = service.subscribe();
    let audio = id("system.audio.write");

    mock.revoke_permission(&audio).unwrap();
    let changes = service.refresh().unwrap();

    assert_eq!(changes.len(), 1);
    assert_eq!(drain(&rx), changes);
    let change = &changes[0];
    assert_eq!(change.id, audio);
    assert_eq!(change.previous, Availability::Supported);
    assert_eq!(change.current, Availability::PermissionRequired);
    assert_eq!(change.kind(), ChangeKind::Changed);
    assert!(change.is_revocation());
    assert_eq!(change.generation, service.snapshot().generation());
    assert_eq!(
        service.require(&audio).unwrap_err().code(),
        CapabilityErrorCode::PermissionRequired
    );
}

#[test]
fn device_removal_emits_a_change_event() {
    let (mock, mut service) = audio_service();
    let rx = service.subscribe();
    let brightness = id("system.brightness.read");

    mock.remove_device(&brightness, "external display unplugged")
        .unwrap();
    service.refresh().unwrap();

    let changes = drain(&rx);
    assert_eq!(changes.len(), 1);
    assert!(changes[0].is_revocation());
    let err = service.require(&brightness).unwrap_err();
    assert_eq!(err.code(), CapabilityErrorCode::HardwareUnavailable);
    assert!(err.message().contains("unplugged"));

    // The device comes back.
    mock.set_availability(&brightness, Availability::Supported)
        .unwrap();
    service.refresh().unwrap();
    let back = drain(&rx);
    assert_eq!(back.len(), 1);
    assert!(back[0].became_usable());
    assert!(!back[0].is_revocation());
    assert!(service.require(&brightness).is_ok());
}

#[test]
fn capabilities_appearing_and_disappearing_are_reported() {
    let (mock, mut service) = audio_service();
    let rx = service.subscribe();

    mock.set(cap("desktop.workspaces.read", Availability::Experimental));
    service.refresh().unwrap();
    let added = drain(&rx);
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].kind(), ChangeKind::Added);
    assert_eq!(added[0].current, Availability::Experimental);

    mock.remove(&id("desktop.workspaces.read"));
    service.refresh().unwrap();
    let removed = drain(&rx);
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].kind(), ChangeKind::Removed);
    assert!(removed[0].is_revocation());
    assert_eq!(
        service
            .require(&id("desktop.workspaces.read"))
            .unwrap_err()
            .code(),
        CapabilityErrorCode::UnknownCapability
    );
}

#[test]
fn walking_one_capability_through_every_state_emits_one_event_per_transition() {
    let (mock, mut service) = audio_service();
    let rx = service.subscribe();
    let audio = id("system.audio.write");

    let mut path = vec![
        Availability::ReadOnly,
        Availability::PermissionRequired,
        Availability::Experimental,
    ];
    for kind in UnavailableKind::ALL {
        path.push(Availability::unavailable(*kind, "synthetic").unwrap());
    }
    path.push(Availability::Supported);

    let mut previous = Availability::Supported;
    for state in path {
        mock.set_availability(&audio, state.clone()).unwrap();
        service.refresh().unwrap();
        let changes = drain(&rx);
        assert_eq!(changes.len(), 1, "transition to {state:?}");
        assert_eq!(changes[0].previous, previous);
        assert_eq!(changes[0].current, state);
        assert_eq!(service.availability(&audio), state);
        previous = state;
    }
}

#[test]
fn unchanged_refresh_emits_nothing() {
    let (_mock, mut service) = audio_service();
    let rx = service.subscribe();
    let before = service.snapshot().generation();
    assert!(service.refresh().unwrap().is_empty());
    assert!(drain(&rx).is_empty());
    assert_eq!(service.snapshot().generation(), before);
}

#[test]
fn adapter_timeout_degrades_everything_and_recovers() {
    let (mock, mut service) = audio_service();
    let rx = service.subscribe();

    mock.fail_with(Some(CapabilityError::new(
        CapabilityErrorCode::Timeout,
        None,
        "discovery took longer than 2s",
    )));
    let err = service.refresh().unwrap_err();
    assert_eq!(err.code(), CapabilityErrorCode::Timeout);
    assert_eq!(service.snapshot().adapter_error(), Some(&err));

    // Last known Supported is not reported as success while the adapter is down.
    let changes = drain(&rx);
    assert_eq!(changes.len(), 2);
    for change in &changes {
        assert!(change.is_revocation());
        let err = service.require(&change.id).unwrap_err();
        assert_eq!(err.code(), CapabilityErrorCode::TemporaryUnavailable);
    }

    mock.fail_with(None);
    let restored = service.refresh().unwrap();
    assert_eq!(restored.len(), 2);
    assert!(restored.iter().all(CapabilityChange::became_usable));
    assert_eq!(service.snapshot().adapter_error(), None);
    assert!(service.require(&id("system.audio.write")).is_ok());
}

#[test]
fn adapter_failure_is_typed_and_distinct_from_timeout() {
    let (mock, mut service) = audio_service();
    mock.fail_with(Some(CapabilityError::new(
        CapabilityErrorCode::AdapterFailure,
        None,
        "backend crashed",
    )));
    let err = service.refresh().unwrap_err();
    assert_eq!(err.code(), CapabilityErrorCode::AdapterFailure);
    assert_ne!(err.category(), CapabilityErrorCode::Timeout.category());
    assert!(service.require(&id("system.audio.write")).is_err());
}

struct DuplicatingAdapter;

impl CapabilityAdapter for DuplicatingAdapter {
    fn name(&self) -> &str {
        "duplicating"
    }

    fn discover(&self) -> Result<Vec<Capability>, CapabilityError> {
        Ok(vec![
            cap("surface.panel", Availability::Supported),
            cap(
                "surface.panel",
                Availability::unavailable(UnavailableKind::UnsupportedSession, "no layer shell")
                    .unwrap(),
            ),
        ])
    }
}

#[test]
fn contradictory_discovery_is_refused_not_guessed() {
    let mut service = CapabilityService::new(DuplicatingAdapter);
    let err = service.refresh().unwrap_err();
    assert_eq!(err.code(), CapabilityErrorCode::AdapterFailure);
    assert_eq!(err.capability(), Some(&id("surface.panel")));
    assert!(service.require(&id("surface.panel")).is_err());
}

#[test]
fn every_subscriber_is_notified_and_dropped_ones_are_pruned() {
    let (mock, mut service) = audio_service();
    let first = service.subscribe();
    let second = service.subscribe();
    let dropped = service.subscribe();
    drop(dropped);

    mock.revoke_permission(&id("system.audio.write")).unwrap();
    service.refresh().unwrap();
    assert_eq!(drain(&first).len(), 1);
    assert_eq!(drain(&second).len(), 1);
    assert_eq!(service.subscriber_count(), 2);
}

#[test]
fn mock_refuses_to_change_a_capability_it_does_not_have() {
    let mock = MockAdapter::new("mock");
    let err = mock
        .set_availability(&id("surface.panel"), Availability::Supported)
        .unwrap_err();
    assert_eq!(err.code(), CapabilityErrorCode::InvalidState);
    assert_eq!(
        mock.revoke_permission(&id("surface.panel"))
            .unwrap_err()
            .code(),
        CapabilityErrorCode::InvalidState
    );
}

#[test]
fn snapshots_and_changes_round_trip_through_json() {
    let (mock, mut service) = audio_service();
    let rx = service.subscribe();
    mock.revoke_permission(&id("system.audio.write")).unwrap();
    service.refresh().unwrap();

    let snapshot = service.snapshot().clone();
    let text = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(
        serde_json::from_str::<CapabilitySnapshot>(&text).unwrap(),
        snapshot
    );

    let change = drain(&rx).remove(0);
    let value = serde_json::to_value(&change).unwrap();
    assert_eq!(value["id"], "system.audio.write");
    assert_eq!(value["previous"]["state"], "supported");
    assert_eq!(value["current"]["state"], "permission-required");
    assert_eq!(
        serde_json::from_value::<CapabilityChange>(value).unwrap(),
        change
    );
}
