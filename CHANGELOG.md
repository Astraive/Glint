# Changelog

All notable changes to this repository will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and future releases will follow [VERSIONING.md](VERSIONING.md) and [docs/release-process.md](docs/release-process.md).

## [Unreleased]

**Repository status:** planning archive and repository-infrastructure baseline. **No application code, binary, SDK package, or published release tag exists yet.**

### Added

- OS test matrix and environment pinning: [docs/quality/test-matrix.md](docs/quality/test-matrix.md), with honest `not-run` rows for Linux and macOS hardware (`[QA-007]`).
- Architecture decision records: [docs/adr/](docs/adr/README.md) with the process, a template and ADRs 0001 to 0005 recording decisions already made (`[DOC-001]`).
- Initial repository MIT license ([LICENSE](LICENSE), `Copyright (c) 2026 Astraive`).
- Glint planning and project knowledge archive under [docs/plan/](docs/plan/README.md): history and decision register, product vision, architecture, platform notes, theme/package specifications, distribution design, engineering roadmap and work breakdown, examples, research, and Superpowers design spec and implementation plans (snapshot 2026-11-23). This is design material, not implemented or tested software.
- Repository infrastructure baseline (`[PRJ-001]`): governance and community files ([README.md](README.md), [CONTRIBUTING.md](CONTRIBUTING.md), [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md), [SECURITY.md](SECURITY.md), [SUPPORT.md](SUPPORT.md), [GOVERNANCE.md](GOVERNANCE.md), [VERSIONING.md](VERSIONING.md)), `.editorconfig`, `.gitattributes`, `.gitignore`.
- Opt-in local Git hooks ([.githooks/README.md](.githooks/README.md)) and read-only repository checks (`scripts/check-repository.py`, `scripts/check-conventions.py`, `scripts/check-powershell.ps1`) (`[PRJ-001]`).
- Repository operations guides ([docs/development.md](docs/development.md), [docs/issue-conventions.md](docs/issue-conventions.md), [docs/release-process.md](docs/release-process.md)) (`[PRJ-001]`).
- GitHub configuration, backlog validation and publication tooling, and their guides ([docs/github-setup.md](docs/github-setup.md), [docs/backlog-validation.md](docs/backlog-validation.md), [docs/backlog-publication.md](docs/backlog-publication.md)) (`[PRJ-001]`).
- `glint-schema` crate with a strict parser for the *proposed* `glint.json` v1 theme manifest, the proposed [JSON Schema](schemas/glint-theme-v1.schema.json) and a minimal valid fixture (`[SCH-001]`).
- Glint [threat model](docs/security/threat-model.md): trust boundaries, assets, actors, STRIDE per boundary and a hazard register with owner tickets, stop conditions and honest Implemented/Prototyped/Planned status; indexed in [docs/security/](docs/security/README.md) (`[SEC-001]`).
- Manifest version negotiation in `glint-schema`: `schemaVersion`, `compatibility.glint`, `compatibility.sdk` and `renderer.entrypointVersion` are compared independently and every failing axis is reported as `theme-incompatible` with a stable `GLINT_COMPAT_*` code; unknown fields stay rejected in security-sensitive sections and are ignored with a `GLINT_MANIFEST_WARN_UNKNOWN_FIELD` warning inside `author`; packages are never migrated. Specified in [docs/architecture/manifest-compatibility.md](docs/architecture/manifest-compatibility.md) (`[SCH-003]`).
- Shared manifest fixture corpus ([tests/package-fixtures/manifests/](tests/package-fixtures/README.md)) with golden expectations, and the `@glint/theme-schema` TypeScript package: types generated from the proposed JSON Schema with a CI drift check, and a validator that matches the Rust parser's verdict, error code and JSON Pointer on every fixture. CI gains a pinned Node job running the parity tests, type check and drift check (`[SCH-002]`).
- `glint-package` crate with `inspect_archive`: a strict, bounded `.glint` reader that identifies archives by magic bytes, copies them into Glint-owned private staging while hashing (SHA-256), rejects traversal, absolute, drive, UNC/device, symlink/hardlink, case-folded duplicate, oversized, over-deep and high-ratio entries before extraction, validates `glint.json` and its referenced files, and extracts into a read-only staging directory, with stable `GLINT_PACKAGE_*` error codes and a generated hostile-fixture suite (`[PKG-001]`).
- `glint-package` `pack_directory`: a deterministic `.glint` packer (sorted entries, fixed 1980-01-01 timestamps and `0644` modes, Deflate, no extra fields) that refuses symlinks, junctions, executables and invalid manifests, skips hidden files, `node_modules` and build-tool files, and returns the archive SHA-256; packing twice is byte-identical and pack → inspect → pack keeps the digest (`[PKG-003]`).
