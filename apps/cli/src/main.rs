//! `glint` CLI entry point (CLI-001).
//!
//! Thin by design: argument parsing, output rendering and the exit code
//! contract live here; the work is done by the library crates. Commands in
//! the proposed tree that are not implemented yet fail with the typed
//! `unsupported` error (exit 3) and never report success. The contract is
//! documented in `docs/cli/reference.md`.

mod cli;
mod commands;
mod error;
mod output;

use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;

use crate::commands::Terminal;
use crate::error::{CliError, codes};

fn main() -> ExitCode {
    let parsed = match std::env::args_os()
        .skip(1)
        .map(|arg| {
            arg.into_string().map_err(|arg| {
                CliError::usage(format!(
                    "argument {} is not valid Unicode",
                    arg.to_string_lossy()
                ))
            })
        })
        .collect::<Result<Vec<String>, CliError>>()
    {
        Ok(args) => cli::parse(&args),
        Err(error) => cli::Parsed {
            json: false,
            command: None,
            action: Err(error),
        },
    };
    let terminal = Terminal {
        interactive: io::stdin().is_terminal() && io::stdout().is_terminal(),
    };
    let result = parsed
        .action
        .and_then(|action| commands::execute(action, parsed.json, terminal));
    let rendered = output::render(parsed.json, parsed.command.as_deref(), result);

    // A closed stdout (`glint ... | head`) is not worth a panic; report it
    // as a failure on stderr if stderr still works.
    if let Err(error) = io::stdout().write_all(rendered.stdout.as_bytes()) {
        let _ = writeln!(
            io::stderr(),
            "error[failure]: cannot write output: {error} ({})",
            codes::IO
        );
        return ExitCode::from(error::ErrorKind::Failure.exit_code());
    }
    let _ = io::stderr().write_all(rendered.stderr.as_bytes());
    ExitCode::from(rendered.exit_code)
}
