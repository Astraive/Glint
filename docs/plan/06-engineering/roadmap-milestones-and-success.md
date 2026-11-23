# Phased development roadmap — incremental working software

The roadmap is **sequenced by dependency and testability**, not invented calendar dates. Shipping on all three OSes is the overall goal; deep native parity is deliberately phased.

## Phase 0 — Feasibility spikes / proof points

- Native shell surface spike: create an edge panel with web-rendered content on **Hyprland Wayland**, especially GTK + WebKitGTK/`wry` integration and input transparency.
- Windows spike: Win32 appbar + WebView2 in multi-DPI setup, with safe coexistence with Explorer.
- macOS spike: NSWindow desktop-like overlay + WKWebView, Spaces and click-through.
- Git + `.glint` offline installer spike and package validation malicious fixture suite.
- Confirm whether same compiled React entrypoint can render with acceptable visuals across native WebViews.

**Exit:** a written feasibility decision for renderer and host backend, with limitations. No full implementation until these risks are understood.

## Phase 1 — Core + package + safe preview (first real deliverable)

- Rust workspace, manifest schema, `.glint` reader/packer/validator and built-in default theme.
- Per-user daemon, profile storage and typed IPC; CLI to list/import/activate/revert packages.
- Tauri manager with installed themes + local file import and safe preview.
- Mock capabilities/providers for all platforms; basic wallpaper OS adapter where supported.
- Default fallback and recovery mode; no native shell replacement required.

**Exit:** an offline `.glint` can install on Windows/Linux/macOS manager, preview, activate Glint-owned preview surfaces and export/reimport.

## Phase 2 — Programmable surface runtime

- Standardized surface manifest, renderer host, React SDK hooks and event bridge.
- Built-in panel, launcher, widget and control center UI in default theme.
- Permission/capability engine enforced at runtime.
- Creator scaffolding, HMR, build, validate and pack.

**Exit:** third party can create a coded launcher/panel theme and share it as `.glint`, with unsupported controls gracefully disabled.

## Phase 3 — Platform shell integration breadth

- Linux Hyprland layer-shell and workspaces, with additional compositor/DE adapters planned.
- Windows appbar/overlay and practical system providers.
- macOS overlays/status item, desktop widgets and practical system providers.
- Per-monitor geometry, native wallpaper and safe integration requests.

**Exit:** meaningful Glint-managed shell surfaces on all three OS families, with published capability matrix and tests.

## Phase 4 — Official library and open community

- `glint-library` official public Git registry, browsing UI, previews, PR validation.
- Arbitrary Git URL install and custom registries, robust caching, pinned versions/updates.
- Independent packaging/publishing docs, creator samples and community contribution policy.

**Exit:** author can publish on own Git repo or `.glint` and another user can install without a Glint account.

## Phase 5 — Power-user functionality

- Theme composition across surfaces, per-monitor/workspace profiles, advanced widgets.
- Additional Linux KDE/GNOME integration, X11 adapters, app theme/cursor packs.
- Performance/permission audits, plugin API evolution, optional secure WASM extensions.
- Discovery ecosystem/optional marketplace only after open package system stabilizes.

## MVP slice recommendation

If resources are constrained, prioritize **one complete vertical slice**: create a React clock+panel, package it to `.glint`, import offline, preview, display in a safe managed window, use mock/real time, persist across daemon restart, remove/revert safely. This proves format, SDK, IPC, renderer and profile lifecycle before building dozens of widgets.

## Measurement plan

| Signal | Why it matters |
|---|---|
| Install success and error distribution (locally observable) | Package interoperability |
| Time from scaffold to functional custom panel | Creator ergonomics |
| Crash-safe restore success | Trust and safety |
| Supported capability count per tested OS config | Honest portability |
| Idle memory, CPU and renderer count | Desktop performance |
| Offline import/export round-trip | Open distribution |

No telemetry collection should be enabled by default just to measure these. Manual benchmarks, opt-in diagnostics or local test data are sufficient early.
