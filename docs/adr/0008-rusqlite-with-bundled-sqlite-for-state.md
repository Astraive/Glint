# ADR 0008: rusqlite with bundled SQLite for the state database

- **Status:** Accepted
- **Date:** 2026-11-23
- **Ticket:** INS-002
- **Supersedes:** none
- **Superseded by:** none
- **Plan label affected:** RECOMMENDED ("Choose rusqlite or SQLx in a recorded decision")
- **Plan sections affected:** [State stores](../plan/02-architecture/renderer-and-state.md#state-stores), [Task 4: Installer store, provenance and rollback](../plan/docs/superpowers/plans/2026-11-23-glint-foundation-plan.md#task-4-installer-store-provenance-and-rollback), [Review Focus](../plan/docs/superpowers/plans/2026-11-23-glint-foundation-plan.md#review-focus)

## Context

The plan keeps the package index, grants, profile metadata, install locks
and migrations in one SQLite database written only by the core process, and
asks INS-002 to choose between rusqlite and SQLx. The core is one process
(ADR 0001) with no async runtime yet; installs, journals and rollback need
short, strictly ordered transactions on the same thread that moves files.
The review focus requires that a corrupt previous migration or transaction
recovers safely without losing the active theme.

## Decision

`glint-storage` uses **rusqlite `0.43.0`** with the **`bundled`** feature and
no default features. `bundled` compiles the SQLite amalgamation shipped in
`libsqlite3-sys` 0.40.0 (SQLite 3.54.1) into Glint, so every platform runs
the same SQLite. The crate re-exports `rusqlite` so table-owning crates use
the same version.

Schema rules that come with it (`crates/glint-storage/src/db.rs`):

- The schema version is `PRAGMA user_version`. Migrations are numbered SQL
  files (`src/migrations/NNN_name.sql`), applied in order, each in its own
  `BEGIN IMMEDIATE` transaction together with the version bump and a check
  that its tables exist. An applied migration is never edited.
- A failed migration rolls back completely; the database stays readable at
  its previous version (`GLINT_STORAGE_MIGRATION_FAILED`). A database newer
  than the build is left untouched (`GLINT_STORAGE_SCHEMA_TOO_NEW`).
- A file that is not a database, fails `PRAGMA quick_check`, or lacks the
  tables of an applied migration is quarantined (renamed, never deleted) and
  recreated. The active theme survives through a small record file mirrored
  after every committed activation; the package index is rebuilt from the
  immutable store by the installer.
- Rollback journal mode (`DELETE`) with `synchronous = FULL`; foreign keys
  on; a 5 second busy timeout reported as category `timeout`.

## Consequences

- No system SQLite is needed on any host, and behaviour does not vary with
  distribution SQLite versions. Builds need a C compiler (already true for
  the Windows spikes); compile time grows by the amalgamation build.
- SQLite upgrades arrive only through deliberate `rusqlite`/`libsqlite3-sys`
  bumps, which must re-run `tests/migrations.rs`.
- The license inventory gains `rusqlite`, `libsqlite3-sys` (MIT; SQLite
  itself is public domain) and build-only `cc`, `pkg-config`, `vcpkg`.
- Async callers (a future IPC layer) must call storage from a blocking task;
  acceptable for a single-writer daemon.

## Alternatives considered

- **SQLx**: async-first and compile-time-checked queries, but it brings an
  async runtime and a much larger dependency tree into a synchronous,
  single-process core, and its migrator's failure handling is less direct
  to test with injected failures.
- **System SQLite** (rusqlite without `bundled`): smaller binaries on Linux,
  but versions differ per distribution and Windows has no system SQLite for
  applications to link.
- **A JSON file** (`installed-themes.json`, mentioned in the plan): no
  transactions across packages, grants and journals.

## Migration notes

None: no public contract changes. The database is internal state; its
schema evolves only through new numbered migrations.
