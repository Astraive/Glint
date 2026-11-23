# ADR 0007: `dirs` for OS configuration, data and cache paths

- **Status:** Accepted
- **Date:** 2026-11-23
- **Ticket:** INS-001
- **Supersedes:** none
- **Superseded by:** none
- **Plan label affected:** RECOMMENDED ("resolved with a tested platform-aware path library, not hardcoded username paths")
- **Plan sections affected:** [Suggested configuration locations](../plan/02-architecture/renderer-and-state.md#suggested-configuration-locations), [Task 4: Installer store, provenance and rollback](../plan/docs/superpowers/plans/2026-11-23-glint-foundation-plan.md#task-4-installer-store-provenance-and-rollback)

## Context

Glint writes its configuration, state database, package store and caches in
per-user locations. The plan suggests `%APPDATA%\Glint\config.toml` and
`%LOCALAPPDATA%\Glint\data`/`cache` on Windows, `~/Library/Application
Support/Glint` and `Caches/Glint` on macOS, and the XDG base directories on
Linux, and requires a tested path library instead of hardcoded home or user
name paths. Picking a dependency is an ADR-level change (see the
[ADR README](README.md)).

Candidates (all MIT OR Apache-2.0 on crates.io):

- `dirs` 7: base directories only (`config_dir`, `data_local_dir`,
  `cache_dir`). Windows answers come from `SHGetKnownFolderPath`, Linux from
  the XDG specification (relative `$XDG_*` values are ignored, as the
  specification requires), macOS from the Standard Directories. Dependencies:
  `dirs-sys`, `option-ext` (MPL-2.0, allowed by [deny.toml](../../deny.toml)
  for unmodified use), `windows-sys` (already in the lock file) and `libc`.
- `directories` 6: the same `dirs-sys` backend plus a `ProjectDirs` layer
  that builds application paths from a qualifier, organization and name. Its
  layout differs from the plan's: Windows data goes to the roaming profile
  (`{RoamingAppData}\<app>\data`), and macOS uses a reverse-DNS bundle
  directory.
- `etcetera`: strategy-based (XDG, Apple, Windows). Comparable, with a
  smaller user base; it offers nothing extra for three base directories.

## Decision

`glint-storage` depends on `dirs` (`7.0.0`) for the three base directories
and adds Glint's own sub-paths in `crates/glint-storage/src/paths.rs`:

| Layout | Config | Data | Cache |
|---|---|---|---|
| Windows | `{RoamingAppData}\Glint` | `{LocalAppData}\Glint\data` | `{LocalAppData}\Glint\cache` |
| macOS | `~/Library/Application Support/Glint` | `~/Library/Application Support/Glint/data` | `~/Library/Caches/Glint` |
| Linux (XDG) | `$XDG_CONFIG_HOME/glint` | `$XDG_DATA_HOME/glint` | `$XDG_CACHE_HOME/glint` |

Only configuration roams on Windows; the database and package store stay
local. `GLINT_HOME=<absolute dir>` (or `GlintPaths::under_root`) moves all
three under one directory for tests and portable runs; a relative override is
an error. This confirms the plan's recommendation; the macOS data
sub-directory (`.../Glint/data`) is the one choice the plan left open.

## Consequences

- No user name, home directory or drive letter appears in Glint's code; a
  test (`resolver_source_has_no_literal_user_paths`) keeps it that way.
- Layouts are pure functions of `BaseDirectories`, so all three are tested on
  the Windows-only test host from synthetic inputs; the live OS answer is
  checked against `dirs` on each CI host.
- `option-ext` adds an MPL-2.0 crate to the inventory (unmodified use).
- Follow-up: a Windows redirected `AppData` (folder redirection to a share)
  makes the config root a network path; Glint keeps data in `LocalAppData`,
  which is not redirected by default.

## Alternatives considered

- `directories`: its `ProjectDirs` layout puts Windows data in the roaming
  profile, which would roam the package store and database; composing on top
  of its `BaseDirs` gives the same result as `dirs` with one more crate.
- `etcetera`: viable; no advantage for three base directories.
- Reading `%APPDATA%`, `$HOME` and `$XDG_*` by hand: exactly what the plan
  rules out; easy to get wrong (relative XDG values, missing variables).

## Migration notes

None: no public contract changes. Nothing is installed yet, so no existing
data needs to move.
