# Governance

## Governance Model

Glint (`Astraive/Glint`) currently operates under a **single-maintainer bootstrap governance model** led by organization owner and repository administrator **`@NeerajCodz`** (`https://github.com/NeerajCodz`).

This model keeps accountability clear while the project moves from its planning archive ([docs/plan/README.md](docs/plan/README.md)) through foundation and feasibility work (milestone `M0`) toward initial implementation.

## Roles and Responsibilities

### Project Maintainer (`@NeerajCodz`)

- Owns repository settings, branch protection, CI trust boundaries, release signing key custody (once configured), and final merge authority on `main`.
- Signs off on milestone gates (`QA-M0` through `QA-M5`, and `LQA-L0` through `LQA-L3` for the library) and verifies that no destructive-data, archive-escape, permission-bypass, or security failure is waived.
- Triages private security reports under [SECURITY.md](SECURITY.md) and enforces [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
- Coordinates cross-repository contracts with [`Astraive/glint-library`](https://github.com/Astraive/glint-library).

### Contributors

- Submit scoped changes aligned with [CONTRIBUTING.md](CONTRIBUTING.md) and [docs/issue-conventions.md](docs/issue-conventions.md).
- Maintain architectural boundaries (local-first, capability-aware adapters, data-only package defaults), provide truthful test and manual verification evidence, and update affected docs and schemas in the same PR.

## Decision Making and Architectural Changes

- **Decision register:** product decisions are tracked in [docs/plan/00-history/decisions-and-open-questions.md](docs/plan/00-history/decisions-and-open-questions.md). Substantial shifts to core contracts, package formats, capability semantics, or trust boundaries require an ADR or migration entry and synchronized updates across affected plans, schemas and tests before release.
- **Format and compatibility contracts:** app, package, manifest, SDK, registry, profile, config, and IPC versions evolve independently per [VERSIONING.md](VERSIONING.md).
- **Truthful status and support promotion:** no platform or milestone gate is promoted to `Supported` or `Complete` without recorded verification evidence.

## Review Policy and Bootstrap Branch Protection

- **Protected `main` branch:** changes land through Pull Requests using squash merges, linear history, resolved review conversations, and required passing CI checks ([docs/github-setup.md](docs/github-setup.md)).
- **Dependabot PR triage (`REL-009`):** CI enforces the strict `type(scope): subject [TICKET-ID]` PR title convention without bot bypasses. Because Dependabot cannot add trailing ticket IDs automatically, the maintainer triages automatic dependency PRs and renames the PR title to include `[REL-009]` (or a dedicated security/upgrade ticket ID) before merge, re-running checks on `pull_request` `edited`.
- **Single-maintainer bootstrap exception:** because GitHub does not permit a sole maintainer to approve their own pull requests, required PR approvals are set to `0` during bootstrap. Every PR must still satisfy `.github/CODEOWNERS` review expectations, the structured PR checklist, and required CI checks. Once additional trusted maintainers hold write access, required approvals and strict `CODEOWNERS` enforcement will increase to `>= 1`.
- **High-scrutiny areas:** changes touching archive extraction, package trust and provenance, the permission bridge and renderer isolation, local IPC, unsafe Rust/FFI, CI workflows, or release/signing logic require explicit security review.

## Licensing and Repository Separation

- **Code and specification license:** `Astraive/Glint` repository files are licensed under the MIT License ([LICENSE](LICENSE)). Themes, fonts, icons and other assets in other repositories or packages carry their own licenses; any future licensing adjustment requires explicit provenance review ([docs/release-process.md](docs/release-process.md)).
- **Companion library separation:** `Astraive/glint-library` is an independent public repository with its own governance, license policy and per-asset provenance records. It is the default official catalog but never the only permitted source: independent Git repositories and standalone `.glint` files remain first-class.
