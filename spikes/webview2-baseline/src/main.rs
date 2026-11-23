//! SPK-008 throwaway spike: hosts one React probe panel in WebView2 in a
//! normal top-level window and records what the engine renders and supports:
//! fonts, layout, translucency, CSS effects, frame timing, images, the UI
//! Automation view of the accessibility tree and process-tree memory.
//!
//! Not product code: it lives under `spikes/`, is never shipped and must not
//! be imported by `crates/` or `apps/`. Findings:
//! `docs/research/spikes/webview2-baseline.md`.

// The logic module is only exercised by the Windows host and the unit tests.
#[cfg_attr(not(windows), allow(dead_code))]
mod logic;
#[cfg(windows)]
mod win;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match logic::parse_args(&args) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("error: {err}\n{}", logic::USAGE);
            return ExitCode::from(2);
        }
    };
    if options.help {
        println!("{}", logic::USAGE);
        return ExitCode::SUCCESS;
    }
    run(options)
}

#[cfg(windows)]
fn run(options: logic::Options) -> ExitCode {
    win::run(options)
}

#[cfg(not(windows))]
fn run(_options: logic::Options) -> ExitCode {
    // Compile-only on Linux/macOS CI: report unsupported, never fake success.
    eprintln!("windows-only spike: unsupported on this platform (WebKit rows wait for SPK-009)");
    ExitCode::from(2)
}
