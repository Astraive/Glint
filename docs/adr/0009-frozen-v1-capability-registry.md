# ADR 0009: Frozen v1 capability registry

- **Status:** Proposed
- **Date:** 2026-11-24
- **Ticket:** CAP-005
- **Supersedes:** none
- **Superseded by:** none
- **Plan label affected:** RECOMMENDED (the capability namespace list) and RESEARCH REQUIRED ("Which permissions/capability names will be frozen for SDK v1?", capability names only)
- **Plan sections affected:** [Capability namespaces](../plan/02-architecture/capabilities-permissions-and-errors.md#capability-namespaces), [Open design choices requiring evidence](../plan/06-engineering/risks-feasibility-and-tradeoffs.md#open-design-choices-requiring-evidence) (question 4)

> **Maintainer sign-off needed.** This ADR is *Proposed*. The registry is
> implemented and enforced (`crates/glint-platform/src/registry.rs`, tests in
> `crates/glint-platform/tests/registry.rs`) so it can be reviewed against
> running tests. Points marked **[sign-off]** are choices a maintainer must
> confirm or change before SDK v1 ships.

## Context

The plan lists capability IDs in five namespaces as an example, not a
contract. Before this ADR three places disagreed:

- the plan list (`surface.notification-overlay`, `surface.desktop-widget`, no
  dock, calendar or OSD surfaces);
- the implemented surface catalog (SRF-001), which names one `surface.<kind>`
  per standard kind plus `surface.custom`;
- `glint-package`, which accepted the catalog's IDs plus a provisional
  four-entry `PROVIDER_CAPABILITIES` list (PKG-004) and so refused plan IDs
  such as `native.wallpaper.set` or `host.appbar`.

At runtime an ID was checked only for syntax: an adapter could advertise any
well-formed ID and a theme could declare one as optional and only see it
disabled. Threat-model rows B3.5 and B5.6 require unknown IDs to fail closed.
The [Windows providers spike](../research/spikes/windows-system-providers.md)
(SPK-010) recommends per-sub-capability IDs, a separate SSID capability with
its own privacy scope, and separate battery and power-source IDs.

Still unknown: real availability of brightness, Bluetooth, media, windows,
workspaces and every `native.*` / `host.*` ID on Windows (not yet evidenced),
and everything on Linux and macOS (no hands-on host). Permission names and
grant scopes (CAP-004) are out of scope.

## Decision

1. **One registry, owned by `glint-platform`.** `glint_platform::registry::REGISTRY`
   is the only source of truth for v1 capability IDs. Each entry has an ID,
   an access kind, a sensitivity class, an `IntegrationEffect`, a summary and
   platform notes. The published list is
   [docs/spec/capabilities-v1.md](../spec/capabilities-v1.md), whose table and
   `packages/types/src/capabilities.ts` are generated from the registry and
   drift-checked.
2. **Read/write split.** Access is `read` (observes, includes change events),
   `write` (changes state or performs an action) or `present` (shows
   Glint-owned UI or uses a host window mechanism). Every `surface.*` and
   `host.*` ID is `present`; every `native.*` ID is `write`; reads never change
   OS state and writes always do (tested).
3. **The v1 list is 43 IDs**: 15 `surface.*` (the 14 catalog kinds and
   `surface.custom`), 15 `system.*`, 4 `desktop.*`, 4 `native.*`, 5 `host.*`.
   Compared with the plan list:
   - `surface.notification-overlay` and `surface.desktop-widget` are **not**
     registered; the catalog names `surface.notification` and
     `surface.desktop` are. **[sign-off]**
   - `system.network.ssid` (high sensitivity, permission-required under the
     Windows location consent), `system.battery.read` and
     `system.power.source.read` are added on SPK-010 evidence.
   - Change events are part of the matching `read` ID; no `*.events` or
     `*.device-events` IDs exist. Network details (level, interface type,
     cost) are part of `system.network.read`. **[sign-off]**
4. **Unknown IDs fail closed everywhere.** `CapabilityId::parse` stays a
   syntax check and `glint-schema` stays syntax-only (it cannot depend on
   `glint-platform`). Every consumer applies the registry: the package
   reader/packer refuses unregistered IDs (`GLINT_PACKAGE_UNKNOWN_CAPABILITY`);
   the capability service refuses an adapter report containing one
   (`adapter-failure`, every known capability then temporary-unavailable);
   snapshot lookups report `unknown-capability`; the activation gate refuses a
   theme declaring one, **required or optional** (`theme-incompatible`).
5. **Freeze rules.** Within v1 an ID is never removed, renamed or
   re-purposed, and its access never changes. IDs may be appended, each by an
   ADR. Sensitivity may be raised, and summaries and platform notes corrected,
   without an ADR; lowering sensitivity needs one. Removing, renaming or
   splitting an ID needs registry v2. The process is in
   [Adding a capability](../spec/capabilities-v1.md#adding-a-capability).

This promotes the plan's RECOMMENDED namespace list to a tested contract and
answers question 4 for capability names. Sensitivity classes and platform
notes are descriptive and grant nothing.

## Consequences

- Packages that declare `native.*`, `host.*`, `system.network.ssid`,
  `system.battery.read` or `system.power.source.read` are now accepted (they
  were refused by the provisional set). No ID that was accepted before is
  refused now.
- A theme declaring an unknown optional capability is now refused at
  activation instead of having a control disabled. Packages cannot carry one
  anyway, because the reader refuses it.
- A buggy adapter that advertises an unregistered ID makes discovery fail
  loudly instead of exposing the ID.
- The TypeScript `CapabilityId` type is a closed union, and surface catalog
  types use it, so the SDK (SDK-001) cannot name an unregistered ID.
- Changed in the same change: `crates/glint-platform` (registry, service,
  gate), `crates/glint-surfaces` (catalog IDs resolved through the registry,
  TS types), `crates/glint-package` (acceptance derived from the registry,
  `PROVIDER_CAPABILITIES` removed), `packages/types`, the spec, the
  [module boundaries](../architecture/module-boundaries.md), the
  [threat model](../security/threat-model.md) (B3.5, B5.6) and the
  [package fixtures](../security/package-fixtures.md) notes.
- Follow-ups: CAP-004 maps permissions onto these IDs; native adapters
  (WIN/LNX/MAC) must advertise only registered IDs; spikes for the
  not-yet-evidenced Windows IDs update the platform notes.

## Alternatives considered

- **Keep the plan list verbatim.** Rejected: its surface names disagree with
  the implemented catalog, and it lacks the SSID and battery IDs the spike
  showed are needed.
- **Registry in `glint-schema`, so the parser rejects unknown IDs.** Rejected:
  capabilities are the CAP epic's vocabulary, owned by `glint-platform`;
  `glint-schema` must not depend on it, and the parser's job is syntax. Every
  consumer already depends on `glint-platform` directly or through
  `glint-surfaces`.
- **Warn and ignore unknown optional IDs.** Rejected: B5.6 forbids silently
  allowing unknown IDs, and ignoring lets a future host interpret an ID the
  theme author never saw defined.
- **Separate `*.events` IDs.** Rejected for v1: a read without updates is not
  useful to a widget, and events reveal nothing the read does not. A host
  that cannot deliver events reports the read as experimental or polls.
- **Allow adapter-defined extension IDs.** Rejected: an adapter could then
  introduce capabilities no permission or review covers.

## Migration notes

Public capability contract change. The manifest `schemaVersion` stays `1`:
the syntax is unchanged, and the accepted set only grows relative to the
provisional PKG-004 set. `glint_package::PROVIDER_CAPABILITIES` is removed;
use `glint_platform::registry` (or the unchanged `known_capabilities` /
`is_known_capability`). Theme authors who used the plan spellings
`surface.notification-overlay` or `surface.desktop-widget` (never accepted by
the package reader) use `surface.notification` and `surface.desktop`.
