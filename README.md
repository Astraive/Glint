# Glint

> **Project status:** **Planning archive and repository-infrastructure baseline (pre-implementation).**
> There is **no application code, Rust workspace (`Cargo.toml`), binary, installer, SDK package, or published `.glint` theme** in this repository yet. Everything described under `docs/plan/` is a design proposal or planned engineering specification, not shipped or tested software.

Glint is a planned open-source, local-first, Rust-powered, cross-platform desktop experience and theme platform. It is intended to let someone apply a whole desktop aesthetic (wallpaper, typography, icons, widgets, panels, launchers, quick settings, notification surfaces and supported native integrations) from a portable theme, and to let theme creators code entire Glint-managed screens with a programmable SDK or ship simpler declarative themes. Glint is **not** a new window manager or compositor and cannot universally replace OS-owned secure or native interfaces.

The target operating systems are Windows, Linux and macOS. Platform behavior differs by OS and desktop environment; the plan treats those differences as capability states rather than blanket promises. Testing hardware today is Windows 11 only: Linux and macOS work is limited to compile-only CI until real hardware verification exists.

---

## Documentation Entrypoints

The product source of truth is the planning archive in [docs/plan/](docs/plan/README.md) (snapshot 2026-11-23). Start here:

- [docs/plan/README.md](docs/plan/README.md) — master plan, reading order, directory map and accuracy labels (`CONFIRMED`, `RECOMMENDED`, `RESEARCH REQUIRED`, `NOT GUARANTEED`)
- [docs/plan/INDEX.md](docs/plan/INDEX.md) — full file index of the archive
- [docs/plan/00-history/decisions-and-open-questions.md](docs/plan/00-history/decisions-and-open-questions.md) — what was explicitly decided versus what is only recommended
- [docs/plan/docs/superpowers/specs/2026-11-23-glint-platform-design.md](docs/plan/docs/superpowers/specs/2026-11-23-glint-platform-design.md) — consolidated platform design specification
- [docs/plan/06-engineering/roadmap-milestones-and-success.md](docs/plan/06-engineering/roadmap-milestones-and-success.md) — phased roadmap
- [docs/plan/06-engineering/work-breakdown-and-issue-backlog.md](docs/plan/06-engineering/work-breakdown-and-issue-backlog.md) — work breakdown behind the backlog

Repository operations guides:

- [docs/development.md](docs/development.md) — current local verification tooling, opt-in hooks, and future workspace workflow
- [docs/issue-conventions.md](docs/issue-conventions.md) — stable ticket IDs, label taxonomy, PR/commit rules, and external backlog separation
- [docs/release-process.md](docs/release-process.md) — release independence, versioning and gates (nothing has been released)
- [docs/github-setup.md](docs/github-setup.md) · [docs/backlog-validation.md](docs/backlog-validation.md) · [docs/backlog-publication.md](docs/backlog-publication.md)

---

## Current Repository Contents

Only actual tracked planning documents, policies, templates, hooks, and validation/configuration scripts live here today:

| Path | Current Contents |
| --- | --- |
| `LICENSE` | MIT License (`Copyright (c) 2026 Astraive`) |
| `docs/plan/` | Planning and knowledge archive (47 files: history, product, architecture, platforms, themes, distribution, engineering, examples, research, and Superpowers specs/plans) |
| `docs/` | Repository operations guides (development, issue conventions, release process, GitHub setup, backlog validation/publication) |
| `scripts/` | `check-repository.py`, `check-conventions.py`, `check-powershell.ps1`, `validate-backlog.py`, `publish-issues.ps1`, `configure-github.ps1` |
| `.github/` | Issue forms, PR template, `CODEOWNERS`, `release.yml`, `dependabot.yml`, `project-management/` static registries, CI workflows |
| `.githooks/` | Read-only `pre-commit` / `commit-msg` hooks and opt-in `install.ps1` ([.githooks/README.md](.githooks/README.md)) |
| Root files | `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, `SUPPORT.md`, `GOVERNANCE.md`, `CODE_OF_CONDUCT.md`, `CHANGELOG.md`, `VERSIONING.md`, `.editorconfig`, `.gitattributes`, `.gitignore` |

> **No fake scaffolds:** Cargo workspaces, crates, TypeScript packages, and placeholder CI jobs are not created until a real implementation ticket needs them. The planned layout in [docs/plan/06-engineering/monorepo-and-module-boundaries.md](docs/plan/06-engineering/monorepo-and-module-boundaries.md) is a proposal.

---

## Current Verification and Local Checks

On a fresh clone with **Python 3.12+** and **PowerShell 7 (`pwsh`)**, you can run the repository's read-only verification tools directly:

```sh
python scripts/check-repository.py
python scripts/check-conventions.py --title "docs(repo): verify convention checker [DOC-001]"
pwsh -NoProfile -File scripts/check-powershell.ps1
```

Contributors can opt in to local Git hooks (configuring only repository-local `core.hooksPath`, without altering global Git settings or author identity):

```powershell
pwsh -NoProfile -File .githooks/install.ps1
```

See [.githooks/README.md](.githooks/README.md) and [CONTRIBUTING.md](CONTRIBUTING.md) for details.

---

## Planning Backlog and Companion Library

- **External local backlog (not pushed to Git):** detailed planning records live **outside** the Git repositories in `../.issues` (`.issues/glint/<ID>/` and `.issues/glint-library/<ID>/`), so unpublished backlog records and derived reports are never committed.
  - Read-only local backlog validation: `python scripts/validate-backlog.py` ([docs/backlog-validation.md](docs/backlog-validation.md))
  - Offline publication plan (`-DryRun`, zero remote writes): `pwsh -NoProfile -File scripts/publish-issues.ps1 -DryRun` ([docs/backlog-publication.md](docs/backlog-publication.md))
  - Stable ticket IDs such as `PRJ-001` are not GitHub issue numbers.
- **Coordination board:** a GitHub Projects v2 board titled "Glint — Master Development Roadmap" under the `Astraive` organization is planned; see [docs/github-setup.md](docs/github-setup.md) for its current configuration status.
- **Companion repository:** [**`Astraive/glint-library`**](https://github.com/Astraive/glint-library) — the official public theme library. It is an optional source: independent Git repositories and standalone `.glint` files are first-class, and Glint must work without any hosted service.

---

## Key Design Invariants (from the plan)

1. **Local-first and optional network:** no mandatory account, backend, or marketplace; installed themes apply offline.
2. **Data-only packages by default:** `.glint` packages never carry arbitrary shell scripts, native binaries or privileged hooks; programmable themes run in restricted renderers behind a permission bridge.
3. **Capability-aware platforms:** each OS adapter reports capability state rather than a blanket "supported" claim; unsupported functions are surfaced, not faked.
4. **Safe fallback:** a built-in default theme and a safe-mode recovery command stay available.
5. **Separated rights layers:** repository source is MIT-licensed ([LICENSE](LICENSE)); individual themes, fonts and icons carry their own licenses.

These are design commitments in [docs/plan/](docs/plan/README.md), not yet verified behavior.

---

## Policies and Governance

- [CONTRIBUTING.md](CONTRIBUTING.md) — workflow, PR/commit rules (`type(scope): subject [TICKET-ID]`), and review checklist
- [SECURITY.md](SECURITY.md) — private vulnerability reporting via [GitHub Security Advisories](https://github.com/Astraive/Glint/security/advisories/new) and security boundaries
- [SUPPORT.md](SUPPORT.md) — current pre-release boundaries and where to ask
- [GOVERNANCE.md](GOVERNANCE.md) — `@NeerajCodz` single-maintainer bootstrap governance and review model
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) — community participation standards
- [VERSIONING.md](VERSIONING.md) — independent application SemVer and draft format versioning
- [CHANGELOG.md](CHANGELOG.md) — `[Unreleased]` baseline summary
