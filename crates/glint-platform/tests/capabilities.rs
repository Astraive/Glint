//! CAP-001: capability serialization, ID validation and error codes.

use glint_platform::{
    Availability, Capability, CapabilityError, CapabilityErrorCode, CapabilityId,
    CapabilityIdError, ErrorCategory, IntegrationEffect, Namespace, UnavailableKind,
    UnavailableReason,
};
use serde_json::json;

fn id(s: &str) -> CapabilityId {
    CapabilityId::parse(s).expect("valid capability id")
}

fn every_availability() -> Vec<Availability> {
    let mut all = vec![
        Availability::Supported,
        Availability::ReadOnly,
        Availability::PermissionRequired,
        Availability::Experimental,
    ];
    for kind in UnavailableKind::ALL {
        all.push(Availability::unavailable(*kind, "synthetic reason").unwrap());
    }
    all
}

#[test]
fn valid_ids_parse_with_their_namespace() {
    for (raw, ns) in [
        ("surface.panel", Namespace::Surface),
        ("system.audio.write", Namespace::System),
        ("desktop.workspaces.switch", Namespace::Desktop),
        ("native.system-theme.apply", Namespace::Native),
        ("host.layer-shell", Namespace::Host),
        ("surface.notification-overlay", Namespace::Surface),
        ("system.a1.b2", Namespace::System),
    ] {
        let parsed = id(raw);
        assert_eq!(parsed.as_str(), raw);
        assert_eq!(parsed.namespace(), ns);
        assert_eq!(parsed.to_string(), raw);
        assert_eq!(raw.parse::<CapabilityId>().unwrap(), parsed);
    }
}

#[test]
fn invalid_ids_are_rejected_with_a_typed_reason() {
    let cases: &[(&str, CapabilityIdError)] = &[
        ("", CapabilityIdError::Empty),
        ("surface", CapabilityIdError::MissingName),
        ("panel.surface", CapabilityIdError::UnknownNamespace),
        ("Surface.panel", CapabilityIdError::UnknownNamespace),
        ("network.read", CapabilityIdError::UnknownNamespace),
        ("surface.Panel", CapabilityIdError::InvalidSegment),
        ("surface.1panel", CapabilityIdError::InvalidSegment),
        ("surface.-panel", CapabilityIdError::InvalidSegment),
        ("surface.panel-", CapabilityIdError::InvalidSegment),
        ("surface.pa--nel", CapabilityIdError::InvalidSegment),
        ("surface..panel", CapabilityIdError::InvalidSegment),
        ("surface.panel.", CapabilityIdError::InvalidSegment),
        ("surface.pa nel", CapabilityIdError::InvalidSegment),
        ("surface.pa_nel", CapabilityIdError::InvalidSegment),
        ("surface.panél", CapabilityIdError::InvalidSegment),
        (" surface.panel", CapabilityIdError::UnknownNamespace),
    ];
    for (raw, expected) in cases {
        assert_eq!(
            CapabilityId::parse(raw).unwrap_err(),
            *expected,
            "{raw:?} should be rejected"
        );
    }
    let too_long = format!("system.{}", "a".repeat(CapabilityId::MAX_LEN));
    assert_eq!(
        CapabilityId::parse(&too_long).unwrap_err(),
        CapabilityIdError::TooLong
    );
    let max = format!(
        "system.{}",
        "a".repeat(CapabilityId::MAX_LEN - "system.".len())
    );
    assert!(CapabilityId::parse(&max).is_ok());
}

#[test]
fn ids_round_trip_as_plain_json_strings_and_invalid_ones_do_not_deserialize() {
    let parsed = id("system.audio.read");
    let text = serde_json::to_string(&parsed).unwrap();
    assert_eq!(text, "\"system.audio.read\"");
    assert_eq!(serde_json::from_str::<CapabilityId>(&text).unwrap(), parsed);
    assert!(serde_json::from_str::<CapabilityId>("\"bogus.read\"").is_err());
    assert!(serde_json::from_str::<CapabilityId>("\"surface\"").is_err());
    assert!(serde_json::from_str::<CapabilityId>("42").is_err());
}

#[test]
fn availability_round_trips_through_json_in_every_state() {
    for availability in every_availability() {
        let text = serde_json::to_string(&availability).unwrap();
        let back: Availability = serde_json::from_str(&text).unwrap();
        assert_eq!(back, availability, "{text}");
    }
}

#[test]
fn availability_json_shape_is_stable() {
    assert_eq!(
        serde_json::to_value(Availability::Supported).unwrap(),
        json!({"state": "supported"})
    );
    assert_eq!(
        serde_json::to_value(Availability::ReadOnly).unwrap(),
        json!({"state": "read-only"})
    );
    assert_eq!(
        serde_json::to_value(Availability::PermissionRequired).unwrap(),
        json!({"state": "permission-required"})
    );
    assert_eq!(
        serde_json::to_value(Availability::Experimental).unwrap(),
        json!({"state": "experimental"})
    );
    assert_eq!(
        serde_json::to_value(
            Availability::unavailable(UnavailableKind::HardwareUnavailable, "no backlight")
                .unwrap()
        )
        .unwrap(),
        json!({
            "state": "unavailable",
            "reason": {"kind": "hardware-unavailable", "detail": "no backlight"}
        })
    );
}

#[test]
fn unavailable_always_carries_a_reason() {
    assert!(UnavailableReason::new(UnavailableKind::UnsupportedPlatform, "").is_err());
    assert!(UnavailableReason::new(UnavailableKind::UnsupportedPlatform, "   ").is_err());
    assert!(Availability::unavailable(UnavailableKind::TemporaryUnavailable, "\t\n").is_err());
    // The JSON form cannot smuggle in a reasonless state either.
    for bad in [
        json!({"state": "unavailable"}),
        json!({"state": "unavailable", "reason": {"kind": "hardware-unavailable"}}),
        json!({"state": "unavailable", "reason": {"kind": "hardware-unavailable", "detail": ""}}),
        json!({"state": "unavailable", "reason": {"kind": "nope", "detail": "x"}}),
        json!({"state": "unavailable", "reason": "a bare string"}),
        json!({"state": "maybe"}),
    ] {
        assert!(
            serde_json::from_value::<Availability>(bad.clone()).is_err(),
            "{bad} should not deserialize"
        );
    }
    let reason = UnavailableReason::new(UnavailableKind::HardwareUnavailable, "unplugged").unwrap();
    assert_eq!(reason.kind(), UnavailableKind::HardwareUnavailable);
    assert_eq!(reason.detail(), "unplugged");
}

#[test]
fn only_supported_states_are_usable() {
    assert!(Availability::Supported.is_usable());
    assert!(Availability::Experimental.is_usable());
    assert!(Availability::ReadOnly.is_usable());
    assert!(!Availability::PermissionRequired.is_usable());
    for kind in UnavailableKind::ALL {
        assert!(!Availability::unavailable(*kind, "x").unwrap().is_usable());
    }
    assert!(Availability::Supported.is_writable());
    assert!(Availability::Experimental.is_writable());
    assert!(!Availability::ReadOnly.is_writable());
    assert!(!Availability::PermissionRequired.is_writable());
}

#[test]
fn non_usable_states_map_to_a_typed_error_never_success() {
    let cap = id("system.brightness.write");
    assert_eq!(Availability::Supported.check(&cap), Ok(()));
    assert_eq!(Availability::Experimental.check(&cap), Ok(()));
    assert_eq!(Availability::ReadOnly.check(&cap), Ok(()));

    let err = Availability::PermissionRequired.check(&cap).unwrap_err();
    assert_eq!(err.code(), CapabilityErrorCode::PermissionRequired);
    assert_eq!(err.capability(), Some(&cap));

    for kind in UnavailableKind::ALL {
        let err = Availability::unavailable(*kind, "why")
            .unwrap()
            .check(&cap)
            .unwrap_err();
        assert_eq!(err.code(), kind.code());
        assert!(err.message().contains("why"));
    }

    assert_eq!(Availability::Supported.check_writable(&cap), Ok(()));
    assert_eq!(
        Availability::ReadOnly
            .check_writable(&cap)
            .unwrap_err()
            .code(),
        CapabilityErrorCode::ReadOnly
    );
}

#[test]
fn capability_round_trips_with_scope_and_effect() {
    let cap = Capability::new(
        id("system.brightness.write"),
        Availability::unavailable(UnavailableKind::HardwareUnavailable, "external display")
            .unwrap(),
        IntegrationEffect::OsManagedSetting,
    )
    .with_scope("display:2");
    let value = serde_json::to_value(&cap).unwrap();
    assert_eq!(
        value,
        json!({
            "id": "system.brightness.write",
            "availability": {
                "state": "unavailable",
                "reason": {"kind": "hardware-unavailable", "detail": "external display"}
            },
            "scope": "display:2",
            "effect": "os-managed-setting"
        })
    );
    assert_eq!(serde_json::from_value::<Capability>(value).unwrap(), cap);

    let plain = Capability::new(
        id("surface.panel"),
        Availability::Supported,
        IntegrationEffect::GlintOwnedUi,
    );
    let value = serde_json::to_value(&plain).unwrap();
    assert_eq!(
        value,
        json!({"id": "surface.panel", "availability": {"state": "supported"}, "effect": "glint-owned-ui"})
    );
    assert_eq!(serde_json::from_value::<Capability>(value).unwrap(), plain);
}

#[test]
fn integration_effect_distinguishes_glint_ui_from_os_settings() {
    assert_eq!(
        serde_json::to_value(IntegrationEffect::GlintOwnedUi).unwrap(),
        json!("glint-owned-ui")
    );
    assert_eq!(
        serde_json::to_value(IntegrationEffect::OsManagedSetting).unwrap(),
        json!("os-managed-setting")
    );
    assert!(IntegrationEffect::OsManagedSetting.changes_os_state());
    assert!(!IntegrationEffect::GlintOwnedUi.changes_os_state());
}

#[test]
fn error_codes_are_stable_unique_and_carry_guidance() {
    let expected = [
        (
            CapabilityErrorCode::UnsupportedPlatform,
            "unsupported-platform",
            ErrorCategory::Unsupported,
        ),
        (
            CapabilityErrorCode::UnsupportedSession,
            "unsupported-session",
            ErrorCategory::Unsupported,
        ),
        (
            CapabilityErrorCode::PermissionRequired,
            "permission-required",
            ErrorCategory::Permission,
        ),
        (
            CapabilityErrorCode::HardwareUnavailable,
            "hardware-unavailable",
            ErrorCategory::Unavailable,
        ),
        (
            CapabilityErrorCode::TemporaryUnavailable,
            "temporary-unavailable",
            ErrorCategory::Unavailable,
        ),
        (
            CapabilityErrorCode::Conflict,
            "conflict",
            ErrorCategory::Unavailable,
        ),
        (
            CapabilityErrorCode::ThemeIncompatible,
            "theme-incompatible",
            ErrorCategory::Unsupported,
        ),
        (
            CapabilityErrorCode::InvalidState,
            "invalid-state",
            ErrorCategory::Failure,
        ),
        (
            CapabilityErrorCode::PermissionDenied,
            "permission-denied",
            ErrorCategory::Permission,
        ),
        (
            CapabilityErrorCode::ReadOnly,
            "read-only",
            ErrorCategory::Unsupported,
        ),
        (
            CapabilityErrorCode::UnknownCapability,
            "unknown-capability",
            ErrorCategory::Unsupported,
        ),
        (
            CapabilityErrorCode::Timeout,
            "timeout",
            ErrorCategory::Timeout,
        ),
        (
            CapabilityErrorCode::AdapterFailure,
            "adapter-failure",
            ErrorCategory::Failure,
        ),
    ];
    assert_eq!(CapabilityErrorCode::ALL.len(), expected.len());
    let mut seen = std::collections::BTreeSet::new();
    for (code, text, category) in expected {
        assert!(CapabilityErrorCode::ALL.contains(&code));
        assert_eq!(code.as_str(), text);
        assert_eq!(code.to_string(), text);
        assert_eq!(code.category(), category, "{text}");
        assert!(!code.guidance().trim().is_empty(), "{text} needs guidance");
        assert!(seen.insert(text), "{text} duplicated");
        assert_eq!(serde_json::to_value(code).unwrap(), json!(text));
        assert_eq!(
            serde_json::from_value::<CapabilityErrorCode>(json!(text)).unwrap(),
            code
        );
        assert_eq!(text.parse::<CapabilityErrorCode>().unwrap(), code);
    }
    assert!("nope".parse::<CapabilityErrorCode>().is_err());
}

#[test]
fn error_categories_distinguish_the_five_failure_families() {
    use CapabilityErrorCode as C;
    let unsupported = C::UnsupportedPlatform.category();
    let permission = C::PermissionRequired.category();
    let unavailable = C::HardwareUnavailable.category();
    let timeout = C::Timeout.category();
    let failure = C::AdapterFailure.category();
    let all = [unsupported, permission, unavailable, timeout, failure];
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            assert_ne!(a, b);
        }
    }
    assert!(C::Timeout.is_retryable());
    assert!(C::TemporaryUnavailable.is_retryable());
    assert!(C::Conflict.is_retryable());
    assert!(!C::UnsupportedPlatform.is_retryable());
    assert!(!C::PermissionDenied.is_retryable());
}

#[test]
fn unavailable_kinds_map_to_unavailable_family_codes() {
    assert_eq!(
        UnavailableKind::ALL
            .iter()
            .map(|k| k.code())
            .collect::<Vec<_>>(),
        vec![
            CapabilityErrorCode::UnsupportedPlatform,
            CapabilityErrorCode::UnsupportedSession,
            CapabilityErrorCode::HardwareUnavailable,
            CapabilityErrorCode::TemporaryUnavailable,
            CapabilityErrorCode::Conflict,
            CapabilityErrorCode::UnknownCapability,
        ]
    );
}

#[test]
fn capability_error_round_trips_and_displays_code_and_capability() {
    let err = CapabilityError::new(
        CapabilityErrorCode::Timeout,
        Some(id("system.network.read")),
        "network adapter did not answer within 2s",
    );
    assert_eq!(err.code(), CapabilityErrorCode::Timeout);
    assert_eq!(err.guidance(), CapabilityErrorCode::Timeout.guidance());
    let shown = err.to_string();
    assert!(shown.contains("timeout"), "{shown}");
    assert!(shown.contains("system.network.read"), "{shown}");

    let value = serde_json::to_value(&err).unwrap();
    assert_eq!(
        value,
        json!({
            "code": "timeout",
            "capability": "system.network.read",
            "message": "network adapter did not answer within 2s"
        })
    );
    assert_eq!(
        serde_json::from_value::<CapabilityError>(value).unwrap(),
        err
    );

    let general = CapabilityError::new(CapabilityErrorCode::AdapterFailure, None, "crashed");
    let value = serde_json::to_value(&general).unwrap();
    assert_eq!(
        value,
        json!({"code": "adapter-failure", "message": "crashed"})
    );
    assert_eq!(
        serde_json::from_value::<CapabilityError>(value).unwrap(),
        general
    );
    let _: &dyn std::error::Error = &general;
}
