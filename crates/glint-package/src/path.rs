//! Host-independent entry-name and content-type rules.
//!
//! The rules are identical on every host. The `zip` crate's own path helpers
//! (`enclosed_name`, `mangled_name`) are never used: they rewrite hostile
//! names instead of rejecting them and behave differently on Windows.

use crate::error::PathRejection;
use crate::limits::PackageLimits;

/// The manifest file name at the package root.
pub const MANIFEST_NAME: &str = "glint.json";

/// Extensions of native executables, installers and scripts. Themes are data
/// plus sandboxed UI, so these are refused outright. `wasm` is included as a
/// conservative v1 choice: the renderer CSP (RND) does not grant
/// `wasm-unsafe-eval`, so a module could never run anyway.
pub const EXECUTABLE_EXTENSIONS: &[&str] = &[
    "exe", "dll", "sys", "com", "scr", "msi", "msix", "appx", "bat", "cmd", "ps1", "psm1", "vbs",
    "vbe", "wsf", "wsh", "hta", "lnk", "url", "reg", "cpl", "jar", "sh", "bash", "zsh", "fish",
    "so", "dylib", "app", "pkg", "deb", "rpm", "appimage", "command", "node", "wasm",
];

/// Validate a raw entry name. Returns the normalized path (no trailing `/`)
/// and whether it names a directory.
///
/// # Errors
///
/// Returns the first [`PathRejection`] rule the name breaks.
pub fn validate_entry_name(
    raw: &[u8],
    limits: &PackageLimits,
) -> Result<(String, bool), PathRejection> {
    use PathRejection as P;
    if raw.is_empty() {
        return Err(P::Empty);
    }
    if raw.len() > limits.max_path_len {
        return Err(P::TooLong);
    }
    let name = std::str::from_utf8(raw).map_err(|_| P::NotUtf8)?;
    if name.chars().any(char::is_control) {
        return Err(P::ControlChar);
    }
    if name.starts_with("\\\\") || name.starts_with("//") {
        return Err(P::UncOrDevicePath);
    }
    if name.starts_with('/') || name.starts_with('\\') {
        return Err(P::Absolute);
    }
    if name.contains(':') {
        return Err(P::DriveOrColon);
    }
    if name.contains('\\') {
        return Err(P::Backslash);
    }
    if limits.ascii_paths_only && !name.is_ascii() {
        return Err(P::NonAscii);
    }
    let (body, is_dir) = match name.strip_suffix('/') {
        Some(body) => (body, true),
        None => (name, false),
    };
    if body.is_empty() {
        return Err(P::Empty);
    }
    let mut depth = 0usize;
    for component in body.split('/') {
        depth += 1;
        if depth > limits.max_depth {
            return Err(P::TooDeep);
        }
        validate_component(component, limits)?;
    }
    Ok((body.to_owned(), is_dir))
}

/// Validate one path component (also used by the packer for directory
/// entries it walks).
pub(crate) fn validate_component(
    component: &str,
    limits: &PackageLimits,
) -> Result<(), PathRejection> {
    use PathRejection as P;
    match component {
        "" => return Err(P::EmptyComponent),
        "." => return Err(P::CurDir),
        ".." => return Err(P::ParentDir),
        _ => {}
    }
    if component.len() > limits.max_component_len {
        return Err(P::ComponentTooLong);
    }
    if component.contains(['<', '>', '"', '|', '?', '*', '~']) {
        return Err(P::ForbiddenChar);
    }
    if component.ends_with(['.', ' ']) || component.starts_with(' ') {
        return Err(P::TrailingDotOrSpace);
    }
    if is_reserved_device_name(component) {
        return Err(P::ReservedDeviceName);
    }
    Ok(())
}

/// Windows reserved device names, matched on the part before the first `.`,
/// case-insensitively, including the superscript-digit `COM¹` forms.
pub fn is_reserved_device_name(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$" | "CONFIG$"
    ) {
        return true;
    }
    let mut chars = stem.chars();
    let prefix: String = chars.by_ref().take(3).collect();
    if prefix == "COM" || prefix == "LPT" {
        let rest: Vec<char> = chars.collect();
        return matches!(rest.as_slice(), [c] if matches!(c, '0'..='9' | '¹' | '²' | '³'));
    }
    false
}

/// The key two paths must not share. ASCII folding under the default policy;
/// `to_lowercase` (no Unicode normalization) when non-ASCII is allowed.
pub(crate) fn fold_key(path: &str, limits: &PackageLimits) -> String {
    if limits.ascii_paths_only {
        path.to_ascii_lowercase()
    } else {
        path.to_lowercase()
    }
}

/// Lowercased extension of the last component, or `""`.
pub(crate) fn extension(path: &str) -> String {
    path.rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

/// True when the name has an executable or script extension.
pub fn is_executable_name(path: &str) -> bool {
    EXECUTABLE_EXTENSIONS.contains(&extension(path).as_str())
}

/// True for files kept in memory and scanned for remote references.
pub(crate) fn is_scanned_text(path: &str) -> bool {
    path == MANIFEST_NAME
        || matches!(
            extension(path).as_str(),
            "html" | "htm" | "xhtml" | "svg" | "js" | "mjs" | "cjs" | "css" | "json"
        )
}

/// True when content starts with native-executable, script or WebAssembly
/// magic bytes.
pub(crate) fn is_executable_magic(head: &[u8]) -> bool {
    head.starts_with(b"MZ")
        || head.starts_with(b"\x7fELF")
        || head.starts_with(b"#!")
        || head.starts_with(b"\0asm")
        || head.starts_with(&[0xCF, 0xFA, 0xED, 0xFE])
        || head.starts_with(&[0xCE, 0xFA, 0xED, 0xFE])
        || head.starts_with(&[0xFE, 0xED, 0xFA, 0xCF])
        || head.starts_with(&[0xFE, 0xED, 0xFA, 0xCE])
        || head.starts_with(&[0xCA, 0xFE, 0xBA, 0xBE])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ordinary_paths() {
        let limits = PackageLimits::default();
        assert_eq!(
            validate_entry_name(b"dist/panel/index.html", &limits),
            Ok(("dist/panel/index.html".to_owned(), false))
        );
        assert_eq!(
            validate_entry_name(b"theme/", &limits),
            Ok(("theme".to_owned(), true))
        );
    }

    #[test]
    fn device_names_match_with_any_extension() {
        for name in ["CON", "nul.txt", "Com1.json", "lpt9", "CONIN$", "COM\u{b9}"] {
            assert!(is_reserved_device_name(name), "{name}");
        }
        for name in ["console", "com10", "lpt", "nullable.txt"] {
            assert!(!is_reserved_device_name(name), "{name}");
        }
    }

    #[test]
    fn executable_magic_is_detected() {
        assert!(is_executable_magic(b"MZ\x90\0"));
        assert!(is_executable_magic(b"#!/bin/sh"));
        assert!(is_executable_magic(b"\0asm\x01\0\0\0"));
        assert!(!is_executable_magic(b"\x89PNG"));
    }
}
