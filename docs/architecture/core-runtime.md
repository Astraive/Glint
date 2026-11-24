# Core runtime: one process, separate crates

Status: **implemented for the headless MVP** (COR-001, COR-002, COR-003). This page records how
the core process is put together, what the combined process costs in trust,
and when it must be split. It follows [ADR 0001](../adr/0001-single-process-mvp-with-separate-crates.md)
and the plan's [process table](../plan/02-architecture/system-architecture.md#processes-recommended);
the plan's multi-process design stays the target (COR-007).

## Crates and the binary

| Piece | Role | Depends on |
|---|---|---|
| `crates/glint-daemon` | State authority: the only holder of the `Installer`, and with it the only read-write handle to the state database and the package store. Opens state with startup recovery, resolves config, discovers capabilities, selects the theme | storage, installer, schema, platform, surfaces |
| `crates/glint-shell` | Surface presentation: opens the selected surfaces through a `SurfaceBackend`, closes them and releases work-area reservations | surfaces only |
| `crates/glint-core` | Lifecycle and orchestration: `Runtime` drives both services through their traits, emits typed events, owns shutdown; `CoreIpc` publishes its state over local IPC | daemon, shell, storage (paths), ipc |
| `crates/glint-ipc` | Local IPC (COR-003, [ADR 0010](../adr/0010-interprocess-for-local-ipc.md)): versioned envelope, snapshot hub, server, client, named pipe / Unix socket endpoint | storage (paths), `interprocess` |
| `apps/daemon` (binary `glint-daemon`) | Thin host: arguments, stdin control channel, IPC wiring, exit codes | core |

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
2. `acquire-instance`: the per-user single-instance lock (see
   [single instance](#single-instance)), before anything opens the
   database.
3. `open-state`: the daemon opens the database (migrating, or quarantining a
   corrupt one), takes the installer lock and runs `recover_pending`.
4. `resolve-config`: bundled defaults plus the user `config.toml`. A
   rejected user config is a warning and the defaults apply.
5. `discover-capabilities`: through the daemon's `CapabilityAdapter`. Until
   the native adapters exist the headless adapter advertises nothing, so
   nothing is claimed that the host cannot provide. A failed discovery is a
   warning and every capability is unusable (fail closed).
6. `select-theme`: the active profile's installed release, read from the
   store and passed through the capability gate (`evaluate_manifest`), or
   the bundled default theme (`themes/default/glint.json`, embedded in the
   binary) with a typed `FallbackReason`: `no-active-theme`,
   `not-installed`, `manifest-unusable` or `capability-refused`. With
   `shell.use_default_fallback = false` the boot fails with
   `GLINT_DAEMON_FALLBACK_DISABLED` instead of presenting anything.
   Surfaces whose required capability is unusable are left out and listed.
7. `start-shell`: the shell presents the remaining surfaces. A surface the
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
surface in reverse order, every work-area reservation released),
`flush-state` (the active-theme record mirrored, the database closed, the
installer lock released) and `release-instance` (owner record removed, then
the instance lock released, so a new instance can only start once state is
closed). Every step runs even when an earlier one failed; failures are
listed in `ShutdownReport::errors`. A failed boot releases the same
resources. All state writes already commit with `synchronous = FULL`, so
there is no buffered state to lose.

Shutdown triggers converge on a `ShutdownSignal` (first reason wins), and
`Runtime::run_until` waits on it. The `glint-daemon` binary fires it when
stdin carries `shutdown` (`requested`) or closes (`control-channel-closed`).
Console Ctrl+C, closing the console window, `WM_ENDSESSION` and SIGTERM
are not hooked yet: catching them needs OS calls that belong in the native
host crates (`glint-windows` and peers), which will forward them as
`session-ending`. Until then those paths terminate the process without the
clean sequence. That is safe for state (the OS releases both file locks,
every write is an already-committed transaction, startup recovery handles
the rest; `a_killed_instance_leaves_no_stale_lock_and_a_healthy_database`)
but a native surface host would rely on its own exit-path cleanup for
work-area reservations (threat-model row B9.1).

## Single instance

Only one core runs per user, or more exactly per data root (a `GLINT_HOME`
root is its own instance). `InstanceLock::acquire` opens
`<data>/instance.lock` and takes an exclusive `File::try_lock`, the same
mechanism as the installer lock (`LockFileEx` on Windows, `flock`
elsewhere). The OS drops the lock when the process exits for any reason,
so a crash never leaves a stale lock. The holder then writes
`<data>/instance.owner` (`glint-instance 1`, `pid`, `started_at`) by
write-and-rename; it is a separate file because Windows blocks reads of a
locked range.

A second launch fails `acquire-instance` with `GLINT_CORE_ALREADY_RUNNING`
(category unavailable) carrying the owner record, before the database or
the installer lock is touched; the binary prints `already-running
pid=<pid> ipc=<state>` and exits with code 9. `probe_instance` answers "is
a core running?" without taking the lock, for clients such as the CLI. As
the plan asks, the second launch connects to the running instance: it
sends `get-status` over local IPC (`probe_ipc`) and prints the running
core's lifecycle state, or `unreachable` when the endpoint does not answer
(for example while the first instance is still booting). It does not yet
forward a request; the CLI routing that would use this is CLI-006. A stale
owner record left by a crash is replaced by the next holder.

## Local IPC

Once `Runtime::start` returns, the host serves the core over local IPC
(COR-003, [ADR 0010](../adr/0010-interprocess-for-local-ipc.md)); the
`glint-daemon` binary prints `ipc listening <endpoint>`.

- **Endpoint.** A named pipe `\\.\pipe\glint-ipc-<16 hex>` on Windows (a
  random suffix per start, so the name cannot be predicted and squatted)
  or `<data>/ipc.sock` on Unix, through the portable `interprocess` crate.
  The server records it in `<data>/ipc.endpoint` after binding and removes
  the record when it stops; clients read the record and refuse names Glint
  would not create.
- **Who may connect.** Windows: a protected DACL granting access only to
  `OWNER RIGHTS` (the user that runs the core; checked live, the pipe's
  owner is the user's SID and the DACL has that single entry),
  `FILE_FLAG_FIRST_PIPE_INSTANCE`, `PIPE_REJECT_REMOTE_CLIENTS`, default
  medium integrity. Unix: socket mode `0600` in the `0700` data root.
  Peer credential checks and the cross-user refusal test are SEC-002's.
- **Protocol (API version 1).** Length-prefixed JSON frames (requests up to
  64 KiB, responses up to 1 MiB, checked before the body is read), a
  `hello`/`welcome` version negotiation, then request envelopes with a
  `request_id` and `api_version` and exactly one response each: a typed
  body or a typed error with a stable `GLINT_IPC_*` code and a category
  (unsupported, permission-denied, unavailable, timeout, failure). The
  requests are a closed set: `ping`, `get-status`, `get-snapshot`,
  `get-state`, `get-config`, `get-capabilities`, `get-theme`, `subscribe`,
  `shutdown`; anything else is `GLINT_IPC_UNKNOWN_REQUEST`. Golden
  fixtures: `crates/glint-ipc/tests/fixtures/`.
- **Single writer kept.** `CoreIpc` feeds a `Hub` from the runtime's
  observer (lifecycle changes, warnings, shutdown steps) and from the boot
  reports (`publish_ready`). Reads are answered from that published
  snapshot; no IPC request reaches the daemon's database or installer. The
  only action, `shutdown`, fires the same `ShutdownSignal` as the stdin
  `shutdown` line.
- **Subscriptions.** `subscribe` returns the snapshot and its sequence
  number, then the connection streams deltas (`sequence + 1` each),
  heartbeats, a resync snapshot when the subscriber's bounded queue
  overflowed, and `closed` after the shutdown deltas. The client library
  refuses a gap (`GLINT_IPC_SEQUENCE_GAP`) and `reconnect` subscribes again
  for the current snapshot, so a client never depends on deltas it missed.
- **Bounds and timeouts.** At most 32 connections (the next gets
  `GLINT_IPC_BUSY`); 256 queued deltas per subscriber; client requests time
  out (5 s by default) and a timed-out connection is not reused. Windows
  pipes have no read timeouts, so the server has no per-connection read
  deadline; a same-user client can hold a connection slot open (threat
  model B7.5).
- **Failure.** If the endpoint cannot be served the binary shuts the
  runtime down and exits 1 (`GLINT_CORE_IPC_FAILED` with the
  `GLINT_IPC_*` source code): a core nobody can reach is not reported as
  running.

Tests: `crates/glint-ipc/tests/` (codec, golden fixtures, the real
transport in process), `crates/glint-core/tests/ipc.rs` and
`apps/daemon/tests/ipc.rs` (the real binary: requests, a subscription
snapshot followed by ordered shutdown deltas, reconnect, an unknown request
refused, the second launch).

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
transport COR-003 defines (`glint-ipc`); the contracts above do not change.
Today's protocol only exposes reads of what each daemon stage reported and
`shutdown`; the stage calls themselves become requests when the split
needs them, as an additive protocol change.
