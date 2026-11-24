//! Profile locks against installed releases (PRF-002): generate a
//! `profile.lock` from what is installed, detect drift, and resolve through
//! the lock so only the pinned releases are ever used.

use std::fmt;

use glint_schema::{
    LockProblem, PackageKey, PackageRelease, Profile, ProfileLock, ThemeRef, verify_profile_lock,
};

use crate::input::{ThemeLookup, ThemeProvider, Unavailable};

/// Stable codes for a lock that cannot be generated or no longer matches
/// what is installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum LockCode {
    /// No installed release matches the reference (or the pin).
    NotInstalled,
    /// Several installed releases match one reference (different origins or
    /// digests for one id and version); the lock cannot choose.
    Ambiguous,
    /// The release installed for a pin's id and version has another digest.
    DigestMismatch,
    /// The release installed for a pin's id and version comes from another
    /// origin.
    OriginMismatch,
}

impl LockCode {
    /// Every code, in declaration order.
    pub const ALL: &'static [LockCode] = &[
        LockCode::NotInstalled,
        LockCode::Ambiguous,
        LockCode::DigestMismatch,
        LockCode::OriginMismatch,
    ];

    /// The stable string form, e.g. `GLINT_LOCK_NOT_INSTALLED`.
    pub const fn as_str(self) -> &'static str {
        match self {
            LockCode::NotInstalled => "GLINT_LOCK_NOT_INSTALLED",
            LockCode::Ambiguous => "GLINT_LOCK_AMBIGUOUS",
            LockCode::DigestMismatch => "GLINT_LOCK_DIGEST_MISMATCH",
            LockCode::OriginMismatch => "GLINT_LOCK_ORIGIN_MISMATCH",
        }
    }
}

impl fmt::Display for LockCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One lock finding.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LockIssue {
    /// Stable code.
    pub code: LockCode,
    /// JSON Pointer: into `profile.json` from [`generate_lock`], into
    /// `profile.lock` from [`check_lock`].
    pub pointer: String,
    /// The package concerned.
    pub key: PackageKey,
    /// Human-readable detail (not stable).
    pub message: String,
}

impl fmt::Display for LockIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} (at {})", self.code, self.message, self.pointer)
    }
}

fn matching<'a>(
    installed: &'a [PackageRelease],
    key: &PackageKey,
    version: &semver::Version,
) -> Vec<&'a PackageRelease> {
    let mut found: Vec<&PackageRelease> = installed
        .iter()
        .filter(|r| &r.key == key && &r.version == version)
        .collect();
    found.sort_by(|a, b| (&a.origin, &a.digest).cmp(&(&b.origin, &b.digest)));
    found.dedup();
    found
}

/// Pin every theme `profile` references to the exact installed release
/// (kind, id, version, origin, digest). The result is sorted by package key
/// and written with [`ProfileLock::to_json_string`], so the same profile
/// and inventory always give a byte-identical `profile.lock`.
///
/// # Errors
///
/// Every reference that cannot be pinned, as [`LockCode::NotInstalled`] or
/// [`LockCode::Ambiguous`] with a pointer into `profile.json`. No partial
/// lock is returned.
pub fn generate_lock(
    profile: &Profile,
    installed: &[PackageRelease],
) -> Result<ProfileLock, Vec<LockIssue>> {
    let mut pins: Vec<PackageRelease> = Vec::new();
    let mut issues = Vec::new();
    for (pointer, theme) in profile.theme_refs() {
        if pins.iter().any(|pin| pin.key == theme.key) {
            continue; // the parser guarantees one version per theme
        }
        match matching(installed, &theme.key, &theme.version).as_slice() {
            [release] => pins.push((*release).clone()),
            [] => issues.push(LockIssue {
                code: LockCode::NotInstalled,
                pointer,
                key: theme.key.clone(),
                message: format!("{}@{} is not installed", theme.key.id(), theme.version),
            }),
            several => issues.push(LockIssue {
                code: LockCode::Ambiguous,
                pointer,
                key: theme.key.clone(),
                message: format!(
                    "{} releases of {}@{} are installed",
                    several.len(),
                    theme.key.id(),
                    theme.version
                ),
            }),
        }
    }
    if !issues.is_empty() {
        return Err(issues);
    }
    // The profile id passed the same rule in `parse_profile` and pins are
    // unique per key, so construction cannot fail.
    Ok(ProfileLock::new(&profile.id, pins)
        .expect("a parsed profile id with unique pins always makes a lock"))
}

/// The result of [`check_lock`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct LockReport {
    /// Profile/lock drift from `glint_schema::verify_profile_lock`
    /// (`GLINT_PROFILE_LOCK_*`: unlocked reference, version mismatch, stale
    /// pin, other profile).
    pub problems: Vec<LockProblem>,
    /// Lock/installed drift: a pin whose exact release is not installed.
    pub drift: Vec<LockIssue>,
}

impl LockReport {
    /// Whether the lock covers the profile exactly and every pin is
    /// installed with its origin and digest.
    pub fn is_clean(&self) -> bool {
        self.problems.is_empty() && self.drift.is_empty()
    }
}

/// Detect drift between `profile`, its `lock` and the `installed` releases.
pub fn check_lock(
    profile: &Profile,
    lock: &ProfileLock,
    installed: &[PackageRelease],
) -> LockReport {
    let problems = verify_profile_lock(profile, lock);
    let mut drift = Vec::new();
    for (index, pinned) in lock.packages.iter().enumerate() {
        if installed.contains(pinned) {
            continue;
        }
        let pointer = format!("/packages/{index}");
        let candidates = matching(installed, &pinned.key, &pinned.version);
        let label = format!("{}@{}", pinned.key.id(), pinned.version);
        let (code, message) =
            if let Some(found) = candidates.iter().find(|r| r.origin == pinned.origin) {
                (
                    LockCode::DigestMismatch,
                    format!(
                        "{label} is installed with digest {}, not the pinned {}",
                        found.digest, pinned.digest
                    ),
                )
            } else if let Some(found) = candidates.first() {
                (
                    LockCode::OriginMismatch,
                    format!(
                        "{label} is installed from {}, not the pinned {}",
                        found.origin, pinned.origin
                    ),
                )
            } else {
                (
                    LockCode::NotInstalled,
                    format!("the pinned {label} is not installed"),
                )
            };
        drift.push(LockIssue {
            code,
            pointer,
            key: pinned.key.clone(),
            message,
        });
    }
    LockReport { problems, drift }
}

/// A [`ThemeProvider`] that only hands out the releases `lock` pins: an
/// unpinned theme, another version, digest or origin is unavailable, so the
/// resolver falls back (and reports it) instead of loading something the
/// user never locked.
#[derive(Debug, Clone, Copy)]
pub struct LockedThemes<'a, P: ?Sized> {
    lock: &'a ProfileLock,
    themes: &'a P,
}

impl<'a, P: ThemeProvider + ?Sized> LockedThemes<'a, P> {
    /// Enforce `lock` on top of `themes`.
    pub fn new(lock: &'a ProfileLock, themes: &'a P) -> Self {
        Self { lock, themes }
    }
}

impl<P: ThemeProvider + ?Sized> ThemeProvider for LockedThemes<'_, P> {
    fn lookup(&self, theme: &ThemeRef) -> ThemeLookup<'_> {
        let Some(pinned) = self.lock.find(&theme.key) else {
            return ThemeLookup::Unavailable(Unavailable::Unlocked);
        };
        if pinned.version != theme.version {
            return ThemeLookup::Unavailable(Unavailable::LockVersionMismatch {
                pinned: pinned.version.clone(),
            });
        }
        match self.themes.lookup(theme) {
            ThemeLookup::Available(input) if input.release.digest != pinned.digest => {
                ThemeLookup::Unavailable(Unavailable::DigestMismatch {
                    pinned: pinned.digest.clone(),
                    found: input.release.digest.clone(),
                })
            }
            ThemeLookup::Available(input) if input.release.origin != pinned.origin => {
                ThemeLookup::Unavailable(Unavailable::OriginMismatch {
                    pinned: pinned.origin.clone(),
                    found: input.release.origin.clone(),
                })
            }
            other => other,
        }
    }
}
