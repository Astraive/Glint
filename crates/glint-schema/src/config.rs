//! The **proposed** application configuration (`config.toml`, v1) and its
//! merge policy (SCH-007).
//!
//! Layers, from lowest to highest authority:
//!
//! 1. **Bundled defaults** (`config/default.toml`, embedded as
//!    [`BUNDLED_DEFAULT_CONFIG`]). Read-only, and it must set every key.
//! 2. **User config** (the user-level `config.toml`). Each key overrides the
//!    default; `[registries.<name>]` entries merge by name, field by field.
//!    A user may change a security setting, but every change that makes it
//!    weaker than the bundled default is reported as
//!    [`ConfigWarningCode::SecurityWeakened`].
//! 3. **Package requests** (settings a theme or other package asks for).
//!    Never applied: every requested key is ignored with a diagnostic,
//!    [`ConfigWarningCode::PackageSecurityOverrideIgnored`] for security
//!    settings and registries, [`ConfigWarningCode::PackageSettingIgnored`]
//!    otherwise. (The v1 `glint.json` has no settings field at all: a
//!    manifest that carries `packages` or `security` is rejected with
//!    `GLINT_MANIFEST_UNKNOWN_FIELD`.)
//!
//! Unknown keys are rejected in `[packages]`, `[security]`,
//! `[registries.<name>]`, at the top level and as unknown tables, because an
//! older reader that ignored them could drop a setting the user relies on.
//! Unknown keys inside `[app]`, `[registry]`, `[theme]` and `[shell]` are
//! ignored with [`ConfigWarningCode::UnknownField`].
//!
//! The config format is distinct from the registry index, the theme manifest
//! and the profile: none of them is read through this module.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use serde_json::{Map, Number, Value};

use crate::identity::PackageOrigin;
use crate::parse::{Obj, as_str, child_pointer, is_valid_key, wrong_type};
use crate::profile::is_valid_profile_id;

/// The bundled, read-only defaults (`config/default.toml`).
pub const BUNDLED_DEFAULT_CONFIG: &str = include_str!("../../../config/default.toml");

/// The only config `schema_version` this crate understands. The key is
/// optional; absent means this version.
pub const SUPPORTED_CONFIG_SCHEMA_VERSION: u32 = 1;

/// Largest accepted config file, in bytes (256 KiB).
pub const MAX_CONFIG_BYTES: usize = 256 * 1024;

const TABLES: &[&str] = &[
    "app",
    "registry",
    "registries",
    "theme",
    "packages",
    "security",
    "shell",
];
/// Tables whose unknown keys are rejected rather than ignored.
const STRICT_TABLES: &[&str] = &["packages", "security"];
const REGISTRY_FIELDS: &[&str] = &["type", "url", "branch"];
const MAX_LOCALE_LEN: usize = 35;
const MAX_BRANCH_LEN: usize = 255;
const MAX_TTL_SECS: u64 = 365 * 24 * 60 * 60;

// ------------------------------------------------------------- settings

#[derive(Debug, Clone, Copy)]
enum Kind {
    Bool,
    Choice(&'static [&'static str]),
    Locale,
    ProfileId,
    RegistryName,
    Duration,
    Count { min: u32, max: u32 },
}

/// How a setting relates to security, measured against the bundled default.
#[derive(Debug, Clone, Copy)]
enum Safety {
    /// Not security-relevant.
    Neutral,
    /// A boolean whose safe value is the payload.
    SafeBool(bool),
    /// A choice whose values are listed from safest to weakest.
    Ranked,
    /// A count where larger is weaker.
    LowerIsSafer,
}

#[derive(Debug)]
struct Setting {
    table: &'static str,
    key: &'static str,
    kind: Kind,
    safety: Safety,
}

const STARTUP: &[&str] = &["user-session", "manual"];
/// Safest first.
const NETWORK_DEFAULTS: &[&str] = &["deny", "ask", "allow"];

const SETTINGS: &[Setting] = &[
    Setting {
        table: "app",
        key: "startup",
        kind: Kind::Choice(STARTUP),
        safety: Safety::Neutral,
    },
    Setting {
        table: "app",
        key: "locale",
        kind: Kind::Locale,
        safety: Safety::Neutral,
    },
    Setting {
        table: "registry",
        key: "default",
        kind: Kind::RegistryName,
        safety: Safety::Neutral,
    },
    Setting {
        table: "registry",
        key: "auto_sync",
        kind: Kind::Bool,
        safety: Safety::Neutral,
    },
    Setting {
        table: "registry",
        key: "cache_ttl",
        kind: Kind::Duration,
        safety: Safety::Neutral,
    },
    Setting {
        table: "theme",
        key: "active_profile",
        kind: Kind::ProfileId,
        safety: Safety::Neutral,
    },
    Setting {
        table: "theme",
        key: "auto_update",
        kind: Kind::Bool,
        safety: Safety::SafeBool(false),
    },
    Setting {
        table: "theme",
        key: "preview_before_apply",
        kind: Kind::Bool,
        safety: Safety::SafeBool(true),
    },
    Setting {
        table: "packages",
        key: "allow_external_git",
        kind: Kind::Bool,
        safety: Safety::SafeBool(false),
    },
    Setting {
        table: "packages",
        key: "allow_local_archives",
        kind: Kind::Bool,
        safety: Safety::SafeBool(false),
    },
    Setting {
        table: "packages",
        key: "verify_integrity",
        kind: Kind::Bool,
        safety: Safety::SafeBool(true),
    },
    Setting {
        table: "packages",
        key: "allow_native_scripts",
        kind: Kind::Bool,
        safety: Safety::SafeBool(false),
    },
    Setting {
        table: "security",
        key: "confirm_new_permissions",
        kind: Kind::Bool,
        safety: Safety::SafeBool(true),
    },
    Setting {
        table: "security",
        key: "untrusted_network_default",
        kind: Kind::Choice(NETWORK_DEFAULTS),
        safety: Safety::Ranked,
    },
    Setting {
        table: "shell",
        key: "use_default_fallback",
        kind: Kind::Bool,
        safety: Safety::SafeBool(true),
    },
    Setting {
        table: "shell",
        key: "safe_mode_after_crashes",
        kind: Kind::Count { min: 1, max: 100 },
        safety: Safety::LowerIsSafer,
    },
];

impl Setting {
    fn pointer(&self) -> String {
        format!("/{}/{}", self.table, self.key)
    }

    fn is_security(&self) -> bool {
        !matches!(self.safety, Safety::Neutral)
    }

    /// Whether moving from `default` to `new` makes this setting less safe.
    fn weakens(&self, default: &SettingValue, new: &SettingValue) -> bool {
        match (self.safety, default, new) {
            (Safety::SafeBool(safe), SettingValue::Bool(d), SettingValue::Bool(n)) => {
                *d == safe && *n != safe
            }
            (Safety::Ranked, SettingValue::Text(d), SettingValue::Text(n)) => {
                let rank = |v: &str| match self.kind {
                    Kind::Choice(choices) => choices.iter().position(|c| *c == v),
                    _ => None,
                };
                rank(n) > rank(d)
            }
            (Safety::LowerIsSafer, SettingValue::Count(d), SettingValue::Count(n)) => n > d,
            _ => false,
        }
    }
}

// --------------------------------------------------------------- errors

/// Stable, machine-readable config error codes. The string form is the
/// contract; codes are only ever added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ConfigErrorCode {
    /// Not valid TOML (including a repeated key).
    InvalidToml,
    /// Larger than [`MAX_CONFIG_BYTES`].
    TooLarge,
    /// `schema_version` is not [`SUPPORTED_CONFIG_SCHEMA_VERSION`].
    UnsupportedSchemaVersion,
    /// A key or table is not part of the v1 format in a strict section.
    UnknownField,
    /// A key has the wrong TOML type.
    InvalidType,
    /// A key has the right type but a disallowed value.
    InvalidValue,
    /// A new registry is missing a required field.
    MissingField,
    /// `registry.default` names a registry that is not defined.
    UnknownRegistry,
    /// The bundled defaults do not set every key (a build defect).
    IncompleteDefaults,
}

impl ConfigErrorCode {
    /// Every code, in declaration order.
    pub const ALL: &'static [ConfigErrorCode] = &[
        ConfigErrorCode::InvalidToml,
        ConfigErrorCode::TooLarge,
        ConfigErrorCode::UnsupportedSchemaVersion,
        ConfigErrorCode::UnknownField,
        ConfigErrorCode::InvalidType,
        ConfigErrorCode::InvalidValue,
        ConfigErrorCode::MissingField,
        ConfigErrorCode::UnknownRegistry,
        ConfigErrorCode::IncompleteDefaults,
    ];

    /// The stable string form, e.g. `GLINT_CONFIG_UNKNOWN_FIELD`.
    pub const fn as_str(self) -> &'static str {
        match self {
            ConfigErrorCode::InvalidToml => "GLINT_CONFIG_INVALID_TOML",
            ConfigErrorCode::TooLarge => "GLINT_CONFIG_TOO_LARGE",
            ConfigErrorCode::UnsupportedSchemaVersion => "GLINT_CONFIG_UNSUPPORTED_SCHEMA_VERSION",
            ConfigErrorCode::UnknownField => "GLINT_CONFIG_UNKNOWN_FIELD",
            ConfigErrorCode::InvalidType => "GLINT_CONFIG_INVALID_TYPE",
            ConfigErrorCode::InvalidValue => "GLINT_CONFIG_INVALID_VALUE",
            ConfigErrorCode::MissingField => "GLINT_CONFIG_MISSING_FIELD",
            ConfigErrorCode::UnknownRegistry => "GLINT_CONFIG_UNKNOWN_REGISTRY",
            ConfigErrorCode::IncompleteDefaults => "GLINT_CONFIG_INCOMPLETE_DEFAULTS",
        }
    }
}

impl fmt::Display for ConfigErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where a config layer came from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConfigSource {
    /// The read-only bundled defaults.
    BundledDefaults,
    /// The user-level `config.toml`.
    User,
    /// Settings requested by a package, named by package id. Never applied.
    Package(String),
}

impl fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigSource::BundledDefaults => f.write_str("bundled defaults"),
            ConfigSource::User => f.write_str("user config"),
            ConfigSource::Package(id) => write!(f, "package {id}"),
        }
    }
}

/// A config rejection: a stable [`ConfigErrorCode`], the layer it came from,
/// a JSON Pointer over the TOML document (e.g. `/packages/verify_integrity`)
/// and a message (not stable).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    code: ConfigErrorCode,
    source: ConfigSource,
    pointer: String,
    message: String,
}

impl ConfigError {
    fn new(
        code: ConfigErrorCode,
        source: &ConfigSource,
        pointer: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            source: source.clone(),
            pointer: pointer.into(),
            message: message.into(),
        }
    }

    /// The stable error code.
    pub fn code(&self) -> ConfigErrorCode {
        self.code
    }

    /// The layer the problem is in.
    pub fn source(&self) -> &ConfigSource {
        &self.source
    }

    /// JSON Pointer to the offending key; `""` for the whole document.
    pub fn pointer(&self) -> &str {
        &self.pointer
    }

    /// Human-readable detail (not stable).
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let at = if self.pointer.is_empty() {
            "(root)"
        } else {
            self.pointer.as_str()
        };
        write!(
            f,
            "{} in {} at {}: {}",
            self.code, self.source, at, self.message
        )
    }
}

impl std::error::Error for ConfigError {}

/// Stable, machine-readable config warning codes. A warning never makes a
/// layer invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ConfigWarningCode {
    /// An unknown key in a non-strict table was ignored.
    UnknownField,
    /// The user config made a security setting weaker than the bundled
    /// default. The value is applied; the warning makes it visible.
    SecurityWeakened,
    /// A package asked to change a security setting or a registry; ignored.
    PackageSecurityOverrideIgnored,
    /// A package asked to change any other setting; ignored.
    PackageSettingIgnored,
}

impl ConfigWarningCode {
    /// Every code, in declaration order.
    pub const ALL: &'static [ConfigWarningCode] = &[
        ConfigWarningCode::UnknownField,
        ConfigWarningCode::SecurityWeakened,
        ConfigWarningCode::PackageSecurityOverrideIgnored,
        ConfigWarningCode::PackageSettingIgnored,
    ];

    /// The stable string form, e.g. `GLINT_CONFIG_WARN_SECURITY_WEAKENED`.
    pub const fn as_str(self) -> &'static str {
        match self {
            ConfigWarningCode::UnknownField => "GLINT_CONFIG_WARN_UNKNOWN_FIELD",
            ConfigWarningCode::SecurityWeakened => "GLINT_CONFIG_WARN_SECURITY_WEAKENED",
            ConfigWarningCode::PackageSecurityOverrideIgnored => {
                "GLINT_CONFIG_WARN_PACKAGE_SECURITY_OVERRIDE_IGNORED"
            }
            ConfigWarningCode::PackageSettingIgnored => "GLINT_CONFIG_WARN_PACKAGE_SETTING_IGNORED",
        }
    }
}

impl fmt::Display for ConfigWarningCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A non-fatal config diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConfigDiagnostic {
    /// Stable code.
    pub code: ConfigWarningCode,
    /// The layer the key came from.
    pub source: ConfigSource,
    /// JSON Pointer to the key.
    pub pointer: String,
    /// Human-readable detail (not stable).
    pub message: String,
}

type Result<T> = std::result::Result<T, ConfigError>;

// ----------------------------------------------------------------- layer

#[derive(Debug, Clone, PartialEq, Eq)]
enum SettingValue {
    Bool(bool),
    Text(String),
    Count(u32),
}

/// A registry entry as written in one layer; fields absent from the layer
/// are `None` and come from a lower layer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RegistryPatch {
    kind: Option<RegistryKind>,
    url: Option<String>,
    branch: Option<String>,
}

/// One validated config document: only the keys it sets, plus warnings for
/// what it ignored. Produced by [`parse_config_layer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigLayer {
    source: ConfigSource,
    /// Index into [`SETTINGS`] to value.
    values: BTreeMap<usize, SettingValue>,
    registries: BTreeMap<String, RegistryPatch>,
    warnings: Vec<ConfigDiagnostic>,
}

impl ConfigLayer {
    /// Where this layer came from.
    pub fn source(&self) -> &ConfigSource {
        &self.source
    }

    /// Ignored content found while parsing, in key order.
    pub fn warnings(&self) -> &[ConfigDiagnostic] {
        &self.warnings
    }

    /// JSON Pointers of every key this layer sets, in a stable order
    /// (settings first, then registry fields).
    pub fn keys(&self) -> Vec<String> {
        let settings = self.values.keys().map(|&i| SETTINGS[i].pointer());
        let registries = self.registries.iter().flat_map(|(name, patch)| {
            let base = child_pointer("/registries", name);
            [
                patch.kind.map(|_| "type"),
                patch.url.as_ref().map(|_| "url"),
                patch.branch.as_ref().map(|_| "branch"),
            ]
            .into_iter()
            .flatten()
            .map(move |field| child_pointer(&base, field))
        });
        settings.chain(registries).collect()
    }
}

/// The bundled defaults, parsed.
///
/// # Panics
///
/// Only if the embedded `config/default.toml` is invalid, which the crate's
/// tests rule out.
pub fn bundled_defaults() -> ConfigLayer {
    parse_config_layer(BUNDLED_DEFAULT_CONFIG, ConfigSource::BundledDefaults)
        .expect("config/default.toml is valid (checked by tests)")
}

/// Parse and validate one config document. `source` says which layer it is;
/// it only affects diagnostics and how [`resolve_config`] treats the layer.
///
/// # Errors
///
/// A [`ConfigError`] for the first problem, e.g.
/// [`ConfigErrorCode::InvalidToml`], [`ConfigErrorCode::UnknownField`] in a
/// strict section, or [`ConfigErrorCode::InvalidValue`].
pub fn parse_config_layer(text: &str, source: ConfigSource) -> Result<ConfigLayer> {
    if text.len() > MAX_CONFIG_BYTES {
        return Err(ConfigError::new(
            ConfigErrorCode::TooLarge,
            &source,
            "",
            format!(
                "config is {} bytes; the limit is {MAX_CONFIG_BYTES}",
                text.len()
            ),
        ));
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let table: toml::Table = text.parse().map_err(|err: toml::de::Error| {
        ConfigError::new(
            ConfigErrorCode::InvalidToml,
            &source,
            "",
            err.message().to_owned(),
        )
    })?;
    let document = toml_table_to_json(&table, "", &source)?;
    LayerParser {
        source,
        warnings: Vec::new(),
    }
    .parse(&document)
}

/// TOML to the JSON data model, so the JSON Pointer helpers apply. TOML has
/// no null, so dates and times (not part of the config format) become JSON
/// `null` and are rejected as a type error wherever they appear.
fn toml_table_to_json(
    table: &toml::Table,
    pointer: &str,
    source: &ConfigSource,
) -> Result<Map<String, Value>> {
    table
        .iter()
        .map(|(key, value)| {
            let pointer = child_pointer(pointer, key);
            Ok((key.clone(), toml_to_json(value, &pointer, source)?))
        })
        .collect()
}

fn toml_to_json(value: &toml::Value, pointer: &str, source: &ConfigSource) -> Result<Value> {
    Ok(match value {
        toml::Value::String(s) => Value::String(s.clone()),
        toml::Value::Integer(i) => Value::from(*i),
        toml::Value::Float(f) => Value::Number(Number::from_f64(*f).ok_or_else(|| {
            ConfigError::new(
                ConfigErrorCode::InvalidValue,
                source,
                pointer,
                "non-finite numbers are not allowed",
            )
        })?),
        toml::Value::Boolean(b) => Value::Bool(*b),
        toml::Value::Datetime(_) => Value::Null,
        toml::Value::Array(items) => Value::Array(
            items
                .iter()
                .enumerate()
                .map(|(index, item)| toml_to_json(item, &format!("{pointer}/{index}"), source))
                .collect::<Result<_>>()?,
        ),
        toml::Value::Table(inner) => Value::Object(toml_table_to_json(inner, pointer, source)?),
    })
}

struct LayerParser {
    source: ConfigSource,
    warnings: Vec<ConfigDiagnostic>,
}

impl LayerParser {
    fn err(
        &self,
        code: ConfigErrorCode,
        pointer: impl Into<String>,
        msg: impl Into<String>,
    ) -> ConfigError {
        ConfigError::new(code, &self.source, pointer, msg)
    }

    fn type_err(&self, value: &Value, pointer: &str, expected: &str) -> ConfigError {
        let manifest_err = wrong_type(value, pointer, expected);
        let message = if matches!(value, Value::Null) {
            format!("expected {expected}, found a date or time")
        } else {
            manifest_err.message().to_owned()
        };
        self.err(ConfigErrorCode::InvalidType, pointer, message)
    }

    fn warn_unknown(&mut self, pointer: String, key: &str, known: &[&str]) {
        self.warnings.push(ConfigDiagnostic {
            code: ConfigWarningCode::UnknownField,
            source: self.source.clone(),
            pointer,
            message: format!(
                "unknown key `{key}` was ignored; known keys here are: {}",
                known.join(", ")
            ),
        });
    }

    fn parse(mut self, document: &Map<String, Value>) -> Result<ConfigLayer> {
        let root = Obj::root(document);
        if let Some(value) = root.get("schema_version") {
            let supported = value.as_u64() == Some(u64::from(SUPPORTED_CONFIG_SCHEMA_VERSION));
            if !value.is_number() {
                return Err(self.type_err(value, "/schema_version", "an integer"));
            }
            if !supported {
                return Err(self.err(
                    ConfigErrorCode::UnsupportedSchemaVersion,
                    "/schema_version",
                    format!(
                        "schema_version {value} is not supported; this parser supports \
                         {SUPPORTED_CONFIG_SCHEMA_VERSION}"
                    ),
                ));
            }
        }
        for key in document.keys() {
            if key != "schema_version" && !TABLES.contains(&key.as_str()) {
                return Err(self.err(
                    ConfigErrorCode::UnknownField,
                    root.child(key),
                    format!(
                        "unknown key or table `{key}`; known tables are: {}",
                        TABLES.join(", ")
                    ),
                ));
            }
        }

        let mut values = BTreeMap::new();
        for &table in TABLES.iter().filter(|t| **t != "registries") {
            let Some(value) = root.get(table) else {
                continue;
            };
            let pointer = root.child(table);
            let Value::Object(map) = value else {
                return Err(self.type_err(value, &pointer, "a table"));
            };
            let known: Vec<&str> = SETTINGS
                .iter()
                .filter(|s| s.table == table)
                .map(|s| s.key)
                .collect();
            for (key, value) in map {
                let key_ptr = child_pointer(&pointer, key);
                let Some(index) = SETTINGS
                    .iter()
                    .position(|s| s.table == table && s.key == key)
                else {
                    if STRICT_TABLES.contains(&table) {
                        return Err(self.err(
                            ConfigErrorCode::UnknownField,
                            key_ptr,
                            format!(
                                "unknown key `{key}` in security-sensitive table [{table}]; known \
                                 keys are: {}",
                                known.join(", ")
                            ),
                        ));
                    }
                    self.warn_unknown(key_ptr, key, &known);
                    continue;
                };
                values.insert(index, self.setting(&SETTINGS[index], value, &key_ptr)?);
            }
        }

        let mut registries = BTreeMap::new();
        if let Some(value) = root.get("registries") {
            let Value::Object(map) = value else {
                return Err(self.type_err(value, "/registries", "a table"));
            };
            for (name, entry) in map {
                let pointer = child_pointer("/registries", name);
                if !is_valid_key(name) {
                    return Err(self.err(
                        ConfigErrorCode::InvalidValue,
                        pointer,
                        format!(
                            "{name:?} is not a registry name: use lowercase a-z, 0-9 and inner \
                             hyphens"
                        ),
                    ));
                }
                let Value::Object(fields) = entry else {
                    return Err(self.type_err(entry, &pointer, "a table"));
                };
                registries.insert(name.clone(), self.registry(fields, &pointer)?);
            }
        }

        Ok(ConfigLayer {
            source: self.source,
            values,
            registries,
            warnings: self.warnings,
        })
    }

    fn setting(&self, setting: &Setting, value: &Value, pointer: &str) -> Result<SettingValue> {
        let text = |expected: &str| -> Result<&str> {
            value
                .as_str()
                .ok_or_else(|| self.type_err(value, pointer, expected))
        };
        match setting.kind {
            Kind::Bool => value
                .as_bool()
                .map(SettingValue::Bool)
                .ok_or_else(|| self.type_err(value, pointer, "a boolean")),
            Kind::Choice(choices) => {
                let v = text("a string")?;
                if choices.contains(&v) {
                    Ok(SettingValue::Text(v.to_owned()))
                } else {
                    Err(self.err(
                        ConfigErrorCode::InvalidValue,
                        pointer,
                        format!("{v:?} is not one of: {}", choices.join(", ")),
                    ))
                }
            }
            Kind::Locale => {
                let v = text("a string")?;
                if v == "system" || is_locale_tag(v) {
                    Ok(SettingValue::Text(v.to_owned()))
                } else {
                    Err(self.err(
                        ConfigErrorCode::InvalidValue,
                        pointer,
                        format!("{v:?} is not \"system\" or a language tag such as en-GB"),
                    ))
                }
            }
            Kind::ProfileId => {
                let v = text("a string")?;
                if is_valid_profile_id(v) {
                    Ok(SettingValue::Text(v.to_owned()))
                } else {
                    Err(self.err(
                        ConfigErrorCode::InvalidValue,
                        pointer,
                        format!("{v:?} is not a profile id"),
                    ))
                }
            }
            Kind::RegistryName => {
                let v = text("a string")?;
                if is_valid_key(v) {
                    Ok(SettingValue::Text(v.to_owned()))
                } else {
                    Err(self.err(
                        ConfigErrorCode::InvalidValue,
                        pointer,
                        format!("{v:?} is not a registry name"),
                    ))
                }
            }
            Kind::Duration => {
                let v = text("a string")?;
                match parse_duration(v) {
                    Some(_) => Ok(SettingValue::Text(v.to_owned())),
                    None => Err(self.err(
                        ConfigErrorCode::InvalidValue,
                        pointer,
                        format!(
                            "{v:?} is not a duration: use a whole number and one of s, m, h, d \
                             (e.g. 24h), at most 365d"
                        ),
                    )),
                }
            }
            Kind::Count { min, max } => {
                let Value::Number(number) = value else {
                    return Err(self.type_err(value, pointer, "an integer"));
                };
                match number.as_u64().and_then(|n| u32::try_from(n).ok()) {
                    Some(n) if (min..=max).contains(&n) => Ok(SettingValue::Count(n)),
                    _ => Err(self.err(
                        ConfigErrorCode::InvalidValue,
                        pointer,
                        format!("must be a whole number from {min} to {max}"),
                    )),
                }
            }
        }
    }

    fn registry(&self, fields: &Map<String, Value>, pointer: &str) -> Result<RegistryPatch> {
        let mut patch = RegistryPatch::default();
        for (field, value) in fields {
            let field_ptr = child_pointer(pointer, field);
            match field.as_str() {
                "type" => {
                    let v = as_str(value, &field_ptr)
                        .map_err(|_| self.type_err(value, &field_ptr, "a string"))?;
                    patch.kind = Some(RegistryKind::from_name(v).ok_or_else(|| {
                        self.err(
                            ConfigErrorCode::InvalidValue,
                            &field_ptr,
                            format!("registry type {v:?} is not supported; expected \"git\""),
                        )
                    })?);
                }
                "url" => {
                    let v = as_str(value, &field_ptr)
                        .map_err(|_| self.type_err(value, &field_ptr, "a string"))?;
                    PackageOrigin::registry(v).map_err(|e| {
                        self.err(ConfigErrorCode::InvalidValue, &field_ptr, e.message())
                    })?;
                    patch.url = Some(v.to_owned());
                }
                "branch" => {
                    let v = as_str(value, &field_ptr)
                        .map_err(|_| self.type_err(value, &field_ptr, "a string"))?;
                    if !is_branch_name(v) {
                        return Err(self.err(
                            ConfigErrorCode::InvalidValue,
                            field_ptr,
                            format!("{v:?} is not a safe Git branch name"),
                        ));
                    }
                    patch.branch = Some(v.to_owned());
                }
                _ => {
                    return Err(self.err(
                        ConfigErrorCode::UnknownField,
                        field_ptr,
                        format!(
                            "unknown registry key `{field}`; known keys are: {}",
                            REGISTRY_FIELDS.join(", ")
                        ),
                    ));
                }
            }
        }
        Ok(patch)
    }
}

/// `xx`, `xxx` or `xx-YY` style tags: 2-3 letters, then `-`-separated
/// alphanumeric subtags of 1-8 characters.
fn is_locale_tag(tag: &str) -> bool {
    let mut parts = tag.split('-');
    tag.len() <= MAX_LOCALE_LEN
        && parts.next().is_some_and(|p| {
            (2..=3).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphabetic())
        })
        && parts.all(|p| (1..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// Conservative Git branch names: `[A-Za-z0-9._/-]`, no `..`, `//`, leading
/// `-`, `/` or `.`, trailing `/`, `.` or `.lock`.
fn is_branch_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_BRANCH_LEN
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'/' | b'-'))
        && !name.contains("..")
        && !name.contains("//")
        && !name.starts_with(['-', '/', '.'])
        && !name.ends_with(['/', '.'])
        && !name.ends_with(".lock")
}

/// `<n><unit>` with unit `s`, `m`, `h` or `d`, at most 365 days.
fn parse_duration(text: &str) -> Option<Duration> {
    let unit = text.chars().last()?;
    let digits = &text[..text.len() - unit.len_utf8()];
    if digits.is_empty() || digits.len() > 10 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u64 = digits.parse().ok()?;
    let secs = n.checked_mul(match unit {
        's' => 1,
        'm' => 60,
        'h' => 60 * 60,
        'd' => 24 * 60 * 60,
        _ => return None,
    })?;
    (secs <= MAX_TTL_SECS).then(|| Duration::from_secs(secs))
}

// --------------------------------------------------------------- resolved

/// When Glint starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Startup {
    /// Start with the user session (`"user-session"`).
    UserSession,
    /// Only when the user starts it (`"manual"`).
    Manual,
}

/// Default network decision for untrusted content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum NetworkDefault {
    /// Refuse (`"deny"`); the safe default.
    Deny,
    /// Ask the user (`"ask"`).
    Ask,
    /// Allow (`"allow"`).
    Allow,
}

/// Registry transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RegistryKind {
    /// A Git repository holding a registry index (`"git"`).
    Git,
}

impl RegistryKind {
    fn from_name(name: &str) -> Option<Self> {
        (name == "git").then_some(RegistryKind::Git)
    }
}

/// A configured registry after merging.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Registry {
    /// Transport.
    pub kind: RegistryKind,
    /// Index location: an `https` URL without credentials.
    pub url: String,
    /// Branch to read; `None` means the repository's default branch.
    pub branch: Option<String>,
}

impl Registry {
    /// The package origin of everything installed from this registry
    /// (ADR 0006: identified by URL, not by the local registry name).
    pub fn origin(&self) -> PackageOrigin {
        PackageOrigin::registry(&self.url).expect("registry URLs are validated when parsed")
    }
}

/// `[app]`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AppSettings {
    /// `startup`.
    pub startup: Startup,
    /// `locale`: `"system"` or a language tag.
    pub locale: String,
}

/// `[registry]`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RegistrySettings {
    /// `default`: name of a configured registry.
    pub default: String,
    /// `auto_sync`.
    pub auto_sync: bool,
    /// `cache_ttl`.
    pub cache_ttl: Duration,
}

/// `[theme]`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ThemeSettings {
    /// `active_profile`: a profile id.
    pub active_profile: String,
    /// `auto_update` (security: `false` is safe).
    pub auto_update: bool,
    /// `preview_before_apply` (security: `true` is safe).
    pub preview_before_apply: bool,
}

/// `[packages]`: package source and execution policy.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PackagePolicy {
    /// `allow_external_git`.
    pub allow_external_git: bool,
    /// `allow_local_archives`.
    pub allow_local_archives: bool,
    /// `verify_integrity`.
    pub verify_integrity: bool,
    /// `allow_native_scripts`.
    pub allow_native_scripts: bool,
}

/// `[security]`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SecurityPolicy {
    /// `confirm_new_permissions`.
    pub confirm_new_permissions: bool,
    /// `untrusted_network_default`.
    pub untrusted_network_default: NetworkDefault,
}

/// `[shell]`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ShellSettings {
    /// `use_default_fallback`.
    pub use_default_fallback: bool,
    /// `safe_mode_after_crashes`.
    pub safe_mode_after_crashes: u32,
}

/// The effective application configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AppConfig {
    /// `[app]`.
    pub app: AppSettings,
    /// `[registry]`.
    pub registry: RegistrySettings,
    /// `[registries.<name>]`, merged by name.
    pub registries: BTreeMap<String, Registry>,
    /// `[theme]`.
    pub theme: ThemeSettings,
    /// `[packages]`.
    pub packages: PackagePolicy,
    /// `[security]`.
    pub security: SecurityPolicy,
    /// `[shell]`.
    pub shell: ShellSettings,
}

/// The result of [`resolve_config`]: the effective configuration and every
/// diagnostic, parse warnings first, in layer order.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResolvedConfig {
    /// Effective configuration.
    pub config: AppConfig,
    /// Ignored content and visible security changes.
    pub diagnostics: Vec<ConfigDiagnostic>,
}

/// Merge the layers: `defaults` < `user`; `package_requests` are never
/// applied and only produce diagnostics. Layers are treated by position, not
/// by their [`ConfigSource`].
///
/// # Errors
///
/// [`ConfigErrorCode::IncompleteDefaults`] if `defaults` does not set every
/// key, [`ConfigErrorCode::MissingField`] if a registry ends up without
/// `type` or `url`, and [`ConfigErrorCode::UnknownRegistry`] if
/// `registry.default` names no configured registry.
pub fn resolve_config(
    defaults: &ConfigLayer,
    user: Option<&ConfigLayer>,
    package_requests: &[ConfigLayer],
) -> Result<ResolvedConfig> {
    let mut diagnostics: Vec<ConfigDiagnostic> = defaults.warnings.clone();

    if let Some(missing) = (0..SETTINGS.len()).find(|i| !defaults.values.contains_key(i)) {
        return Err(ConfigError::new(
            ConfigErrorCode::IncompleteDefaults,
            &defaults.source,
            SETTINGS[missing].pointer(),
            "the bundled defaults must set every key",
        ));
    }
    let mut values = defaults.values.clone();
    let mut set_by: BTreeMap<usize, ConfigSource> = BTreeMap::new();
    let mut registries = defaults.registries.clone();
    let mut registry_source: BTreeMap<String, ConfigSource> = defaults
        .registries
        .keys()
        .map(|name| (name.clone(), defaults.source.clone()))
        .collect();

    if let Some(user) = user {
        diagnostics.extend(user.warnings.iter().cloned());
        for (&index, value) in &user.values {
            let setting = &SETTINGS[index];
            if setting.weakens(&defaults.values[&index], value) {
                diagnostics.push(ConfigDiagnostic {
                    code: ConfigWarningCode::SecurityWeakened,
                    source: user.source.clone(),
                    pointer: setting.pointer(),
                    message: format!(
                        "{} weakens the bundled security default for `{}.{}`",
                        user.source, setting.table, setting.key
                    ),
                });
            }
            values.insert(index, value.clone());
            set_by.insert(index, user.source.clone());
        }
        for (name, patch) in &user.registries {
            let merged = registries.entry(name.clone()).or_default();
            if patch.kind.is_some() {
                merged.kind = patch.kind;
            }
            if patch.url.is_some() {
                merged.url.clone_from(&patch.url);
            }
            if patch.branch.is_some() {
                merged.branch.clone_from(&patch.branch);
            }
            registry_source
                .entry(name.clone())
                .or_insert_with(|| user.source.clone());
        }
    }

    for request in package_requests {
        diagnostics.extend(request.warnings.iter().cloned());
        for &index in request.values.keys() {
            let setting = &SETTINGS[index];
            let (code, what) = if setting.is_security() {
                (
                    ConfigWarningCode::PackageSecurityOverrideIgnored,
                    "security setting",
                )
            } else {
                (ConfigWarningCode::PackageSettingIgnored, "setting")
            };
            diagnostics.push(ConfigDiagnostic {
                code,
                source: request.source.clone(),
                pointer: setting.pointer(),
                message: format!(
                    "{} asked to change {what} `{}.{}`; packages cannot change application \
                     settings, so it was ignored",
                    request.source, setting.table, setting.key
                ),
            });
        }
        for name in request.registries.keys() {
            diagnostics.push(ConfigDiagnostic {
                code: ConfigWarningCode::PackageSecurityOverrideIgnored,
                source: request.source.clone(),
                pointer: child_pointer("/registries", name),
                message: format!(
                    "{} asked to add or change registry `{name}`; packages cannot add package \
                     sources, so it was ignored",
                    request.source
                ),
            });
        }
    }

    let mut resolved_registries = BTreeMap::new();
    for (name, patch) in registries {
        let source = &registry_source[&name];
        let pointer = child_pointer("/registries", &name);
        let missing = |field: &str| {
            ConfigError::new(
                ConfigErrorCode::MissingField,
                source,
                child_pointer(&pointer, field),
                format!("registry `{name}` needs `{field}`"),
            )
        };
        let kind = patch.kind.ok_or_else(|| missing("type"))?;
        let url = patch.url.clone().ok_or_else(|| missing("url"))?;
        resolved_registries.insert(
            name,
            Registry {
                kind,
                url,
                branch: patch.branch,
            },
        );
    }

    let get = |table: &str, key: &str| -> &SettingValue {
        let index = SETTINGS
            .iter()
            .position(|s| s.table == table && s.key == key)
            .expect("known setting");
        &values[&index]
    };
    let boolean = |table: &str, key: &str| matches!(get(table, key), SettingValue::Bool(true));
    let text = |table: &str, key: &str| match get(table, key) {
        SettingValue::Text(t) => t.clone(),
        other => unreachable!("{table}.{key} holds {other:?}"),
    };

    let default_registry = text("registry", "default");
    if !resolved_registries.contains_key(&default_registry) {
        let index = SETTINGS
            .iter()
            .position(|s| s.table == "registry" && s.key == "default")
            .expect("known setting");
        let source = set_by.get(&index).unwrap_or(&defaults.source);
        return Err(ConfigError::new(
            ConfigErrorCode::UnknownRegistry,
            source,
            "/registry/default",
            format!("registry.default names `{default_registry}`, which is not configured"),
        ));
    }

    let config = AppConfig {
        app: AppSettings {
            startup: match text("app", "startup").as_str() {
                "manual" => Startup::Manual,
                _ => Startup::UserSession,
            },
            locale: text("app", "locale"),
        },
        registry: RegistrySettings {
            default: default_registry,
            auto_sync: boolean("registry", "auto_sync"),
            cache_ttl: parse_duration(&text("registry", "cache_ttl"))
                .expect("validated when parsed"),
        },
        registries: resolved_registries,
        theme: ThemeSettings {
            active_profile: text("theme", "active_profile"),
            auto_update: boolean("theme", "auto_update"),
            preview_before_apply: boolean("theme", "preview_before_apply"),
        },
        packages: PackagePolicy {
            allow_external_git: boolean("packages", "allow_external_git"),
            allow_local_archives: boolean("packages", "allow_local_archives"),
            verify_integrity: boolean("packages", "verify_integrity"),
            allow_native_scripts: boolean("packages", "allow_native_scripts"),
        },
        security: SecurityPolicy {
            confirm_new_permissions: boolean("security", "confirm_new_permissions"),
            untrusted_network_default: match text("security", "untrusted_network_default").as_str()
            {
                "allow" => NetworkDefault::Allow,
                "ask" => NetworkDefault::Ask,
                _ => NetworkDefault::Deny,
            },
        },
        shell: ShellSettings {
            use_default_fallback: boolean("shell", "use_default_fallback"),
            safe_mode_after_crashes: match get("shell", "safe_mode_after_crashes") {
                SettingValue::Count(n) => *n,
                other => unreachable!("safe_mode_after_crashes holds {other:?}"),
            },
        },
    };
    Ok(ResolvedConfig {
        config,
        diagnostics,
    })
}

/// JSON Pointers of every security-relevant setting, for documentation and
/// tests: these are the keys whose weakening is reported and that packages
/// can never change.
pub fn security_setting_pointers() -> Vec<String> {
    SETTINGS
        .iter()
        .filter(|s| s.is_security())
        .map(Setting::pointer)
        .collect()
}

/// JSON Pointers of every setting the format knows (registries excluded).
pub fn setting_pointers() -> Vec<String> {
    SETTINGS.iter().map(Setting::pointer).collect()
}
