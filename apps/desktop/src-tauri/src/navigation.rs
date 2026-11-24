//! The manager window's navigation policy.
//!
//! The manager is trusted UI with access to the core's commands, so it only
//! ever shows its own embedded assets. Every other navigation (a remote
//! site, `file://`, a theme's page, a data or blob URL) is cancelled before
//! it starts; theme content is never loaded here (threat model B7.4).

/// Whether the window may navigate to a URL with this `scheme` and `host`.
///
/// Tauri serves embedded assets from `tauri://localhost` on macOS and Linux
/// and from `http://tauri.localhost` (or `https://` with
/// `useHttpsScheme`) on Windows.
pub fn is_app_origin(scheme: &str, host: Option<&str>) -> bool {
    matches!(
        (scheme, host),
        ("tauri", Some("localhost")) | ("http" | "https", Some("tauri.localhost"))
    )
}

#[cfg(test)]
mod tests {
    use super::is_app_origin;

    #[test]
    fn only_the_embedded_origin_is_allowed() {
        assert!(is_app_origin("http", Some("tauri.localhost")));
        assert!(is_app_origin("https", Some("tauri.localhost")));
        assert!(is_app_origin("tauri", Some("localhost")));

        assert!(!is_app_origin("https", Some("example.com")));
        assert!(!is_app_origin("http", Some("localhost")));
        assert!(!is_app_origin("http", Some("tauri.localhost.example.com")));
        assert!(!is_app_origin("file", None));
        assert!(!is_app_origin("data", None));
        assert!(!is_app_origin("blob", None));
        assert!(!is_app_origin("javascript", None));
        assert!(!is_app_origin("tauri", Some("example.com")));
    }
}
