//! SPK-010 throwaway spike: query the Windows Core Audio endpoint volume,
//! network connectivity and battery providers, map each (and each
//! sub-capability) to an availability status, and exercise provider restart,
//! stale subscriptions and COM apartment rules.
//!
//! Not product code: it lives under `spikes/`, is never shipped and must not
//! be imported by `crates/` or `apps/`. It never changes system volume.
//! Findings: `docs/research/spikes/windows-system-providers.md`.

// The model is exercised by the Windows probe and by the unit tests.
#[cfg_attr(not(windows), allow(dead_code))]
mod model;
#[cfg(windows)]
mod win;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match model::parse_args(&args) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("error: {err}\n{}", model::USAGE);
            return ExitCode::from(2);
        }
    };
    if options.help {
        println!("{}", model::USAGE);
        return ExitCode::SUCCESS;
    }
    run(options)
}

#[cfg(windows)]
fn run(options: model::Options) -> ExitCode {
    win::run(&options)
}

#[cfg(not(windows))]
fn run(_options: model::Options) -> ExitCode {
    // Compile-only on Linux/macOS CI (LNX-004 / MAC-008 own those providers):
    // report unsupported, never fake success.
    eprintln!("windows-only spike: unsupported on this platform");
    ExitCode::from(2)
}
