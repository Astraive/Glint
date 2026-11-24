//! The proposed command tree and a small, dependency-free argument parser.
//!
//! Command and flag names follow `docs/plan/06-engineering/cli-and-developer-experience.md`
//! and stay *proposed* until documented as stable. Every command in the tree
//! parses its arguments, so a malformed call is a usage error (exit 2) even
//! when the command itself is not implemented yet (exit 3).

use crate::error::{CliError, ErrorKind, codes};

/// Whether a command does real work yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Implemented in this binary.
    Implemented,
    /// Recognised but not implemented; the ticket that owns it, if one is
    /// scheduled.
    Planned(Option<&'static str>),
}

/// One positional argument.
#[derive(Debug)]
pub struct Arg {
    /// Placeholder shown in usage, e.g. `PATH`.
    pub name: &'static str,
    /// Whether the argument must be given.
    pub required: bool,
}

/// One option that takes a value, e.g. `-o PATH`.
#[derive(Debug)]
pub struct Opt {
    /// Long name without dashes.
    pub long: &'static str,
    /// Optional one-letter alias.
    pub short: Option<char>,
    /// Placeholder for the value.
    pub value: &'static str,
}

/// One command (a leaf of the tree).
#[derive(Debug)]
pub struct Command {
    /// Words that select the command, e.g. `["theme", "export"]`.
    pub path: &'static [&'static str],
    /// One-line description.
    pub summary: &'static str,
    /// Positional arguments, required ones first.
    pub args: &'static [Arg],
    /// Options that take a value.
    pub options: &'static [Opt],
    /// Implementation status.
    pub status: Status,
}

impl Command {
    /// The command words joined by spaces, e.g. `theme export`.
    pub fn name(&self) -> String {
        self.path.join(" ")
    }

    /// One-line usage, e.g. `glint theme export ID [-o PATH]`.
    pub fn usage(&self) -> String {
        let mut out = format!("glint {}", self.name());
        for opt in self.options {
            let flag = match opt.short {
                Some(short) => format!("-{short}"),
                None => format!("--{}", opt.long),
            };
            out.push_str(&format!(" [{flag} {}]", opt.value));
        }
        for arg in self.args {
            if arg.required {
                out.push_str(&format!(" {}", arg.name));
            } else {
                out.push_str(&format!(" [{}]", arg.name));
            }
        }
        out
    }
}

/// Command groups: words that only prefix other commands. `theme` is also a
/// command of its own (the interactive chooser).
pub const GROUPS: &[(&str, &str)] = &[
    (
        "theme",
        "Browse, install and manage themes; create and package them",
    ),
    ("profile", "List and switch theme profiles"),
    ("registry", "Manage theme registries"),
    ("permissions", "Inspect capability grants"),
];

const ID: Arg = Arg {
    name: "ID",
    required: true,
};
const OPTIONAL_PATH: Arg = Arg {
    name: "PATH",
    required: false,
};
const OUTPUT: Opt = Opt {
    long: "output",
    short: Some('o'),
    value: "PATH",
};

const CLI_003: Status = Status::Planned(Some("CLI-003"));
const CLI_004: Status = Status::Planned(Some("CLI-004"));
const CLI_005: Status = Status::Planned(Some("CLI-005"));

/// The whole proposed command tree.
pub const COMMANDS: &[Command] = &[
    Command {
        path: &["version"],
        summary: "Print the Glint version",
        args: &[],
        options: &[],
        status: Status::Implemented,
    },
    Command {
        path: &["doctor"],
        summary: "Diagnose the installation (redacted by default)",
        args: &[],
        options: &[],
        status: CLI_003,
    },
    Command {
        path: &["status"],
        summary: "Show daemon, shell and active theme state",
        args: &[],
        options: &[],
        status: CLI_003,
    },
    Command {
        path: &["safe-mode"],
        summary: "Activate the bundled default theme in the safe-mode profile",
        args: &[],
        options: &[],
        status: Status::Implemented,
    },
    Command {
        path: &["theme"],
        summary: "Choose a theme interactively (terminal only)",
        args: &[],
        options: &[],
        status: Status::Planned(None),
    },
    Command {
        path: &["theme", "browse"],
        summary: "Browse registry catalogs",
        args: &[],
        options: &[],
        status: CLI_005,
    },
    Command {
        path: &["theme", "search"],
        summary: "Search registry catalogs",
        args: &[Arg {
            name: "QUERY",
            required: true,
        }],
        options: &[],
        status: CLI_005,
    },
    Command {
        path: &["theme", "info"],
        summary: "Show a theme's metadata, permissions and support",
        args: &[ID],
        options: &[],
        status: CLI_004,
    },
    Command {
        path: &["theme", "install"],
        summary: "Install a theme from a registry or Git URL",
        args: &[Arg {
            name: "SOURCE",
            required: true,
        }],
        options: &[],
        status: CLI_005,
    },
    Command {
        path: &["theme", "import"],
        summary: "Import a local .glint package",
        args: &[Arg {
            name: "PATH",
            required: true,
        }],
        options: &[],
        status: Status::Implemented,
    },
    Command {
        path: &["theme", "use"],
        summary: "Activate an installed theme (`default`: the bundled theme)",
        args: &[ID],
        options: &[],
        status: Status::Implemented,
    },
    Command {
        path: &["theme", "list"],
        summary: "List installed themes",
        args: &[],
        options: &[],
        status: Status::Implemented,
    },
    Command {
        path: &["theme", "update"],
        summary: "Update an installed theme",
        args: &[ID],
        options: &[],
        status: CLI_005,
    },
    Command {
        path: &["theme", "export"],
        summary: "Export an installed theme as a .glint package",
        args: &[ID],
        options: &[OUTPUT],
        status: Status::Implemented,
    },
    Command {
        path: &["theme", "remove"],
        summary: "Remove an installed theme",
        args: &[ID],
        options: &[],
        status: Status::Implemented,
    },
    Command {
        path: &["theme", "revert"],
        summary: "Return to the previously active theme",
        args: &[],
        options: &[],
        status: CLI_004,
    },
    Command {
        path: &["theme", "create"],
        summary: "Scaffold a new theme",
        args: &[Arg {
            name: "NAME",
            required: true,
        }],
        options: &[Opt {
            long: "template",
            short: None,
            value: "TEMPLATE",
        }],
        status: Status::Planned(Some("CRT-001")),
    },
    Command {
        path: &["theme", "dev"],
        summary: "Run the theme preview server",
        args: &[OPTIONAL_PATH],
        options: &[],
        status: Status::Planned(Some("CRT-005")),
    },
    Command {
        path: &["theme", "build"],
        summary: "Build per-surface entrypoints",
        args: &[OPTIONAL_PATH],
        options: &[],
        status: Status::Planned(Some("CRT-002")),
    },
    Command {
        path: &["theme", "validate"],
        summary: "Validate a theme directory or .glint package (default: .)",
        args: &[OPTIONAL_PATH],
        options: &[],
        status: Status::Implemented,
    },
    Command {
        path: &["theme", "pack"],
        summary: "Pack a theme directory into a .glint package",
        args: &[OPTIONAL_PATH],
        options: &[],
        status: Status::Planned(Some("CRT-004")),
    },
    Command {
        path: &["profile", "list"],
        summary: "List theme profiles",
        args: &[],
        options: &[],
        status: Status::Planned(None),
    },
    Command {
        path: &["profile", "use"],
        summary: "Activate a theme profile",
        args: &[ID],
        options: &[],
        status: Status::Planned(None),
    },
    Command {
        path: &["registry", "list"],
        summary: "List configured registries",
        args: &[],
        options: &[],
        status: CLI_005,
    },
    Command {
        path: &["registry", "add"],
        summary: "Add a registry",
        args: &[
            Arg {
                name: "NAME",
                required: true,
            },
            Arg {
                name: "URL",
                required: true,
            },
        ],
        options: &[],
        status: CLI_005,
    },
    Command {
        path: &["registry", "sync"],
        summary: "Refresh registry indexes",
        args: &[],
        options: &[],
        status: CLI_005,
    },
    Command {
        path: &["permissions", "list"],
        summary: "List capability grants",
        args: &[],
        options: &[],
        status: CLI_004,
    },
];

/// What the user asked for.
#[derive(Debug)]
pub enum Action {
    /// Print help for a command, a group (`theme`) or the whole tree.
    Help(Vec<String>),
    /// Print the version (`--version`, `-V`).
    Version,
    /// Run a command.
    Run(Invocation),
}

/// A parsed command call.
#[derive(Debug)]
pub struct Invocation {
    /// The selected command.
    pub command: &'static Command,
    /// Positional arguments in order.
    pub args: Vec<String>,
    /// Option values by long name.
    pub options: Vec<(&'static str, String)>,
}

impl Invocation {
    /// The value of option `long`, if given.
    pub fn option(&self, long: &str) -> Option<&str> {
        self.options
            .iter()
            .find(|(name, _)| *name == long)
            .map(|(_, value)| value.as_str())
    }
}

/// The parse result plus the global output mode, which is known even when
/// parsing fails so usage errors can be reported as JSON.
#[derive(Debug)]
pub struct Parsed {
    /// `--json` was given.
    pub json: bool,
    /// The command words that were recognised (for the JSON `command`
    /// field), or `None` when not even the first word was.
    pub command: Option<String>,
    /// The action, or a usage error.
    pub action: Result<Action, CliError>,
}

/// True when `--json` appears before any `--` terminator.
fn wants_json(args: &[String]) -> bool {
    args.iter()
        .take_while(|arg| arg.as_str() != "--")
        .any(|arg| arg == "--json")
}

/// Parse the arguments after the program name.
pub fn parse(args: &[String]) -> Parsed {
    let json = wants_json(args);
    let mut help = false;
    let mut version = false;
    let mut rest: Vec<&str> = Vec::new();
    let mut terminated = false;
    for arg in args {
        if terminated {
            rest.push(arg);
            continue;
        }
        match arg.as_str() {
            "--json" => {}
            "--help" | "-h" => help = true,
            "--version" | "-V" => version = true,
            "--" => {
                terminated = true;
                rest.push(arg);
            }
            other => rest.push(other),
        }
    }

    // Leading words that are not options select the command.
    let words: Vec<&str> = rest
        .iter()
        .take_while(|word| !word.starts_with('-'))
        .copied()
        .collect();

    if words.first() == Some(&"help") {
        let topic: Vec<String> = words[1..].iter().map(|w| (*w).to_owned()).collect();
        return Parsed {
            json,
            command: Some("help".to_owned()),
            action: resolve_help_topic(&topic).map(Action::Help),
        };
    }

    let Some(command) = longest_match(&words) else {
        let command = words.first().map(|w| (*w).to_owned());
        let action = if help || words.is_empty() && !version {
            // `glint`, `glint --help`, `glint theme --help`.
            resolve_help_topic(&words.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>())
                .map(Action::Help)
        } else if version && words.is_empty() {
            Ok(Action::Version)
        } else {
            Err(unknown_or_missing(&words))
        };
        return Parsed {
            json,
            command: command.filter(|w| GROUPS.iter().any(|(g, _)| g == w)),
            action,
        };
    };

    let name = command.name();
    if help {
        return Parsed {
            json,
            command: Some(name),
            action: Ok(Action::Help(
                command.path.iter().map(|w| (*w).to_owned()).collect(),
            )),
        };
    }
    if version {
        return Parsed {
            json,
            command: Some(name),
            action: Err(CliError::usage(format!(
                "`--version` cannot be combined with `{}`",
                command.name()
            ))),
        };
    }
    let action = parse_arguments(command, &rest[command.path.len()..]).map(Action::Run);
    Parsed {
        json,
        command: Some(name),
        action,
    }
}

/// The command whose path is the longest prefix of `words`.
fn longest_match(words: &[&str]) -> Option<&'static Command> {
    let command = COMMANDS
        .iter()
        .filter(|c| c.path.len() <= words.len() && c.path.iter().zip(words).all(|(a, b)| a == b))
        .max_by_key(|c| c.path.len())?;
    // `glint theme frobnicate`: the bare group command takes no positional
    // argument, so an extra word is an unknown subcommand, not an argument.
    if command.path.len() == 1
        && command.args.is_empty()
        && words.len() > 1
        && GROUPS.iter().any(|(g, _)| *g == command.path[0])
    {
        return None;
    }
    Some(command)
}

fn unknown_or_missing(words: &[&str]) -> CliError {
    match words {
        [group] if GROUPS.iter().any(|(g, _)| g == group) => CliError::new(
            ErrorKind::Usage,
            codes::MISSING_SUBCOMMAND,
            format!("`{group}` needs a subcommand; see `glint {group} --help`"),
        ),
        _ => {
            let shown = words.iter().take(2).copied().collect::<Vec<_>>().join(" ");
            CliError::new(
                ErrorKind::Usage,
                codes::UNKNOWN_COMMAND,
                format!("unknown command `{shown}`; see `glint --help`"),
            )
        }
    }
}

fn resolve_help_topic(topic: &[String]) -> Result<Vec<String>, CliError> {
    if topic.is_empty() {
        return Ok(Vec::new());
    }
    let is_command = COMMANDS
        .iter()
        .any(|c| c.path.iter().copied().eq(topic.iter().map(String::as_str)));
    let is_group = topic.len() == 1 && GROUPS.iter().any(|(g, _)| *g == topic[0]);
    if is_command || is_group {
        Ok(topic.to_vec())
    } else {
        let words: Vec<&str> = topic.iter().map(String::as_str).collect();
        Err(unknown_or_missing(&words))
    }
}

fn parse_arguments(command: &'static Command, rest: &[&str]) -> Result<Invocation, CliError> {
    let mut args = Vec::new();
    let mut options: Vec<(&'static str, String)> = Vec::new();
    let mut iter = rest.iter();
    let mut terminated = false;
    while let Some(&token) = iter.next() {
        if terminated || !token.starts_with('-') || token == "-" {
            args.push(token.to_owned());
            continue;
        }
        if token == "--" {
            terminated = true;
            continue;
        }
        let (flag, inline) = match token.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => (flag, Some(value)),
            _ => (token, None),
        };
        let opt = command
            .options
            .iter()
            .find(|o| {
                flag.strip_prefix("--") == Some(o.long)
                    || o.short.is_some_and(|s| {
                        flag.len() == 2 && flag.ends_with(s) && !flag.starts_with("--")
                    })
            })
            .ok_or_else(|| {
                CliError::usage(format!(
                    "unknown option `{flag}` for `glint {}`; usage: {}",
                    command.name(),
                    command.usage()
                ))
            })?;
        let value = match inline {
            Some(value) => value.to_owned(),
            None => iter.next().map(|v| (*v).to_owned()).ok_or_else(|| {
                CliError::usage(format!("option `{flag}` needs a {} value", opt.value))
            })?,
        };
        if options.iter().any(|(name, _)| *name == opt.long) {
            return Err(CliError::usage(format!(
                "option `--{}` given twice",
                opt.long
            )));
        }
        options.push((opt.long, value));
    }

    let required = command.args.iter().filter(|a| a.required).count();
    if args.len() < required {
        let missing = &command.args[args.len()];
        return Err(CliError::usage(format!(
            "missing {} argument; usage: {}",
            missing.name,
            command.usage()
        )));
    }
    if args.len() > command.args.len() {
        return Err(CliError::usage(format!(
            "unexpected argument `{}`; usage: {}",
            args[command.args.len()],
            command.usage()
        )));
    }
    Ok(Invocation {
        command,
        args,
        options,
    })
}

#[cfg(test)]
mod tests {
    use super::{Action, COMMANDS, GROUPS, parse};
    use crate::error::{ErrorKind, codes};

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    fn run(line: &str) -> super::Invocation {
        match parse(&args(line)).action {
            Ok(Action::Run(invocation)) => invocation,
            other => panic!("`{line}` did not parse to a command: {other:?}"),
        }
    }

    fn usage_code(line: &str) -> String {
        match parse(&args(line)).action {
            Err(error) => {
                assert_eq!(error.kind, ErrorKind::Usage, "{line}");
                error.code
            }
            other => panic!("`{line}` should be a usage error: {other:?}"),
        }
    }

    #[test]
    fn command_paths_are_unique_and_grouped() {
        for (i, a) in COMMANDS.iter().enumerate() {
            assert!(
                COMMANDS[i + 1..].iter().all(|b| a.path != b.path),
                "{}",
                a.name()
            );
            if a.path.len() > 1 {
                assert!(GROUPS.iter().any(|(g, _)| *g == a.path[0]), "{}", a.name());
            }
            let required_after_optional = a
                .args
                .windows(2)
                .any(|pair| !pair[0].required && pair[1].required);
            assert!(!required_after_optional, "{}", a.name());
        }
    }

    #[test]
    fn selects_the_longest_command_and_collects_arguments() {
        let invocation = run("theme export dev.example.midnight -o out.glint");
        assert_eq!(invocation.command.name(), "theme export");
        assert_eq!(invocation.args, ["dev.example.midnight"]);
        assert_eq!(invocation.option("output"), Some("out.glint"));

        let invocation = run("theme export dev.example.midnight --output=out.glint");
        assert_eq!(invocation.option("output"), Some("out.glint"));

        assert_eq!(run("theme").command.name(), "theme");
        assert_eq!(run("theme validate").args, Vec::<String>::new());
        assert_eq!(run("theme validate -- -odd").args, ["-odd"]);
    }

    #[test]
    fn json_flag_is_global_and_known_even_on_usage_errors() {
        let parsed = parse(&args("theme --json frobnicate"));
        assert!(parsed.json);
        assert!(parsed.action.is_err());
        assert!(!parse(&args("theme validate -- --json")).json);
    }

    #[test]
    fn malformed_calls_are_usage_errors() {
        assert_eq!(usage_code("frobnicate"), codes::UNKNOWN_COMMAND);
        assert_eq!(usage_code("theme frobnicate"), codes::UNKNOWN_COMMAND);
        assert_eq!(usage_code("registry"), codes::MISSING_SUBCOMMAND);
        assert_eq!(usage_code("theme import"), codes::USAGE);
        assert_eq!(usage_code("theme list extra"), codes::USAGE);
        assert_eq!(usage_code("theme list --bogus"), codes::USAGE);
        assert_eq!(usage_code("theme export x -o"), codes::USAGE);
        assert_eq!(usage_code("theme export x -o a -o b"), codes::USAGE);
        assert_eq!(usage_code("theme list --version"), codes::USAGE);
        assert_eq!(usage_code("help frobnicate"), codes::UNKNOWN_COMMAND);
    }

    #[test]
    fn help_and_version() {
        assert!(matches!(parse(&[]).action, Ok(Action::Help(ref t)) if t.is_empty()));
        assert!(matches!(
            parse(&args("--version")).action,
            Ok(Action::Version)
        ));
        assert!(matches!(parse(&args("-V")).action, Ok(Action::Version)));
        assert!(
            matches!(parse(&args("theme --help")).action, Ok(Action::Help(ref t)) if t == &["theme"])
        );
        assert!(
            matches!(parse(&args("registry -h")).action, Ok(Action::Help(ref t)) if t == &["registry"])
        );
        assert!(
            matches!(parse(&args("help theme import")).action, Ok(Action::Help(ref t)) if t.len() == 2)
        );
        assert!(matches!(
            parse(&args("theme import --help")).action,
            Ok(Action::Help(_))
        ));
    }
}
