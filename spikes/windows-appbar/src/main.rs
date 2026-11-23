//! SPK-001 throwaway spike: a Win32 appbar (`SHAppBarMessage`) docked to the
//! top of the primary monitor, hosting a WebView2 panel with bundled HTML and
//! a toggleable control-center popup.
//!
//! Not product code: it lives under `spikes/`, is never shipped and must not
//! be imported by `crates/` or `apps/`. It never hides or modifies the
//! Explorer taskbar. Findings: `docs/research/spikes/windows-appbar.md`.

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
    eprintln!("windows-only spike: unsupported on this platform");
    ExitCode::from(2)
}
