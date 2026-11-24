//! Synthetic, repository-owned theme builders shared by the resolver tests.
//! Every theme here is made up for the tests (MIT, like the repository).

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use glint_profiles::ThemeInput;
use glint_schema::{
    PackageKey, PackageKind, PackageOrigin, PackageRelease, Profile, Sha256Digest, TokenFile,
    parse_profile, parse_theme_manifest, parse_tokens,
};
use serde_json::{Value, json};

pub fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

pub fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

pub fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The default theme's real `tokens.json` as JSON.
pub fn default_tokens_doc() -> Value {
    serde_json::from_str(&read(&repo_path("themes/default/theme/tokens.json"))).unwrap()
}

pub fn tokens(doc: &Value) -> TokenFile {
    parse_tokens(&doc.to_string()).unwrap()
}

/// The default tokens with some dark-scheme colours replaced.
pub fn tokens_with(colours: &[(&str, &str)]) -> TokenFile {
    let mut doc = default_tokens_doc();
    let map = doc["colorSchemes"]["dark"].as_object_mut().unwrap();
    for (name, value) in colours {
        map.insert((*name).to_owned(), json!(value));
    }
    tokens(&doc)
}

/// The default tokens with only the dark scheme.
pub fn dark_only_tokens() -> TokenFile {
    let mut doc = default_tokens_doc();
    doc["colorSchemes"].as_object_mut().unwrap().remove("light");
    tokens(&doc)
}

/// A deterministic digest derived from `id@version`.
pub fn digest_for(id: &str, version: &str) -> Sha256Digest {
    let mut bytes = [0u8; 32];
    for (i, b) in format!("{id}@{version}").bytes().enumerate() {
        bytes[i % 32] = bytes[i % 32].wrapping_mul(31).wrapping_add(b);
    }
    Sha256Digest::from_bytes(bytes)
}

pub fn origin_for(id: &str) -> PackageOrigin {
    if id.starts_with("official.") {
        PackageOrigin::local()
    } else {
        PackageOrigin::registry("https://registry.example/glint").unwrap()
    }
}

pub fn release(id: &str, version: &str) -> PackageRelease {
    PackageRelease::new(
        PackageKey::new(PackageKind::Theme, id).unwrap(),
        origin_for(id),
        version.parse().unwrap(),
        digest_for(id, version),
    )
}

/// A theme manifest with the given `surfaces` and `assets` objects.
pub fn theme(
    id: &str,
    version: &str,
    surfaces: Value,
    assets: Value,
    tokens: Option<TokenFile>,
) -> ThemeInput {
    let mut doc = json!({
        "schemaVersion": 1,
        "kind": "theme",
        "id": id,
        "name": "Synthetic test theme",
        "version": version,
        "author": { "name": "Glint tests" },
        "license": "MIT",
        "compatibility": { "glint": ">=0.0.0", "platforms": ["windows"] },
        "renderer": { "kind": "glint-web", "entrypointVersion": 1 },
        "assets": assets,
        "surfaces": surfaces,
    });
    if tokens.is_some() {
        doc["tokens"] = json!("theme/tokens.json");
    }
    let manifest = parse_theme_manifest(&doc.to_string())
        .unwrap_or_else(|e| panic!("synthetic manifest {id}: {e}"));
    ThemeInput::new(release(id, version), manifest, tokens).unwrap()
}

/// A stand-in for the bundled default theme: one panel, real tokens.
pub fn default_theme() -> ThemeInput {
    theme(
        "official.default",
        "0.1.0",
        json!({ "panel": { "entry": "surfaces/panel/index.html" } }),
        json!({ "preview": "assets/preview.svg" }),
        Some(tokens(&default_tokens_doc())),
    )
}

/// A base theme: panel and dock, a wallpaper and its own palette.
pub fn mono() -> ThemeInput {
    theme(
        "dev.example.mono",
        "1.0.0",
        json!({
            "panel": { "entry": "mono/panel.html", "placement": { "anchor": "top", "height": 36 } },
            "dock": { "entry": "mono/dock.html" },
        }),
        json!({ "wallpaper": "assets/mono.png", "alt-wallpaper": "assets/mono-alt.png" }),
        Some(tokens_with(&[("background.default", "#000000")])),
    )
}

/// An override theme that also declares a panel, tokens and a wallpaper,
/// none of which may leak into a profile that only borrows its dock.
pub fn glassdock() -> ThemeInput {
    theme(
        "dev.example.glassdock",
        "1.1.0",
        json!({
            "dock": { "entry": "glass/dock.html", "placement": { "kind": "edge", "anchor": "bottom" } },
            "panel": { "entry": "glass/panel.html" },
        }),
        json!({ "wallpaper": "assets/glass.png" }),
        Some(tokens_with(&[("background.default", "#101010")])),
    )
}

/// A theme with only a notification surface.
pub fn notifs() -> ThemeInput {
    theme(
        "dev.example.notifs",
        "0.4.0",
        json!({ "notification": { "entry": "notifs/index.html" } }),
        json!({}),
        None,
    )
}

pub fn profile(doc: Value) -> Profile {
    parse_profile(&doc.to_string()).unwrap_or_else(|e| panic!("profile: {e}"))
}
