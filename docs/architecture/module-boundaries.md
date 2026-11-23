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
| `spikes/windows-appbar` (`spike-windows-appbar`) | SPK-001 | Throwaway Win32 appbar + WebView2 host; Windows only (stub that reports "unsupported" elsewhere); findings in [the spike report](../research/spikes/windows-appbar.md) |
| `spikes/package-security` (`glint-spike-package-security`) | SPK-007 | Throwaway bounded `.glint` validator and hostile ZIP fixture sweep; findings in [package security spike](../research/spikes/package-security.md); never imported by product code |

## Toolchain pins

- Rust `1.95.0` via `rust-toolchain.toml` (edition 2024, resolver 3), compiled on the Windows 11 host.
- Node `>=24`, pnpm `10.18.0` via `packageManager` (run through Corepack).
- Linux and macOS results come from CI compile jobs only.
