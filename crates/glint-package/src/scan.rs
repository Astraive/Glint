//! Heuristic static scan for remote references in UI entrypoints.
//!
//! This is defence in depth only. The real control is the renderer's Content
//! Security Policy (RND epic): a scanner cannot see `eval`, string-built URLs or
//! `fetch` calls. It exists so obviously networked packages fail at import with
//! a clear message instead of failing silently offline.
//!
//! Ported unchanged from the SPK-007 spike (`spikes/package-security`).

/// True for values that would make the renderer reach the network or a share.
pub fn is_remote(value: &str) -> bool {
    let value = value
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .trim()
        .to_ascii_lowercase();
    [
        "http:", "https:", "//", "\\\\", "ftp:", "ws:", "wss:", "file:",
    ]
    .iter()
    .any(|prefix| value.starts_with(prefix))
}

/// Return the first remote reference found in a scanned text file.
pub fn find_remote_reference(name: &str, bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let lower = text.to_ascii_lowercase();
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "html" | "htm" | "xhtml" | "svg" => {
            scan_markup(&lower).or_else(|| scan_css(&lower).or_else(|| scan_js(&lower)))
        }
        "css" => scan_css(&lower),
        "js" | "mjs" | "cjs" => scan_js(&lower),
        _ => None,
    }
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
        "xlink:href",
        "formaction",
    ];
    let mut rest = lower;
    while let Some(start) = rest.find('<') {
        rest = &rest[start + 1..];
        let end = rest.find('>').unwrap_or(rest.len());
        let tag = &rest[..end];
        let tag_name: String = tag
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == ':')
            .collect();
        if tag_name != "a" {
            for attr in ATTRS {
                if let Some(value) = attribute_value(tag, attr)
                    && value.split(',').any(is_remote)
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
        }
        rest = &rest[end..];
    }
    None
}

/// Find `attr=value` inside one tag (whitespace before the attribute name).
fn attribute_value(tag: &str, attr: &str) -> Option<String> {
    let mut search = tag;
    while let Some(pos) = search.find(attr) {
        let before = search[..pos].chars().last();
        let after = search[pos + attr.len()..].trim_start();
        search = &search[pos + attr.len()..];
        if !before.is_some_and(char::is_whitespace) {
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
    None
}

fn scan_js(lower: &str) -> Option<String> {
    for marker in [
        "import(",
        "from",
        "import",
        "importscripts(",
        "new worker(",
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
