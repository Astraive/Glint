# Versioning Policy

Glint tracks **application versions and format/protocol contracts independently** so a bug-fix or UI patch never silently breaks saved profiles, `.glint` packages, SDK-built themes, or IPC boundaries. Release execution, signing key custody, and distribution are governed by [docs/release-process.md](docs/release-process.md).

All format and schema versions below are **proposals from the planning archive** ([docs/plan/04-themes/manifest-and-glint-package-spec.md](docs/plan/04-themes/manifest-and-glint-package-spec.md), [docs/plan/README.md](docs/plan/README.md)); none is a frozen external standard.

## Current Pre-Release Status

- **Application / CLI:** Unreleased (pre-implementation baseline; no `0.x` or `1.0` tags exist yet).
- **Package, manifest, SDK, registry, profile, config, and IPC contracts:** Draft only, not frozen.

## Independent Version Streams

| Stream | Scheme | Current State | Stability Rule |
| --- | --- | --- | --- |
| Application and CLI (`glint`) | Semantic Versioning (`MAJOR.MINOR.PATCH[-prerelease]`) | Unreleased | Follows SemVer once `0.1.0` begins; after `1.0.0`, breaking CLI or host behavior requires a major bump. |
| `.glint` package format | Integer `schemaVersion` of the container | Draft | Frozen only after the reader/packer, validator, installer and cross-platform path conformance agree; extraction limits are part of the contract. |
| `glint.json` manifest schema | Integer schema version (`schemaVersion`) | Draft | New fields are additive within a version; semantic changes require a new schema version and a documented migration. |
| SDK (`@glint/sdk`) | SemVer plus an integer `entrypointVersion` | Draft | The entrypoint version declares which host runtime contract a built theme targets; hosts report which entrypoint versions they support. |
| Registry index schema | Integer schema version | Draft | Versioned independently in the registry; clients reject unknown major schema versions explicitly rather than guessing. |
| Profile schema | Integer schema version | Draft | Profiles lock package version and hash; migrations preserve backups and never rewrite packages in place. |
| Local IPC protocol | Protocol version | Draft | Daemon, shell, renderer bridge and CLI negotiate explicit envelope compatibility. |
| Config schema | Integer schema version | Draft | Migrations preserve user backups and tolerate downgrades read-only where feasible. |

Example identifiers such as `schemaVersion: 1` or `@glint/sdk` in the plan are design examples, not published packages.

## Pre-Release Identifiers and Format Freeze

1. **Pre-`1.0.0` releases (`0.MINOR.PATCH[-alpha.N|-beta.N|-rc.N]`):** minor releases may adjust internal crate APIs and draft schemas before any format-freeze gate passes, always documenting breaking changes in [CHANGELOG.md](CHANGELOG.md).
2. **Format freeze:** a format is frozen only after reference implementations, validators, fixtures (including hostile-archive tests), and the official library integration agree, with the evidence recorded at the relevant quality gate.
3. **Compatibility vs. validity:** a syntactically valid package can still report incompatibility with an older runtime, an unsupported platform, or a missing capability. Incompatible items must explain why rather than silently altering behavior.
4. **Downgrade and rollback safety:** rolling back an application binary must not destructively overwrite newer configuration or package data; read-only inspection and safe fallback (default theme, safe mode) remain available where feasible.
5. **Library repository cadence:** `Astraive/glint-library` releases and registry snapshots are versioned independently of the application and declare which schema and entrypoint versions they target ([docs/release-process.md](docs/release-process.md)).
