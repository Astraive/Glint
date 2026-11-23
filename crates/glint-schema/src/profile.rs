//! The **proposed** `profile.json` v1 and `profile.lock` v1 formats
//! (SCH-006).
//!
//! A profile describes the user's active composition: an optional base
//! theme, per-surface override themes, a wallpaper, per-monitor placement
//! overrides and semantic token overrides. Its lock pins every referenced
//! theme to an exact release (`kind`, `id`, `version`, `origin`, `digest`), as
//! defined by the proposed ADR 0006. JSON Schemas:
//! `schemas/glint-profile-v1.schema.json` and
//! `schemas/glint-profile-lock-v1.schema.json`. The parsers here are the
//! authority where a schema and the parser disagree.
//!
//! Both documents use the strict document rules of `glint.json` (size limit,
//! no duplicate keys, object root) and reject unknown fields everywhere except
//! inside monitor placement objects, whose shape the surface/layout engine
//! (SRF) owns. Errors carry a stable [`ProfileErrorCode`]
//! (`GLINT_PROFILE_*`) and an RFC 6901 JSON Pointer.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use semver::Version;
use serde_json::{Map, Number, Value};

use crate::error::{ErrorCode, ManifestError};
use crate::identity::{IdentityErrorKind, PackageKey, PackageOrigin, PackageRelease, Sha256Digest};
use crate::manifest::PackageKind;
use crate::parse::{
    MAX_DESCRIPTION_LEN, MAX_NAME_LEN, MAX_SHORT_TEXT_LEN, MAX_URL_LEN, Obj, as_array, as_object,
    as_str, child_pointer, is_id_segment, is_valid_key, is_valid_surface_id, load_json_object,
    package_id, package_path, text_value, version, wrong_type,
};

/// The only profile and profile-lock `schemaVersion` this crate understands.
pub const SUPPORTED_PROFILE_SCHEMA_VERSION: u32 = 1;

/// Largest accepted `profile.json` or `profile.lock`, in bytes (1 MiB).
pub const MAX_PROFILE_BYTES: usize = 1024 * 1024;

/// Monitor keys that name a role rather than a physical display; every other
/// monitor key is treated as machine-specific and local-only.
pub const PORTABLE_MONITOR_KEYS: &[&str] = &["primary", "secondary"];

const PROFILE_FIELDS: &[&str] = &[
    "$schema",
    "schemaVersion",
    "id",
    "name",
    "description",
    "baseTheme",
    "surfaces",
    "wallpaper",
    "monitors",
    "tokenOverrides",
];
const LOCK_FIELDS: &[&str] = &["$schema", "schemaVersion", "profile", "packages"];
const LOCK_ENTRY_FIELDS: &[&str] = &["kind", "id", "version", "origin", "digest"];
const MAX_PROFILE_ID_LEN: usize = 128;
const MAX_TOKEN_NAME_LEN: usize = 128;

// ------------------------------------------------------------- errors

/// Stable, machine-readable profile and profile-lock error codes. The string
/// form is the contract; codes are only ever added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ProfileErrorCode {
    /// Not well-formed JSON.
    InvalidJson,
    /// Larger than [`MAX_PROFILE_BYTES`].
    TooLarge,
    /// An object repeats a key.
    DuplicateKey,
    /// The root is not a JSON object.
    NotAnObject,
    /// `schemaVersion` is absent.
    MissingSchemaVersion,
    /// `schemaVersion` is not [`SUPPORTED_PROFILE_SCHEMA_VERSION`].
    UnsupportedSchemaVersion,
    /// A required field is absent; the pointer names it.
    MissingField,
    /// A field is not part of the v1 format.
    UnknownField,
    /// A field has the wrong JSON type.
    InvalidType,
    /// A field has the right type but a disallowed value.
    InvalidValue,
    /// The profile id (or the lock's `profile`) is malformed.
    InvalidProfileId,
    /// A theme reference is not a valid package id.
    InvalidPackageId,
    /// A version is not an exact semantic version.
    InvalidVersion,
    /// A path is absolute, escapes the profile directory or is otherwise unsafe.
    InvalidPath,
    /// A surface id is malformed.
    InvalidSurface,
    /// One theme is referenced with two different versions.
    ConflictingVersions,
    /// A lock origin is not a canonical origin (see ADR 0006).
    InvalidOrigin,
    /// A lock digest is not `sha256:` plus 64 lowercase hex digits.
    InvalidDigest,
    /// A lock lists the same (`kind`, `id`) twice.
    DuplicatePackage,
}

impl ProfileErrorCode {
    /// Every code, in declaration order.
    pub const ALL: &'static [ProfileErrorCode] = &[
        ProfileErrorCode::InvalidJson,
        ProfileErrorCode::TooLarge,
        ProfileErrorCode::DuplicateKey,
        ProfileErrorCode::NotAnObject,
        ProfileErrorCode::MissingSchemaVersion,
        ProfileErrorCode::UnsupportedSchemaVersion,
        ProfileErrorCode::MissingField,
        ProfileErrorCode::UnknownField,
        ProfileErrorCode::InvalidType,
        ProfileErrorCode::InvalidValue,
        ProfileErrorCode::InvalidProfileId,
        ProfileErrorCode::InvalidPackageId,
        ProfileErrorCode::InvalidVersion,
        ProfileErrorCode::InvalidPath,
        ProfileErrorCode::InvalidSurface,
        ProfileErrorCode::ConflictingVersions,
        ProfileErrorCode::InvalidOrigin,
        ProfileErrorCode::InvalidDigest,
        ProfileErrorCode::DuplicatePackage,
    ];

    /// The stable string form, e.g. `GLINT_PROFILE_MISSING_FIELD`.
    pub const fn as_str(self) -> &'static str {
        match self {
            ProfileErrorCode::InvalidJson => "GLINT_PROFILE_INVALID_JSON",
            ProfileErrorCode::TooLarge => "GLINT_PROFILE_TOO_LARGE",
            ProfileErrorCode::DuplicateKey => "GLINT_PROFILE_DUPLICATE_KEY",
            ProfileErrorCode::NotAnObject => "GLINT_PROFILE_NOT_AN_OBJECT",
            ProfileErrorCode::MissingSchemaVersion => "GLINT_PROFILE_MISSING_SCHEMA_VERSION",
            ProfileErrorCode::UnsupportedSchemaVersion => {
                "GLINT_PROFILE_UNSUPPORTED_SCHEMA_VERSION"
            }
            ProfileErrorCode::MissingField => "GLINT_PROFILE_MISSING_FIELD",
            ProfileErrorCode::UnknownField => "GLINT_PROFILE_UNKNOWN_FIELD",
            ProfileErrorCode::InvalidType => "GLINT_PROFILE_INVALID_TYPE",
            ProfileErrorCode::InvalidValue => "GLINT_PROFILE_INVALID_VALUE",
            ProfileErrorCode::InvalidProfileId => "GLINT_PROFILE_INVALID_PROFILE_ID",
            ProfileErrorCode::InvalidPackageId => "GLINT_PROFILE_INVALID_PACKAGE_ID",
            ProfileErrorCode::InvalidVersion => "GLINT_PROFILE_INVALID_VERSION",
            ProfileErrorCode::InvalidPath => "GLINT_PROFILE_INVALID_PATH",
            ProfileErrorCode::InvalidSurface => "GLINT_PROFILE_INVALID_SURFACE",
            ProfileErrorCode::ConflictingVersions => "GLINT_PROFILE_CONFLICTING_VERSIONS",
            ProfileErrorCode::InvalidOrigin => "GLINT_PROFILE_INVALID_ORIGIN",
            ProfileErrorCode::InvalidDigest => "GLINT_PROFILE_INVALID_DIGEST",
            ProfileErrorCode::DuplicatePackage => "GLINT_PROFILE_DUPLICATE_PACKAGE",
        }
    }
}

impl fmt::Display for ProfileErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A profile or lock rejection: a stable [`ProfileErrorCode`], the JSON
/// Pointer of the offending value and a message (not stable).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileError {
    code: ProfileErrorCode,
    pointer: String,
    message: String,
}

impl ProfileError {
    fn new(code: ProfileErrorCode, pointer: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code,
            pointer: pointer.into(),
            message: message.into(),
        }
    }

    /// The stable error code.
    pub fn code(&self) -> ProfileErrorCode {
        self.code
    }

    /// JSON Pointer (RFC 6901) to the offending value; `""` means the root.
    pub fn pointer(&self) -> &str {
        &self.pointer
    }

    /// Human-readable detail (not stable).
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let at = if self.pointer.is_empty() {
            "(root)"
        } else {
            self.pointer.as_str()
        };
        write!(f, "{} at {}: {}", self.code, at, self.message)
    }
}

impl std::error::Error for ProfileError {}

/// The shared helpers report manifest codes; translate them.
impl From<ManifestError> for ProfileError {
    fn from(err: ManifestError) -> Self {
        let code = match err.code() {
            ErrorCode::InvalidJson => ProfileErrorCode::InvalidJson,
            ErrorCode::TooLarge => ProfileErrorCode::TooLarge,
            ErrorCode::DuplicateKey => ProfileErrorCode::DuplicateKey,
            ErrorCode::NotAnObject => ProfileErrorCode::NotAnObject,
            ErrorCode::MissingField => ProfileErrorCode::MissingField,
            ErrorCode::UnknownField => ProfileErrorCode::UnknownField,
            ErrorCode::InvalidType => ProfileErrorCode::InvalidType,
            ErrorCode::InvalidId => ProfileErrorCode::InvalidPackageId,
            ErrorCode::InvalidVersion => ProfileErrorCode::InvalidVersion,
            ErrorCode::InvalidPath => ProfileErrorCode::InvalidPath,
            _ => ProfileErrorCode::InvalidValue,
        };
        ProfileError::new(code, err.pointer(), err.message())
    }
}

type Result<T> = std::result::Result<T, ProfileError>;

// -------------------------------------------------------------- types

/// A `profile.json` that passed [`parse_profile`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Profile {
    /// Always [`SUPPORTED_PROFILE_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Local profile id, e.g. `default`, `work-setup` or `profile.custom-mix`.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// Base theme; `None` means the built-in default safe theme only.
    pub base_theme: Option<ThemeRef>,
    /// Surface id (e.g. `dock`, `custom:dev.example.clock`) to the theme that
    /// replaces only that surface.
    pub surfaces: BTreeMap<String, ThemeRef>,
    /// Wallpaper selection.
    pub wallpaper: Option<Wallpaper>,
    /// Monitor key to surface id to placement override. The placement shape is
    /// owned by the surface/layout engine (SRF) and kept as raw JSON.
    pub monitors: BTreeMap<String, BTreeMap<String, Map<String, Value>>>,
    /// Semantic token name (e.g. `background.default`) to override value.
    pub token_overrides: BTreeMap<String, TokenValue>,
}

/// A reference from a profile to an installed theme at an exact version. The
/// origin and digest live in the lock, not in the profile.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ThemeRef {
    /// Package key of the theme (`kind` is always `theme`).
    pub key: PackageKey,
    /// Exact version.
    pub version: Version,
}

/// Where the wallpaper comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Wallpaper {
    /// A private user file, relative to the profile's own directory
    /// (`{"path": "user-assets/wallpaper.png"}`). Local-only: stripped on
    /// export.
    UserFile {
        /// Profile-relative path.
        path: String,
    },
    /// A named asset of the base theme (`{"asset": "wallpaper"}`); portable.
    ThemeAsset {
        /// Asset name from the theme manifest's `assets`.
        name: String,
    },
}

/// A token override value.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TokenValue {
    /// A string value such as `#0B0C0F`.
    String(String),
    /// A finite number.
    Number(Number),
}

/// Why a profile value must not leave the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LocalOnlyReason {
    /// A private user file (wallpaper path).
    UserAsset,
    /// A monitor key naming a physical display rather than a portable role.
    MachineMonitor,
}

/// A local-only value in a profile, flagged for stripping on export.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LocalOnlyRef {
    /// JSON Pointer of the value in `profile.json`.
    pub pointer: String,
    /// Why it is local-only.
    pub reason: LocalOnlyReason,
}

impl Profile {
    /// Every theme the profile references, with the JSON Pointer of the
    /// reference: `/baseTheme` first, then `/surfaces/<id>` in key order.
    pub fn theme_refs(&self) -> Vec<(String, &ThemeRef)> {
        let base = self
            .base_theme
            .iter()
            .map(|theme| ("/baseTheme".to_owned(), theme));
        let surfaces = self
            .surfaces
            .iter()
            .map(|(surface, theme)| (child_pointer("/surfaces", surface), theme));
        base.chain(surfaces).collect()
    }

    /// Values that are local-only and must be stripped on export: a
    /// user-file wallpaper and machine-specific monitor keys (anything not in
    /// [`PORTABLE_MONITOR_KEYS`]).
    pub fn local_only_refs(&self) -> Vec<LocalOnlyRef> {
        let mut refs = Vec::new();
        if let Some(Wallpaper::UserFile { .. }) = self.wallpaper {
            refs.push(LocalOnlyRef {
                pointer: "/wallpaper".to_owned(),
                reason: LocalOnlyReason::UserAsset,
            });
        }
        for monitor in self.monitors.keys() {
            if !PORTABLE_MONITOR_KEYS.contains(&monitor.as_str()) {
                refs.push(LocalOnlyRef {
                    pointer: child_pointer("/monitors", monitor),
                    reason: LocalOnlyReason::MachineMonitor,
                });
            }
        }
        refs
    }

    /// A copy with every [`Profile::local_only_refs`] value removed: the
    /// default for export. Explicitly selected assets are copied by the
    /// exporter, not here.
    pub fn without_local_only(&self) -> Profile {
        let mut copy = self.clone();
        if let Some(Wallpaper::UserFile { .. }) = copy.wallpaper {
            copy.wallpaper = None;
        }
        copy.monitors
            .retain(|monitor, _| PORTABLE_MONITOR_KEYS.contains(&monitor.as_str()));
        copy
    }
}

/// A `profile.lock` that passed [`parse_profile_lock`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ProfileLock {
    /// Always [`SUPPORTED_PROFILE_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Id of the profile this lock belongs to.
    pub profile: String,
    /// Pinned releases, in document order; at most one per package key.
    pub packages: Vec<PackageRelease>,
}

impl ProfileLock {
    /// The pinned release for `key`, if any.
    pub fn find(&self, key: &PackageKey) -> Option<&PackageRelease> {
        self.packages.iter().find(|release| &release.key == key)
    }
}

// ------------------------------------------------------------- parsing

/// Parse and validate a `profile.json`.
///
/// # Errors
///
/// A [`ProfileError`] for the first problem, e.g.
/// [`ProfileErrorCode::InvalidProfileId`] or
/// [`ProfileErrorCode::ConflictingVersions`].
pub fn parse_profile(text: &str) -> Result<Profile> {
    let document = load_json_object(text, MAX_PROFILE_BYTES, "profile")?;
    let root = Obj::root(&document);
    let schema_version = schema_version(&root)?;
    root.deny_unknown(PROFILE_FIELDS)?;
    if let Some(value) = root.get("$schema") {
        text_value(value, "/$schema", MAX_URL_LEN)?;
    }
    let id = profile_id(root.required("id")?, "/id")?;
    let name = text_value(root.required("name")?, "/name", MAX_NAME_LEN)?;
    let description = root
        .get("description")
        .map(|v| text_value(v, "/description", MAX_DESCRIPTION_LEN))
        .transpose()?;

    let base_theme = root
        .optional_object("baseTheme")?
        .map(|obj| theme_ref(&obj, "id"))
        .transpose()?;
    let mut surfaces = BTreeMap::new();
    if let Some(obj) = root.optional_object("surfaces")? {
        for (surface, value) in obj.map {
            let pointer = obj.child(surface);
            surface_id(surface, &pointer)?;
            surfaces.insert(
                surface.clone(),
                theme_ref(&as_object(value, pointer)?, "theme")?,
            );
        }
    }
    let wallpaper = root
        .optional_object("wallpaper")?
        .map(|obj| wallpaper(&obj))
        .transpose()?;
    let monitors = monitors(&root)?;
    let token_overrides = token_overrides(&root)?;

    let profile = Profile {
        schema_version,
        id,
        name,
        description,
        base_theme,
        surfaces,
        wallpaper,
        monitors,
        token_overrides,
    };
    // One version of a theme per profile (ADR 0006: one active version).
    let mut seen: BTreeMap<&PackageKey, &Version> = BTreeMap::new();
    for (pointer, theme) in profile.theme_refs() {
        match seen.get(&theme.key) {
            Some(version) if **version != theme.version => {
                return Err(ProfileError::new(
                    ProfileErrorCode::ConflictingVersions,
                    child_pointer(&pointer, "version"),
                    format!(
                        "theme {:?} is referenced at {version} and {}; a profile uses one version \
                         of each theme",
                        theme.key.id(),
                        theme.version
                    ),
                ));
            }
            _ => {
                seen.insert(&theme.key, &theme.version);
            }
        }
    }
    Ok(profile)
}

/// Parse and validate a `profile.lock`. Every entry must pin `kind`, `id`,
/// `version`, `origin` and `digest`; a lock without a digest is rejected.
///
/// # Errors
///
/// A [`ProfileError`] for the first problem, e.g.
/// [`ProfileErrorCode::MissingField`] at `/packages/0/digest`,
/// [`ProfileErrorCode::InvalidDigest`] or
/// [`ProfileErrorCode::DuplicatePackage`].
pub fn parse_profile_lock(text: &str) -> Result<ProfileLock> {
    let document = load_json_object(text, MAX_PROFILE_BYTES, "profile lock")?;
    let root = Obj::root(&document);
    let schema_version = schema_version(&root)?;
    root.deny_unknown(LOCK_FIELDS)?;
    if let Some(value) = root.get("$schema") {
        text_value(value, "/$schema", MAX_URL_LEN)?;
    }
    let profile = profile_id(root.required("profile")?, "/profile")?;
    let list_ptr = root.child("packages");
    let items = as_array(root.required("packages")?, &list_ptr)?;
    let mut packages: Vec<PackageRelease> = Vec::with_capacity(items.len());
    let mut keys = BTreeSet::new();
    for (index, item) in items.iter().enumerate() {
        let pointer = format!("{list_ptr}/{index}");
        let release = lock_entry(&as_object(item, pointer.clone())?)?;
        if !keys.insert(release.key.clone()) {
            return Err(ProfileError::new(
                ProfileErrorCode::DuplicatePackage,
                pointer,
                format!("{} is pinned twice", release.key),
            ));
        }
        packages.push(release);
    }
    Ok(ProfileLock {
        schema_version,
        profile,
        packages,
    })
}

fn schema_version(root: &Obj<'_>) -> Result<u32> {
    let pointer = "/schemaVersion";
    let Some(value) = root.get("schemaVersion") else {
        return Err(ProfileError::new(
            ProfileErrorCode::MissingSchemaVersion,
            pointer,
            "required field `schemaVersion` is missing",
        ));
    };
    let Value::Number(number) = value else {
        return Err(wrong_type(value, pointer, "an integer").into());
    };
    match number.as_u64() {
        Some(v) if v == u64::from(SUPPORTED_PROFILE_SCHEMA_VERSION) => {
            Ok(SUPPORTED_PROFILE_SCHEMA_VERSION)
        }
        _ => Err(ProfileError::new(
            ProfileErrorCode::UnsupportedSchemaVersion,
            pointer,
            format!(
                "schemaVersion {number} is not supported; this parser supports \
                 {SUPPORTED_PROFILE_SCHEMA_VERSION}"
            ),
        )),
    }
}

/// `[a-z0-9]+(-[a-z0-9]+)*` segments joined by `.`, at most 128 characters.
pub(crate) fn is_valid_profile_id(id: &str) -> bool {
    id.len() <= MAX_PROFILE_ID_LEN && id.split('.').all(is_id_segment)
}

fn profile_id(value: &Value, pointer: &str) -> Result<String> {
    let id = as_str(value, pointer)?;
    if !is_valid_profile_id(id) {
        return Err(ProfileError::new(
            ProfileErrorCode::InvalidProfileId,
            pointer,
            format!(
                "{id:?} is not a profile id: use dot-separated lowercase segments of a-z, 0-9 and \
                 inner hyphens (e.g. work-setup), at most {MAX_PROFILE_ID_LEN} characters"
            ),
        ));
    }
    Ok(id.to_owned())
}

fn theme_ref(obj: &Obj<'_>, id_field: &str) -> Result<ThemeRef> {
    obj.deny_unknown(&[id_field, "version"])?;
    let id = package_id(obj.required(id_field)?, &obj.child(id_field))?;
    let key = PackageKey::new(PackageKind::Theme, &id).expect("package_id validated the id");
    let version = version(obj.required("version")?, &obj.child("version"))?;
    Ok(ThemeRef { key, version })
}

fn surface_id(surface: &str, pointer: &str) -> Result<()> {
    if is_valid_surface_id(surface) {
        Ok(())
    } else {
        Err(ProfileError::new(
            ProfileErrorCode::InvalidSurface,
            pointer,
            format!(
                "{surface:?} is not a surface id: use a lowercase name (a-z, 0-9, inner hyphens) \
                 or custom:<package-id>"
            ),
        ))
    }
}

fn wallpaper(obj: &Obj<'_>) -> Result<Wallpaper> {
    obj.deny_unknown(&["path", "asset"])?;
    match (obj.get("path"), obj.get("asset")) {
        (Some(path), None) => Ok(Wallpaper::UserFile {
            path: package_path(path, &obj.child("path"))?,
        }),
        (None, Some(asset)) => {
            let pointer = obj.child("asset");
            let name = as_str(asset, &pointer)?;
            if !is_valid_key(name) {
                return Err(ProfileError::new(
                    ProfileErrorCode::InvalidValue,
                    pointer,
                    format!("{name:?} is not an asset name"),
                ));
            }
            Ok(Wallpaper::ThemeAsset {
                name: name.to_owned(),
            })
        }
        _ => Err(ProfileError::new(
            ProfileErrorCode::InvalidValue,
            "/wallpaper",
            "wallpaper needs exactly one of `path` (a private user file) or `asset` (a theme asset)",
        )),
    }
}

type MonitorMap = BTreeMap<String, BTreeMap<String, Map<String, Value>>>;

fn monitors(root: &Obj<'_>) -> Result<MonitorMap> {
    let mut out = BTreeMap::new();
    let Some(obj) = root.optional_object("monitors")? else {
        return Ok(out);
    };
    for (monitor, value) in obj.map {
        let pointer = obj.child(monitor);
        if !is_valid_key(monitor) {
            return Err(ProfileError::new(
                ProfileErrorCode::InvalidValue,
                pointer,
                format!(
                    "{monitor:?} is not a monitor key: use a lowercase name (a-z, 0-9, inner \
                     hyphens), e.g. primary"
                ),
            ));
        }
        let surfaces = as_object(value, pointer)?;
        let mut placements = BTreeMap::new();
        for (surface, placement) in surfaces.map {
            let pointer = surfaces.child(surface);
            surface_id(surface, &pointer)?;
            let placement = as_object(placement, pointer)?;
            placements.insert(surface.clone(), placement.map.clone());
        }
        out.insert(monitor.clone(), placements);
    }
    Ok(out)
}

/// `[A-Za-z][A-Za-z0-9_-]*` segments joined by `.`.
fn is_valid_token_name(name: &str) -> bool {
    name.len() <= MAX_TOKEN_NAME_LEN
        && name.split('.').all(|segment| {
            segment
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        })
}

fn token_overrides(root: &Obj<'_>) -> Result<BTreeMap<String, TokenValue>> {
    let mut out = BTreeMap::new();
    let Some(obj) = root.optional_object("tokenOverrides")? else {
        return Ok(out);
    };
    for (name, value) in obj.map {
        let pointer = obj.child(name);
        if !is_valid_token_name(name) {
            return Err(ProfileError::new(
                ProfileErrorCode::InvalidValue,
                pointer,
                format!(
                    "{name:?} is not a token name: use dot-separated segments of letters, digits, \
                     `_` and `-` starting with a letter"
                ),
            ));
        }
        let token = match value {
            Value::String(_) => {
                TokenValue::String(text_value(value, &pointer, MAX_SHORT_TEXT_LEN)?)
            }
            Value::Number(number) => TokenValue::Number(number.clone()),
            other => return Err(wrong_type(other, pointer, "a string or number").into()),
        };
        out.insert(name.clone(), token);
    }
    Ok(out)
}

fn lock_entry(obj: &Obj<'_>) -> Result<PackageRelease> {
    obj.deny_unknown(LOCK_ENTRY_FIELDS)?;
    let kind_ptr = obj.child("kind");
    let kind_name = as_str(obj.required("kind")?, &kind_ptr)?;
    let kind = PackageKind::from_name(kind_name).ok_or_else(|| {
        ProfileError::new(
            ProfileErrorCode::InvalidValue,
            kind_ptr,
            format!("unknown package kind {kind_name:?}; expected \"theme\""),
        )
    })?;
    let id = package_id(obj.required("id")?, &obj.child("id"))?;
    let key = PackageKey::new(kind, &id).expect("package_id validated the id");
    let version = version(obj.required("version")?, &obj.child("version"))?;

    let origin_ptr = obj.child("origin");
    let origin_text = as_str(obj.required("origin")?, &origin_ptr)?;
    let origin = PackageOrigin::parse(origin_text).map_err(|err| {
        ProfileError::new(ProfileErrorCode::InvalidOrigin, &origin_ptr, err.message())
    })?;
    if origin.as_str() != origin_text {
        return Err(ProfileError::new(
            ProfileErrorCode::InvalidOrigin,
            origin_ptr,
            format!("origin must be written in canonical form: {origin}"),
        ));
    }

    let digest_ptr = obj.child("digest");
    let digest_text = as_str(obj.required("digest")?, &digest_ptr)?;
    let digest = Sha256Digest::parse(digest_text).map_err(|err| {
        debug_assert_eq!(err.kind(), IdentityErrorKind::InvalidDigest);
        ProfileError::new(ProfileErrorCode::InvalidDigest, &digest_ptr, err.message())
    })?;
    Ok(PackageRelease::new(key, origin, version, digest))
}

// --------------------------------------------------------- verification

/// Stable codes for a profile and its lock disagreeing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum LockProblemCode {
    /// The lock's `profile` is not the profile's `id`.
    ProfileMismatch,
    /// The profile references a theme the lock does not pin.
    Unlocked,
    /// The lock pins a different version than the profile references.
    VersionMismatch,
    /// The lock pins a package the profile no longer references (stale; safe
    /// to drop when re-locking).
    Unused,
}

impl LockProblemCode {
    /// The stable string form, e.g. `GLINT_PROFILE_LOCK_UNLOCKED`.
    pub const fn as_str(self) -> &'static str {
        match self {
            LockProblemCode::ProfileMismatch => "GLINT_PROFILE_LOCK_PROFILE_MISMATCH",
            LockProblemCode::Unlocked => "GLINT_PROFILE_LOCK_UNLOCKED",
            LockProblemCode::VersionMismatch => "GLINT_PROFILE_LOCK_VERSION_MISMATCH",
            LockProblemCode::Unused => "GLINT_PROFILE_LOCK_UNUSED",
        }
    }
}

impl fmt::Display for LockProblemCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One disagreement between a profile and its lock. The pointer is into
/// `profile.json` for profile-side problems and into `profile.lock` for
/// [`LockProblemCode::Unused`] and [`LockProblemCode::ProfileMismatch`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LockProblem {
    /// Stable code.
    pub code: LockProblemCode,
    /// JSON Pointer of the offending reference or entry.
    pub pointer: String,
    /// Human-readable detail (not stable).
    pub message: String,
}

/// Check that `lock` pins exactly the themes `profile` references, at the
/// referenced versions. An empty result means the lock fully covers the
/// profile. Whether each pinned release is actually installed (origin and
/// digest) is the installer's check (INS).
pub fn verify_profile_lock(profile: &Profile, lock: &ProfileLock) -> Vec<LockProblem> {
    let mut problems = Vec::new();
    if lock.profile != profile.id {
        problems.push(LockProblem {
            code: LockProblemCode::ProfileMismatch,
            pointer: "/profile".to_owned(),
            message: format!(
                "lock belongs to profile {:?}, not {:?}",
                lock.profile, profile.id
            ),
        });
    }
    let mut used = BTreeSet::new();
    for (pointer, theme) in profile.theme_refs() {
        let key = &theme.key;
        match lock.find(key) {
            None => problems.push(LockProblem {
                code: LockProblemCode::Unlocked,
                pointer,
                message: format!("{key} is not pinned by the lock"),
            }),
            Some(release) if release.version != theme.version => problems.push(LockProblem {
                code: LockProblemCode::VersionMismatch,
                pointer: child_pointer(&pointer, "version"),
                message: format!(
                    "profile references {key} {} but the lock pins {}",
                    theme.version, release.version
                ),
            }),
            Some(_) => {}
        }
        used.insert(key.clone());
    }
    for (index, release) in lock.packages.iter().enumerate() {
        if !used.contains(&release.key) {
            problems.push(LockProblem {
                code: LockProblemCode::Unused,
                pointer: format!("/packages/{index}"),
                message: format!("{} is pinned but not referenced", release.key),
            });
        }
    }
    problems
}
