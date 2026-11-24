# `glint` CLI reference

Status: **proposed** (CLI-001, CLI-002, CLI-003, CLI-006). Command and flag names follow
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
listed, `-n VALUE`. Boolean flags (`--include-sensitive`, `--yes`) take no
value; each may be given once. Arguments must be valid Unicode.

## Commands

"Not implemented" commands parse their arguments (a malformed call is still
a usage error, exit 2) and then fail with `unsupported` (exit 3) and code
`GLINT_CLI_NOT_IMPLEMENTED`. They never change anything and never report
success. `details.plannedIn` names the owning ticket.

| Command | Status | Planned in |
|---|---|---|
| `glint version` | implemented | - |
| `glint doctor [-o PATH] [--include-sensitive] [--yes]` | implemented (redacted by default) | - |
| `glint status [--include-sensitive] [--yes]` | implemented (redacted by default) | - |
| `glint safe-mode` | implemented | - |
| `glint theme` | interactive chooser, terminal only; not implemented | not scheduled |
| `glint theme browse` | not implemented | CLI-005 |
| `glint theme search QUERY` | not implemented | CLI-005 |
| `glint theme info ID` | not implemented | CLI-004 |
| `glint theme install SOURCE` | not implemented | CLI-005 |
| `glint theme import PATH` | implemented | - |
| `glint theme use ID` | implemented (`ID` may be `default`) | - |
| `glint theme list` | implemented | - |
| `glint theme update ID` | not implemented | CLI-005 |
| `glint theme export [-o PATH] ID` | implemented | - |
| `glint theme remove ID` | implemented | - |
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

### Theme state commands (CLI-002, CLI-006)

`theme import`, `list`, `export`, `remove`, `use` and `safe-mode` work on
Glint's own state: the content-addressed store, the state database and the
active-theme record under the Glint home (`GLINT_HOME` when set, otherwise
the OS directories; see `glint-storage`). They never change a native OS
setting and never run a package script.

While a Glint core runs it is the single writer of that state (it holds the
installer lock), so these commands go through it (see
[Routing through the core](#routing-through-the-core-cli-006)). With no core
running they open the installer themselves, which takes the exclusive
installer lock and first recovers interrupted operations and a corrupt
database. Both paths run the same installer operations and print the same
`data`, apart from `route`.

### Routing through the core (CLI-006)

Before touching state, a theme state command looks for a running core: it
reads the endpoint record `<data>/ipc.endpoint`, connects with the typed IPC
client (`glint-ipc`, API version 1, the same API the manager uses) and
probes the per-user single-instance lock. The decision never silently
diverges and never mutates behind a running core:

| Endpoint record | Connect | Instance lock | Route |
|---|---|---|---|
| present | ok | - | `daemon`: the core runs the operation on its own installer |
| present | nothing listens (stale record after a crash) | free | `offline` |
| present | nothing listens | held | exit 6 |
| absent or unusable | - | free | `offline` |
| absent or unusable | - | held (a core that is booting or not serving IPC) | exit 6, `GLINT_CLI_DAEMON_UNAVAILABLE` |
| - | no answer in time | - | exit 8, `GLINT_IPC_TIMEOUT` |
| - | access refused | - | exit 5, `GLINT_IPC_PERMISSION_DENIED` |
| - | busy, incompatible version, shutting down | - | exit 6 with the `GLINT_IPC_*` code |

A core that starts between the decision and the offline installer opening
is also exit 6 (`GLINT_CLI_DAEMON_UNAVAILABLE`, `details.installCode:
GLINT_INSTALL_BUSY`), so a CLI never reports the lock as a plain failure.
Every result reports where it ran: `data.route` (`daemon` or `offline`) on
success, `error.details.route` on failure.

The IPC requests are `theme-list`, `theme-import`, `theme-use`,
`theme-remove`, `theme-export` and `safe-mode`. Paths are made absolute
against the CLI's working directory before they are sent (the core's working
directory differs). `theme export` without `-o` asks the core to write a
temporary file in the working directory and renames it to
`./<id>-<version>.glint`. A failed operation keeps the component's code in
`error.code` (`GLINT_INSTALL_*`, `GLINT_IDENTITY_*`, `GLINT_PACKAGE_*`,
`GLINT_SOURCE_*`, ...) with the same exit codes as offline, plus
`details.ipcCode: GLINT_IPC_OPERATION_FAILED`. A core that does not perform
operations (`GLINT_IPC_UNKNOWN_REQUEST`, `GLINT_IPC_OPERATION_UNSUPPORTED`)
is exit 3.

Deadlines: 5 s for the handshake and `theme list`, 90 s for operations (the
core gives up after 60 s and answers `GLINT_IPC_TIMEOUT` itself).
`GLINT_IPC_TIMEOUT_MS` overrides every deadline in milliseconds. A timed-out
operation may still complete in the core; run `glint theme list` to see the
outcome.

`theme validate` never routes: it only reads the given package through the
libraries.

**Theme IDs.** `ID` is a package ID (`dev.example.midnight`), `ID@VERSION`,
a digest (64 hex digits, optionally `sha256:`-prefixed) or `default` for the
bundled default theme (`official.default`). An ID with several installed
versions is `GLINT_INSTALL_AMBIGUOUS` (exit 2) with `details.candidates`; an
unknown one is `GLINT_INSTALL_NOT_INSTALLED` (exit 7).

**Bundled default theme.** The binary embeds `themes/default` as a `.glint`
archive packed at build time by `apps/cli/build.rs` with the deterministic
`glint-package` writer (a unit test checks the embedded bytes equal a fresh
pack). `theme use default` and `safe-mode` install it from those bytes when
it is missing and repair its store entry when it is damaged, so they work on
an empty store, a damaged entry and a recovered database. Its ID is
reserved: importing another package named `official.default` is
`GLINT_INSTALL_RESERVED_ID` (exit 1); re-importing the exported default
theme is a no-op.

| Command | Does | `data` shape (`$defs/data/...`) |
|---|---|---|
| `glint theme import PATH` | Stages a `.glint` (or a theme directory, packed deterministically) through the strict reader and installs it **without activating**. A rejected package changes nothing: the inventory and the active theme are kept and staging is cleaned. Importing an installed digest is a no-op (`alreadyInstalled: true`) | `themeImport` |
| `glint theme list` | Every installed release (sorted by ID, version, digest) and the active profile | `themeList` |
| `glint theme export [-o PATH] ID` | Writes the release to `PATH` (default `./<id>-<version>.glint`) without local data (PKG-005): only the package's own files, re-packed deterministically; no provenance, paths, timestamps, profiles or grants. `canonical: true` means byte-identical to the installed archive, so re-importing it yields the same digest | `themeExport` |
| `glint theme remove ID` | Removes a release no profile uses; one in use is `GLINT_INSTALL_IN_USE` (exit 1) with `details.profiles` | `themeRemove` |
| `glint theme use ID` | Makes the release the base theme of the active profile, or of the `default` profile when none is active or safe mode is on | `themeActivation` |
| `glint safe-mode` | Activates the bundled default theme in the separate `safe-mode` profile; other profiles keep their themes | `themeActivation` |

Every shape also has `route` (`daemon` or `offline`).
`themePackage` (in `themeImport`, `themeList` and `themeActivation`) has
`id`, `version`, `digest`, `origin` (ADR 0006: `local`, ...), `source.kind`
and `source.locator` (the bundled theme's locator is
`bundled:official.default`), `size`, `installedAt` (Unix seconds),
`bundled` and `active`. `active` and `previous` are `{profile, digest,
revision}` or `null`.

Installer failures report the installer code in `details.installCode` and
the most specific code in `error.code` (`GLINT_IDENTITY_*` for identity
conflicts, `GLINT_PACKAGE_*` for export failures, `GLINT_STORAGE_*` for the
database). Exit codes: not installed 7, ambiguous ID or unusable `-o` path
2, permission denied 5, everything else 1. Package verdicts from `import`
use the same codes as `theme validate`.

### `glint doctor` and `glint status` (CLI-003)

Read-only diagnostics. They never change state: the core is asked over IPC
with a read (`get-snapshot`), the state database is opened read-only and
only when it exists (`PRAGMA quick_check`, `user_version`, row counts), and
the installer is never opened, so no recovery runs and the installer lock is
never taken. Both exit 0 whatever they find (a broken installation is what
they report); exit 2 for usage, 1 when the support bundle cannot be written.

`glint status` (`data` shape `status`): the core (`core.state` is `running`,
`not-running` or `unreachable`, with pid, lifecycle, sequence, server and
API version, or the `GLINT_IPC_*` error), the active profile (`theme.active`,
from the database or the active-theme record), the theme the core presents
(`theme.presented`, with its fallback reason), the installed count and the
number of core warnings.

`glint doctor` (`data` shape `doctor`) adds `checks` (`paths`, `database`,
`recovery`, `core`, `capabilities`, `theme`, each `ok`/`warn`/`fail`) and
`overall`; `system` (OS, family, architecture, session type reduced to a
fixed vocabulary, compositor, Wayland detection); `versions` (Glint, CLI
output schema, IPC API range, the core's server name, database, manifest
and entrypoint schema versions); `paths` (config, data and cache roots:
exists, real directory); `storage` (database present/readable, schema
version against the latest, quick check, active record, packages, profiles
and pins, i.e. install locks); `installer` recovery state (journalled
installs and removals, interrupted applies, staging and trash entries);
`core`; `capabilities` (adapter, generation, advertised, usable, discovery
error; from the core); `renderer` (the shipped core's headless backend);
`protocols` (IPC transport and API range; Wayland protocols not checked
yet); `theme`; the core's `warnings`; and `privacy`.

**Redaction by default.** Two layers:

1. Never collected, so never printed: Wi-Fi SSIDs, window titles, access
   tokens, environment variables (only `SESSIONNAME`, `XDG_SESSION_TYPE`,
   `XDG_CURRENT_DESKTOP` and the presence of `WAYLAND_DISPLAY` are read, and
   reduced to a fixed vocabulary or a short identifier) and package
   contents. `privacy.notCollected` lists them.
2. Replaced in every string of the JSON, the human text and the support
   bundle, longest match first and case-insensitively, with either slash:
   the Glint roots (`<glint-config>`, `<glint-data>`, `<glint-cache>`,
   `GLINT_HOME` as `<glint-home>`), the home directory (`<home>`, from
   `USERPROFILE`/`HOME`), the user name (`<user>`, from
   `USERNAME`/`USER`/`LOGNAME`) and the computer name (`<host>`, from
   `COMPUTERNAME`/`HOSTNAME`). A value shorter than three characters is not
   replaced (it would mangle unrelated text) and is named in
   `privacy.notRedacted`.

`--include-sensitive` skips layer 2 (and `status` then reports `dataDir`).
It needs confirmation: `--yes`, or answering `y` to a prompt on stderr when
stdin and stdout are terminals and `--json` is absent. Without either it is
a usage error, `GLINT_CLI_CONFIRMATION_REQUIRED` (exit 2); `--yes` without
`--include-sensitive` is `GLINT_CLI_USAGE`. Declining the prompt prints the
redacted report. Layer 1 always applies.

**Support bundle.** `glint doctor -o PATH` also writes
`{"kind": "glint-support-bundle", "bundleVersion": 1, "glintVersion", "report"}`
to `PATH`, where `report` is exactly the `data` printed (redacted unless
sensitive output was confirmed); `data.bundle.path` names it.

### `glint theme validate [PATH]`

Validates a theme directory or a `.glint` archive (default: the current
directory) through the same pipeline the installer uses: `glint-sources`
`LocalSource` packs a directory deterministically and hands the archive to
the strict `glint-package` reader, which checks the archive structure, the
`glint.json` manifest (`glint-schema`), capability IDs, surface definitions
(`glint-surfaces`), referenced files and surface entrypoints, executable
content, remote references and active SVG content. The private staging copy is
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
(`GLINT_MANIFEST_*` or `GLINT_SURFACE_*` over `GLINT_PACKAGE_*` over
`GLINT_SOURCE_*`) and the whole chain in `error.details` (`sourceCode`,
`packageCode`, `manifestCode` or `surfaceCode`, `pointer`).

## Exit codes

| Code | Kind | Meaning |
|---|---|---|
| 0 | `ok` | The command did what it reports |
| 1 | `failure` | Anything else: I/O error, internal error, unusable staging directory |
| 2 | `usage` | Unknown command or option, missing or extra argument, interactive mode without a terminal |
| 3 | `unsupported` | Recognised but not implemented or not supported here |
| 4 | `invalid-package` | A theme package or manifest failed validation |
| 5 | `permission-denied` | The operating system refused access |
| 6 | `daemon-unavailable` | A Glint core runs (or is shutting down) but cannot be asked: not serving IPC yet, busy, incompatible; nothing was changed |
| 7 | `not-found` | A named file, directory, theme or profile does not exist |
| 8 | `timeout` | The core did not answer in time (`GLINT_IPC_TIMEOUT`) |

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
  "command": "permissions list",
  "ok": false,
  "exitCode": 3,
  "error": {
    "kind": "unsupported",
    "code": "GLINT_CLI_NOT_IMPLEMENTED",
    "message": "`glint permissions list` is not implemented yet (planned in CLI-004); nothing was changed",
    "details": { "plannedIn": "CLI-004" }
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
`GLINT_CLI_MISSING_SUBCOMMAND`, `GLINT_CLI_NOT_IMPLEMENTED`, `GLINT_CLI_IO`,
`GLINT_CLI_DAEMON_UNAVAILABLE`, `GLINT_CLI_CONFIRMATION_REQUIRED`. Errors from the core keep their
`GLINT_IPC_*` code (`details.ipcCode`, `details.ipcCategory`).

The schema also defines the documented per-command `data` shapes under
`$defs/data` (`themeImport`, `themeList`, `themeExport`, `themeRemove`,
`themeActivation`, `status`, `doctor`, with the shared `coreState` and
`presentedTheme`); the envelope's `data` stays a plain object, and tests
validate each command's `data` against its shape.

## Tests

- `cargo test -p glint-cli --test exit_codes` runs the binary for every
  reachable failure class and validates each `--json` document against the
  schema.
- Unit tests in `apps/cli/src` cover the parser, the interactive-mode rule
  and the mapping of typed source and installer errors (including
  `permission-denied`) to exit codes.
- `cargo test -p glint-cli --test cli_roundtrip` runs import, list, export,
  re-import, remove, `theme use` and `safe-mode` with a temporary
  `GLINT_HOME` (including an invalid archive that must leave the active
  theme unchanged, a damaged store entry and a corrupt database) and
  validates every `--json` document and its `data` shape.
- `cargo test -p glint-cli --test ipc_client` starts a real `glint-daemon`
  with a temporary `GLINT_HOME` (building it once if the workspace has not)
  and checks that import, use, list, export, remove and safe mode go
  through it (`route: daemon`, no `GLINT_INSTALL_BUSY`), that the change is
  visible to a second IPC client (a subscription delta and a fresh read),
  that the same state is read offline once the core stopped, that a core
  holding the instance lock without IPC is exit 6 and changes nothing, that
  a stale endpoint record is ignored and that a slow operation is exit 8.
- `cargo test -p glint-cli --test doctor` runs `doctor` and `status`
  offline and against a real `glint-daemon`, with a `GLINT_HOME`, home
  directory, user name, computer name and token-like environment values
  made up for the test, and checks that default JSON, text and the support
  bundle contain none of them, that `--include-sensitive` needs `--yes`,
  and that every document matches its `data` shape.
