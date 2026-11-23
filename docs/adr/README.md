# Architecture decision records (ADRs)

ADRs are the additive change log for decisions about Glint. The planning archive under [`docs/plan/`](../plan/README.md) is the **frozen baseline** (snapshot 2026-11-23): it is never edited to record a change. Anything that revises, narrows, confirms or promotes a plan statement is recorded here instead, and the ADR links back to the plan section it affects.

## Decision register

| ADR | Title | Status | Date |
|---|---|---|---|
| [0001](0001-single-process-mvp-with-separate-crates.md) | Single-process MVP with separate crates | Accepted | 2026-11-23 |
| [0002](0002-windows-11-only-hands-on-test-host.md) | Windows 11 is the only hands-on test host | Accepted | 2026-11-23 |
| [0003](0003-ui-sdk-for-glint-manager-ui.md) | UI SDK for Glint's own manager and settings UI | Accepted | 2026-11-23 |
| [0004](0004-plan-docs-frozen-changes-via-adrs.md) | Plan docs are a frozen baseline; changes go through ADRs | Accepted | 2026-11-23 |
| [0005](0005-conventional-commits-with-ticket-ids.md) | Conventional commits with external ticket IDs | Accepted | 2026-11-23 |
| [0006](0006-package-identity-and-origin-collisions.md) | Package identity and cross-origin collision policy | Proposed | 2026-11-23 |
| [0007](0007-dirs-for-os-path-resolution.md) | `dirs` for OS configuration, data and cache paths | Accepted | 2026-11-23 |
| [0008](0008-rusqlite-with-bundled-sqlite-for-state.md) | rusqlite with bundled SQLite for the state database | Accepted | 2026-11-23 |

Every new ADR must add a row to this table in the same change. The original plan-era register of what was decided versus only recommended stays at [`docs/plan/00-history/decisions-and-open-questions.md`](../plan/00-history/decisions-and-open-questions.md); ADRs 0001 to 0005 record the decisions already made at that snapshot.

## When to write an ADR

Write one when a change:

- alters, contradicts, narrows or confirms a statement in `docs/plan/` (including promoting a `RECOMMENDED` or `RESEARCH REQUIRED` item to a decision);
- changes a **public contract**: the manifest/`glint.json` schema, the `.glint` package format, the SDK API, the capability/permission model, or IPC command names;
- picks or changes a platform support policy, test-host policy, toolchain, or a major dependency;
- changes repository process (commit rules, branching, release rules).

Do not write one for routine implementation inside an agreed design, bug fixes, or wording edits to non-plan docs.

Accuracy labels (`CONFIRMED`, `RECOMMENDED`, `RESEARCH REQUIRED`, `NOT GUARANTEED`) keep their plan meaning. Never promote a recommendation silently: promotion is itself an ADR, backed by evidence (a spike report, test result or explicit owner decision).

## Statuses

| Status | Meaning |
|---|---|
| Proposed | Under discussion; not binding. Implementation must not depend on it. |
| Accepted | Binding. The register row, the dependent specs and the code agree with it. |
| Superseded | Replaced by a later ADR, named in the `Superseded by` field. The text is kept unchanged for history. |

## Numbering and files

- Four-digit, zero-padded, sequential numbers; the next number is one above the highest in the register. `0000` is reserved for the template.
- File name: `NNNN-short-kebab-title.md`. Never renumber or reuse a number.
- An accepted ADR is not rewritten. To change a decision, add a new ADR that supersedes it, set the old status to `Superseded` and fill in `Superseded by`. Only the status line, the link and typo fixes may change in an old ADR.

## Process

1. Copy [`0000-template.md`](0000-template.md) to the next number with status `Proposed`.
2. Open it in a pull request titled `docs(adr): <subject> [TICKET-ID]`.
3. On acceptance, set status and date, add the register row above, and update every dependent spec, doc or schema in the same pull request. Public manifest, SDK or capability changes also need the contract docs updated and a migration note (see the template).
4. If an ADR replaces an earlier one, update that ADR's status and the register in the same pull request.

Do not edit anything under `docs/plan/` as part of an ADR; the ADR is the record of the difference.
