# ADR 0001: Single-process MVP with separate crates

- **Status:** Accepted
- **Date:** 2026-11-23
- **Ticket:** DOC-001
- **Supersedes:** none
- **Superseded by:** none
- **Plan label affected:** RECOMMENDED (process model)
- **Plan sections affected:** [process model and IPC](../plan/02-architecture/process-model-and-ipc.md), [monorepo and module boundaries](../plan/06-engineering/monorepo-and-module-boundaries.md)

## Context

The plan describes a multi-process design (core, daemon, shell, hosts, CLI). Early work needs to prove feasibility on one host quickly, and IPC contracts are not yet stabilized by tests.

## Decision

The MVP runs as a **single process**. Code is nevertheless split into **separate crates** along the boundaries in [module boundaries](../architecture/module-boundaries.md) (core, daemon, shell, hosts, CLI, schema), so the crates can later be moved behind process boundaries without rewriting their interfaces. Shared core code never contains platform-specific code.

## Consequences

- No IPC protocol is promised publicly for the MVP; the plan's IPC command names stay `RECOMMENDED`.
- Crate boundaries are enforced from the start; in-process calls must go through the same traits a future IPC layer would use.
- A multi-process split needs its own ADR and the evidence behind it.

## Alternatives considered

- Multi-process from day one: more isolation, but premature before IPC contracts and spike results exist.
- A single crate: faster initially, but makes a later split costly and blurs the platform boundary.

## Migration notes

None: no public contract changes.
