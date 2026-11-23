# ADR 0004: Plan docs are a frozen baseline; changes go through ADRs

- **Status:** Accepted
- **Date:** 2026-11-23
- **Ticket:** DOC-001
- **Supersedes:** none
- **Superseded by:** none
- **Plan label affected:** none
- **Plan sections affected:** [document maintenance contract](../plan/00-history/decisions-and-open-questions.md#document-maintenance-contract)

## Context

`docs/plan/` is the verbatim planning archive (snapshot 2026-11-23). Editing it in place would erase the record of what was proposed and what was later decided.

## Decision

Files under `docs/plan/` are kept verbatim and are **not edited**. Corrections, refinements and promotions of `RECOMMENDED` or `RESEARCH REQUIRED` items are recorded as ADRs in `docs/adr/`, following the [ADR process](README.md). Accuracy labels keep their meaning and are never promoted silently.

## Consequences

- Where the plan and an Accepted ADR disagree, the ADR wins; the ADR names the plan sections it affects.
- Dependent specs outside `docs/plan/` (schemas, architecture docs, guides) are updated in the same pull request as the ADR.
- Inconsistencies in the plan are fixed through an ADR, not by hiding them.

## Alternatives considered

- Editing the plan in place: loses history and the baseline.
- A parallel rewritten plan: duplicates content and drifts.

## Migration notes

None: no public contract changes.
