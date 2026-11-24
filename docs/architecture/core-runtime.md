# Core runtime: one process, separate crates

Status: **implemented for the headless MVP** (COR-001). This page records how
the core process is put together, what the combined process costs in trust,
and when it must be split. It follows [ADR 0001](../adr/0001-single-process-mvp-with-separate-crates.md)
and the plan's [process table](../plan/02-architecture/system-architecture.md#processes-recommended);
the plan's multi-process design stays the target (COR-007).

## Crates and the binary

| Piece | Role | Depends on |
|---|---|---|
| `crates/glint-daemon` | State authority: the only holder of the `Installer`, and with it the only read-write handle to the state database and the package store. Opens state with startup recovery, resolves config, discovers capabilities, selects the theme | storage, installer, schema, platform, surfaces |
| `crates/glint-shell` | Surface presentation: opens the selected surfaces through a `SurfaceBackend`, closes them and releases work-area reservations | surfaces only |
| `crates/glint-core` | Lifecycle and orchestration: `Runtime` drives both services through their traits, emits typed events, owns shutdown | daemon, shell, storage (paths) |
| `apps/daemon` (binary `glint-daemon`) | Thin host: arguments, stdin control channel, exit codes | core |

`glint-daemon` and `glint-shell` do not depend on each other or on
`glint-core`, so the graph has no cycle (Cargo would refuse one, and
`daemon_and_shell_crates_have_no_dependency_on_each_other` in
`crates/glint-core/tests/lifecycle.rs` pins it). The shell has no storage or
installer dependency at all (`the_shell_cannot_reach_profile_or_package_state`).

## Service contracts

`DaemonService` (one call per boot stage, plus `status` and `shutdown`) and
`ShellService` (`present`, `status`, `close_all`) take and return owned
data: `GlintPaths`, reports, a `ThemeSelection`, a `PresentRequest`. No
database connection, file handle or installer reference ever crosses them.
In the MVP the runtime holds the implementations directly. After the split
an IPC client implements each trait and the runtime is unchanged: the
transport changes, the semantics do not. Errors keep their stable codes
across the contract (`GLINT_DAEMON_*`, `GLINT_SHELL_*`, and the wrapped
`GLINT_INSTALL_*` or `GLINT_STORAGE_*` code in `CoreError::source_code`).

## Boot sequence

`Runtime::start` walks these stages in order. Each emits a
`LifecycleEvent::StateChanged` into `starting:<stage>` and a
`StageCompleted` summary:

1. `resolve-paths`: `GlintPaths` from `GLINT_HOME` or the OS; Glint's
   directories created (ADR 0007).
2. `open-state`: the daemon opens the database (migrating, or quarantining a
   corrupt one), takes the installer lock and runs `recover_pending`.
3. `resolve-config`: bundled defaults plus the user `config.toml`. A
   rejected user config is a warning and the defaults apply.
4. `discover-capabilities`: through the daemon's `CapabilityAdapter`. Until
   the native adapters exist the headless adapter advertises nothing, so
   nothing is claimed that the host cannot provide. A failed discovery is a
   warning and every capability is unusable (fail closed).
5. `select-theme`: the active profile's installed release, read from the
   store and passed through the capability gate (`evaluate_manifest`), or
   the bundled default theme (`themes/default/glint.json`, embedded in the
   binary) with a typed `FallbackReason`: `no-active-theme`,
   `not-installed`, `manifest-unusable` or `capability-refused`. With
   `shell.use_default_fallback = false` the boot fails with
   `GLINT_DAEMON_FALLBACK_DISABLED` instead of presenting anything.
   Surfaces whose required capability is unusable are left out and listed.
6. `start-shell`: the shell presents the remaining surfaces. A surface the
   backend cannot open is a warning; the rest still open.

Degraded outcomes are `LifecycleEvent::Warning`s with stable
`GLINT_CORE_WARN_*` codes (recovered database, recovery work, journalled
apply, rejected config, config diagnostics, capabilities unavailable, theme
fallback, surface skipped, surface failed). A failed stage closes the shell
and the daemon, sets `failed:<stage>` and returns a `CoreError` with the
stage, a category (unsupported, permission-denied, unavailable, timeout,
failure) and the component's own code. An interrupted apply found at
startup stays journalled: rolling it back needs the native adapters that
made the change.

The runtime never modifies the OS shell. Only a native `SurfaceBackend`
could, and the headless backend used today opens no window and reserves no
work area.

## Shutdown

`Runtime::shutdown` (or dropping a ready runtime) runs `close-shell` (every
surface in reverse order, every work-area reservation released) and then
`flush-state` (the active-theme record mirrored, the database closed, the
installer lock released). Every step runs even when an earlier one failed;
failures are listed in `ShutdownReport::errors`. All state writes already
commit with `synchronous = FULL`, so there is no buffered state to lose.

## Trust trade-off of the combined process

Running the daemon and the shell in one process means:

- **One failure domain.** A crash in surface hosting (a native window loop,
  a webview host) takes the state authority down with it. Installed state
  stays consistent because every write is a committed, journalled
  transaction and startup recovery repairs interrupted work, but the
  desktop loses its Glint surfaces until the process restarts.
- **One privilege set.** Surface hosting runs with the daemon's access to
  the database, the package store and the OS adapters. Theme JavaScript
  itself still runs in the webview engine's own renderer processes, so a
  renderer bug does not reach the core directly; an engine sandbox escape
  does (threat-model rows B4.7 and R1).
- **What still holds.** The single-writer rule is structural, not a
  convention: the shell crate cannot name the storage or installer types.
  Shell inputs are plain data. The single-instance lock (COR-002) keeps a
  second core from opening the same database.

## When to split (COR-007)

Split the shell into its own process, with its own ADR, when any of these
holds:

- a native surface host needs an event loop that cannot share a thread or
  process with the daemon (GTK main loop, AppKit main thread), or
- a renderer or host crash is observed to take the state authority down in
  testing, or
- the shell needs a lower-privilege token or sandbox than the daemon
  (RND-008), or
- the manager or CLI must keep talking to the daemon while surfaces
  restart.

The split replaces the in-process trait calls with IPC clients over the
transport COR-003 defines; the contracts above do not change.
