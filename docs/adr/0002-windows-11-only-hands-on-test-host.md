# ADR 0002: Windows 11 is the only hands-on test host

- **Status:** Accepted
- **Date:** 2026-11-23
- **Ticket:** DOC-001
- **Supersedes:** none
- **Superseded by:** none
- **Plan label affected:** CONFIRMED (testing hardware)
- **Plan sections affected:** [OS matrix](../plan/06-engineering/testing-and-quality-gates.md#os-matrix), [test environment policy](../plan/03-platforms/compatibility-matrix.md#test-environment-policy)

## Context

The only hardware available for hands-on testing today is a Windows 11 machine. The plan lists Linux compositors and macOS as target environments.

## Decision

- Windows 11 is the **only** hands-on and visual test host.
- Linux and macOS are **CI-compile-only** (format, lint, build, unit tests on hosted runners) until real hardware exists.
- Environments that have not been exercised are recorded as `not-run`; they are never reported as passing, and no Linux or macOS behavior is claimed as supported until a real test passes.

See the [test matrix](../quality/test-matrix.md) for the current state per environment.

## Consequences

- Linux/macOS native hosts may be written and compiled but are labeled experimental or unverified.
- Adding a hardware environment requires updating the matrix with its pinned details; a new ADR is needed only if this policy changes.

## Alternatives considered

- Emulation or VMs for Linux compositors and macOS: not a faithful test of layer-shell, Spaces or display behavior, so not accepted as evidence.
- Claiming support from compile success: rejected as misleading.

## Migration notes

None: no public contract changes.
