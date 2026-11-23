//! Package identity and cross-origin collision policy (SCH-004).
//!
//! Run with `cargo test -p glint-schema --test identity`. The numbered tests
//! are the cases listed under "Test cases for INS-004" in
//! `docs/adr/0006-package-identity-and-origin-collisions.md`.

use glint_schema::{
    IdentityErrorKind, InstallOutcome, OriginKind, PackageKey, PackageKind, PackageOrigin,
    PackageRelease, Sha256Digest, classify_install,
};
use semver::Version;

const D1: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const D2: &str = "sha256:2222222222222222222222222222222222222222222222222222222222222222";
const REGISTRY: &str = "registry:https://registry.example/index";
const GIT: &str = "git:https://git.example/user/themes#themes/a";

fn key(id: &str) -> PackageKey {
    PackageKey::new(PackageKind::Theme, id).expect("valid key")
}

fn origin(text: &str) -> PackageOrigin {
    PackageOrigin::parse(text).expect("valid origin")
}

fn release(id: &str, origin_text: &str, version: &str, digest: &str) -> PackageRelease {
    PackageRelease::new(
        key(id),
        origin(origin_text),
        Version::parse(version).expect("version"),
        Sha256Digest::parse(digest).expect("digest"),
    )
}

fn a(origin_text: &str, version: &str, digest: &str) -> PackageRelease {
    release("dev.example.a", origin_text, version, digest)
}

// ------------------------------------------------- ADR 0006 test cases

#[test]
fn case_01_nothing_installed_is_new() {
    let outcome = classify_install(&[], &a(REGISTRY, "1.0.0", D1));
    assert_eq!(outcome, InstallOutcome::New);
    assert_eq!(outcome.code(), "GLINT_IDENTITY_NEW");
    assert!(outcome.may_proceed());
}

#[test]
fn case_02_same_release_again_is_idempotent() {
    let installed = [a(REGISTRY, "1.0.0", D1)];
    let outcome = classify_install(&installed, &a(REGISTRY, "1.0.0", D1));
    assert_eq!(outcome, InstallOutcome::AlreadyInstalled);
    assert!(outcome.may_proceed());
}

#[test]
fn case_03_new_version_from_same_origin_is_additional() {
    let installed = [a(REGISTRY, "1.0.0", D1)];
    let outcome = classify_install(&installed, &a(REGISTRY, "1.1.0", D2));
    assert_eq!(outcome, InstallOutcome::AdditionalVersion);
    assert_eq!(outcome.code(), "GLINT_IDENTITY_ADDITIONAL_VERSION");
    assert!(outcome.may_proceed());
}

#[test]
fn case_04_same_version_different_digest_is_refused() {
    let installed = [a(REGISTRY, "1.0.0", D1)];
    let outcome = classify_install(&installed, &a(REGISTRY, "1.0.0", D2));
    assert_eq!(
        outcome,
        InstallOutcome::DigestConflict {
            installed: Sha256Digest::parse(D1).unwrap()
        }
    );
    assert_eq!(outcome.code(), "GLINT_IDENTITY_DIGEST_CONFLICT");
    assert!(!outcome.may_proceed());
}

#[test]
fn case_05_other_origin_any_version_collides() {
    let installed = [a(REGISTRY, "1.0.0", D1)];
    let outcome = classify_install(&installed, &a(GIT, "2.0.0", D2));
    assert_eq!(
        outcome,
        InstallOutcome::OriginCollision {
            installed: origin(REGISTRY)
        }
    );
    assert_eq!(outcome.code(), "GLINT_IDENTITY_ORIGIN_COLLISION");
    assert!(!outcome.may_proceed());
}

#[test]
fn case_06_equal_bytes_from_other_origin_are_not_a_trusted_duplicate() {
    let installed = [a(REGISTRY, "1.0.0", D1)];
    let outcome = classify_install(&installed, &a(GIT, "1.0.0", D1));
    assert!(matches!(outcome, InstallOutcome::OriginCollision { .. }));
}

#[test]
fn case_07_local_import_of_remote_package_collides() {
    let installed = [a(REGISTRY, "1.0.0", D1)];
    let outcome = classify_install(&installed, &a("local", "1.0.0", D1));
    assert!(matches!(outcome, InstallOutcome::OriginCollision { .. }));
}

#[test]
fn case_08_local_imports_share_one_origin() {
    let installed = [a("local", "1.0.0", D1)];
    let candidate = PackageRelease::new(
        key("dev.example.a"),
        PackageOrigin::local(),
        Version::new(1, 1, 0),
        Sha256Digest::parse(D2).unwrap(),
    );
    assert_eq!(
        classify_install(&installed, &candidate),
        InstallOutcome::AdditionalVersion
    );
}

#[test]
fn case_09_kind_is_part_of_the_key() {
    // Only `theme` exists in v1, so the kind dimension is checked on the key
    // itself: keys of equal id compare by kind first.
    let theme = key("dev.example.a");
    assert_eq!(theme.kind(), PackageKind::Theme);
    assert_eq!(theme.to_string(), "theme:dev.example.a");
    assert_eq!(PackageKind::ALL, &[PackageKind::Theme]);
}

#[test]
fn case_10_host_case_and_trailing_slash_are_the_same_origin() {
    let spelled = origin("git:https://Host.Example/u/r/");
    assert_eq!(spelled, origin("git:https://host.example/u/r"));
    assert_eq!(spelled.as_str(), "git:https://host.example/u/r");

    let installed = [a("git:https://HOST.example/u/r/", "1.0.0", D1)];
    assert_eq!(
        classify_install(&installed, &a("git:https://host.example/u/r", "1.1.0", D2)),
        InstallOutcome::AdditionalVersion
    );
}

#[test]
fn case_11_doubtful_spellings_stay_different_origins() {
    assert_ne!(
        origin("git:https://host.example/u/r"),
        origin("git:https://host.example/u/r.git")
    );
    assert_ne!(
        origin("git:https://host.example/u/r"),
        origin("git:https://host.example/U/R")
    );
    assert_ne!(
        origin("git:https://host.example/u/r"),
        origin("registry:https://host.example/u/r")
    );
    let installed = [a("git:https://host.example/u/r", "1.0.0", D1)];
    assert!(matches!(
        classify_install(
            &installed,
            &a("git:https://host.example/u/r.git", "1.0.0", D1)
        ),
        InstallOutcome::OriginCollision { .. }
    ));
}

#[test]
fn case_12_credentials_in_an_origin_are_refused() {
    for text in [
        "git:https://user:token@host.example/r",
        "registry:https://token@host.example/index",
        "artifact:https://a@b@host.example/x.glint",
    ] {
        let err = PackageOrigin::parse(text).expect_err(text);
        assert_eq!(err.kind(), IdentityErrorKind::InvalidOrigin, "{text}");
    }
}

#[test]
fn case_13_other_keys_are_irrelevant() {
    let installed = [release("dev.example.b", REGISTRY, "1.0.0", D1)];
    assert_eq!(
        classify_install(&installed, &a(GIT, "1.0.0", D1)),
        InstallOutcome::New
    );
}

// ------------------------------------------------------- more edges

#[test]
fn build_metadata_does_not_make_a_new_version() {
    let installed = [a(REGISTRY, "1.0.0+build.1", D1)];
    assert!(matches!(
        classify_install(&installed, &a(REGISTRY, "1.0.0+build.2", D2)),
        InstallOutcome::DigestConflict { .. }
    ));
    assert_eq!(
        classify_install(&installed, &a(REGISTRY, "1.0.0-rc.1", D2)),
        InstallOutcome::AdditionalVersion
    );
}

#[test]
fn broken_invariant_still_reports_a_collision() {
    let installed = [a(REGISTRY, "1.0.0", D1), a(GIT, "2.0.0", D2)];
    assert!(matches!(
        classify_install(&installed, &a(REGISTRY, "1.0.0", D1)),
        InstallOutcome::OriginCollision { installed } if installed == origin(GIT)
    ));
}

#[test]
fn origin_forms_round_trip() {
    for (text, kind) in [
        ("local", OriginKind::Local),
        (
            "registry:https://registry.example/index",
            OriginKind::Registry,
        ),
        ("git:https://git.example/u/r", OriginKind::Git),
        ("git:https://git.example/u/r#themes/mono", OriginKind::Git),
        (
            "artifact:https://cdn.example/a/midnight.glint",
            OriginKind::Artifact,
        ),
        (
            "registry:https://registry.example:8443",
            OriginKind::Registry,
        ),
    ] {
        let parsed = origin(text);
        assert_eq!(parsed.kind(), kind, "{text}");
        assert_eq!(parsed.as_str(), text);
        assert_eq!(parsed.to_string().parse::<PackageOrigin>().unwrap(), parsed);
    }
    assert_eq!(
        PackageOrigin::git("https://git.example/u/r", Some("themes/mono")).unwrap(),
        origin("git:https://git.example/u/r#themes/mono")
    );
    assert_eq!(
        PackageOrigin::registry("HTTPS://Registry.Example/index/").unwrap(),
        origin(REGISTRY)
    );
}

#[test]
fn unsafe_or_malformed_origins_are_refused() {
    for text in [
        "",
        "Local",
        "local:",
        "file:///C:/themes",
        "registry:http://registry.example/index",
        "registry:file:///etc",
        "registry:registry.example/index",
        "registry:https://",
        "registry:https:///path",
        "registry:https://:443/x",
        "git:ssh://git.example/r",
        "git:https://git.example/r?ref=main",
        "artifact:https://cdn.example/a.glint#frag",
        "git:https://git.example/r#",
        "git:https://git.example/r#../escape",
        "git:https://git.example/r#/abs",
        "git:https://git.example/r#a\\b",
        "git:https://git.example/../r",
        "git:https://git.example/./r",
        "git:https://git.example/r//",
        "git:https://git.example/my repo",
        "mirror:https://git.example/r",
    ] {
        let err = PackageOrigin::parse(text).expect_err(text);
        assert_eq!(err.kind(), IdentityErrorKind::InvalidOrigin, "{text:?}");
    }
    let long = format!("registry:https://r.example/{}", "a".repeat(2048));
    assert!(PackageOrigin::parse(&long).is_err());
}

#[test]
fn digests_are_canonical_sha256() {
    let digest = Sha256Digest::parse(D1).unwrap();
    assert_eq!(digest.as_str(), D1);
    assert_eq!(Sha256Digest::from_bytes([0x11; 32]), digest);
    assert_eq!(
        Sha256Digest::from_bytes([0xab; 32]).as_str(),
        format!("sha256:{}", "ab".repeat(32))
    );
    for bad in [
        "",
        "sha256:",
        "1111111111111111111111111111111111111111111111111111111111111111",
        "SHA256:1111111111111111111111111111111111111111111111111111111111111111",
        "sha256:111111111111111111111111111111111111111111111111111111111111111",
        "sha256:11111111111111111111111111111111111111111111111111111111111111111",
        "sha256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "sha256:gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg",
        "sha512:1111111111111111111111111111111111111111111111111111111111111111",
    ] {
        let err = Sha256Digest::parse(bad).expect_err(bad);
        assert_eq!(err.kind(), IdentityErrorKind::InvalidDigest, "{bad:?}");
    }
}

#[test]
fn package_keys_use_manifest_id_rules() {
    assert_eq!(key("official.mono").id(), "official.mono");
    for bad in ["mono", "Dev.Example.A", "dev..a", "dev.-a", ""] {
        let err = PackageKey::new(PackageKind::Theme, bad).expect_err(bad);
        assert_eq!(err.kind(), IdentityErrorKind::InvalidId, "{bad:?}");
    }
}

#[test]
fn every_outcome_code_is_distinct_and_prefixed() {
    let digest = Sha256Digest::parse(D1).unwrap();
    let outcomes = [
        InstallOutcome::New,
        InstallOutcome::AlreadyInstalled,
        InstallOutcome::AdditionalVersion,
        InstallOutcome::DigestConflict { installed: digest },
        InstallOutcome::OriginCollision {
            installed: PackageOrigin::local(),
        },
    ];
    let codes: std::collections::BTreeSet<&str> = outcomes.iter().map(|o| o.code()).collect();
    assert_eq!(codes.len(), outcomes.len());
    assert!(codes.iter().all(|c| c.starts_with("GLINT_IDENTITY_")));
}
