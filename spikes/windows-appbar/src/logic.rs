//! Platform-independent spike logic: argument parsing, the bundled-origin
//! policy, the embedded asset table, host-message parsing and appbar geometry.
//!
//! Kept free of Windows types so it compiles and is unit-tested on every CI
//! host, while the Win32/WebView2 code in `win.rs` stays Windows-only.

/// The only origin the spike webviews may load. Requests are answered from
/// memory by the host; nothing is read from disk or the network.
pub const ORIGIN: &str = "https://glint.spike/";

/// Content-Security-Policy sent as a response header for every bundled asset
/// (the HTML files repeat it in a `<meta>` tag). No remote script, style,
/// connection, frame, object or form target is allowed.
pub const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; \
connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; \
frame-ancestors 'none'";

/// Panel thickness in device-independent pixels (scaled by the monitor DPI).
pub const PANEL_HEIGHT_DIP: i32 = 40;
/// Control-center popup size in device-independent pixels.
pub const POPUP_SIZE_DIP: (i32, i32) = (360, 440);

/// One embedded file served under [`ORIGIN`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Asset {
    pub path: &'static str,
    pub mime: &'static str,
    pub body: &'static str,
}

const ASSETS: &[Asset] = &[
    Asset {
        path: "panel.html",
        mime: "text/html; charset=utf-8",
        body: include_str!("../assets/panel.html"),
    },
    Asset {
        path: "panel.js",
        mime: "text/javascript; charset=utf-8",
        body: include_str!("../assets/panel.js"),
    },
    Asset {
        path: "control-center.html",
        mime: "text/html; charset=utf-8",
        body: include_str!("../assets/control-center.html"),
    },
    Asset {
        path: "control-center.js",
        mime: "text/javascript; charset=utf-8",
        body: include_str!("../assets/control-center.js"),
    },
    Asset {
        path: "spike.css",
        mime: "text/css; charset=utf-8",
        body: include_str!("../assets/spike.css"),
    },
];

/// True only for URLs on the bundled origin. The trailing slash in [`ORIGIN`]
/// rejects look-alikes such as `https://glint.spike.example.com/`.
pub fn is_bundled_url(url: &str) -> bool {
    url.starts_with(ORIGIN)
}

/// Resolves a request URL to an embedded asset, ignoring query and fragment.
pub fn asset_for_url(url: &str) -> Option<Asset> {
    let path = url.strip_prefix(ORIGIN)?;
    let path = path.split(['?', '#']).next().unwrap_or_default();
    ASSETS.iter().copied().find(|asset| asset.path == path)
}

/// Messages the bundled pages may post to the host. Anything else is ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostMessage {
    PanelReady,
    ControlCenterReady,
    ToggleControlCenter,
    CloseControlCenter,
    Quit,
    /// A `securitypolicyviolation` the page observed (directive and blocked URI).
    CspViolation(String),
}

impl HostMessage {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "panel-ready" => Some(Self::PanelReady),
            "control-center-ready" => Some(Self::ControlCenterReady),
            "toggle-control-center" => Some(Self::ToggleControlCenter),
            "close-control-center" => Some(Self::CloseControlCenter),
            "quit" => Some(Self::Quit),
            other => other
                .strip_prefix("csp-violation:")
                .map(|detail| Self::CspViolation(detail.chars().take(200).collect())),
        }
    }
}

/// Screen rectangle in physical pixels (mirrors Win32 `RECT`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

/// Scales a DIP length to physical pixels for `dpi` (96 = 100%), rounding
/// half away from zero like Win32 `MulDiv`.
pub fn scale(dip: i32, dpi: u32) -> i32 {
    let dpi = i64::from(dpi.max(1));
    ((i64::from(dip) * dpi + 48) / 96) as i32
}

/// The rectangle proposed to `ABM_QUERYPOS` for a top-edge bar.
pub fn proposed_top_rect(monitor: Rect, height: i32) -> Rect {
    Rect {
        bottom: monitor.top + height,
        ..monitor
    }
}

/// After `ABM_QUERYPOS` the shell may move `top` down (e.g. below another
/// top-edge appbar); keep the bar's thickness from the adjusted top.
pub fn adjust_queried_top_rect(queried: Rect, height: i32) -> Rect {
    Rect {
        bottom: queried.top + height,
        ..queried
    }
}

/// Places the control-center popup right-aligned directly below the bar,
/// clamped to the monitor.
pub fn popup_rect(bar: Rect, monitor: Rect, dpi: u32) -> Rect {
    let width = scale(POPUP_SIZE_DIP.0, dpi).min(monitor.width());
    let height = scale(POPUP_SIZE_DIP.1, dpi).min((monitor.bottom - bar.bottom).max(0));
    let margin = scale(8, dpi);
    let right = (bar.right - margin).max(monitor.left + width);
    Rect {
        left: right - width,
        top: bar.bottom,
        right,
        bottom: bar.bottom + height,
    }
}

/// Command-line options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Options {
    /// Self-exit cleanly after this many seconds and print observations.
    pub smoke_seconds: Option<u32>,
    pub help: bool,
}

pub const USAGE: &str = "usage: spike-windows-appbar [--smoke <seconds>] [--help]\n\
  (no flags)        run until the panel's Quit button, Ctrl+C or WM_CLOSE\n\
  --smoke <seconds> exercise the popup and policy probes, print observations, exit (3..=600)";

pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => options.help = true,
            "--smoke" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--smoke needs a number of seconds".to_owned())?;
                let seconds: u32 = value
                    .parse()
                    .map_err(|_| format!("--smoke expects whole seconds, got `{value}`"))?;
                if !(3..=600).contains(&seconds) {
                    return Err(format!(
                        "--smoke must be within 3..=600 seconds, got {seconds}"
                    ));
                }
                options.smoke_seconds = Some(seconds);
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parses_smoke_and_rejects_bad_values() {
        assert_eq!(
            parse_args(&args(&["--smoke", "8"])).unwrap().smoke_seconds,
            Some(8)
        );
        assert!(parse_args(&args(&["--smoke"])).is_err());
        assert!(parse_args(&args(&["--smoke", "x"])).is_err());
        assert!(parse_args(&args(&["--smoke", "1"])).is_err());
        assert!(parse_args(&args(&["--bogus"])).is_err());
        assert_eq!(parse_args(&[]).unwrap(), Options::default());
    }

    #[test]
    fn origin_policy_rejects_look_alikes() {
        assert!(is_bundled_url("https://glint.spike/panel.html"));
        assert!(!is_bundled_url(
            "https://glint.spike.example.com/panel.html"
        ));
        assert!(!is_bundled_url("https://glint.spike@example.com/"));
        assert!(!is_bundled_url("http://glint.spike/panel.html"));
        assert!(!is_bundled_url("https://example.com/"));
        assert!(!is_bundled_url("about:blank"));
    }

    #[test]
    fn assets_resolve_only_on_origin() {
        let panel = asset_for_url("https://glint.spike/panel.html?x=1#y").unwrap();
        assert!(panel.mime.starts_with("text/html"));
        assert!(asset_for_url("https://glint.spike/missing.js").is_none());
        assert!(asset_for_url("https://example.com/panel.html").is_none());
        assert!(asset_for_url("https://glint.spike/../panel.html").is_none());
    }

    #[test]
    fn every_html_asset_declares_the_csp_and_no_remote_urls() {
        for asset in ASSETS {
            if asset.path.ends_with(".html") {
                assert!(
                    asset.body.contains("Content-Security-Policy"),
                    "{}",
                    asset.path
                );
            }
            assert!(!asset.body.contains("http://"), "{}", asset.path);
            assert!(
                !asset.body.contains("<script>"),
                "{}: no inline script",
                asset.path
            );
        }
    }

    #[test]
    fn parses_host_messages() {
        assert_eq!(
            HostMessage::parse("toggle-control-center"),
            Some(HostMessage::ToggleControlCenter)
        );
        assert_eq!(
            HostMessage::parse("csp-violation:script-src-elem https://example.com/x.js"),
            Some(HostMessage::CspViolation(
                "script-src-elem https://example.com/x.js".to_owned()
            ))
        );
        assert_eq!(HostMessage::parse("rm -rf"), None);
    }

    #[test]
    fn geometry_scales_with_dpi() {
        assert_eq!(scale(PANEL_HEIGHT_DIP, 96), 40);
        assert_eq!(scale(PANEL_HEIGHT_DIP, 144), 60);
        assert_eq!(scale(PANEL_HEIGHT_DIP, 120), 50);
        let monitor = Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let proposed = proposed_top_rect(monitor, 60);
        assert_eq!(
            (proposed.top, proposed.bottom, proposed.right),
            (0, 60, 1920)
        );
        let shifted = Rect {
            top: 30,
            ..proposed
        };
        assert_eq!(adjust_queried_top_rect(shifted, 60).bottom, 90);
        let popup = popup_rect(proposed, monitor, 96);
        assert_eq!(popup.top, 60);
        assert_eq!(popup.width(), 360);
        assert!(popup.right <= monitor.right && popup.left >= monitor.left);
    }
}
