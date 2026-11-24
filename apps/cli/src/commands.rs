//! Command handlers. Each is thin wiring onto the library crates.

mod theme;

use std::fs;
use std::io;
use std::path::Path;

use glint_package::{ErrorCode as PackageCode, PackageError, default_staging_root};
use glint_schema::Platform;
use glint_sources::{LocalSource, PackageSource, SourceError, SourceErrorCode, ThemeSource};
use serde_json::{Value, json};

use crate::cli::{Action, COMMANDS, Command, GROUPS, Invocation, Status};
use crate::error::{CliError, ErrorKind, codes};
use crate::output::Success;

/// Whether the process is attached to an interactive terminal.
#[derive(Debug, Clone, Copy)]
pub struct Terminal {
    /// stdin and stdout are both terminals.
    pub interactive: bool,
}

/// Execute a parsed action.
pub fn execute(action: Action, json: bool, terminal: Terminal) -> Result<Success, CliError> {
    match action {
        Action::Help(topic) => Ok(help(&topic)),
        Action::Version => Ok(version()),
        Action::Run(invocation) => run(&invocation, json, terminal),
    }
}

fn run(invocation: &Invocation, json: bool, terminal: Terminal) -> Result<Success, CliError> {
    let command = invocation.command;
    match command.path {
        ["version"] => Ok(version()),
        ["theme", "validate"] => {
            let path = invocation.args.first().map_or(".", String::as_str);
            theme_validate(Path::new(path))
        }
        ["theme", "import"] => theme::import(Path::new(&invocation.args[0])),
        ["theme", "list"] => theme::list(),
        ["theme", "export"] => theme::export(&invocation.args[0], invocation.option("output")),
        ["theme", "remove"] => theme::remove(&invocation.args[0]),
        ["theme", "use"] => theme::use_theme(&invocation.args[0]),
        ["safe-mode"] => theme::safe_mode(),
        // The bare `theme` command is the interactive chooser: terminals
        // only, never in JSON mode.
        ["theme"] if json || !terminal.interactive => Err(CliError::new(
            ErrorKind::Usage,
            codes::MISSING_SUBCOMMAND,
            "`theme` needs a subcommand: the interactive chooser runs only on a terminal \
             and never with --json; see `glint theme --help`",
        )),
        _ => Err(not_implemented(command)),
    }
}

/// The typed error for a recognised command that does nothing yet.
pub fn not_implemented(command: &Command) -> CliError {
    let ticket = match command.status {
        Status::Planned(ticket) => ticket,
        Status::Implemented => None,
    };
    let planned = ticket.map_or_else(|| "a later ticket".to_owned(), str::to_owned);
    CliError::new(
        ErrorKind::Unsupported,
        codes::NOT_IMPLEMENTED,
        format!(
            "`glint {}` is not implemented yet (planned in {planned}); nothing was changed",
            command.name()
        ),
    )
    .with_detail("plannedIn", ticket.map_or(Value::Null, Value::from))
}

fn version() -> Success {
    let version = env!("CARGO_PKG_VERSION");
    Success {
        data: json!({ "version": version }),
        text: format!("glint {version}"),
    }
}

fn status_label(status: Status) -> &'static str {
    match status {
        Status::Implemented => "implemented",
        Status::Planned(_) => "not-implemented",
    }
}

fn help(topic: &[String]) -> Success {
    let selected: Vec<&Command> = COMMANDS
        .iter()
        .filter(|c| {
            c.path
                .iter()
                .copied()
                .take(topic.len())
                .eq(topic.iter().map(String::as_str))
        })
        .collect();
    let exact = selected
        .iter()
        .find(|c| c.path.len() == topic.len() && !topic.is_empty());

    let mut text = String::new();
    match (topic.first(), exact) {
        (_, Some(command))
            if command.path.len() > 1 || !GROUPS.iter().any(|(g, _)| *g == command.path[0]) =>
        {
            text.push_str(&format!(
                "usage: {}\n\n{}\n",
                command.usage(),
                command.summary
            ));
            if let Status::Planned(ticket) = command.status {
                text.push_str(&format!(
                    "\nNot implemented yet (planned in {}).\n",
                    ticket.unwrap_or("a later ticket")
                ));
            }
        }
        (None, _) => {
            text.push_str(
                "usage: glint [--json] <command> [args]\n       glint --version | --help\n\n\
                 Commands and flags are proposed until documented as stable.\n\
                 `--json` prints one JSON document on stdout (schema version 1).\n\n",
            );
            push_command_list(&mut text, &selected);
            text.push_str("\nExit codes:\n  0  ok\n");
            for kind in ErrorKind::ALL {
                text.push_str(&format!("  {}  {}\n", kind.exit_code(), kind.as_str()));
            }
        }
        (Some(group), _) => {
            let summary = GROUPS
                .iter()
                .find(|(g, _)| g == group)
                .map_or("", |(_, s)| *s);
            text.push_str(&format!(
                "usage: glint {group} <command> [args]\n\n{summary}\n\n"
            ));
            push_command_list(&mut text, &selected);
        }
    }

    let commands: Vec<Value> = selected
        .iter()
        .map(|c| {
            json!({
                "name": c.name(),
                "usage": c.usage(),
                "summary": c.summary,
                "status": status_label(c.status),
                "plannedIn": match c.status {
                    Status::Planned(ticket) => ticket.map_or(Value::Null, Value::from),
                    Status::Implemented => Value::Null,
                },
            })
        })
        .collect();
    let exit_codes: Vec<Value> = std::iter::once(json!({ "code": 0, "kind": "ok" }))
        .chain(
            ErrorKind::ALL
                .iter()
                .map(|k| json!({ "code": k.exit_code(), "kind": k.as_str() })),
        )
        .collect();
    Success {
        data: json!({
            "topic": topic.join(" "),
            "commands": commands,
            "exitCodes": exit_codes,
        }),
        text,
    }
}

fn push_command_list(text: &mut String, commands: &[&Command]) {
    let width = commands.iter().map(|c| c.name().len()).max().unwrap_or(0);
    for command in commands {
        let marker = match command.status {
            Status::Implemented => "",
            Status::Planned(_) => "  [not implemented]",
        };
        text.push_str(&format!(
            "  {:width$}  {}{marker}\n",
            command.name(),
            command.summary
        ));
    }
}

fn platform_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Windows => "windows",
        Platform::Linux => "linux",
        Platform::Macos => "macos",
        _ => "unknown",
    }
}

/// `glint theme validate [PATH]`: stage a theme directory or `.glint`
/// archive through the same `glint-sources` / `glint-package` pipeline the
/// installer uses, then discard the staging copy.
fn theme_validate(path: &Path) -> Result<Success, CliError> {
    let source = if path.is_dir() {
        ThemeSource::LocalDirectory {
            path: path.to_path_buf(),
        }
    } else {
        ThemeSource::LocalArchive {
            path: path.to_path_buf(),
        }
    };
    let staging_root = default_staging_root();
    fs::create_dir_all(&staging_root).map_err(|error| {
        io_error(&error, "create the staging directory")
            .with_detail("path", staging_root.display().to_string())
    })?;
    let staged = LocalSource::new()
        .stage(&source, &staging_root)
        .map_err(|error| source_error(&error))?;

    let manifest = &staged.candidate.manifest;
    let origin = &staged.origin;
    let platforms: Vec<&str> = manifest
        .compatibility
        .platforms
        .iter()
        .map(|p| platform_name(*p))
        .collect();
    let surfaces: Vec<&str> = manifest.surfaces.keys().map(String::as_str).collect();
    let data = json!({
        "valid": true,
        "source": {
            "kind": origin.kind.as_str(),
            "path": origin.locator,
        },
        "package": {
            "id": manifest.id,
            "name": manifest.name,
            "version": manifest.version.to_string(),
            "license": manifest.license,
            "glint": manifest.compatibility.glint.to_string(),
            "platforms": platforms,
            "surfaces": surfaces,
            "requiredCapabilities": manifest.required_capabilities,
            "optionalCapabilities": manifest.optional_capabilities,
        },
        "archive": {
            "sha256": origin.archive_sha256_hex(),
            "size": staged.candidate.archive_size,
            "files": staged.candidate.files.len(),
            "totalUncompressed": staged.candidate.total_uncompressed,
        },
        "checked": ["archive-structure", "manifest-schema", "capability-ids", "surface-definitions", "referenced-files", "surface-entrypoints", "executable-content", "remote-references", "active-content"],
        "notChecked": ["host-version-compatibility", "surface-build-output", "forbidden-apis"],
    });
    let text = format!(
        "valid: {} {} ({})\n  source   {} {}\n  sha256   {}\n  files    {} ({} bytes uncompressed)\n\
         Checked package structure and manifest only; host compatibility and build output \
         checks are planned in CRT-003.",
        manifest.id,
        manifest.version,
        manifest.name,
        origin.kind.as_str(),
        origin.locator,
        origin.archive_sha256_hex(),
        staged.candidate.files.len(),
        staged.candidate.total_uncompressed,
    );
    Ok(Success { data, text })
}

fn io_error(error: &io::Error, what: &str) -> CliError {
    let kind = if error.kind() == io::ErrorKind::PermissionDenied {
        ErrorKind::PermissionDenied
    } else {
        ErrorKind::Failure
    };
    CliError::new(kind, codes::IO, format!("could not {what}: {error}"))
}

/// Map a `glint-sources` error to a CLI error, keeping the most specific
/// underlying code and the whole code chain in `details`.
pub fn source_error(error: &SourceError) -> CliError {
    let source_code = error.code();
    let mut kind = match source_code {
        SourceErrorCode::NotFound => ErrorKind::NotFound,
        SourceErrorCode::PermissionDenied => ErrorKind::PermissionDenied,
        SourceErrorCode::Unsupported => ErrorKind::Unsupported,
        SourceErrorCode::NotAFile
        | SourceErrorCode::NotADirectory
        | SourceErrorCode::PackageRejected => ErrorKind::InvalidPackage,
        _ => ErrorKind::Failure,
    };
    let mut code = source_code.as_str().to_owned();
    let mut cli_error_details = vec![("sourceCode", Value::from(source_code.as_str()))];
    if let Some(package) = error.package_error() {
        kind = package_kind(package);
        code = package.code().as_str().to_owned();
        cli_error_details.push(("packageCode", Value::from(code.clone())));
        if let Some(manifest) = package.manifest_error() {
            code = manifest.code().as_str().to_owned();
            cli_error_details.push(("manifestCode", Value::from(code.clone())));
            cli_error_details.push(("pointer", Value::from(manifest.pointer())));
        }
        if let Some(surface) = package.surface_error() {
            code = surface.code().as_str().to_owned();
            cli_error_details.push(("surfaceCode", Value::from(code.clone())));
            cli_error_details.push(("pointer", Value::from(surface.pointer())));
        }
    }
    // The library display starts with its own code; the CLI shows codes
    // separately, so drop that prefix from the message.
    let display = error.to_string();
    let message = display
        .strip_prefix(source_code.as_str())
        .and_then(|rest| rest.strip_prefix(": "))
        .unwrap_or(&display);
    let mut cli_error = CliError::new(kind, &code, message);
    for (key, value) in cli_error_details {
        cli_error = cli_error.with_detail(key, value);
    }
    cli_error
}

fn package_kind(error: &PackageError) -> ErrorKind {
    match error {
        PackageError::Io { source, .. } if source.kind() == io::ErrorKind::PermissionDenied => {
            ErrorKind::PermissionDenied
        }
        _ => match error.code() {
            PackageCode::Io | PackageCode::StagingRootInvalid | PackageCode::StagingConflict => {
                ErrorKind::Failure
            }
            _ => ErrorKind::InvalidPackage,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::PathBuf;

    use glint_package::PackageError;
    use glint_sources::{SourceError, SourceKind};

    use super::{Terminal, execute, source_error};
    use crate::cli::{Action, parse};
    use crate::error::{ErrorKind, codes};

    fn exec(
        line: &str,
        interactive: bool,
    ) -> Result<crate::output::Success, crate::error::CliError> {
        let args: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
        let parsed = parse(&args);
        execute(
            parsed.action.unwrap(),
            parsed.json,
            Terminal { interactive },
        )
    }

    #[test]
    fn every_failure_class_has_a_source() {
        let cases = [
            (
                SourceError::NotFound(PathBuf::from("x")),
                ErrorKind::NotFound,
            ),
            (
                SourceError::PermissionDenied(PathBuf::from("x")),
                ErrorKind::PermissionDenied,
            ),
            (
                SourceError::Unsupported {
                    kind: SourceKind::Git,
                    planned: "M4",
                },
                ErrorKind::Unsupported,
            ),
            (
                SourceError::PackageRejected(PackageError::NotZip),
                ErrorKind::InvalidPackage,
            ),
            (
                SourceError::PackageRejected(PackageError::Io {
                    context: "read",
                    source: io::Error::from(io::ErrorKind::PermissionDenied),
                }),
                ErrorKind::PermissionDenied,
            ),
            (
                SourceError::Io {
                    context: "read",
                    source: io::Error::other("disk"),
                },
                ErrorKind::Failure,
            ),
        ];
        for (error, kind) in cases {
            assert_eq!(source_error(&error).kind, kind, "{error}");
        }
        let not_zip = source_error(&SourceError::PackageRejected(PackageError::NotZip));
        assert_eq!(not_zip.code, "GLINT_PACKAGE_NOT_ZIP");
        assert_eq!(
            not_zip.details["sourceCode"],
            "GLINT_SOURCE_PACKAGE_REJECTED"
        );
    }

    #[test]
    fn interactive_chooser_only_on_a_terminal() {
        let error = exec("theme", false).unwrap_err();
        assert_eq!(
            (error.kind, error.code.as_str()),
            (ErrorKind::Usage, codes::MISSING_SUBCOMMAND)
        );
        let error = exec("theme --json", true).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Usage);
        // On a terminal the chooser is recognised but not built yet.
        let error = exec("theme", true).unwrap_err();
        assert_eq!(
            (error.kind, error.code.as_str()),
            (ErrorKind::Unsupported, codes::NOT_IMPLEMENTED)
        );
        assert_eq!(error.details["plannedIn"], serde_json::Value::Null);
    }

    #[test]
    fn unimplemented_commands_never_succeed() {
        for command in crate::cli::COMMANDS {
            if matches!(command.status, crate::cli::Status::Implemented) {
                continue;
            }
            let mut line = command.name();
            for arg in command.args.iter().filter(|a| a.required) {
                line.push_str(&format!(" {}", arg.name.to_lowercase()));
            }
            let error = exec(&line, true).unwrap_err();
            assert_eq!(error.kind, ErrorKind::Unsupported, "{line}");
            assert_eq!(error.code, codes::NOT_IMPLEMENTED, "{line}");
        }
    }

    #[test]
    fn help_is_a_success() {
        assert!(matches!(parse(&[]).action, Ok(Action::Help(_))));
        let success = exec("--help", false).unwrap();
        assert!(success.text.contains("theme validate"));
        assert_eq!(success.data["exitCodes"][4]["kind"], "invalid-package");
    }
}
