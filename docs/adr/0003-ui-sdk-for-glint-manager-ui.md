# ADR 0003: UI SDK for Glint's own manager and settings UI

- **Status:** Superseded
- **Date:** 2026-11-23
- **Ticket:** DOC-001
- **Supersedes:** none
- **Superseded by:** [ADR 0011](0011-manager-ui-stack-tauri-react-ui-sdk.md)
- **Plan label affected:** RECOMMENDED (manager UI stack)
- **Plan sections affected:** [SDK and React authoring](../plan/04-themes/sdk-and-react-authoring.md)

## Context

Glint ships its own manager/settings application (Tauri). It needs a component library with a consistent design language.

## Decision

Glint's **own** manager and settings UI uses [UI SDK](https://ui-sdk.dev): npm packages `@ui-sdk/react` and `@ui-sdk/core`, licensed Apache-2.0. The version is **pinned exactly** to `0.1.0-beta.13` (no range, no caret), and it requires React 19 or newer. It is a beta, so upgrades are deliberate and reviewed.

UI SDK is **not required for third-party theme authors**: it is an implementation choice for the first-party manager, not part of the public theme SDK, manifest or capability contract.

## Consequences

- Beta software: breaking changes between betas are expected; the exact pin and lockfile guard against surprise upgrades.
- Third-party themes must work without UI SDK; nothing in the package format or theme SDK may depend on it.
- Its Apache-2.0 license and notices must be tracked in dependency and licensing checks.

## Alternatives considered

- Building the component set in-house: slower for the MVP.
- Other third-party UI libraries: not chosen at this snapshot.

## Migration notes

None: no public contract changes. Replacing the library later affects only the manager app.
