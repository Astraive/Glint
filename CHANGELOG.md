# Changelog

All notable changes to this repository will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and future releases will follow [VERSIONING.md](VERSIONING.md) and [docs/release-process.md](docs/release-process.md).

## [Unreleased]

**Repository status:** planning archive and repository-infrastructure baseline. **No application code, binary, SDK package, or published release tag exists yet.**

### Added

- Architecture decision records: [docs/adr/](docs/adr/README.md) with the process, a template and ADRs 0001 to 0005 recording decisions already made (`[DOC-001]`).
- Initial repository MIT license ([LICENSE](LICENSE), `Copyright (c) 2026 Astraive`).
- Glint planning and project knowledge archive under [docs/plan/](docs/plan/README.md): history and decision register, product vision, architecture, platform notes, theme/package specifications, distribution design, engineering roadmap and work breakdown, examples, research, and Superpowers design spec and implementation plans (snapshot 2026-11-23). This is design material, not implemented or tested software.
- Repository infrastructure baseline (`[PRJ-001]`): governance and community files ([README.md](README.md), [CONTRIBUTING.md](CONTRIBUTING.md), [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md), [SECURITY.md](SECURITY.md), [SUPPORT.md](SUPPORT.md), [GOVERNANCE.md](GOVERNANCE.md), [VERSIONING.md](VERSIONING.md)), `.editorconfig`, `.gitattributes`, `.gitignore`.
- Opt-in local Git hooks ([.githooks/README.md](.githooks/README.md)) and read-only repository checks (`scripts/check-repository.py`, `scripts/check-conventions.py`, `scripts/check-powershell.ps1`) (`[PRJ-001]`).
- Repository operations guides ([docs/development.md](docs/development.md), [docs/issue-conventions.md](docs/issue-conventions.md), [docs/release-process.md](docs/release-process.md)) (`[PRJ-001]`).
- GitHub configuration, backlog validation and publication tooling, and their guides ([docs/github-setup.md](docs/github-setup.md), [docs/backlog-validation.md](docs/backlog-validation.md), [docs/backlog-publication.md](docs/backlog-publication.md)) (`[PRJ-001]`).
- `glint-schema` crate with a strict parser for the *proposed* `glint.json` v1 theme manifest, the proposed [JSON Schema](schemas/glint-theme-v1.schema.json) and a minimal valid fixture (`[SCH-001]`).
