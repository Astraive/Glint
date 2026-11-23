# ADR 0005: Conventional commits with external ticket IDs

- **Status:** Accepted
- **Date:** 2026-11-23
- **Ticket:** DOC-001
- **Supersedes:** none
- **Superseded by:** none
- **Plan label affected:** none
- **Plan sections affected:** none

## Context

Work is planned in an external backlog (outside the repository) with stable ticket IDs. Commits and pull request titles need to trace to those tickets.

## Decision

Commit subjects and pull request titles use the form `type(scope): subject [TICKET-ID]`, where `TICKET-ID` comes from the external backlog (for example `DOC-001`). Enforcement is by `scripts/check-conventions.py` (commit-msg hook and CI title check). `Co-authored-by:` trailers are allowed. Backlog files never enter the repository. See [issue conventions](../issue-conventions.md) and [development](../development.md).

## Consequences

- Every change traces to a ticket; release notes can be generated from subjects.
- Work without a ticket must get one before it is committed.

## Alternatives considered

- Free-form commit messages: no traceability.
- Mirroring the backlog inside the repository: forbidden by repository policy.

## Migration notes

None: no public contract changes.
