//! Surface entrypoints: package-local `.html` documents only.
//!
//! The rules are the manifest parser's package-path rules (kept in step by
//! `tests/surface_manifest.rs`, which runs both on the same inputs) plus the
//! `.html`/`.htm` suffix. A host loads the document from the verified,
//! immutable package, never from `file://`, a remote origin or a path that
//! escapes the package.

use crate::error::{SurfaceError, SurfaceErrorCode};

/// Longest accepted entrypoint path, in bytes.
pub const MAX_ENTRYPOINT_LEN: usize = 512;

/// Windows reserved device names, matched case-insensitively on the part
/// before the first dot.
const RESERVED_DEVICE_NAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Validates `path` as a package-local HTML entrypoint.
pub(crate) fn check(path: &str, pointer: &str) -> Result<(), SurfaceError> {
    match problem(path) {
        None => Ok(()),
        Some(problem) => Err(SurfaceError::new(
            SurfaceErrorCode::InvalidEntrypoint,
            pointer,
            format!("{path:?} is not a package-local .html entrypoint: {problem}"),
        )),
    }
}

fn problem(path: &str) -> Option<&'static str> {
    if path.is_empty() {
        return Some("path must not be empty");
    }
    if path.len() > MAX_ENTRYPOINT_LEN {
        return Some("path is too long");
    }
    let lower = path.to_ascii_lowercase();
    if lower.contains("://") || lower.starts_with("file:") || lower.starts_with("data:") {
        return Some(
            "URLs (remote pages, file://, data:) are never loaded; ship the document in the package",
        );
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
    if !(lower.ends_with(".html") || lower.ends_with(".htm")) {
        return Some("entrypoints must be .html or .htm documents");
    }
    None
}
