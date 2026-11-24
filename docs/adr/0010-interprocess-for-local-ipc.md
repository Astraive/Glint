# ADR 0010: `interprocess` for the local IPC transport, with a versioned JSON envelope

- **Status:** Accepted
- **Date:** 2026-11-24
- **Ticket:** COR-003
- **Supersedes:** none
- **Superseded by:** none
- **Plan label affected:** RECOMMENDED ("Candidate transport: Unix domain sockets for Linux/macOS and named pipes or equivalent local IPC for Windows"; the illustrative `GlintRequest` / `ResponseEnvelope` contract)
- **Plan sections affected:** [IPC design](../plan/02-architecture/process-model-and-ipc.md#ipc-design), [Protocol evolution](../plan/02-architecture/process-model-and-ipc.md#protocol-evolution), [Event model](../plan/02-architecture/process-model-and-ipc.md#event-model)

## Context

The manager, the CLI and, after the process split (COR-007), the shell
need to talk to the per-user core. The plan proposes Unix domain sockets on
Linux and macOS, named pipes on Windows, typed and explicitly versioned
messages, bounded and timeout-aware responses, distinct permission-denied,
unsupported and failure results, no arbitrary command names, and events
with sequence IDs and snapshots so clients recover after reconnect.

Core crates may not depend on OS-binding crates (`windows*`, `libc` is
portable; [module boundaries](../architecture/module-boundaries.md) rule
R3). Picking a dependency and fixing IPC command names are ADR-level
changes (see the [ADR README](README.md)).

Candidates:

- `interprocess` 2.4 (`0BSD OR Apache-2.0`): one `local_socket` API over
  Windows named pipes and Unix domain sockets, synchronous and Tokio
  flavours. On Windows it creates pipes overlapped (concurrent read and
  write on one connection work), with `FILE_FLAG_FIRST_PIPE_INSTANCE` on
  the first instance and `PIPE_REJECT_REMOTE_CLIENTS` by default, and takes
  a security descriptor (parsed from SDDL); on Unix it takes the socket
  file mode. Dependencies: `windows-sys` 0.61 (already locked), `recvmsg`,
  `widestring` and `doctest-file` on Windows, `libc` on Unix. No async
  runtime unless the `tokio` feature is enabled.
- `parity-tokio-ipc` and similar: Tokio only; Glint's core is synchronous
  threads today, and pulling in a runtime for one transport is not
  justified yet.
- Hand-written transports: `std::os::unix::net` covers Unix, but Windows
  named pipes would need `windows-sys` calls in a core crate (rule R3) or a
  new adapter crate before any native host exists.

## Decision

1. **Transport.** `crates/glint-ipc` depends on `interprocess` (`2.4.9`,
   default features off, synchronous API). The endpoint is a named pipe
   `\\.\pipe\glint-ipc-<16 hex>` on Windows and `<data>/ipc.sock` on Unix.
   The pipe suffix is random per server start, and the server writes the
   endpoint into `<data>/ipc.endpoint` (write and rename) for clients;
   clients accept only names Glint itself would create.
2. **Access restriction (what COR-003 does; SEC-002 completes it).**
   Windows: protected DACL `D:P(A;;GA;;;OW)` (owner rights only: no other
   user, anonymous or AppContainer access), first-instance creation,
   remote clients rejected, default medium integrity kept. Unix: socket
   mode `0600` inside the `0700` data root. Peer credential checks on
   accepted connections, a client-side check of the server's owner, and the
   cross-user refusal test are SEC-002's.
3. **Framing.** A 4-byte big-endian length, then UTF-8 JSON. Requests are
   limited to 64 KiB and responses to 1 MiB; the length is checked before
   the body is read, an oversized or empty frame is refused with a typed
   error and the connection is closed.
4. **Envelope, API version 1.** The client sends
   `{"kind":"hello","versions":{"min","max"},"client"}`; the server answers
   `welcome` with the newest common `api_version` or
   `GLINT_IPC_VERSION_UNSUPPORTED` and closes. Each request is
   `{"kind":"request","request_id","api_version","request":{"type","payload"?}}`
   and gets exactly one `response` with the same `request_id` and
   `result: {"ok": {...}}` or `result: {"error": {code, category, message}}`.
   Request types are a closed, kebab-case set: `ping`, `get-status`,
   `get-snapshot`, `get-state`, `get-config`, `get-capabilities`,
   `get-theme`, `subscribe`, `shutdown`. Anything else is
   `GLINT_IPC_UNKNOWN_REQUEST`; there is no generic command, path-taking
   execution or shell request. Error codes are `GLINT_IPC_*` with the
   categories `unsupported`, `permission-denied`, `unavailable`, `timeout`
   and `failure`. Golden fixtures live in `crates/glint-ipc/tests/fixtures/`.
5. **Subscriptions.** `subscribe` answers with the current snapshot and its
   sequence number; the connection then carries only `event` frames:
   `delta` (always `sequence + 1`), `heartbeat` (the last sequence),
   `snapshot` (a resync after the server dropped deltas for a lagging
   subscriber with a bounded queue) and `closed`. A client that sees a gap,
   or reconnects, subscribes again and gets the current snapshot; it never
   depends on deltas it missed.
6. **Evolution.** Additive changes (new request types, response fields,
   delta types) keep version 1 when old clients can ignore them; anything
   else bumps `api_version`, and servers keep answering the older versions
   they list in `supported`.

## Consequences

- No core crate names `windows-sys`; the Windows-only code in `glint-ipc` is
  a `cfg(windows)` block that hands an SDDL string to `interprocess`.
- Reads are served from a published snapshot (`Hub`), never from the
  daemon's handles, so the daemon stays the only writer.
- Inventory additions: `interprocess` (0BSD OR Apache-2.0, used under
  Apache-2.0) and `widestring` (MIT OR Apache-2.0) pass the allow-list.
  `recvmsg` and `doctest-file` are `0BSD` only, which the Rust allow-list
  does not name; they get per-crate `[[licenses.exceptions]]` entries in
  [deny.toml](../../deny.toml), recorded in the
  [license policy](../legal/license-policy.md#exceptions) and pending
  maintainer approval. 0BSD has no conditions at all.
- Windows named pipes have no read timeouts: clients get per-request
  deadlines from a reader thread; the server relies on the connection cap
  and the per-user DACL instead of server-side read deadlines (a gap noted
  in the threat model, B7.5).
- When the server runs elevated, the pipe's owner may be the
  Administrators group, so a non-elevated client of the same user is
  refused. Glint's core does not run elevated.

## Alternatives considered

- Tokio-based IPC crates: rejected for now (no async runtime in the core).
- A new `glint-windows` crate holding raw named pipe code: premature; the
  portable crate already exposes what is needed. Revisit if SEC-002 needs
  token checks that `interprocess` cannot provide.
- Length-delimited binary (CBOR, bincode): JSON keeps golden fixtures
  readable and matches the CLI's JSON output; the 1 MiB bound keeps cost
  predictable.

## Migration notes

None: no IPC existed before. The CLI does not use the transport yet
(CLI-006).
