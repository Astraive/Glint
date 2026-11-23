# Security documentation

Design-time security material for Glint. To report a vulnerability, use the
private route in [SECURITY.md](../../SECURITY.md), never a public issue.

| Document | Ticket | Status |
|---|---|---|
| [Threat model](threat-model.md) | SEC-001 | Revision 1, draft for review. Almost every control is Planned; the manifest parser is the only implemented product control |

## Maintaining the threat model

- Keep every threat row's status honest: **Implemented** needs a product code
  path and a named test, **Prototyped** means spike code only, **Planned**
  means a design statement and an owner ticket.
- When a ticket lands a control, update its row and the
  [evidence index](threat-model.md#evidence-index) in the same pull request.
- Re-run the model when a [review trigger](threat-model.md#review-triggers)
  fires, and bump its revision.
- Spike evidence lives in [docs/research/spikes](../research/spikes/README.md);
  design sources live in [docs/plan](../plan/README.md), which this folder
  never edits.
