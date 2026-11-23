# Module boundaries

Status: **foundation stage**. This page records where code goes. It does not
claim that the listed modules exist. A module is added only when a ticket
exercises it (PRJ-002), so the tree below grows ticket by ticket.

The responsibility map comes from
[system architecture](../plan/02-architecture/system-architecture.md).

## Layout

| Path | Holds | Rule |
|---|---|---|
| `apps/` | Executables (`glint` CLI, later the daemon, shell and manager) | Thin: argument parsing and wiring only; logic lives in `crates/` |
| `crates/` | Rust libraries (`glint-schema`, `glint-package`, ...) | No OS-specific dependency outside `glint-windows`, `glint-linux` and `glint-macos` |
| `packages/` | JS/TS modules (`@glint/sdk`, theme runtime, types) | Published SDK surface is permission-scoped; no native access |
| `themes/default` | The bundled default theme | A real package built only on the public SDK |
| `platform/` | Packaging resources (installers, icons, manifests) | No source code |
| `spikes/` | Throwaway feasibility code | Never shipped or imported by product code |
| `schemas/` | Published JSON Schemas | Labelled *proposed* until stabilized by tests and an ADR |
| `config/` | Bundled read-only application defaults (`default.toml`) | Embedded in the binary; user overrides live in the user-level `config.toml`, never here |

## Dependency direction

Allowed edges, enforced in CI by
[`scripts/check-crate-boundaries.py`](../../scripts/check-crate-boundaries.py)
(PRJ-004):

```text
apps/*  ->  crates/* (core and adapters)
glint-windows | glint-linux | glint-macos  ->  core crates, OS-binding crates
core crates (every other crates/*)  ->  core crates only
spikes/*  ->  anything;  nothing outside spikes/ -> spikes/*
```

| Rule | Fails when |
|---|---|
| R1 spike-never-imported | a `crates/*` or `apps/*` member depends on a `spikes/*` crate, directly or through other workspace crates |
| R2 core-no-adapter | a core crate reaches `glint-windows`, `glint-linux` or `glint-macos`, directly or transitively |
| R3 core-no-os-binding | a core crate depends directly on an OS-binding crate (`windows*`, `winapi*`, `webview2-com*`, `objc2*`, `cocoa*`, `gtk*`, `webkit2gtk*`, `wayland-*`, `x11*`, `zbus`, ...) |
| R4 app-no-os-binding | an `apps/*` member depends directly on an OS-binding crate; apps use adapters |

Rules are matched on workspace paths, so a new `crates/*` member is a core crate
by default. Only the three adapter names are exempt. Each failure names the
offending edge, for example `R3 core-no-os-binding: glint-core -> windows-sys`.

- **Direct only for OS-binding crates.** A portable dependency such as `dirs`
  or `rusqlite` pulls `windows-sys` under `cfg(windows)`; that is its business
  and is not flagged. Workspace edges are followed transitively.
- **All dependency kinds count**, including dev, build and target-gated ones.
- **Allow-list.** `ALLOW` in the script maps `(crate, dependency)` to a reason
  and is empty. An entry needs a ticket named in its reason.
- **`cfg` in source is not scanned.** Small `cfg(unix)`/`cfg(windows)` blocks
  exist in core crates today (file permissions, path semantics). New
  OS-specific API surface belongs in an adapter crate and is a review matter.
- Run it locally with `python scripts/check-crate-boundaries.py` (needs
  `cargo`); `--self-test` runs synthetic metadata fixtures in
  `scripts/tests/test_crate_boundaries.py` without cargo.

## Current members

| Member | Ticket | State |
|---|---|---|
| `apps/cli` (`glint-cli`, binary `glint`) | PRJ-002, CLI-001 | Thin CLI: a dependency-free parser for the *proposed* command tree, one schema-versioned `--json` envelope ([`schemas/glint-cli-output-v1.schema.json`](../../schemas/glint-cli-output-v1.schema.json)) and stable exit codes (0 ok, 1 failure, 2 usage, 3 unsupported, 4 invalid-package, 5 permission-denied, 6 daemon-unavailable, 7 not-found, 8 timeout). `version` and `theme validate` (stages a directory or `.glint` through `glint-sources`/`glint-package`) are implemented; every other command parses its arguments and fails with `GLINT_CLI_NOT_IMPLEMENTED` (exit 3) naming its owning ticket. Contract in [CLI reference](../cli/reference.md); `tests/exit_codes.rs` validates real output against the schema |
| `crates/glint-schema` (`glint-schema`) | SCH-001, SCH-003, SCH-004, SCH-006, SCH-007, THM-004 | Parses and validates the *proposed* `glint.json` v1 theme manifest with stable `GLINT_MANIFEST_*` error codes and `GLINT_MANIFEST_WARN_*` warnings; negotiates `schemaVersion`, Glint, SDK and entrypoint versions with stable `GLINT_COMPAT_*` reasons ([manifest compatibility](manifest-compatibility.md)); [`schemas/glint-theme-v1.schema.json`](../../schemas/glint-theme-v1.schema.json) is the matching proposed JSON Schema. Pure package identity types (`PackageKey`, canonical `PackageOrigin`, `Sha256Digest`, `PackageRelease`) and `classify_install`, which applies the one-origin-per-key policy of the *proposed* [ADR 0006](../adr/0006-package-identity-and-origin-collisions.md) with stable `GLINT_IDENTITY_*` outcomes; the installed inventory that uses them is INS-004's. Parses the *proposed* `profile.json` and `profile.lock` v1 formats with stable `GLINT_PROFILE_*` codes, flags local-only values for export and verifies a lock against its profile ([profile format](profile-format.md), schemas [`glint-profile-v1`](../../schemas/glint-profile-v1.schema.json) and [`glint-profile-lock-v1`](../../schemas/glint-profile-lock-v1.schema.json)). Parses `config.toml` layers and merges bundled defaults ([`config/default.toml`](../../config/default.toml)) with the user config, never applying package requests, with stable `GLINT_CONFIG_*` codes ([application config](application-config.md), schema [`glint-config-v1`](../../schemas/glint-config-v1.schema.json)). Owns the *proposed* semantic design token contract: the closed `TOKEN_REGISTRY`, `parse_tokens` for `tokens.json` v1 with stable `GLINT_TOKENS_*` codes (brand-constant names refused), WCAG `contrast_ratio` with `CONTRAST_REQUIREMENTS`, and `TokenFile::to_css` for the `--glint-*` custom property mapping with reduced-motion handling ([design tokens](design-tokens.md), schema [`glint-tokens-v1`](../../schemas/glint-tokens-v1.schema.json)) |
| `crates/glint-package` (`glint-package`) | PKG-001, PKG-003 | `inspect_archive` / `inspect_archive_in`: strict, bounded `.glint` (ZIP) reader that copies the archive into Glint-owned private staging while hashing it, validates the copy and its `glint.json` (via `glint-schema`), and extracts into a read-only staging tree; stable `GLINT_PACKAGE_*` error codes. Generated hostile-fixture suite in `tests/security.rs`. `pack_directory`: deterministic packer (sorted entries, fixed timestamps and modes, Deflate, junk excluded, links refused) whose output always passes the reader; round-trip tests in `tests/roundtrip.rs` |
| `crates/glint-sources` (`glint-sources`) | SRC-001, SRC-002 | `ThemeSource::{Registry, Git, LocalArchive, LocalDirectory, ArtifactUrl}` and the source-agnostic `PackageSource::stage` trait, which returns a `StagedPackage` (a `glint-package` candidate plus a `SourceOrigin` provenance record for the installer). `LocalSource` stages a local `.glint` through `inspect_archive_in`, and packs a local directory deterministically with `pack_directory` before inspecting it the same way; it never runs a build. Registry, Git and artifact URL backends are deferred to M4 and return `GLINT_SOURCE_UNSUPPORTED`; stable `GLINT_SOURCE_*` error codes. Generated-fixture tests in `tests/local_source.rs`. `parse_theme_source` (module `url`): pure parser for `registry/package[@version]`, `https`/`git+https` Git URLs with the *proposed* `#ref:subpath` fragment, `https` `.glint` artifact URLs and explicitly prefixed local paths; refuses every other scheme, SSH shorthand, option-like input, credentials, IP-literal and local hosts, network paths, control and invisible characters and ambiguous forms with stable `GLINT_SOURCE_URL_*` codes; table tests in `tests/url_parse.rs`. No networking anywhere in the crate |
| `crates/glint-platform` (`glint-platform`) | CAP-001, CAP-002, CAP-003, CAP-005 | OS-independent capability vocabulary (no OS-specific dependency): validated namespaced `CapabilityId`, `Availability::{Supported, ReadOnly, PermissionRequired, Experimental, Unavailable { reason }}` (the reason is typed and never empty), `Capability` and `IntegrationEffect`; `CapabilityError` with stable kebab-case codes from the [capability errors table](../plan/02-architecture/capabilities-permissions-and-errors.md#capability-errors) plus `permission-denied`, `read-only`, `unknown-capability`, `timeout` and `adapter-failure`, each with UI guidance and an `ErrorCategory` (unsupported / permission / unavailable / timeout / failure). All types round-trip through JSON. `CapabilityService` discovers through a `CapabilityAdapter` trait (native adapters implement it in `glint-windows`/`glint-linux`/`glint-macos`), keeps a generation-numbered `CapabilitySnapshot` and sends a `CapabilityChange` to every subscriber on each availability transition; it fails closed (undiscovered or unadvertised capabilities are `unknown-capability`, a failed discovery turns every known capability `temporary-unavailable`). `mock::MockAdapter` reaches every state and scripts live transitions; `tests/mock_adapter.rs` covers them. The activation gate (`evaluate_manifest`, `gate_activation`) reads a `glint_schema::ThemeManifest` and a snapshot: unusable required capabilities refuse activation with a `theme-incompatible` `ActivationRefusal` listing every blocker (unless a declared fallback is usable), unusable optional ones only disable controls in the `ActivationReport`, and the caller's activation step runs only after the gate passes (`tests/capability_gate.rs`). `registry` holds the frozen v1 capability registry ([spec](../spec/capabilities-v1.md), [ADR 0009](../adr/0009-frozen-v1-capability-registry.md)): 43 IDs with read/write/present access, sensitivity, effect and platform notes; unregistered IDs fail closed in the service (`adapter-failure`), snapshot lookups (`unknown-capability`) and the gate (refused even when optional); `typescript_module` generates `packages/types/src/capabilities.ts` and `markdown_table` the spec table (drift-checked by `tests/registry.rs`). Depends only on `glint-schema` and `serde` |
| `crates/glint-surfaces` (`glint-surfaces`) | SRF-001 | Portable surface catalog and definition parser (no OS-specific dependency): the fourteen standard `StandardSurface` kinds and namespaced `custom:<namespace>.<name>` kinds with their required `surface.*` and optional provider capabilities and placement/behaviour defaults (`CATALOG`); `SurfaceDefinition::parse` / `surface_definitions` validate a manifest surface `{entry, placement, behavior}` into `{id, kind, entrypoint, placement, behavior, capabilities}` with stable `GLINT_SURFACE_*` codes and JSON Pointers ([surfaces](surfaces.md)); `typescript_module` generates `packages/types/src/surfaces.ts` (drift-checked by `tests/typegen.rs`). Shared case corpus in `tests/package-fixtures/surfaces/`. Depends only on `glint-schema`, `glint-platform`, `serde` and `serde_json` |
| `crates/glint-installer` (`glint-installer`) | INS-003, INS-004, INS-005 | `Installer::open` takes the exclusive installer lock (`File::try_lock`, released on crash), opens `glint-storage` and runs `recover_pending`. `install(candidate, origin)` moves a `glint-package` candidate staged under `<data>/staging` into the immutable content-addressed store `<data>/store/<sha256>` (`source.glint`, read-only `root/`, `install.record`) with one rename, journalled in `install_journal` (migration 002) and committed with the package row in one transaction; re-installing a digest is a no-op; activation only with `InstallOptions::activate`. `remove(digest)` refuses releases referenced by any profile (base theme or install lock) and moves the entry to `<data>/trash` under the same journal. `recover_pending` rolls back uncommitted installs, undoes or finishes removals, empties staging and trash, and re-indexes verified store entries after database recovery. Named `FaultPoint`s and a `FaultInjector` simulate crashes; `tests/transaction.rs`. Stable `GLINT_INSTALL_*` codes. Provenance and identity (INS-004, `src/provenance.rs`): each package row and `install.record` carry the ADR 0006 origin (`package_origin` maps a `SourceOrigin`: local kinds to `local`, Git/artifact/registry URLs to their prefixed forms), registry, Git commit, local path, signing status and retrieval time (migration 004); every install is classified with `glint_schema::classify_install` against the installed releases of its key, and `DigestConflict`/`OriginCollision` are refused as `GLINT_INSTALL_IDENTITY_CONFLICT` carrying the `GLINT_IDENTITY_*` outcome; tests in `tests/provenance.rs` reproduce the ADR's table. `apply(plan, surfaces, adapters)` (INS-005, `src/rollback.rs`): validates the target and adapters, requires approval for changes whose `NativeAdapter` lacks `RestoreSemantics { can_snapshot, can_restore }`, snapshots every setting, journals the prior active profile and the planned changes (`apply_journal`/`apply_native`, migration 003), stages surfaces beside the old ones through `SurfaceHost`, marks each native change `applying`/`applied` around the adapter call, and commits activation together with the journal deletion; on failure it restores in reverse and returns a `RollbackReport` (`GLINT_APPLY_ROLLBACK_INCOMPLETE` when anything stayed changed). At startup `RecoveryReport::pending_apply` exposes an interrupted apply and `recover_apply(adapters)` rolls it back; tests in `tests/rollback.rs`. Native adapters and the surface host are traits here; their OS implementations live in the native host crates |
| `crates/glint-storage` (`glint-storage`) | INS-001, INS-002 | `GlintPaths`: Glint's configuration, data and cache roots from the OS base directories via `dirs` ([ADR 0007](../adr/0007-dirs-for-os-path-resolution.md)): `{RoamingAppData}\Glint` + `{LocalAppData}\Glint\data`/`cache` on Windows, `~/Library/Application Support/Glint` (+ `data/`) and `~/Library/Caches/Glint` on macOS, `$XDG_CONFIG_HOME`/`$XDG_DATA_HOME`/`$XDG_CACHE_HOME` + `glint` elsewhere. `GLINT_HOME` (absolute only) or `GlintPaths::under_root` moves all three under one directory. Derives every file Glint writes (database, active-theme record, store, staging, trash, installer lock) inside the roots; `check_within` refuses anything else; `ensure_dirs` creates only Glint-owned directories (`0700` on Unix) and refuses files, symlinks and junctions in their place. Stable `GLINT_STORAGE_*` codes with an unsupported / permission-denied / unavailable / timeout / failure category. Tests in `tests/paths.rs`. `Storage`: the SQLite state database (rusqlite, bundled SQLite 3.54.1, [ADR 0008](../adr/0008-rusqlite-with-bundled-sqlite-for-state.md)); numbered migrations in `src/migrations/` (001: packages, grants, profiles, install locks, active profile; 002: install journal; 003: apply journal; 004: provenance) applied one transaction each with `user_version`; a failed migration rolls back and `read_active_theme` still reads the old schema; a corrupt database (not a database, failed `quick_check`, missing tables of an applied migration) is quarantined and recreated with the active theme restored from the mirrored `active-theme.record`; a newer schema is never touched. Tests in `tests/migrations.rs`. Re-exports `rusqlite` for the installer |
| `spikes/windows-appbar` (`spike-windows-appbar`) | SPK-001 | Throwaway Win32 appbar + WebView2 host; Windows only (stub that reports "unsupported" elsewhere); findings in [the spike report](../research/spikes/windows-appbar.md) |
| `spikes/package-security` (`glint-spike-package-security`) | SPK-007 | Throwaway bounded `.glint` validator and hostile ZIP fixture sweep; findings in [package security spike](../research/spikes/package-security.md); never imported by product code |
| `spikes/windows-system-providers` (`spike-windows-system-providers`) | SPK-010 | Throwaway Core Audio, Network List Manager / WinRT network and battery provider probe with a spike-local `Availability` mapping; Windows only (stub that reports "unsupported" elsewhere); findings in [the spike report](../research/spikes/windows-system-providers.md); never imported by product code |
| `spikes/webview2-baseline` (`spike-webview2-baseline`) | SPK-008 | Throwaway WebView2 host for one React probe panel (bundle built from `web/`, which sits outside the pnpm workspace, and committed as `assets/app.js`); Windows only (stub that reports "unsupported" elsewhere); findings in [WebView2 rendering baseline](../research/spikes/webview2-baseline.md); never imported by product code |
| `themes/default` (`official.default`) | THM-001 | Bundled fallback theme and a real `.glint` package: v1 manifest with no required capability (`surface.panel` optional) compatible with every Glint build, graphite/silver `theme/tokens.json` (provisional until THM-004), a static script-free panel surface, `assets/preview.svg` and MIT `LICENSE`; repository-owned, no remote references, not a pnpm workspace package. `crates/glint-package/tests/default_theme.rs` packs it deterministically and runs it through the strict reader; the SDK surfaces arrive in THM-002 |
| `packages/types` (`@glint/types`) | SRF-001, CAP-005 | Shared TypeScript types; `src/surfaces.ts` is generated from `crates/glint-surfaces` (surface kinds, placement/behaviour vocabularies and interfaces, catalog, error codes) and checked against the shared surface case corpus; `src/capabilities.ts` is generated from the `glint-platform` capability registry (the closed `CapabilityId` union, which the surface types use) |
| `packages/theme-schema` (`@glint/theme-schema`) | SCH-002 | TypeScript types generated from `schemas/glint-theme-v1.schema.json` (CI fails on drift) and a validator that mirrors the Rust parser; both run the shared corpus in [`tests/package-fixtures/manifests/`](../../tests/package-fixtures/README.md) and must agree on every verdict, code and pointer |

## Manifest source of truth

The Rust parser in `crates/glint-schema` is the authority for manifest rules.
`schemas/glint-theme-v1.schema.json` is the single source for TypeScript
types: `packages/theme-schema/scripts/generate-types.mjs` (pinned
`json-schema-to-typescript`) writes `src/generated/manifest.ts`, and its
`--check` mode (`corepack pnpm@10.18.0 --filter @glint/theme-schema check`)
fails when the committed file differs. Rust tests keep the schema's field set
aligned with the parser, and the shared fixture corpus keeps the two
validators' behaviour identical.

## Capability ownership

`crates/glint-platform` owns the capability vocabulary, the capability
service and the activation gate. Notes on the current split:

- **Registry of known capability IDs** (CAP-005). `glint_platform::registry`
  is the single source of truth ([spec](../spec/capabilities-v1.md),
  [ADR 0009](../adr/0009-frozen-v1-capability-registry.md)).
  `glint-package` derives `known_capabilities()` / `is_known_capability()`
  from it (the provisional `PROVIDER_CAPABILITIES` list is gone);
  `glint-surfaces` resolves catalog IDs through it and tests that every
  catalog capability is registered; `glint-schema` stays syntax-only because
  it cannot depend on `glint-platform`. Unregistered IDs fail closed in the
  package reader, the capability service, snapshot lookups and the
  activation gate.
- **Activation gate location.** CAP-003 proposed
  `crates/glint-core/src/capability_gate.rs`. No `glint-core` crate exists
  yet (COR epic), so the gate lives in
  `crates/glint-platform/src/capability_gate.rs` as a pure function plus
  `gate_activation`, ready for the daemon to call before it changes the
  active theme.
- **Fallbacks.** The v1 manifest has no fallback field (`integrations` is
  rejected), so manifests declare none; `CapabilityRequirements::with_fallback`
  is the hook for the format version or profile resolver that adds them.

## Toolchain pins

- Rust `1.95.0` via `rust-toolchain.toml` (edition 2024, resolver 3), compiled on the Windows 11 host.
- Node `>=24`, pnpm `10.18.0` via `packageManager` (run through Corepack).
- Linux and macOS results come from CI compile jobs only.
