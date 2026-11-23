//! Provenance and same-ID collision policy (INS-004).
//!
//! Every installed release records where it came from (see
//! [`InstalledPackage`](crate::InstalledPackage)'s provenance fields), and
//! every install is classified against the installed releases of the same
//! package key with `glint-schema`'s [`classify_install`], which implements
//! the one-origin-per-key policy of ADR 0006 (*Proposed*):
//!
//! - `New`, `AdditionalVersion`: installed;
//! - `AlreadyInstalled`: the existing release is returned unchanged;
//! - `DigestConflict` (same origin and version, different bytes) and
//!   `OriginCollision` (same key from another origin, even with equal
//!   bytes): refused with [`ErrorCode::IdentityConflict`](crate::ErrorCode::IdentityConflict);
//!   the installed release is left untouched.
//!
//! Replacing an installed origin (ADR 0006 section 3, point 8) is an
//! explicit user decision that is not implemented yet; the refusal carries
//! both origins and digests for the disambiguation prompt.

use glint_package::PackageCandidate;
use glint_schema::{
    InstallOutcome, PackageKey, PackageOrigin, PackageRelease, Sha256Digest, classify_install,
};
use glint_sources::{SigningStatus, SourceKind, SourceOrigin};

use crate::error::InstallError;
use crate::install::InstalledPackage;

/// The ADR 0006 origin of a staged package.
///
/// Local archives and directories share the origin `local`. Remote kinds map
/// to `registry:`, `git:` and `artifact:` origins from their canonical URL;
/// a registry source without an index URL (the registry name only) cannot
/// be mapped and is refused, because ADR 0006 keys registries by URL.
///
/// # Errors
///
/// [`ErrorCode::InvalidOrigin`](crate::ErrorCode::InvalidOrigin).
pub fn package_origin(origin: &SourceOrigin) -> Result<PackageOrigin, InstallError> {
    let invalid = |detail: String| InstallError::InvalidOrigin(detail);
    match origin.kind {
        SourceKind::LocalArchive | SourceKind::LocalDirectory => Ok(PackageOrigin::local()),
        SourceKind::Git => PackageOrigin::git(&origin.locator, None)
            .map_err(|error| invalid(error.message().to_owned())),
        SourceKind::ArtifactUrl => PackageOrigin::artifact(&origin.locator)
            .map_err(|error| invalid(error.message().to_owned())),
        SourceKind::Registry => PackageOrigin::registry(&origin.locator).map_err(|_| {
            invalid(format!(
                "registry source {:?} has no index URL; ADR 0006 identifies registries by URL",
                origin.locator
            ))
        }),
    }
}

/// The stable name of a signing status recorded in provenance.
pub(crate) fn signing_name(status: SigningStatus) -> &'static str {
    match status {
        SigningStatus::Unsigned => "unsigned",
        _ => "unknown",
    }
}

/// The release a candidate would become.
pub(crate) fn candidate_release(
    candidate: &PackageCandidate,
    origin: PackageOrigin,
) -> Result<PackageRelease, InstallError> {
    let key = PackageKey::new(candidate.manifest.kind, &candidate.manifest.id)
        .map_err(|error| InstallError::InvalidOrigin(error.message().to_owned()))?;
    Ok(PackageRelease::new(
        key,
        origin,
        candidate.manifest.version.clone(),
        Sha256Digest::from_bytes(candidate.sha256),
    ))
}

/// The release an installed package is. Rows that no longer parse (which
/// only a damaged database could produce) are skipped by the caller.
pub(crate) fn installed_release(
    package: &InstalledPackage,
    key: &PackageKey,
) -> Option<PackageRelease> {
    Some(PackageRelease::new(
        key.clone(),
        PackageOrigin::parse(&package.origin).ok()?,
        semver::Version::parse(&package.version).ok()?,
        Sha256Digest::parse(&format!("sha256:{}", package.digest)).ok()?,
    ))
}

/// Refuse outcomes that need an explicit user decision.
pub(crate) fn require_may_proceed(
    outcome: InstallOutcome,
    candidate: &PackageRelease,
) -> Result<InstallOutcome, InstallError> {
    if outcome.may_proceed() {
        Ok(outcome)
    } else {
        Err(InstallError::IdentityConflict {
            outcome: Box::new(outcome),
            candidate_origin: candidate.origin.as_str().to_owned(),
            candidate_digest: candidate.digest.as_str().to_owned(),
        })
    }
}

/// Classify a candidate against the installed releases of its key.
pub(crate) fn classify(
    installed: &[InstalledPackage],
    candidate: &PackageRelease,
) -> Result<InstallOutcome, InstallError> {
    let releases: Vec<PackageRelease> = installed
        .iter()
        .filter(|p| p.kind == candidate.key.kind().as_str() && p.id == candidate.key.id())
        .filter_map(|p| installed_release(p, &candidate.key))
        .collect();
    require_may_proceed(classify_install(&releases, candidate), candidate)
}
