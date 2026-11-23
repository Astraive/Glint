<!--
Documentation PR template.
PR title format: type(scope): subject [TICKET-ID], for example "docs(plan): clarify M0 spike criteria [DOC-001]".
Co-authored-by trailers are allowed; contributors still commit under their own identity.
-->

**Ticket:** <!-- Stable ticket ID (e.g. DOC-001) and `Refs #N` / `Closes #N` once published; omit `#N` if unpublished. -->

## What and why

<!-- Which documentation files changed, why, and which design/infrastructure gaps they close. -->

## Documentation scope

- [ ] Product, architecture, or platform plans (`docs/plan/`)
- [ ] Theme, package, or SDK documentation
- [ ] Root policy or governance files (`README.md`, `CONTRIBUTING.md`, `SECURITY.md`, etc.)
- [ ] CI / repository setup guides (`docs/github-setup.md`, `docs/backlog-*.md`)

## Acceptance criteria

- [ ] Links and heading fragments resolve cleanly inside the repository (`python scripts/check-repository.py`).
- [ ] Status claims remain truthful (planning-stage status, unimplemented Rust workspace, and unverified platform hosts are never described as complete or supported).
- [ ] No external `.issues/` backlog records, coverage registers, ledgers, or generated reports are committed or linked as GitHub URLs.

## Test evidence

<!-- Exact checks run (e.g. `python scripts/check-repository.py`) and observed output. -->

## Checklist

- [ ] No secrets, tokens, personal data, unredacted local paths, or external `.issues/` artifacts included.
- [ ] Documentation does not restate Glint invariants inconsistently (offline core, sandboxed renderer with permission-scoped bridge, data-only `.glint` packages, capability honesty, safe fallback and rollback).
- [ ] Navigation indexes or cross-references affected by this change are updated in the same PR.
