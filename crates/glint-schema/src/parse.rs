//! Semantic validation of a `glint.json` document into a [`ThemeManifest`].

use std::collections::{BTreeMap, BTreeSet};

use semver::{Version, VersionReq};
use serde_json::{Map, Value};

use crate::error::{ErrorCode, ManifestError, ManifestWarning, WarningCode};
use crate::manifest::{
    Author, Compatibility, PackageKind, ParsedManifest, Platform, Renderer, RendererKind, Surface,
    ThemeManifest,
};
use crate::{
    MAX_MANIFEST_BYTES, SUPPORTED_ENTRYPOINT_VERSION, SUPPORTED_SCHEMA_VERSION, strict_json,
};

type Result<T> = std::result::Result<T, ManifestError>;

const TOP_LEVEL_FIELDS: &[&str] = &[
    "$schema",
    "schemaVersion",
    "kind",
    "id",
    "version",
    "name",
    "description",
    "author",
    "license",
    "compatibility",
    "renderer",
    "requiredCapabilities",
    "optionalCapabilities",
    "tokens",
    "assets",
    "layouts",
    "surfaces",
];

/// Capability namespaces from `docs/plan/02-architecture/capabilities-permissions-and-errors.md`.
const CAPABILITY_NAMESPACES: &[&str] = &["surface", "system", "desktop", "native", "host"];

const MAX_ID_LEN: usize = 128;
const MAX_NAME_LEN: usize = 128;
const MAX_DESCRIPTION_LEN: usize = 2048;
const MAX_SHORT_TEXT_LEN: usize = 256;
const MAX_URL_LEN: usize = 2048;
const MAX_VERSION_LEN: usize = 64;
const MAX_PATH_LEN: usize = 512;
const MAX_KEY_LEN: usize = 64;
const MAX_CAPABILITY_LEN: usize = 128;

/// Size limit, byte-order mark, strict JSON and root type: the document-level
/// rules shared by the full parser and the version-envelope probe.
pub(crate) fn load_document(text: &str) -> Result<Map<String, Value>> {
    if text.len() > MAX_MANIFEST_BYTES {
        return Err(ManifestError::new(
            ErrorCode::TooLarge,
            "",
            format!(
                "manifest is {} bytes; the limit is {MAX_MANIFEST_BYTES}",
                text.len()
            ),
        ));
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    match strict_json::parse(text)? {
        Value::Object(root) => Ok(root),
        _ => Err(ManifestError::new(
            ErrorCode::NotAnObject,
            "",
            "manifest root must be a JSON object",
        )),
    }
}

pub(crate) fn parse_theme_manifest(text: &str) -> Result<ParsedManifest> {
    let document = load_document(text)?;
    let root = Obj::root(&document);
    let mut warnings = Vec::new();

    // Version and kind first: without them no other field can be interpreted.
    let schema_version = schema_version(&root)?;
    let kind = kind(&root)?;
    root.deny_unknown(TOP_LEVEL_FIELDS)?;

    if let Some(value) = root.get("$schema") {
        // Editor hint only; never dereferenced.
        text_value(value, &root.child("$schema"), MAX_URL_LEN)?;
    }

    let id = match root.get("id") {
        None => {
            return Err(ManifestError::new(
                ErrorCode::MissingId,
                "/id",
                "required field `id` is missing",
            ));
        }
        Some(value) => package_id(value, "/id")?,
    };
    let name = text_value(root.required("name")?, "/name", MAX_NAME_LEN)?;
    let version = version(root.required("version")?, "/version")?;
    let description = root
        .get("description")
        .map(|v| text_value(v, "/description", MAX_DESCRIPTION_LEN))
        .transpose()?;
    let author = author(&root.required_object("author")?, &mut warnings)?;
    let license = text_value(root.required("license")?, "/license", MAX_SHORT_TEXT_LEN)?;
    let compatibility = compatibility(&root.required_object("compatibility")?)?;
    let renderer = root
        .optional_object("renderer")?
        .map(|o| renderer(&o))
        .transpose()?;

    let required_capabilities = capabilities(&root, "requiredCapabilities", &BTreeSet::new())?;
    let already: BTreeSet<&str> = required_capabilities.iter().map(String::as_str).collect();
    let optional_capabilities = capabilities(&root, "optionalCapabilities", &already)?;

    let tokens = root
        .get("tokens")
        .map(|v| package_path(v, "/tokens"))
        .transpose()?;
    let assets = path_map(&root, "assets")?;
    let layouts = path_map(&root, "layouts")?;
    let surfaces = surfaces(&root)?;

    if !surfaces.is_empty() && renderer.is_none() {
        return Err(ManifestError::new(
            ErrorCode::MissingField,
            "/renderer",
            "`renderer` is required when `surfaces` declares any surface",
        ));
    }

    let manifest = ThemeManifest {
        schema_version,
        kind,
        id,
        version,
        name,
        description,
        author,
        license,
        compatibility,
        renderer,
        required_capabilities,
        optional_capabilities,
        tokens,
        assets,
        layouts,
        surfaces,
    };
    Ok(ParsedManifest { manifest, warnings })
}

// ------------------------------------------------------------------- helpers

/// A JSON object together with its JSON Pointer.
pub(crate) struct Obj<'a> {
    pub(crate) map: &'a Map<String, Value>,
    ptr: String,
}

impl<'a> Obj<'a> {
    pub(crate) fn root(map: &'a Map<String, Value>) -> Self {
        Obj {
            map,
            ptr: String::new(),
        }
    }

    pub(crate) fn child(&self, key: &str) -> String {
        child_pointer(&self.ptr, key)
    }

    pub(crate) fn get(&self, key: &str) -> Option<&'a Value> {
        self.map.get(key)
    }

    pub(crate) fn required(&self, key: &str) -> Result<&'a Value> {
        self.map.get(key).ok_or_else(|| {
            ManifestError::new(
                ErrorCode::MissingField,
                self.child(key),
                format!("required field `{key}` is missing"),
            )
        })
    }

    pub(crate) fn required_object(&self, key: &str) -> Result<Obj<'a>> {
        let value = self.required(key)?;
        as_object(value, self.child(key))
    }

    pub(crate) fn optional_object(&self, key: &str) -> Result<Option<Obj<'a>>> {
        self.map
            .get(key)
            .map(|value| as_object(value, self.child(key)))
            .transpose()
    }

    fn deny_unknown(&self, allowed: &[&str]) -> Result<()> {
        match self.map.keys().find(|k| !allowed.contains(&k.as_str())) {
            Some(key) => Err(ManifestError::new(
                ErrorCode::UnknownField,
                self.child(key),
                format!(
                    "unknown field `{key}`; allowed fields here are: {}",
                    allowed.join(", ")
                ),
            )),
            None => Ok(()),
        }
    }

    /// Forward compatibility for a non-security-sensitive object: unknown
    /// keys are ignored and reported as warnings, never interpreted.
    fn warn_unknown(&self, allowed: &[&str], warnings: &mut Vec<ManifestWarning>) {
        for key in self.map.keys().filter(|k| !allowed.contains(&k.as_str())) {
            warnings.push(ManifestWarning::new(
                WarningCode::UnknownField,
                self.child(key),
                format!(
                    "unknown field `{key}` was ignored; known fields here are: {}",
                    allowed.join(", ")
                ),
            ));
        }
    }
}

/// RFC 6901 child pointer: `~` becomes `~0` and `/` becomes `~1`.
fn child_pointer(parent: &str, key: &str) -> String {
    format!("{parent}/{}", key.replace('~', "~0").replace('/', "~1"))
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

pub(crate) fn wrong_type(
    value: &Value,
    pointer: impl Into<String>,
    expected: &str,
) -> ManifestError {
    ManifestError::new(
        ErrorCode::InvalidType,
        pointer,
        format!("expected {expected}, found {}", type_name(value)),
    )
}

pub(crate) fn invalid(
    code: ErrorCode,
    pointer: impl Into<String>,
    message: impl Into<String>,
) -> ManifestError {
    ManifestError::new(code, pointer, message)
}

pub(crate) fn as_object(value: &Value, pointer: String) -> Result<Obj<'_>> {
    match value {
        Value::Object(map) => Ok(Obj { map, ptr: pointer }),
        other => Err(wrong_type(other, pointer, "an object")),
    }
}

fn as_str<'v>(value: &'v Value, pointer: &str) -> Result<&'v str> {
    value
        .as_str()
        .ok_or_else(|| wrong_type(value, pointer, "a string"))
}

fn as_array<'v>(value: &'v Value, pointer: &str) -> Result<&'v Vec<Value>> {
    value
        .as_array()
        .ok_or_else(|| wrong_type(value, pointer, "an array"))
}

/// Non-blank display text without control characters, at most `max` characters.
fn text_value(value: &Value, pointer: &str, max: usize) -> Result<String> {
    let text = as_str(value, pointer)?;
    if text.trim().is_empty() {
        return Err(invalid(
            ErrorCode::InvalidValue,
            pointer,
            "must not be blank",
        ));
    }
    if text.chars().count() > max {
        return Err(invalid(
            ErrorCode::InvalidValue,
            pointer,
            format!("must be at most {max} characters"),
        ));
    }
    if text.chars().any(char::is_control) {
        return Err(invalid(
            ErrorCode::InvalidValue,
            pointer,
            "must not contain control characters",
        ));
    }
    Ok(text.to_owned())
}

// -------------------------------------------------------------------- fields

fn schema_version(root: &Obj<'_>) -> Result<u32> {
    let pointer = "/schemaVersion";
    let Some(value) = root.get("schemaVersion") else {
        return Err(invalid(
            ErrorCode::MissingSchemaVersion,
            pointer,
            "required field `schemaVersion` is missing",
        ));
    };
    let Value::Number(number) = value else {
        return Err(wrong_type(value, pointer, "an integer"));
    };
    match number.as_u64() {
        Some(v) if v == u64::from(SUPPORTED_SCHEMA_VERSION) => Ok(SUPPORTED_SCHEMA_VERSION),
        _ => Err(invalid(
            ErrorCode::UnsupportedSchemaVersion,
            pointer,
            format!(
                "schemaVersion {number} is not supported; this parser supports {SUPPORTED_SCHEMA_VERSION}"
            ),
        )),
    }
}

fn kind(root: &Obj<'_>) -> Result<PackageKind> {
    let pointer = "/kind";
    let Some(value) = root.get("kind") else {
        return Err(invalid(
            ErrorCode::MissingKind,
            pointer,
            "required field `kind` is missing",
        ));
    };
    match as_str(value, pointer)? {
        "theme" => Ok(PackageKind::Theme),
        other => Err(invalid(
            ErrorCode::WrongKind,
            pointer,
            format!("expected kind \"theme\", found {other:?}"),
        )),
    }
}

/// `[a-z0-9]+(-[a-z0-9]+)*`
fn is_id_segment(segment: &str) -> bool {
    !segment.is_empty()
        && !segment.starts_with('-')
        && !segment.ends_with('-')
        && !segment.contains("--")
        && segment
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn is_valid_id(id: &str) -> bool {
    id.len() <= MAX_ID_LEN && id.split('.').count() >= 2 && id.split('.').all(is_id_segment)
}

fn package_id(value: &Value, pointer: &str) -> Result<String> {
    let id = as_str(value, pointer)?;
    if !is_valid_id(id) {
        return Err(invalid(
            ErrorCode::InvalidId,
            pointer,
            format!(
                "{id:?} is not a package id: use at least two dot-separated lowercase segments of \
                 a-z, 0-9 and inner hyphens (e.g. dev.example.midnight), at most {MAX_ID_LEN} characters"
            ),
        ));
    }
    Ok(id.to_owned())
}

fn version(value: &Value, pointer: &str) -> Result<Version> {
    let text = as_str(value, pointer)?;
    if text.len() > MAX_VERSION_LEN {
        return Err(invalid(
            ErrorCode::InvalidVersion,
            pointer,
            "version is too long",
        ));
    }
    Version::parse(text).map_err(|e| {
        invalid(
            ErrorCode::InvalidVersion,
            pointer,
            format!("{text:?} is not a semantic version: {e}"),
        )
    })
}

pub(crate) fn version_req(value: &Value, pointer: &str) -> Result<VersionReq> {
    let text = as_str(value, pointer)?;
    if text.trim().is_empty() || text.len() > MAX_VERSION_LEN {
        return Err(invalid(
            ErrorCode::InvalidVersionRequirement,
            pointer,
            "version requirement must be non-blank and short",
        ));
    }
    VersionReq::parse(text).map_err(|e| {
        invalid(
            ErrorCode::InvalidVersionRequirement,
            pointer,
            format!("{text:?} is not a semver requirement: {e}"),
        )
    })
}

fn author(obj: &Obj<'_>, warnings: &mut Vec<ManifestWarning>) -> Result<Author> {
    // `author` is display metadata that Glint never acts on, so it is the one
    // section where unknown keys are tolerated (see manifest-compatibility.md).
    obj.warn_unknown(&["name", "email", "url"], warnings);
    let name = text_value(obj.required("name")?, &obj.child("name"), MAX_NAME_LEN)?;
    let email = match obj.get("email") {
        None => None,
        Some(value) => {
            let pointer = obj.child("email");
            let email = text_value(value, &pointer, MAX_SHORT_TEXT_LEN)?;
            if !email.contains('@') || email.contains(char::is_whitespace) {
                return Err(invalid(
                    ErrorCode::InvalidValue,
                    pointer,
                    "not an e-mail address",
                ));
            }
            Some(email)
        }
    };
    let url = match obj.get("url") {
        None => None,
        Some(value) => {
            let pointer = obj.child("url");
            let url = text_value(value, &pointer, MAX_URL_LEN)?;
            if !url.starts_with("https://") || url.contains(char::is_whitespace) {
                return Err(invalid(
                    ErrorCode::InvalidValue,
                    pointer,
                    "author url must be an https:// URL",
                ));
            }
            Some(url)
        }
    };
    Ok(Author { name, email, url })
}

fn compatibility(obj: &Obj<'_>) -> Result<Compatibility> {
    obj.deny_unknown(&["glint", "sdk", "platforms"])?;
    let glint = version_req(obj.required("glint")?, &obj.child("glint"))?;
    let sdk = obj
        .get("sdk")
        .map(|v| version_req(v, &obj.child("sdk")))
        .transpose()?;
    let platforms_ptr = obj.child("platforms");
    let items = as_array(obj.required("platforms")?, &platforms_ptr)?;
    if items.is_empty() {
        return Err(invalid(
            ErrorCode::InvalidValue,
            platforms_ptr,
            "at least one platform is required",
        ));
    }
    let mut platforms = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let pointer = format!("{platforms_ptr}/{index}");
        let name = as_str(item, &pointer)?;
        let platform = Platform::from_name(name).ok_or_else(|| {
            invalid(
                ErrorCode::InvalidValue,
                pointer.clone(),
                format!("unknown platform {name:?}; expected windows, linux or macos"),
            )
        })?;
        if platforms.contains(&platform) {
            return Err(invalid(
                ErrorCode::InvalidValue,
                pointer,
                format!("platform {name:?} is listed twice"),
            ));
        }
        platforms.push(platform);
    }
    Ok(Compatibility {
        glint,
        sdk,
        platforms,
    })
}

fn renderer(obj: &Obj<'_>) -> Result<Renderer> {
    obj.deny_unknown(&["kind", "entrypointVersion"])?;
    let kind_ptr = obj.child("kind");
    let kind = match as_str(obj.required("kind")?, &kind_ptr)? {
        "glint-web" => RendererKind::GlintWeb,
        other => {
            return Err(invalid(
                ErrorCode::UnsupportedRenderer,
                kind_ptr,
                format!("renderer kind {other:?} is not supported; expected \"glint-web\""),
            ));
        }
    };
    let version_ptr = obj.child("entrypointVersion");
    let value = obj.required("entrypointVersion")?;
    let Value::Number(number) = value else {
        return Err(wrong_type(value, version_ptr, "an integer"));
    };
    if number.as_u64() != Some(u64::from(SUPPORTED_ENTRYPOINT_VERSION)) {
        return Err(invalid(
            ErrorCode::UnsupportedRenderer,
            version_ptr,
            format!(
                "entrypointVersion {number} is not supported; expected {SUPPORTED_ENTRYPOINT_VERSION}"
            ),
        ));
    }
    Ok(Renderer {
        kind,
        entrypoint_version: SUPPORTED_ENTRYPOINT_VERSION,
    })
}

/// `[a-z][a-z0-9]*(-[a-z0-9]+)*`
fn is_name_segment(segment: &str) -> bool {
    segment
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_lowercase)
        && is_id_segment(segment)
}

fn is_valid_capability(capability: &str) -> bool {
    let mut segments = capability.split('.');
    capability.len() <= MAX_CAPABILITY_LEN
        && segments
            .next()
            .is_some_and(|ns| CAPABILITY_NAMESPACES.contains(&ns))
        && capability.split('.').count() >= 2
        && segments.all(is_name_segment)
}

fn capabilities(root: &Obj<'_>, key: &str, exclude: &BTreeSet<&str>) -> Result<Vec<String>> {
    let Some(value) = root.get(key) else {
        return Ok(Vec::new());
    };
    let list_ptr = root.child(key);
    let items = as_array(value, &list_ptr)?;
    let mut out: Vec<String> = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let pointer = format!("{list_ptr}/{index}");
        let capability = as_str(item, &pointer)?;
        if !is_valid_capability(capability) {
            return Err(invalid(
                ErrorCode::InvalidCapability,
                pointer,
                format!(
                    "{capability:?} is not a capability id: expected <namespace>.<name>[.<name>...] \
                     with namespace one of {}",
                    CAPABILITY_NAMESPACES.join(", ")
                ),
            ));
        }
        if out.iter().any(|c| c == capability) {
            return Err(invalid(
                ErrorCode::InvalidCapability,
                pointer,
                format!("capability {capability:?} is listed twice"),
            ));
        }
        if exclude.contains(capability) {
            return Err(invalid(
                ErrorCode::InvalidCapability,
                pointer,
                format!(
                    "capability {capability:?} is already required; it cannot also be optional"
                ),
            ));
        }
        out.push(capability.to_owned());
    }
    Ok(out)
}

/// Windows reserved device names, matched case-insensitively on the part
/// before the first dot.
const RESERVED_DEVICE_NAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

fn path_problem(path: &str) -> Option<&'static str> {
    if path.is_empty() {
        return Some("path must not be empty");
    }
    if path.len() > MAX_PATH_LEN {
        return Some("path is too long");
    }
    if path.starts_with('/') {
        return Some("path must be relative to the package root");
    }
    if path.contains('\\') {
        return Some("use `/` as the separator; `\\` is not allowed");
    }
    if path.contains(':') {
        return Some("`:` is not allowed (drive letters, URLs and alternate data streams)");
    }
    if path
        .chars()
        .any(|c| c.is_control() || matches!(c, '*' | '?' | '"' | '<' | '>' | '|'))
    {
        return Some("path contains a character that is not portable");
    }
    for segment in path.split('/') {
        if segment.is_empty() {
            return Some("path must not contain empty segments or a trailing `/`");
        }
        if segment == "." || segment == ".." {
            return Some("`.` and `..` segments are not allowed");
        }
        if segment.ends_with('.') || segment.ends_with(' ') {
            return Some("segments must not end with `.` or a space");
        }
        let stem = segment.split('.').next().unwrap_or(segment);
        if RESERVED_DEVICE_NAMES.contains(&stem.to_ascii_lowercase().as_str()) {
            return Some("segment is a reserved device name");
        }
    }
    None
}

fn package_path(value: &Value, pointer: &str) -> Result<String> {
    let path = as_str(value, pointer)?;
    match path_problem(path) {
        Some(problem) => Err(invalid(
            ErrorCode::InvalidPath,
            pointer,
            format!("{path:?} is not a safe package-relative path: {problem}"),
        )),
        None => Ok(path.to_owned()),
    }
}

fn check_key(key: &str, pointer: &str, valid: bool) -> Result<()> {
    if valid {
        Ok(())
    } else {
        Err(invalid(
            ErrorCode::InvalidValue,
            pointer,
            format!("{key:?} is not a valid name: use lowercase a-z, 0-9 and inner hyphens"),
        ))
    }
}

fn is_valid_key(key: &str) -> bool {
    key.len() <= MAX_KEY_LEN && is_name_segment(key)
}

fn path_map(root: &Obj<'_>, key: &str) -> Result<BTreeMap<String, String>> {
    let Some(obj) = root.optional_object(key)? else {
        return Ok(BTreeMap::new());
    };
    let mut out = BTreeMap::new();
    for (name, value) in obj.map {
        let pointer = obj.child(name);
        check_key(name, &pointer, is_valid_key(name))?;
        out.insert(name.clone(), package_path(value, &pointer)?);
    }
    Ok(out)
}

fn is_valid_surface_id(id: &str) -> bool {
    match id.strip_prefix("custom:") {
        Some(rest) => is_valid_id(rest),
        None => is_valid_key(id),
    }
}

fn surfaces(root: &Obj<'_>) -> Result<BTreeMap<String, Surface>> {
    let Some(obj) = root.optional_object("surfaces")? else {
        return Ok(BTreeMap::new());
    };
    let mut out = BTreeMap::new();
    for (id, value) in obj.map {
        let pointer = obj.child(id);
        check_key(id, &pointer, is_valid_surface_id(id))?;
        let surface = as_object(value, pointer)?;
        surface.deny_unknown(&["entry", "placement"])?;
        let entry_ptr = surface.child("entry");
        let entry = package_path(surface.required("entry")?, &entry_ptr)?;
        let lower = entry.to_ascii_lowercase();
        if !(lower.ends_with(".html") || lower.ends_with(".htm")) {
            return Err(invalid(
                ErrorCode::InvalidPath,
                entry_ptr,
                "surface entry must be an .html document inside the package",
            ));
        }
        let placement = surface.optional_object("placement")?.map(|p| p.map.clone());
        out.insert(id.clone(), Surface { entry, placement });
    }
    Ok(out)
}
