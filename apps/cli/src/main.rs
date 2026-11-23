//! `glint` CLI entry point.
//!
//! Glint is in the planning/foundation stage. This binary only reports its
//! version; commands are added by the CLI-* tickets and must never report
//! success for work they did not perform.

use std::process::ExitCode;

const USAGE: &str = "usage: glint [--version | --help]";

fn run(args: &[String]) -> Result<String, String> {
    match args.first().map(String::as_str) {
        None | Some("--help") | Some("-h") => Ok(USAGE.to_owned()),
        Some("--version") | Some("-V") => Ok(format!("glint {}", env!("CARGO_PKG_VERSION"))),
        Some(other) => Err(format!(
            "unsupported command `{other}`: not implemented yet\n{USAGE}"
        )),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(out) => {
            println!("{out}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn version_flag_reports_package_version() {
        let out = run(&["--version".to_owned()]).unwrap();
        assert_eq!(out, format!("glint {}", env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn unknown_command_is_an_error_not_a_fake_success() {
        assert!(run(&["theme".to_owned()]).is_err());
    }
}
