//! Platform-independent spike logic: arguments, the bundled-origin policy,
//! the embedded asset table, host-message parsing, the expected
//! accessibility names and a tiny JSON string escaper.
//!
//! Free of Windows types so it compiles and is unit-tested on every CI host.

use std::path::PathBuf;

/// The only origin the webview may load; every request is answered from memory.
pub const ORIGIN: &str = "https://glint.spike/";

/// Response-header CSP for every bundled asset (index.html repeats it in a
/// `<meta>` tag, minus `frame-ancestors`, which a meta tag cannot carry).
pub const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; \
connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; \
frame-ancestors 'none'";

/// Largest `report:` payload the host accepts from the page.
pub const MAX_REPORT_BYTES: usize = 512 * 1024;

/// Window client size in DIP (scaled by the monitor DPI).
pub const WINDOW_SIZE_DIP: (i32, i32) = (520, 760);

pub const USAGE: &str = "usage: spike-webview2-baseline [--smoke SECONDS] [--out DIR] [--activate]\n\
  --smoke SECONDS  run the probes, print the JSON report to stdout and exit;\n\
                   fail with a timeout if no report arrives within SECONDS (5-600)\n\
  --out DIR        also write report.json, webview.png (CapturePreview),\n\
                   behind.bmp (screen before the window is shown) and\n\
                   screen.bmp (screen pixels under the window) into DIR\n\
  --activate       show the window activated instead of inactive\n\
  --help           show this text";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Options {
    pub smoke_seconds: Option<u32>,
    pub out_dir: Option<PathBuf>,
    /// Show the window activated (foreground). DWM system backdrops such as
    /// acrylic render only on the active window; inactive ones get a solid
    /// fallback colour, which is the normal state of a desktop panel.
    pub activate: bool,
    pub help: bool,
}

pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => options.help = true,
            "--activate" => options.activate = true,
            "--smoke" => {
                let value = iter.next().ok_or("--smoke needs a number of seconds")?;
                let seconds: u32 = value
                    .parse()
                    .map_err(|_| format!("--smoke: not a number: {value}"))?;
                if !(5..=600).contains(&seconds) {
                    return Err(format!("--smoke: {seconds} is outside 5-600"));
                }
                options.smoke_seconds = Some(seconds);
            }
            "--out" => {
                let value = iter.next().ok_or("--out needs a directory")?;
                options.out_dir = Some(PathBuf::from(value));
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(options)
}

/// One embedded file served under [`ORIGIN`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Asset {
    pub path: &'static str,
    pub mime: &'static str,
    pub body: &'static [u8],
}

const ASSETS: &[Asset] = &[
    Asset {
        path: "index.html",
        mime: "text/html; charset=utf-8",
        body: include_bytes!("../assets/index.html"),
    },
    Asset {
        path: "baseline.css",
        mime: "text/css; charset=utf-8",
        body: include_bytes!("../assets/baseline.css"),
    },
    // Built from web/ by esbuild and committed; see web/build.mjs.
    Asset {
        path: "app.js",
        mime: "text/javascript; charset=utf-8",
        body: include_bytes!("../assets/app.js"),
    },
    Asset {
        path: "img/probe.png",
        mime: "image/png",
        body: include_bytes!("../assets/img/probe.png"),
    },
    Asset {
        path: "img/probe.svg",
        mime: "image/svg+xml",
        body: include_bytes!("../assets/img/probe.svg"),
    },
    Asset {
        path: "img/probe.webp",
        mime: "image/webp",
        body: include_bytes!("../assets/img/probe.webp"),
    },
    Asset {
        path: "img/probe.avif",
        mime: "image/avif",
        body: include_bytes!("../assets/img/probe.avif"),
    },
];

/// True only for URLs on the bundled origin (the trailing slash rejects
/// look-alikes such as `https://glint.spike.example.com/`).
pub fn is_bundled_url(url: &str) -> bool {
    url.starts_with(ORIGIN)
}

/// Resolves a request URL to an embedded asset, ignoring query and fragment.
pub fn asset_for_url(url: &str) -> Option<Asset> {
    let path = url.strip_prefix(ORIGIN)?;
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let path = if path.is_empty() { "index.html" } else { path };
    ASSETS.iter().copied().find(|asset| asset.path == path)
}

/// Messages the page may post. Anything else is counted and dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostMessage {
    /// The page's JSON report. Only its outer shape is checked: the host
    /// embeds it verbatim and never interprets it.
    Report(String),
}

impl HostMessage {
    pub fn parse(raw: &str) -> Option<Self> {
        let json = raw.strip_prefix("report:")?;
        let trimmed = json.trim();
        (trimmed.len() <= MAX_REPORT_BYTES && trimmed.starts_with('{') && trimmed.ends_with('}'))
            .then(|| Self::Report(trimmed.to_owned()))
    }
}

/// Accessible names the host looks for through UI Automation, with the ARIA
/// role the page authored. Keep in sync with `EXPECTED` in
/// `web/src/probes/a11y.ts`.
pub const EXPECTED_A11Y: &[(&str, &str)] = &[
    ("Start", "button"),
    ("Workspace 1", "button"),
    ("Wi-Fi", "switch"),
    ("Volume", "slider"),
    ("Battery", "progressbar"),
    ("Open control center", "button"),
    ("Glint probe panel", "toolbar"),
    ("Quick settings", "region"),
];

/// Escapes `text` as a JSON string literal, quotes included.
pub fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Scales a DIP length to physical pixels for `dpi` (96 = 100%).
pub fn scale(dip: i32, dpi: u32) -> i32 {
    let dpi = i64::from(dpi.max(1));
    ((i64::from(dip) * dpi + 48) / 96) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parses_arguments_and_rejects_bad_values() {
        let options = parse_args(&args(&["--smoke", "30", "--out", "x", "--activate"])).unwrap();
        assert!(options.activate);
        assert_eq!(options.smoke_seconds, Some(30));
        assert_eq!(options.out_dir, Some(PathBuf::from("x")));
        assert!(parse_args(&args(&["--help"])).unwrap().help);
        assert!(parse_args(&args(&["--smoke"])).is_err());
        assert!(parse_args(&args(&["--smoke", "abc"])).is_err());
        assert!(parse_args(&args(&["--smoke", "4"])).is_err());
        assert!(parse_args(&args(&["--smoke", "601"])).is_err());
        assert!(parse_args(&args(&["--out"])).is_err());
        assert!(parse_args(&args(&["--bogus"])).is_err());
    }

    #[test]
    fn origin_policy_rejects_look_alikes() {
        assert!(is_bundled_url("https://glint.spike/index.html"));
        assert!(!is_bundled_url("https://glint.spike.example.com/"));
        assert!(!is_bundled_url("http://glint.spike/index.html"));
        assert!(!is_bundled_url("https://example.com/https://glint.spike/"));
    }

    #[test]
    fn assets_resolve_only_on_origin() {
        assert_eq!(asset_for_url(ORIGIN).unwrap().path, "index.html");
        assert_eq!(
            asset_for_url("https://glint.spike/img/probe.avif?x#y")
                .unwrap()
                .mime,
            "image/avif"
        );
        assert!(asset_for_url("https://glint.spike/missing.js").is_none());
        assert!(asset_for_url("https://example.com/app.js").is_none());
    }

    #[test]
    fn bundle_is_present_and_self_contained() {
        let html = std::str::from_utf8(asset_for_url(ORIGIN).unwrap().body).unwrap();
        assert!(html.contains("Content-Security-Policy"));
        assert!(html.contains("script-src 'self'"));
        for asset in ASSETS {
            assert!(!asset.body.is_empty(), "{} is empty", asset.path);
            if asset.mime.starts_with("text/") {
                let text = std::str::from_utf8(asset.body).unwrap();
                // No remote loads: no protocol-relative or http(s) src/href/url().
                for needle in ["src=\"http", "href=\"http", "url(http", "src=\"//"] {
                    assert!(!text.contains(needle), "{} contains {needle}", asset.path);
                }
            }
        }
        // The committed bundle must be the production React build.
        let js =
            std::str::from_utf8(asset_for_url("https://glint.spike/app.js").unwrap().body).unwrap();
        assert!(js.contains("report:"));
        assert!(!js.contains("react-dom.development"));
    }

    #[test]
    fn probe_images_have_their_format_signatures() {
        let body = |p: &str| asset_for_url(&format!("{ORIGIN}img/{p}")).unwrap().body;
        assert!(body("probe.png").starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(body("probe.webp").starts_with(b"RIFF") && &body("probe.webp")[8..12] == b"WEBP");
        assert_eq!(&body("probe.avif")[4..12], b"ftypavif");
        assert!(body("probe.svg").starts_with(b"<svg"));
    }

    #[test]
    fn parses_only_report_messages() {
        assert_eq!(
            HostMessage::parse("report:{\"a\":1}"),
            Some(HostMessage::Report("{\"a\":1}".to_owned()))
        );
        assert_eq!(HostMessage::parse("report:[1]"), None);
        assert_eq!(HostMessage::parse("report:"), None);
        assert_eq!(HostMessage::parse("quit"), None);
        let huge = format!("report:{{\"x\":\"{}\"}}", "a".repeat(MAX_REPORT_BYTES));
        assert_eq!(HostMessage::parse(&huge), None);
    }

    #[test]
    fn json_string_escapes_controls_and_quotes() {
        assert_eq!(json_string("a\"b\\c\nd\u{1}"), "\"a\\\"b\\\\c\\nd\\u0001\"");
        assert_eq!(json_string("Wi-Fi"), "\"Wi-Fi\"");
    }

    #[test]
    fn scale_rounds_like_muldiv() {
        assert_eq!(scale(520, 96), 520);
        assert_eq!(scale(520, 120), 650);
        assert_eq!(scale(1, 144), 2);
    }
}
