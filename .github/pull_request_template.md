<!--
Pull request template for Astraive/Glint.
Rules: CONTRIBUTING.md and docs/issue-conventions.md (https://github.com/Astraive/Glint/blob/main/docs/issue-conventions.md).
PR title MUST match: type(scope): subject [TICKET-ID]
  Allowed types: feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert
  Scope: lowercase kebab-case (e.g. docs, core, renderer, windows-host, ci, repo)
  Ticket ID: uppercase prefix and suffix (e.g. [PRJ-001], [REL-009], [SPK-001], [DOC-001])
PRs are squash-merged into main, so the PR title becomes the commit subject.
Co-authored-by trailers are allowed; contributors still commit under their own identity.
Dependabot PRs have no CI title bypass: maintainers triage and rename automatic dependency PR titles to append [REL-009] (or a dedicated ticket ID) before merge, re-triggering the check on `edited`.
Specialized templates are also available via ?template=docs.md and ?template=spike.md.
-->

**Ticket:** <!-- Stable ticket ID (e.g. PRJ-001 or REL-009) and `Closes #N` / `Refs #N` once published on GitHub. Do not invent `#N` numbers prior to issue publication. -->

## What and why

<!-- Summarize what changes, why it is needed now, and which `docs/plan/` specifications or planning contracts it implements. -->

## Platforms affected

- [ ] All / Cross-platform shared
- [ ] Windows
- [ ] macOS
- [ ] Linux (shared)
- [ ] Wayland
- [ ] X11
- [ ] Hyprland
- [ ] KDE Plasma
- [ ] GNOME
- [ ] Sway
- [ ] CLI
- [ ] Documentation, tooling, or CI only

## Acceptance criteria

<!-- Copy the ticket's acceptance criteria and check each one with observed evidence (test name, command output, or verified environment tuple). Leave unfinished items unchecked and name the follow-up ticket. -->

- [ ]

## Test evidence

<!-- Record the exact commands you ran and their observed results (e.g. `python scripts/check-repository.py`, `pwsh -NoProfile -File scripts/check-powershell.ps1`, `python scripts/check-conventions.py --title "..."`). For Rust, renderer, or native host changes once present, record the exact OS build, session/compositor, WebView runtime, GPU/driver, and manual verification steps. Never claim unrun checks passed. -->

## Checklist

- [ ] Only actually executed checks and verified platform tuples are claimed in Test evidence.
- [ ] Glint invariants remain intact: the core works offline; theme code runs in the sandboxed renderer and reaches the system only through the permission-scoped SDK bridge; no executable install scripts or native binaries are placed in `.glint` packages.
- [ ] Capability honesty: unsupported or denied capabilities are reported as such and degrade gracefully; no platform support is claimed beyond verified tuples.
- [ ] A safe fallback and rollback path (default theme, safe mode, previous profile) exists for any change that affects activation, installation, or shell surfaces.
- [ ] Affected documentation (`docs/`), schemas, or fixtures are updated in this PR.
- [ ] No secrets, tokens, proprietary fonts or icons, trademarked assets, unredacted local paths, SSIDs, or window titles, and no external `.issues/` backlog files or reports are included.
- [ ] Any added third-party dependencies, fonts, icons, or test fixtures include verified licenses and notices.
