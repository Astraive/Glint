# Development workflow and local tooling

> **Current repository state:** Today's `Glint` repository contains the planning archive under `docs/plan/`, project governance files, `.github/` configuration and templates, `.githooks/`, documentation, and Python/PowerShell tooling. **There is no Rust `Cargo.toml`, application binary, SDK package, or buildable installer yet.** Do not interpret future `cargo`, `npm`, or `glint` CLI examples in the plan as currently runnable commands.

## Current repository tooling and prerequisites

To inspect, validate, and work on the current repository and the local external backlog (`../.issues/`), you need:

- **Python 3.12+** (standard library only; no third-party pip packages are required for repository or backlog checkers).
- **PowerShell 7+** (`pwsh`) for `scripts/publish-issues.ps1`, `scripts/configure-github.ps1`, `scripts/check-powershell.ps1`, and `.githooks/install.ps1`.
- **Git** (configured for LF normalization via [`.gitattributes`](../.gitattributes) and [`.editorconfig`](../.editorconfig)).
- **GitHub CLI (`gh`)**: optional for local development; required only for read-only API inspection or explicitly authorized remote configuration and publication runs.

Windows 11 is the only current verification host. Linux and macOS work is compile-only in CI until real hardware exists. See the [OS test matrix and environment pinning](quality/test-matrix.md).

## Local repository and convention checks

These scripts inspect repository files (excluding Git metadata and build/cache directories) and work on a **fresh clone without requiring `../.issues/`**:

| Check | Exact command | Scope |
|---|---|---|
| Repository integrity and link check | `python scripts/check-repository.py` | Validates JSON syntax, balanced Markdown code fences, relative Markdown links and heading fragments inside the repo (including `docs/plan/`), and ensures no forbidden external backlog or ledger files were committed |
| Commit / PR title conventions | `python scripts/check-conventions.py --message-file <path>` | Validates the Conventional Commit subject format `type(scope): subject [TICKET-ID]` |
| PR title check | `python scripts/check-conventions.py --title "type(scope): subject [TICKET-ID]"` | Mutually exclusive alternative to `--message-file` for checking a proposed pull-request title |
| PowerShell validation | `pwsh -NoProfile -File scripts/check-powershell.ps1` | Parses repository `.ps1` files with PowerShell's AST parser without executing them |
| Crate dependency boundaries | `python scripts/check-crate-boundaries.py` (`--self-test` for the fixture tests) | Needs `cargo`; reads `cargo metadata --locked --no-deps` and fails on spike imports, core-to-adapter edges and OS-binding dependencies in core crates and apps ([module boundaries](architecture/module-boundaries.md#dependency-direction)). CI runs it in the Rust workspace job on Ubuntu |

`Co-authored-by:` trailers are allowed; the convention checker validates only the subject line.

CI runs these checks through the workflows under `.github/workflows/` using SHA-pinned actions, read-only contents permissions, and no `pull_request_target` execution. CI-only tooling is not an application runtime dependency and is not native-platform verification. Static repository and project configuration is documented in [GitHub setup](github-setup.md).

## External backlog regeneration and dry-run commands

The backlog, coverage slices, and derived reports stay strictly outside both Git repositories in `../.issues/` (excluded defensively by [`.gitignore`](../.gitignore)). Full tool behavior is documented in [backlog validation](backlog-validation.md) and [backlog publication](backlog-publication.md).

```bash
# Read-only external backlog validation (zero writes, network-free)
python scripts/validate-backlog.py
python scripts/validate-backlog.py --backlog-root ../.issues --source-root . --json
```

To preview the offline publication plan from `../.issues/` without touching GitHub or creating any local ledger or lock file:

```powershell
pwsh -NoProfile -File ./scripts/publish-issues.ps1 -DryRun
```

Live issue publication requires explicit authorization for its scope. Never copy local reports or `../.issues/` contents into Git.

## Opt-in Git hooks and commit rules

Repository hooks live under `.githooks/` (`pre-commit`, `commit-msg`, and `.githooks/install.ps1`) and are **opt-in** rather than silently forced on clone:

- `.githooks/pre-commit` requires a working Python 3.12+ and PowerShell 7, then runs repository hygiene and PowerShell parsing. It probes `python` and `python3` rather than accepting an unusable Windows Store alias. Installation refuses to replace another custom local hook path.
- `.githooks/commit-msg` invokes `scripts/check-conventions.py --message-file` to enforce the subject format.
- **Contributor identity rule:** commit under your own standard Git author identity (`user.name` / `user.email`). Hooks never force or rewrite identities, and `Co-authored-by:` trailers are permitted to credit shared work.

## Future workspace workflows

The plan proposes a Rust workspace with separate crates for core, daemon, shell, hosts and CLI, plus TypeScript packages for the SDK and manager UI ([monorepo and module boundaries](plan/06-engineering/monorepo-and-module-boundaries.md), [testing and quality gates](plan/06-engineering/testing-and-quality-gates.md)). Once the foundation tickets (`PRJ`, `SPK`) establish the workspace, implementation follows a test-first loop:

1. Write a failing contract test for the target ticket and verify the intended failure.
2. Implement the minimal complete behavior inside the owning crate without leaking platform-specific code into the shared core.
3. Run scoped format, lint and test commands for the touched components, plus manual native host verification on Windows 11 where desktop integration is affected.
4. Keep a recovery path (safe mode / CLI) reachable when testing debug desktop hosts, so a faulty surface never strands the desktop.

## Contributor safety and workflow rules

- Never place `../.issues/` files, `issue.json` records, `publication-ledger.json`, `publication-ledger.lock`, derived backlog reports, personal media, proprietary fonts, or unredacted host paths inside either repository.
- Never commit unverified placeholder manifests, mock packages, or fake CI passes for unimplemented components.
- Record decisions as ADRs under [`docs/adr/`](adr/README.md) (register and process live there); `docs/plan/` is a frozen baseline.
- Do not edit `docs/plan/` to hide inconsistencies; fix them deliberately and keep accuracy labels (`CONFIRMED`, `RECOMMENDED`, `RESEARCH REQUIRED`, `NOT GUARANTEED`) honest.
- Verify internal relative links with `python scripts/check-repository.py` before opening a pull request.
