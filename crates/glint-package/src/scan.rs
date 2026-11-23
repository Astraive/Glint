//! Heuristic static content restrictions for UI files and static assets.
//!
//! Two scans, both defence in depth only:
//!
//! - [`find_remote_reference`]: HTML, SVG, CSS and JavaScript must not load
//!   anything from the network or a share (`GLINT_PACKAGE_REMOTE_REFERENCE`).
//! - [`find_active_content`]: SVG files are images, i.e. inert data, and must
//!   not carry scripts, `foreignObject`, event handlers, `javascript:` URLs or
//!   XML entity declarations (`GLINT_PACKAGE_ACTIVE_CONTENT`).
//!
//! The real control is the renderer's Content Security Policy (RND epic): a
//! scanner cannot see `eval`, string-built URLs, character references or
//! `fetch(variable)`. The scans exist so obviously networked or scripted
//! packages fail at import with a clear message instead of failing silently
//! offline. Matching is ASCII case-insensitive.
//!
//! Started from the SPK-007 spike (`spikes/package-security`); PKG-004 added
//! `/` and quote attribute separators, more loading attributes, `<a ping>`,
//! CSS `image-set()`, more JavaScript network APIs and the SVG scan.

/// URL prefixes that reach the network or a share.
const REMOTE_PREFIXES: &[&str] = &[
    "http:", "https:", "//", "\\\\", "ftp:", "ftps:", "sftp:", "smb:", "ws:", "wss:", "file:",
];

/// True for values that would make the renderer reach the network or a share.
pub fn is_remote(value: &str) -> bool {
    let value = value
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .trim()
        .to_ascii_lowercase();
    REMOTE_PREFIXES
        .iter()
        .any(|prefix| value.starts_with(prefix))
}

/// Return the first remote reference found in a scanned text file.
pub fn find_remote_reference(name: &str, bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let lower = text.to_ascii_lowercase();
    match extension(name).as_str() {
        "html" | "htm" | "xhtml" | "svg" => {
            scan_markup(&lower).or_else(|| scan_css(&lower).or_else(|| scan_js(&lower)))
        }
        "css" => scan_css(&lower),
        "js" | "mjs" | "cjs" => scan_js(&lower),
        _ => None,
    }
}

/// Return the first active construct in a file that must be inert data.
/// Only SVG images are checked; HTML entrypoints are sandboxed UI governed by
/// the renderer CSP.
pub fn find_active_content(name: &str, bytes: &[u8]) -> Option<String> {
    if extension(name) != "svg" {
        return None;
    }
    let text = String::from_utf8_lossy(bytes);
    let lower = text.to_ascii_lowercase();
    // Entity declarations enable billion-laughs expansion and external
    // entities; an SVG image never needs them.
    if lower.contains("<!entity") {
        return Some("<!ENTITY".to_owned());
    }
    if lower.contains("javascript:") {
        return Some("javascript:".to_owned());
    }
    let mut rest = lower.as_str();
    while let Some(start) = rest.find('<') {
        rest = &rest[start + 1..];
        let end = rest.find('>').unwrap_or(rest.len());
        let tag = &rest[..end];
        let name = tag_name(tag);
        // `svg:script` and other prefixed names: compare the local name.
        let local = name.rsplit(':').next().unwrap_or("");
        if matches!(local, "script" | "foreignobject" | "handler" | "listener") {
            return Some(format!("<{name}"));
        }
        if let Some(handler) = event_handler(tag) {
            return Some(handler);
        }
        rest = &rest[end..];
    }
    None
}

fn extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

fn tag_name(tag: &str) -> String {
    tag.chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == ':')
        .collect()
}

/// True for the characters that may precede an attribute name: whitespace,
/// `/` (`<script/src=...>`) and a closing quote (`alt="x"src=...`), all of
/// which browsers accept as separators.
fn is_attribute_separator(c: char) -> bool {
    c.is_whitespace() || matches!(c, '/' | '"' | '\'')
}

/// An `on<event>=` attribute inside one tag.
fn event_handler(tag: &str) -> Option<String> {
    let mut search = tag;
    while let Some(pos) = search.find("on") {
        let before = search[..pos].chars().last();
        let after = &search[pos + 2..];
        search = after;
        if !before.is_some_and(is_attribute_separator) {
            continue;
        }
        let event: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if event.is_empty() {
            continue;
        }
        if after[event.len()..].trim_start().starts_with('=') {
            return Some(format!("on{event}"));
        }
    }
    None
}

/// Attributes on any tag except `<a>` that load content.
fn scan_markup(lower: &str) -> Option<String> {
    const ATTRS: &[&str] = &[
        "src",
        "href",
        "data",
        "action",
        "poster",
        "srcset",
        "imagesrcset",
        "xlink:href",
        "formaction",
        "background",
        "manifest",
        "codebase",
        "archive",
        "ping",
    ];
    let mut rest = lower;
    while let Some(start) = rest.find('<') {
        rest = &rest[start + 1..];
        let end = rest.find('>').unwrap_or(rest.len());
        let tag = &rest[..end];
        let tag_name = tag_name(tag);
        // An `<a href>` is navigation, not a dependency, but `<a ping>`
        // sends a request when the link is followed.
        let attrs: &[&str] = if tag_name == "a" { &["ping"] } else { ATTRS };
        for attr in attrs {
            if let Some(value) = attribute_value(tag, attr)
                && value.split([',', ' ']).any(is_remote)
            {
                return Some(value);
            }
        }
        // <meta http-equiv="refresh" content="0;url=https://...">
        if tag_name == "meta"
            && let Some(content) = attribute_value(tag, "content")
            && let Some((_, url)) = content.split_once("url=")
            && is_remote(url)
        {
            return Some(content);
        }
        rest = &rest[end..];
    }
    None
}

/// Find `attr=value` inside one tag (a separator before the attribute name).
fn attribute_value(tag: &str, attr: &str) -> Option<String> {
    let mut search = tag;
    while let Some(pos) = search.find(attr) {
        let before = search[..pos].chars().last();
        let after = search[pos + attr.len()..].trim_start();
        search = &search[pos + attr.len()..];
        if !before.is_some_and(is_attribute_separator) {
            continue;
        }
        let Some(after) = after.strip_prefix('=') else {
            continue;
        };
        let after = after.trim_start();
        let value = match after.chars().next() {
            Some(q @ ('"' | '\'')) => after[1..].split(q).next().unwrap_or(""),
            _ => after
                .split(|c: char| c.is_whitespace() || c == '>')
                .next()
                .unwrap_or(""),
        };
        return Some(value.to_owned());
    }
    None
}

fn scan_css(lower: &str) -> Option<String> {
    for marker in ["url(", "@import"] {
        let mut rest = lower;
        while let Some(pos) = rest.find(marker) {
            rest = &rest[pos + marker.len()..];
            let candidate = rest.trim_start().trim_start_matches("url(");
            let value: String = candidate
                .chars()
                .take_while(|c| *c != ')' && *c != ';' && *c != '\n')
                .collect();
            if is_remote(&value) {
                return Some(value.trim().to_owned());
            }
        }
    }
    // image-set("https://..." 1x): bare strings load images too.
    let mut rest = lower;
    while let Some(pos) = rest.find("image-set(") {
        rest = &rest[pos + "image-set(".len()..];
        let body = rest.split([')', ';']).next().unwrap_or("");
        for (index, part) in body.split(['"', '\'']).enumerate() {
            if index % 2 == 1 && is_remote(part) {
                return Some(part.to_owned());
            }
        }
    }
    None
}

fn scan_js(lower: &str) -> Option<String> {
    for marker in [
        "import(",
        "from",
        "import",
        "importscripts(",
        "new worker(",
        "new sharedworker(",
        "new websocket(",
        "new eventsource(",
        "sendbeacon(",
        "fetch(",
    ] {
        let mut rest = lower;
        while let Some(pos) = rest.find(marker) {
            rest = &rest[pos + marker.len()..];
            let candidate = rest.trim_start();
            let Some(quote @ ('"' | '\'' | '`')) = candidate.chars().next() else {
                continue;
            };
            let value = candidate[1..].split(quote).next().unwrap_or("");
            if is_remote(value) {
                return Some(value.to_owned());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote(name: &str, text: &str) -> Option<String> {
        find_remote_reference(name, text.as_bytes())
    }

    fn active(text: &str) -> Option<String> {
        find_active_content("assets/x.svg", text.as_bytes())
    }

    #[test]
    fn slash_and_quote_separate_attributes() {
        assert!(remote("i.html", "<script/src=\"https://x.example/a.js\">").is_some());
        assert!(remote("i.html", "<img alt=\"x\"src=\"https://x.example/a.png\">").is_some());
        assert!(remote("i.html", "<img datasrc=\"https://x.example\">").is_none());
    }

    #[test]
    fn anchor_href_is_navigation_but_ping_is_a_request() {
        assert!(remote("i.html", "<a href=\"https://example.org\">x</a>").is_none());
        assert!(remote("i.html", "<a href=\"#\" ping=\"https://t.example\">x</a>").is_some());
    }

    #[test]
    fn more_loading_attributes_and_apis_are_seen() {
        assert!(remote("i.html", "<body background=\"http://x.example/b.png\">").is_some());
        assert!(
            remote(
                "i.html",
                "<link imagesrcset=\"a.png 1x, https://x/b.png 2x\">"
            )
            .is_some()
        );
        assert!(
            remote(
                "s.css",
                "a { background: image-set(\"https://x/a.png\" 1x); }"
            )
            .is_some()
        );
        assert!(remote("s.css", "a { background: image-set(\"a.png\" 1x); }").is_none());
        assert!(remote("m.js", "new WebSocket('wss://x.example/s')").is_some());
        assert!(remote("m.js", "navigator.sendBeacon(\"https://t.example\", d)").is_some());
        assert!(remote("m.js", "new EventSource(`https://x.example/e`)").is_some());
    }

    #[test]
    fn local_and_data_references_are_allowed() {
        assert!(
            remote(
                "i.html",
                "<script type=\"module\" src=\"./main.js\"></script>"
            )
            .is_none()
        );
        assert!(
            remote(
                "s.css",
                "a { background: url(data:image/png;base64,AAAA); }"
            )
            .is_none()
        );
        assert!(remote("m.js", "const a = Array.from('abc');").is_none());
    }

    #[test]
    fn svg_active_content_is_found() {
        assert_eq!(
            active("<svg><script>alert(1)</script></svg>").as_deref(),
            Some("<script")
        );
        assert_eq!(
            active("<svg><SVG:Script/></svg>").as_deref(),
            Some("<svg:script")
        );
        assert_eq!(
            active("<svg><foreignObject><div/></foreignObject></svg>").as_deref(),
            Some("<foreignobject")
        );
        assert_eq!(active("<svg onload=\"x()\"/>").as_deref(), Some("onload"));
        assert_eq!(
            active("<svg><rect/onclick='x()'/></svg>").as_deref(),
            Some("onclick")
        );
        assert_eq!(
            active("<svg><a href=\"javascript:x()\"/></svg>").as_deref(),
            Some("javascript:")
        );
        assert_eq!(
            active("<!DOCTYPE svg [<!ENTITY a \"aaaa\">]><svg>&a;</svg>").as_deref(),
            Some("<!ENTITY")
        );
    }

    #[test]
    fn inert_svg_passes() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" role="img" aria-labelledby="t">
            <title id="t">Preview</title><rect width="10" height="10" fill="#000" font="mono"/>
            <text x="1" y="2" content="one two">Done</text></svg>"##;
        assert_eq!(active(svg), None);
        assert_eq!(find_active_content("i.html", b"<script></script>"), None);
    }
}
