//! Proposed application config (`config.toml` v1) and merge policy (SCH-007).
//!
//! Run with `cargo test -p glint-schema --test config_merge`.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use glint_schema::{
    BUNDLED_DEFAULT_CONFIG, ConfigError, ConfigErrorCode, ConfigLayer, ConfigSource,
    ConfigWarningCode, ErrorCode, NetworkDefault, PackageOrigin, RegistryKind, ResolvedConfig,
    Startup, bundled_defaults, parse_config_layer, parse_theme_manifest, resolve_config,
    security_setting_pointers, setting_pointers,
};
use serde_json::{Value, json};

fn user(text: &str) -> ConfigLayer {
    parse_config_layer(text, ConfigSource::User).unwrap_or_else(|e| panic!("user layer: {e}"))
}

fn package(id: &str, text: &str) -> ConfigLayer {
    parse_config_layer(text, ConfigSource::Package(id.to_owned()))
        .unwrap_or_else(|e| panic!("package layer: {e}"))
}

fn user_err(text: &str) -> ConfigError {
    parse_config_layer(text, ConfigSource::User).expect_err("layer must be rejected")
}

fn resolve(user_layer: Option<&ConfigLayer>, packages: &[ConfigLayer]) -> ResolvedConfig {
    resolve_config(&bundled_defaults(), user_layer, packages).expect("resolves")
}

fn codes(resolved: &ResolvedConfig) -> Vec<(ConfigWarningCode, &str)> {
    resolved
        .diagnostics
        .iter()
        .map(|d| (d.code, d.pointer.as_str()))
        .collect()
}

fn assert_err(err: &ConfigError, code: ConfigErrorCode, pointer: &str) {
    assert_eq!(
        (err.code(), err.pointer()),
        (code, pointer),
        "unexpected error: {err}"
    );
}

// --------------------------------------------------------- defaults

#[test]
fn bundled_defaults_are_complete_safe_and_quiet() {
    let defaults = bundled_defaults();
    assert_eq!(defaults.source(), &ConfigSource::BundledDefaults);
    assert!(defaults.warnings().is_empty());
    let resolved = resolve(None, &[]);
    assert!(resolved.diagnostics.is_empty());
    let config = resolved.config;

    assert_eq!(config.app.startup, Startup::UserSession);
    assert_eq!(config.app.locale, "system");
    assert_eq!(config.registry.default, "official");
    assert!(config.registry.auto_sync);
    assert_eq!(config.registry.cache_ttl, Duration::from_secs(24 * 3600));
    assert_eq!(config.theme.active_profile, "default");
    assert!(!config.theme.auto_update);
    assert!(config.theme.preview_before_apply);
    assert!(config.packages.allow_external_git);
    assert!(config.packages.allow_local_archives);
    assert!(config.packages.verify_integrity);
    assert!(config.shell.use_default_fallback);
    assert_eq!(config.shell.safe_mode_after_crashes, 3);

    // The ticket's security defaults.
    assert!(!config.packages.allow_native_scripts);
    assert!(config.security.confirm_new_permissions);
    assert_eq!(
        config.security.untrusted_network_default,
        NetworkDefault::Deny
    );

    let official = &config.registries["official"];
    assert_eq!(official.kind, RegistryKind::Git);
    assert_eq!(official.branch.as_deref(), Some("main"));
    // The placeholder can never resolve (RFC 2606 reserved TLD).
    assert!(official.url.contains(".invalid/"));
    assert_eq!(
        official.origin(),
        PackageOrigin::registry(&official.url).unwrap()
    );
}

#[test]
fn bundled_defaults_are_the_shipped_file() {
    let on_disk = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/default.toml"),
    )
    .expect("config/default.toml exists");
    assert_eq!(on_disk, BUNDLED_DEFAULT_CONFIG);
}

#[test]
fn defaults_must_set_every_key() {
    let partial =
        parse_config_layer("[app]\nlocale = \"en\"\n", ConfigSource::BundledDefaults).unwrap();
    let err = resolve_config(&partial, None, &[]).unwrap_err();
    assert_eq!(err.code(), ConfigErrorCode::IncompleteDefaults);
    assert_eq!(err.pointer(), "/app/startup");
}

// ------------------------------------------------------- user merge

#[test]
fn plan_user_override_merges_by_key() {
    let layer = user(
        r#"
[theme]
active_profile = "work-setup"

[registries.friends]
type = "git"
url = "https://git.example/example/glint-collection.git"
branch = "main"

[packages]
allow_external_git = true
"#,
    );
    let resolved = resolve(Some(&layer), &[]);
    // allow_external_git = true equals the default: nothing is weakened.
    assert!(
        resolved.diagnostics.is_empty(),
        "{:?}",
        resolved.diagnostics
    );
    let config = resolved.config;
    assert_eq!(config.theme.active_profile, "work-setup");
    // Other keys of the same table keep their defaults.
    assert!(!config.theme.auto_update);
    assert!(config.theme.preview_before_apply);
    // Named registries merge by key: the default one survives.
    assert_eq!(
        config.registries.keys().collect::<Vec<_>>(),
        ["friends", "official"]
    );
    assert_eq!(
        config.registries["friends"].url,
        "https://git.example/example/glint-collection.git"
    );
    assert_eq!(config.registry.default, "official");
}

#[test]
fn registry_entries_merge_field_by_field() {
    let layer = user("[registries.official]\nbranch = \"stable\"\n");
    let config = resolve(Some(&layer), &[]).config;
    let official = &config.registries["official"];
    assert_eq!(official.branch.as_deref(), Some("stable"));
    assert_eq!(
        official.url,
        "https://glint-library.invalid/glint-library.git"
    );
}

#[test]
fn new_registry_must_be_complete() {
    let layer = user("[registries.friends]\ntype = \"git\"\n");
    let err = resolve_config(&bundled_defaults(), Some(&layer), &[]).unwrap_err();
    assert_err(
        &err,
        ConfigErrorCode::MissingField,
        "/registries/friends/url",
    );
    assert_eq!(err.source(), &ConfigSource::User);

    let layer = user("[registries.friends]\nurl = \"https://git.example/r\"\n");
    let err = resolve_config(&bundled_defaults(), Some(&layer), &[]).unwrap_err();
    assert_err(
        &err,
        ConfigErrorCode::MissingField,
        "/registries/friends/type",
    );

    // Branch is optional: the repository's default branch is used.
    let layer = user("[registries.friends]\ntype = \"git\"\nurl = \"https://git.example/r\"\n");
    let config = resolve(Some(&layer), &[]).config;
    assert_eq!(config.registries["friends"].branch, None);
}

#[test]
fn default_registry_must_exist() {
    let layer = user("[registry]\ndefault = \"friends\"\n");
    let err = resolve_config(&bundled_defaults(), Some(&layer), &[]).unwrap_err();
    assert_err(&err, ConfigErrorCode::UnknownRegistry, "/registry/default");
    assert_eq!(err.source(), &ConfigSource::User);

    let layer = user(
        "[registry]\ndefault = \"friends\"\n[registries.friends]\ntype = \"git\"\nurl = \"https://git.example/r\"\n",
    );
    assert_eq!(
        resolve(Some(&layer), &[]).config.registry.default,
        "friends"
    );
}

#[test]
fn user_may_weaken_security_but_it_is_reported() {
    let layer = user(
        r#"
[packages]
allow_native_scripts = true
verify_integrity = false

[security]
confirm_new_permissions = false
untrusted_network_default = "ask"

[theme]
auto_update = true

[shell]
safe_mode_after_crashes = 10
"#,
    );
    let resolved = resolve(Some(&layer), &[]);
    let config = &resolved.config;
    assert!(config.packages.allow_native_scripts);
    assert!(!config.packages.verify_integrity);
    assert!(!config.security.confirm_new_permissions);
    assert_eq!(
        config.security.untrusted_network_default,
        NetworkDefault::Ask
    );
    assert!(config.theme.auto_update);
    assert_eq!(config.shell.safe_mode_after_crashes, 10);
    let weakened: BTreeSet<&str> = resolved
        .diagnostics
        .iter()
        .filter(|d| d.code == ConfigWarningCode::SecurityWeakened)
        .map(|d| d.pointer.as_str())
        .collect();
    assert_eq!(
        weakened,
        BTreeSet::from([
            "/packages/allow_native_scripts",
            "/packages/verify_integrity",
            "/security/confirm_new_permissions",
            "/security/untrusted_network_default",
            "/theme/auto_update",
            "/shell/safe_mode_after_crashes",
        ])
    );
    assert!(
        resolved
            .diagnostics
            .iter()
            .all(|d| d.source == ConfigSource::User)
    );
}

#[test]
fn tightening_or_keeping_security_is_not_reported() {
    let layer = user(
        r#"
[packages]
allow_external_git = false
allow_local_archives = false
allow_native_scripts = false

[security]
untrusted_network_default = "deny"

[shell]
safe_mode_after_crashes = 1
"#,
    );
    let resolved = resolve(Some(&layer), &[]);
    assert!(
        resolved.diagnostics.is_empty(),
        "{:?}",
        resolved.diagnostics
    );
    assert!(!resolved.config.packages.allow_external_git);
    assert_eq!(resolved.config.shell.safe_mode_after_crashes, 1);
}

// --------------------------------------------- package requests

const EVIL: &str = r#"
[packages]
allow_native_scripts = true
verify_integrity = false

[security]
confirm_new_permissions = false
untrusted_network_default = "allow"

[theme]
active_profile = "takeover"

[registries.evil]
type = "git"
url = "https://evil.example/index.git"
"#;

#[test]
fn package_cannot_weaken_security_and_is_reported() {
    let baseline = resolve(None, &[]).config;
    let resolved = resolve(None, &[package("dev.example.evil", EVIL)]);
    // Nothing a package asked for is applied.
    assert_eq!(resolved.config, baseline);
    assert!(!resolved.config.packages.allow_native_scripts);
    assert!(resolved.config.security.confirm_new_permissions);
    assert_eq!(
        resolved.config.security.untrusted_network_default,
        NetworkDefault::Deny
    );
    assert!(!resolved.config.registries.contains_key("evil"));

    assert_eq!(
        codes(&resolved),
        [
            (
                ConfigWarningCode::PackageSettingIgnored,
                "/theme/active_profile"
            ),
            (
                ConfigWarningCode::PackageSecurityOverrideIgnored,
                "/packages/verify_integrity"
            ),
            (
                ConfigWarningCode::PackageSecurityOverrideIgnored,
                "/packages/allow_native_scripts"
            ),
            (
                ConfigWarningCode::PackageSecurityOverrideIgnored,
                "/security/confirm_new_permissions"
            ),
            (
                ConfigWarningCode::PackageSecurityOverrideIgnored,
                "/security/untrusted_network_default"
            ),
            (
                ConfigWarningCode::PackageSecurityOverrideIgnored,
                "/registries/evil"
            ),
        ]
    );
    assert!(
        resolved
            .diagnostics
            .iter()
            .all(|d| d.source == ConfigSource::Package("dev.example.evil".to_owned()))
    );
    assert_eq!(
        ConfigWarningCode::PackageSecurityOverrideIgnored.as_str(),
        "GLINT_CONFIG_WARN_PACKAGE_SECURITY_OVERRIDE_IGNORED"
    );
}

#[test]
fn package_cannot_override_the_user_either() {
    let layer = user("[packages]\nallow_native_scripts = false\nallow_external_git = false\n");
    let request = package(
        "dev.example.evil",
        "[packages]\nallow_external_git = true\n",
    );
    let resolved = resolve(Some(&layer), &[request]);
    assert!(!resolved.config.packages.allow_external_git);
    assert_eq!(
        codes(&resolved),
        [(
            ConfigWarningCode::PackageSecurityOverrideIgnored,
            "/packages/allow_external_git"
        )]
    );
}

#[test]
fn layers_are_treated_by_position() {
    // A layer labelled User but passed as a package request is still ignored.
    let resolved = resolve(None, &[user("[packages]\nallow_native_scripts = true\n")]);
    assert!(!resolved.config.packages.allow_native_scripts);
    assert_eq!(resolved.diagnostics.len(), 1);
}

#[test]
fn theme_manifest_cannot_carry_settings() {
    let manifest = json!({
        "schemaVersion": 1,
        "kind": "theme",
        "id": "dev.example.evil",
        "name": "Evil",
        "version": "1.0.0",
        "author": { "name": "Example" },
        "license": "MIT",
        "compatibility": { "glint": ">=0.1.0", "platforms": ["windows"] },
    });
    for (field, value) in [
        ("packages", json!({ "allow_native_scripts": true })),
        ("security", json!({ "untrusted_network_default": "allow" })),
        (
            "config",
            json!({ "packages": { "allow_native_scripts": true } }),
        ),
    ] {
        let mut doc = manifest.clone();
        doc[field] = value;
        let err = parse_theme_manifest(&doc.to_string()).expect_err(field);
        assert_eq!(err.code(), ErrorCode::UnknownField, "{field}");
        assert_eq!(err.pointer(), format!("/{field}"));
    }
}

// ------------------------------------------------- strictness

#[test]
fn unknown_keys_in_security_sections_are_rejected() {
    assert_err(
        &user_err("[packages]\nalow_native_scripts = true\n"),
        ConfigErrorCode::UnknownField,
        "/packages/alow_native_scripts",
    );
    assert_err(
        &user_err("[security]\nallow_everything = true\n"),
        ConfigErrorCode::UnknownField,
        "/security/allow_everything",
    );
    assert_err(
        &user_err(
            "[registries.friends]\ntype = \"git\"\nurl = \"https://g.example/r\"\ntoken = \"x\"\n",
        ),
        ConfigErrorCode::UnknownField,
        "/registries/friends/token",
    );
    assert_err(
        &user_err("[policy]\nlocked = true\n"),
        ConfigErrorCode::UnknownField,
        "/policy",
    );
    assert_err(
        &user_err("debug = true\n"),
        ConfigErrorCode::UnknownField,
        "/debug",
    );
}

#[test]
fn unknown_keys_elsewhere_are_ignored_with_a_warning() {
    let layer = user("[theme]\naccent = \"red\"\n[app]\ntelemetry = false\n");
    let warnings: Vec<(ConfigWarningCode, &str)> = layer
        .warnings()
        .iter()
        .map(|w| (w.code, w.pointer.as_str()))
        .collect();
    assert_eq!(
        warnings,
        [
            (ConfigWarningCode::UnknownField, "/app/telemetry"),
            (ConfigWarningCode::UnknownField, "/theme/accent"),
        ]
    );
    assert!(layer.keys().is_empty());
    let resolved = resolve(Some(&layer), &[]);
    assert_eq!(resolved.diagnostics.len(), 2);
}

#[test]
fn types_are_checked() {
    for (text, pointer) in [
        (
            "[packages]\nallow_native_scripts = \"false\"\n",
            "/packages/allow_native_scripts",
        ),
        (
            "[packages]\nallow_native_scripts = 0\n",
            "/packages/allow_native_scripts",
        ),
        (
            "[security]\nuntrusted_network_default = false\n",
            "/security/untrusted_network_default",
        ),
        (
            "[shell]\nsafe_mode_after_crashes = \"3\"\n",
            "/shell/safe_mode_after_crashes",
        ),
        (
            "[registry]\ncache_ttl = 1979-05-27T07:32:00Z\n",
            "/registry/cache_ttl",
        ),
        ("[registry]\ncache_ttl = 86400\n", "/registry/cache_ttl"),
        ("packages = [1, 2]\n", "/packages"),
        (
            "[registries]\nofficial = \"https://x.example\"\n",
            "/registries/official",
        ),
        ("registries = 1\n", "/registries"),
        ("[registries.friends]\nurl = 1\n", "/registries/friends/url"),
    ] {
        assert_err(&user_err(text), ConfigErrorCode::InvalidType, pointer);
    }
}

#[test]
fn values_are_checked() {
    for (text, pointer) in [
        ("[app]\nstartup = \"boot\"\n", "/app/startup"),
        ("[app]\nlocale = \"english\"\n", "/app/locale"),
        ("[app]\nlocale = \"en_GB\"\n", "/app/locale"),
        ("[registry]\ncache_ttl = \"1y\"\n", "/registry/cache_ttl"),
        ("[registry]\ncache_ttl = \"366d\"\n", "/registry/cache_ttl"),
        ("[registry]\ncache_ttl = \"h\"\n", "/registry/cache_ttl"),
        ("[registry]\ncache_ttl = \"-1h\"\n", "/registry/cache_ttl"),
        ("[registry]\ndefault = \"Official\"\n", "/registry/default"),
        (
            "[theme]\nactive_profile = \"Work Setup\"\n",
            "/theme/active_profile",
        ),
        (
            "[security]\nuntrusted_network_default = \"prompt\"\n",
            "/security/untrusted_network_default",
        ),
        (
            "[shell]\nsafe_mode_after_crashes = 0\n",
            "/shell/safe_mode_after_crashes",
        ),
        (
            "[shell]\nsafe_mode_after_crashes = 101\n",
            "/shell/safe_mode_after_crashes",
        ),
        (
            "[shell]\nsafe_mode_after_crashes = -1\n",
            "/shell/safe_mode_after_crashes",
        ),
        (
            "[shell]\nsafe_mode_after_crashes = 2.5\n",
            "/shell/safe_mode_after_crashes",
        ),
        (
            "[registries.Friends]\ntype = \"git\"\n",
            "/registries/Friends",
        ),
        (
            "[registries.friends]\ntype = \"http\"\n",
            "/registries/friends/type",
        ),
        (
            "[registries.friends]\nurl = \"http://g.example/r\"\n",
            "/registries/friends/url",
        ),
        (
            "[registries.friends]\nurl = \"https://user:token@g.example/r\"\n",
            "/registries/friends/url",
        ),
        (
            "[registries.friends]\nurl = \"file:///C:/themes\"\n",
            "/registries/friends/url",
        ),
        (
            "[registries.friends]\nbranch = \"../main\"\n",
            "/registries/friends/branch",
        ),
        (
            "[registries.friends]\nbranch = \"-main\"\n",
            "/registries/friends/branch",
        ),
        (
            "[registries.friends]\nbranch = \"main.lock\"\n",
            "/registries/friends/branch",
        ),
        (
            "[registries.friends]\nbranch = \"a b\"\n",
            "/registries/friends/branch",
        ),
        (
            "[registries.friends]\nbranch = \"\"\n",
            "/registries/friends/branch",
        ),
    ] {
        assert_err(&user_err(text), ConfigErrorCode::InvalidValue, pointer);
    }
    for good in [
        "[app]\nlocale = \"en\"\n",
        "[app]\nlocale = \"en-GB\"\n",
        "[app]\nlocale = \"zh-Hant-TW\"\n",
        "[app]\nstartup = \"manual\"\n",
        "[registry]\ncache_ttl = \"0s\"\n",
        "[registry]\ncache_ttl = \"365d\"\n",
        "[registry]\ncache_ttl = \"90m\"\n",
        "[registries.friends]\nbranch = \"release/1.x\"\n",
    ] {
        user(good);
    }
}

#[test]
fn schema_version_is_optional_but_checked() {
    user("schema_version = 1\n");
    assert_err(
        &user_err("schema_version = 2\n"),
        ConfigErrorCode::UnsupportedSchemaVersion,
        "/schema_version",
    );
    assert_err(
        &user_err("schema_version = \"1\"\n"),
        ConfigErrorCode::InvalidType,
        "/schema_version",
    );
}

#[test]
fn document_level_rules() {
    for text in [
        "[app\n",
        "[app]\nlocale = \"en\"\nlocale = \"de\"\n",
        "[app]\n[app]\n",
        "x =",
    ] {
        let err = user_err(text);
        assert_eq!(err.code(), ConfigErrorCode::InvalidToml, "{text:?}");
        assert_eq!(err.pointer(), "");
    }
    let big = format!("# {}\n", "x".repeat(glint_schema::MAX_CONFIG_BYTES));
    assert_eq!(user_err(&big).code(), ConfigErrorCode::TooLarge);
    user("\u{feff}[app]\nlocale = \"en\"\n");
    user("");
}

#[test]
fn layer_keys_list_what_a_layer_sets() {
    let layer = user(
        "[theme]\nactive_profile = \"work\"\n[registries.friends]\ntype = \"git\"\nurl = \"https://g.example/r\"\n",
    );
    assert_eq!(
        layer.keys(),
        [
            "/theme/active_profile",
            "/registries/friends/type",
            "/registries/friends/url"
        ]
    );
}

#[test]
fn config_is_distinct_from_other_documents() {
    // A theme manifest, profile or registry index is not a config file.
    assert_err(
        &user_err("schemaVersion = 1\nid = \"dev.example.a\"\n"),
        ConfigErrorCode::UnknownField,
        "/id",
    );
    assert_err(
        &user_err("baseTheme = { id = \"official.mono\", version = \"1.0.0\" }\n"),
        ConfigErrorCode::UnknownField,
        "/baseTheme",
    );
}

#[test]
fn codes_are_stable_and_distinct() {
    let errors: BTreeSet<&str> = ConfigErrorCode::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(errors.len(), ConfigErrorCode::ALL.len());
    assert!(errors.iter().all(|c| c.starts_with("GLINT_CONFIG_")));
    let warnings: BTreeSet<&str> = ConfigWarningCode::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(warnings.len(), ConfigWarningCode::ALL.len());
    assert!(warnings.iter().all(|c| c.starts_with("GLINT_CONFIG_WARN_")));
}

// ------------------------------------------------------------ schema

#[test]
fn config_schema_matches_the_parser() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/glint-config-v1.schema.json");
    let schema: Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("schema")).expect("JSON");
    assert_eq!(schema["x-glint-status"], json!("proposed"));
    assert_eq!(schema["additionalProperties"], json!(false));

    let mut pointers = BTreeSet::new();
    let tables = schema["properties"].as_object().unwrap();
    for (table, spec) in tables {
        if table == "schema_version" || table == "registries" {
            continue;
        }
        for key in spec["properties"].as_object().unwrap().keys() {
            pointers.insert(format!("/{table}/{key}"));
        }
        let strict = table == "packages" || table == "security";
        assert_eq!(
            spec["additionalProperties"],
            json!(!strict),
            "[{table}] additionalProperties"
        );
    }
    assert_eq!(
        pointers,
        setting_pointers().into_iter().collect::<BTreeSet<_>>()
    );
    let registry = &tables["registries"]["additionalProperties"];
    assert_eq!(registry["additionalProperties"], json!(false));
    assert_eq!(
        registry["properties"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ["branch", "type", "url"]
    );
    let security: BTreeSet<String> = schema["x-glint-security-settings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        security,
        security_setting_pointers()
            .into_iter()
            .collect::<BTreeSet<_>>()
    );
}
