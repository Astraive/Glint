# ADR 0011: Manager UI stack: Tauri 2, React 19, UI SDK and Vite, with exact pins

- **Status:** Accepted
- **Date:** 2026-11-25
- **Ticket:** MGR-001
- **Supersedes:** ADR 0003
- **Superseded by:** none
- **Plan label affected:** RECOMMENDED ("Manager: `glint-desktop` (Tauri 2 + React/TS proposed)"; the manager UI component library)
- **Plan sections affected:** [System architecture](../plan/02-architecture/system-architecture.md), [Glint app areas](../plan/01-product/brand-and-design-language.md#glint-app-areas-recommended), [Accepted palette](../plan/01-product/brand-and-design-language.md#accepted-palette-from-earlier-discussion), [SDK and React authoring](../plan/04-themes/sdk-and-react-authoring.md)

## Context

[ADR 0003](0003-ui-sdk-for-glint-manager-ui.md) chose UI SDK for Glint's own
manager and settings UI, pinned to `0.1.0-beta.13`, before any manager code
existed. MGR-001 scaffolds the manager, so the rest of the stack (window
host, framework, bundler), the exact versions, the import rule and the
upgrade policy for a beta library have to be fixed and checkable. The plan
proposes `glint-desktop` as an on-demand Tauri 2 + React/TypeScript app that
talks to the core over `glint-ipc` and never manages sources itself.

Facts at decision time (checked with `npm view` and `cargo metadata` on
2026-11-25):

- `@ui-sdk/react` and `@ui-sdk/core` are Apache-2.0, published by
  vercel-labs/ui; the newest release is `0.1.0-beta.16`, and `0.1.0-beta.13`
  is still published. `@ui-sdk/react` declares peers `react >=19`,
  `react-dom >=19`, `@types/react >=19.2.10` and `@ui-sdk/core
  ^0.1.0-beta.13`. It has no root export: every component is its own
  subpath (`@ui-sdk/react/dialog`, `@ui-sdk/react/tabs`, ...). Components
  are unstyled behaviours; `@ui-sdk/core/web.css` is an optional base.
  Neither package ships a NOTICE file.
- UI SDK adds its few behaviour rules (an `ui-sdk` cascade layer) through a
  constructable stylesheet when the page has no CSP nonce, so a strict
  `style-src 'self'` works without `'unsafe-inline'`.
- Tauri `2.12.7` (MIT OR Apache-2.0) hosts the system webview (WebView2 on
  Windows) and has a compile-time capability system: with an app manifest,
  each app command needs an explicit permission in a capability file.
  It needs WebKitGTK on Linux, which the hosted CI runners do not have
  installed, and the manager is only hands-on tested on Windows
  ([ADR 0002](0002-windows-11-only-hands-on-test-host.md)).

## Decision

1. **Stack.** The manager is `apps/desktop`: a Tauri 2 host
   (`apps/desktop/src-tauri`, crate `glint-desktop`) and a React 19 +
   TypeScript UI built with Vite (pnpm package `@glint/desktop`). The UI is
   embedded in the binary from `apps/desktop/dist`; the window never loads
   a dev server or any remote URL.
2. **Exact pins.** npm versions are exact in `apps/desktop/package.json`
   (no `^` or `~`) and locked in `pnpm-lock.yaml`:

   | Package | Pin | License |
   |---|---|---|
   | `@ui-sdk/react` | `0.1.0-beta.13` | Apache-2.0 |
   | `@ui-sdk/core` | `0.1.0-beta.13` | Apache-2.0 |
   | `react`, `react-dom` | `19.3.1` | MIT |
   | `@types/react`, `@types/react-dom` | `19.3.0` (meets `>=19.2.10`) | MIT |
   | `@tauri-apps/api` | `2.12.5` | Apache-2.0 OR MIT |
   | `vite` | `8.3.8` | MIT |
   | `@vitejs/plugin-react` | `6.3.0` | MIT |
   | `vitest` | `5.1.1` | MIT |
   | `typescript` | `5.9.3` | Apache-2.0 |

   Rust: `tauri = "2.12.7"` (default features off, `wry` only) and
   `tauri-build = "2.7.6"`, locked in `Cargo.lock` like every other crate.
   The JS API and the crate stay on the same 2.x minor line.
3. **Per-component imports only.** Code imports UI SDK by component subpath
   (`@ui-sdk/react/button`, `@ui-sdk/react/tabs`, ...) and `@ui-sdk/core`
   for primitives such as `createColorScheme`. `@ui-sdk/core/web.css` may be
   imported only as a base under Glint's own styles; it is not used today.
4. **Brand theming.** Glint's brand palette (Obsidian, Graphite, Gunmetal,
   Titanium, Silver, Chrome, Frost, White, Muted; no green) lives as CSS
   variables in `apps/desktop/src/theme/brand.css` and is mapped onto the
   semantic `--glint-*` names of the [design token contract](../architecture/design-tokens.md)
   with CSS `light-dark()`. Dark mode is UI SDK's `createColorScheme` with
   `Preferences` (`data-color-scheme` on `<html>`, `system` follows
   `prefers-color-scheme`). A palette lint test fails on any green value.
5. **Native boundary.** The Rust side is thin: it reads the core over
   `glint-ipc` and exposes only allow-listed, read-only Tauri commands
   (`daemon_status`, `daemon_ping`), each named in the app manifest and
   granted in `capabilities/main.json`; no Tauri core plugin permission is
   granted. Navigation outside the embedded origin is cancelled, and the
   CSP is `default-src 'none'` with `'self'` for scripts, styles, images and
   fonts and only Tauri's IPC origin for `connect-src`.
6. **Platforms.** Tauri is a `cfg(windows)` dependency. On Linux and macOS
   the crate builds and tests its portable IPC bridge and the binary reports
   the platform as unsupported. This keeps the workspace CI green without
   WebKitGTK and claims nothing about those platforms.
7. **Beta upgrade policy (owned by REL-009).** UI SDK stays on an exact
   beta pin. Upgrading (to `0.1.0-beta.16` or later) is a deliberate,
   reviewed change: bump `@ui-sdk/react` and `@ui-sdk/core` together, read
   their changelog for breaking changes, run the manager typecheck, tests,
   build and a hands-on launch on the Windows host, refresh the licence
   inventory and `apps/desktop/NOTICE`, and record the new pin here or in a
   superseding ADR. Dependabot may propose the bump; it is never merged
   automatically. REL-009 owns the general release-time dependency and
   notice policy this sits in.
8. **Not a theme requirement.** UI SDK is an implementation choice for
   Glint's own app UI. Third-party theme authors may use any UI library or
   none; nothing in the package format, manifest, theme SDK or capability
   contract depends on UI SDK.

This promotes the plan's RECOMMENDED manager stack to a decision on the
evidence of the MGR-001 scaffold: it builds and launches on the Windows 11
host and its tests run in CI.

## Consequences

- ADR 0003 is superseded: this ADR restates its decision (UI SDK, exact
  `0.1.0-beta.13` pin, React 19, not a theme requirement) and adds the
  stack, the import rule, theming and the upgrade policy.
- UI SDK and React become production npm dependencies: they appear in the
  generated [third-party inventory](../legal/third-party-inventory.md) as
  `production`, `pnpm licenses:check` covers them, and
  [apps/desktop/NOTICE](../../apps/desktop/NOTICE) carries the Apache-2.0
  attribution ([license policy](../legal/license-policy.md#ui-sdk)).
- Tauri adds about 340 crates to `Cargo.lock` across all targets (Windows dependencies are
  built; Linux and macOS ones are only locked). The Windows CI job builds
  them and runs the UI build first so the real window is compiled; the
  crate compiles with `cfg(glint_desktop_no_ui)` when `apps/desktop/dist`
  is missing, so a plain workspace build still works.
- Manager webview hardening is partly done (CSP, capability allow-list,
  navigation guard); safe rendering of untrusted registry metadata and
  preview images is still open (threat model B7.4).
- Follow-ups: MGR-002 to MGR-011 build the pages behind the navigation;
  A11Y-001 audits keyboard and screen reader use; COR-004 event streaming
  can replace the 5-second status poll.

## Alternatives considered

- **Keep ADR 0003 and add a narrower ADR for the host only:** two records
  for one decision; the pins, imports and upgrade policy belong together.
- **Pin `0.1.0-beta.16` (newest):** the user's decision named beta.13 and it
  is still published; moving is a reviewed upgrade under the policy above.
- **Electron:** bundles Chromium (large, separate patch stream); Tauri uses
  the WebView2 runtime the Windows host already needs for theme surfaces.
- **Build Tauri on every OS in CI:** needs WebKitGTK packages on Linux
  runners and would suggest support that was never tested.
- **A Cargo feature instead of `cfg(windows)`:** the default workspace
  build would never compile the Tauri code, and `cargo deny` would skip the
  optional dependency tree.

## Migration notes

None: no public contract changes. The manager is a new, unreleased app;
replacing UI SDK later affects only `apps/desktop`.
