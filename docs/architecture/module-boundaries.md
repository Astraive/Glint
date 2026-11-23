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

## Current members

| Member | Ticket | State |
|---|---|---|
| `apps/cli` (`glint-cli`, binary `glint`) | PRJ-002 | Reports its version; every other command returns an "unsupported" error |
| `crates/glint-schema` (`glint-schema`) | SCH-001, SCH-003 | Parses and validates the *proposed* `glint.json` v1 theme manifest with stable `GLINT_MANIFEST_*` error codes and `GLINT_MANIFEST_WARN_*` warnings; negotiates `schemaVersion`, Glint, SDK and entrypoint versions with stable `GLINT_COMPAT_*` reasons ([manifest compatibility](manifest-compatibility.md)); [`schemas/glint-theme-v1.schema.json`](../../schemas/glint-theme-v1.schema.json) is the matching proposed JSON Schema |
| `crates/glint-package` (`glint-package`) | PKG-001, PKG-003 | `inspect_archive` / `inspect_archive_in`: strict, bounded `.glint` (ZIP) reader that copies the archive into Glint-owned private staging while hashing it, validates the copy and its `glint.json` (via `glint-schema`), and extracts into a read-only staging tree; stable `GLINT_PACKAGE_*` error codes. Generated hostile-fixture suite in `tests/security.rs`. `pack_directory`: deterministic packer (sorted entries, fixed timestamps and modes, Deflate, junk excluded, links refused) whose output always passes the reader; round-trip tests in `tests/roundtrip.rs` |
| `crates/glint-sources` (`glint-sources`) | SRC-001, SRC-002 | `ThemeSource::{Registry, Git, LocalArchive, LocalDirectory, ArtifactUrl}` and the source-agnostic `PackageSource::stage` trait, which returns a `StagedPackage` (a `glint-package` candidate plus a `SourceOrigin` provenance record for the installer). `LocalSource` stages a local `.glint` through `inspect_archive_in`, and packs a local directory deterministically with `pack_directory` before inspecting it the same way; it never runs a build. Registry, Git and artifact URL backends are deferred to M4 and return `GLINT_SOURCE_UNSUPPORTED`; stable `GLINT_SOURCE_*` error codes. Generated-fixture tests in `tests/local_source.rs`. `parse_theme_source` (module `url`): pure parser for `registry/package[@version]`, `https`/`git+https` Git URLs with the *proposed* `#ref:subpath` fragment, `https` `.glint` artifact URLs and explicitly prefixed local paths; refuses every other scheme, SSH shorthand, option-like input, credentials, IP-literal and local hosts, network paths, control and invisible characters and ambiguous forms with stable `GLINT_SOURCE_URL_*` codes; table tests in `tests/url_parse.rs`. No networking anywhere in the crate |
| `crates/glint-platform` (`glint-platform`) | CAP-001, CAP-002, CAP-003 | OS-independent capability vocabulary (no OS-specific dependency): validated namespaced `CapabilityId`, `Availability::{Supported, ReadOnly, PermissionRequired, Experimental, Unavailable { reason }}` (the reason is typed and never empty), `Capability` and `IntegrationEffect`; `CapabilityError` with stable kebab-case codes from the [capability errors table](../plan/02-architecture/capabilities-permissions-and-errors.md#capability-errors) plus `permission-denied`, `read-only`, `unknown-capability`, `timeout` and `adapter-failure`, each with UI guidance and an `ErrorCategory` (unsupported / permission / unavailable / timeout / failure). All types round-trip through JSON. `CapabilityService` discovers through a `CapabilityAdapter` trait (native adapters implement it in `glint-windows`/`glint-linux`/`glint-macos`), keeps a generation-numbered `CapabilitySnapshot` and sends a `CapabilityChange` to every subscriber on each availability transition; it fails closed (undiscovered or unadvertised capabilities are `unknown-capability`, a failed discovery turns every known capability `temporary-unavailable`). `mock::MockAdapter` reaches every state and scripts live transitions; `tests/mock_adapter.rs` covers them. The activation gate (`evaluate_manifest`, `gate_activation`) reads a `glint_schema::ThemeManifest` and a snapshot: unusable required capabilities refuse activation with a `theme-incompatible` `ActivationRefusal` listing every blocker (unless a declared fallback is usable), unusable optional ones only disable controls in the `ActivationReport`, and the caller's activation step runs only after the gate passes (`tests/capability_gate.rs`). Depends only on `glint-schema` and `serde` |
| `spikes/windows-appbar` (`spike-windows-appbar`) | SPK-001 | Throwaway Win32 appbar + WebView2 host; Windows only (stub that reports "unsupported" elsewhere); findings in [the spike report](../research/spikes/windows-appbar.md) |
| `spikes/package-security` (`glint-spike-package-security`) | SPK-007 | Throwaway bounded `.glint` validator and hostile ZIP fixture sweep; findings in [package security spike](../research/spikes/package-security.md); never imported by product code |
| `spikes/windows-system-providers` (`spike-windows-system-providers`) | SPK-010 | Throwaway Core Audio, Network List Manager / WinRT network and battery provider probe with a spike-local `Availability` mapping; Windows only (stub that reports "unsupported" elsewhere); findings in [the spike report](../research/spikes/windows-system-providers.md); never imported by product code |
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

- **Registry of known capability IDs.** The CAP epic owns it (CAP-005, not
  yet built). Until then `glint-package` keeps a provisional
  `KNOWN_CAPABILITIES` list that its reader uses to refuse unknown IDs; it is
  deliberately left in place. Follow-up for CAP-005: move the list into
  `glint-platform` as the frozen v1 registry and have `glint-package`
  consume it. Doing it now would change what the package reader accepts
  (the plan's namespace list is larger than the provisional one) without
  the registry's review. Until then the gate fails closed on its own: a
  capability the host adapter does not advertise is `unknown-capability`.
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
