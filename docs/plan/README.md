# Glint — master plan and complete project knowledge archive

**Snapshot:** 2026-11-23  |  **Status:** planning/reference package, not implemented software  |  **Target:** Windows, Linux, macOS

Glint is a **local-first, Rust-powered, cross-platform desktop experience and theme platform**. It lets someone apply an entire desktop aesthetic—wallpaper, typography, icons, desktop widgets, dock/panels, launchers, quick settings, notification surfaces and supported native integrations—from a portable theme. Theme creators can code whole Glint-managed screens using a programmable SDK, or create simpler declarative themes. Glint is not a new window manager or compositor and cannot universally replace OS-owned secure/native interfaces.

This archive consolidates the Glint discussions in the current conversation **and** the earlier *Assess Rust Theme App* discussion (November 2026), plus explicitly labeled implementation recommendations and research. It is an editable design dossier, **not** a verbatim export of every chat message, nor a claim that the referenced code has been built or tested.

## Read in this order

1. [`00-history/decisions-and-open-questions.md`](00-history/decisions-and-open-questions.md) — what the user explicitly decided vs what remains recommended.
2. [`01-product/vision-and-positioning.md`](01-product/vision-and-positioning.md) — product scope, goals and non-goals.
3. [`02-architecture/system-architecture.md`](02-architecture/system-architecture.md) — process boundaries, components and data flows.
4. [`03-platforms/compatibility-matrix.md`](03-platforms/compatibility-matrix.md) — practical OS guarantees and limits.
5. [`04-themes/theme-system-end-to-end.md`](04-themes/theme-system-end-to-end.md) — how a theme owns screens.
6. [`05-distribution/registry-and-glint-library.md`](05-distribution/registry-and-glint-library.md) — official library versus independent creators.
7. [`docs/superpowers/specs/2026-11-23-glint-platform-design.md`](docs/superpowers/specs/2026-11-23-glint-platform-design.md) — consolidated Superpowers-style design spec.
8. [`docs/superpowers/plans/2026-11-23-glint-foundation-plan.md`](docs/superpowers/plans/2026-11-23-glint-foundation-plan.md) — testable first implementation tasks.

## Directory map

| Directory | Purpose |
|---|---|
| `00-history` | discussion timeline, traceability, terminology and decisions |
| `01-product` | vision, detailed feature inventory, UX, branding, journeys |
| `02-architecture` | processes, IPC, state, renderer, surface abstraction, data and reliability |
| `03-platforms` | Windows, Linux/desktop environments, macOS and capability matrix |
| `04-themes` | authoring, SDK, manifests, `.glint`, composition, extensions, sandboxing |
| `05-distribution` | `glint-library`, Git repositories, package delivery and release workflows |
| `06-engineering` | monorepo, CLI, tests, roadmap, backlog, risks |
| `07-examples` | sample configuration, manifests, theme source and usage flows |
| `08-research` | verified prior art, official technical references and feasibility spikes |
| `docs/superpowers` | formal specs and executable implementation plans |

## Accuracy / decision labels

- **CONFIRMED:** explicitly requested or accepted in Glint discussions.
- **RECOMMENDED:** implementation design proposed in conversation or this document, not yet approved as a hard commitment.
- **RESEARCH REQUIRED:** behavior or platform feasibility must be prototyped and verified on actual systems.
- **NOT GUARANTEED:** OS-native surfaces/functions that Glint cannot unilaterally replace.

Do not silently turn a RECOMMENDED interface, concrete API signature, version, schedule or feature into a CONFIRMED requirement. This is a living specification.

## Product boundary in one sentence

**Glint fully controls Glint-managed UI, orchestrates supported desktop settings, and uses capability-aware OS adapters; themes never get unrestricted native OS access simply because they are installed.**

## Important naming

- `glint`: main application, daemon, CLI, SDK and UI runtime repository.
- `glint-library`: **official public theme library maintained by the Glint project**. It is not a mandatory hosting service and not a synonym for the entire ecosystem.
- Third parties may host their own Git repositories or publish portable `.glint` files, and user profiles may combine several installed themes.
- Glint Studio / Glint Hub are names considered in the earlier discussion, **not confirmed standalone products**.

## Release disclaimer

Examples in this archive illustrate future APIs such as `@glint/sdk`, `glint theme install`, and manifest `schemaVersion: 1`. They are **design examples**, not existing published packages or verified commands.
