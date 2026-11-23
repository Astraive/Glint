# `glint` CLI reference

Status: **proposed** (CLI-001). Command and flag names follow
[CLI and developer experience](../plan/06-engineering/cli-and-developer-experience.md#user-cli-proposed)
and stay proposed until this page marks them stable. The exit codes and the
JSON envelope below are the contract scripts may rely on; they only ever
grow, they are never renumbered or reused.

The binary lives in `apps/cli` (package `glint-cli`, binary `glint`). It is
thin: it parses arguments, calls the library crates and renders results.

## Global flags

| Flag | Meaning |
|---|---|
| `--json` | Print exactly one JSON document on stdout (see [JSON output](#json-output)); stderr stays empty. Accepted anywhere before `--`. |
| `-h`, `--help` | Help for the whole tree, a group (`glint theme --help`) or one command. `glint help [COMMAND...]` is the same. |
| `-V`, `--version` | Print the version. Same as `glint version`. |
| `--` | Everything after it is a positional argument, so paths may start with `-`. |

Options that take a value accept `--name VALUE`, `--name=VALUE` and, where
listed, `-n VALUE`. Arguments must be valid Unicode.

## Commands

"Not implemented" commands parse their arguments (a malformed call is still
a usage error, exit 2) and then fail with `unsupported` (exit 3) and code
`GLINT_CLI_NOT_IMPLEMENTED`. They never change anything and never report
success. `details.plannedIn` names the owning ticket.

| Command | Status | Planned in |
|---|---|---|
| `glint version` | implemented | - |
| `glint doctor` | not implemented | CLI-003 |
| `glint status` | not implemented | CLI-003 |
| `glint safe-mode` | not implemented | CLI-002 |
| `glint theme` | interactive chooser, terminal only; not implemented | CLI-002 |
| `glint theme browse` | not implemented | CLI-005 |
| `glint theme search QUERY` | not implemented | CLI-005 |
| `glint theme info ID` | not implemented | CLI-004 |
| `glint theme install SOURCE` | not implemented | CLI-005 |
| `glint theme import PATH` | not implemented | CLI-002 |
| `glint theme use ID` | not implemented | CLI-002 |
| `glint theme list` | not implemented | CLI-002 |
| `glint theme update ID` | not implemented | CLI-005 |
| `glint theme export [-o PATH] ID` | not implemented | CLI-002 |
| `glint theme remove ID` | not implemented | CLI-002 |
| `glint theme revert` | not implemented | CLI-004 |
| `glint theme create [--template TEMPLATE] NAME` | not implemented | CRT-001 |
| `glint theme dev [PATH]` | not implemented | CRT-005 |
| `glint theme build [PATH]` | not implemented | CRT-002 |
| `glint theme validate [PATH]` | implemented (package checks only) | CRT-003 extends it |
| `glint theme pack [PATH]` | not implemented | CRT-004 |
| `glint profile list` | not implemented | not scheduled |
| `glint profile use ID` | not implemented | not scheduled |
| `glint registry list` | not implemented | CLI-005 |
| `glint registry add NAME URL` | not implemented | CLI-005 |
| `glint registry sync` | not implemented | CLI-005 |
| `glint permissions list` | not implemented | CLI-004 |

### Interactive mode

Only the bare `glint theme` command is interactive, and only when both stdin
and stdout are terminals and `--json` is absent. Otherwise it is a usage
error (exit 2, `GLINT_CLI_MISSING_SUBCOMMAND`), so scripts never block on a
prompt. On a terminal the chooser is recognised but not built yet (exit 3).

### `glint theme validate [PATH]`

Validates a theme directory or a `.glint` archive (default: the current
directory) through the same pipeline the installer uses: `glint-sources`
`LocalSource` packs a directory deterministically and hands the archive to
the strict `glint-package` reader, which checks the archive structure, the
`glint.json` manifest (`glint-schema`), capability IDs, referenced files,
executable content and remote references. The private staging copy is
removed afterwards; nothing is installed and no package script runs.

Not checked yet (CRT-003): host version compatibility, surface build output
and forbidden APIs. `data.notChecked` lists them.

`data` on success:

| Field | Meaning |
|---|---|
| `valid` | Always `true` on success |
| `source.kind`, `source.path` | `local-directory` or `local-archive`, canonical path |
| `package.*` | `id`, `name`, `version`, `license`, `glint` requirement, `platforms`, `surfaces`, `requiredCapabilities`, `optionalCapabilities` |
| `archive.*` | `sha256` (of the validated archive; for a directory, of the deterministic pack), `size`, `files`, `totalUncompressed` |
| `checked`, `notChecked` | Check names that ran and that are deferred |

Failures keep the most specific library code in `error.code`
(`GLINT_MANIFEST_*` over `GLINT_PACKAGE_*` over `GLINT_SOURCE_*`) and the
whole chain in `error.details` (`sourceCode`, `packageCode`, `manifestCode`,
`pointer`).

## Exit codes

| Code | Kind | Meaning |
|---|---|---|
| 0 | `ok` | The command did what it reports |
| 1 | `failure` | Anything else: I/O error, internal error, unusable staging directory |
| 2 | `usage` | Unknown command or option, missing or extra argument, interactive mode without a terminal |
| 3 | `unsupported` | Recognised but not implemented or not supported here |
| 4 | `invalid-package` | A theme package or manifest failed validation |
| 5 | `permission-denied` | The operating system refused access |
| 6 | `daemon-unavailable` | The command needs the Glint daemon and it is not reachable (reserved: no daemon exists yet) |
| 7 | `not-found` | A named file, directory, theme or profile does not exist |
| 8 | `timeout` | An operation did not finish in time (reserved) |

`glint --help` prints this table and `glint --help --json` returns it as
`data.exitCodes`.

## JSON output

With `--json`, stdout holds one document matching
[`schemas/glint-cli-output-v1.schema.json`](../../schemas/glint-cli-output-v1.schema.json)
for success and failure alike:

```json
{
  "schemaVersion": 1,
  "glintVersion": "0.0.0",
  "command": "theme list",
  "ok": false,
  "exitCode": 3,
  "error": {
    "kind": "unsupported",
    "code": "GLINT_CLI_NOT_IMPLEMENTED",
    "message": "`glint theme list` is not implemented yet (planned in CLI-002); nothing was changed",
    "details": { "plannedIn": "CLI-002" }
  }
}
```

| Field | Meaning |
|---|---|
| `schemaVersion` | Envelope version, `1`. Fields are only added within a version |
| `glintVersion` | Version of the binary |
| `command` | Recognised command words, or `null` when the first word was not recognised |
| `ok` | `true` exactly when `exitCode` is `0` |
| `exitCode` | The process exit code |
| `data` | Success only: the command's payload (object) |
| `error` | Failure only: `kind` (the exit code's kind), `code` (stable `GLINT_*` code), `message` (human text, not a contract) and `details` (object) |

Keys may appear in any order. Match on `exitCode`, `error.kind` and
`error.code`, never on `message`.

CLI-level codes: `GLINT_CLI_USAGE`, `GLINT_CLI_UNKNOWN_COMMAND`,
`GLINT_CLI_MISSING_SUBCOMMAND`, `GLINT_CLI_NOT_IMPLEMENTED`, `GLINT_CLI_IO`.

## Tests

- `cargo test -p glint-cli --test exit_codes` runs the binary for every
  reachable failure class and validates each `--json` document against the
  schema.
- Unit tests in `apps/cli/src` cover the parser, the interactive-mode rule
  and the mapping of typed source errors (including `permission-denied`) to
  exit codes.
