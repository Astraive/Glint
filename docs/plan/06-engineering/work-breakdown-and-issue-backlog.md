# GitHub issue-ready backlog by epics and testable slices

This document is a **proposed issue breakdown**. No GitHub issues have been created. Each item includes context, outcome and acceptance gate; map to issues/sub-issues later when a repository exists.

## Labels (recommended)

- `type/feature`, `type/research`, `type/bug`, `type/docs`, `type/security`, `type/test`
- `priority/p0`, `priority/p1`, `priority/p2`, `priority/p3`
- `area/core`, `area/platform`, `area/shell`, `area/renderer`, `area/sdk`, `area/theme`, `area/registry`, `area/packaging`, `area/ux`
- `platform/windows`, `platform/linux`, `platform/macos`, `platform/all`
- `size/s`, `size/m`, `size/l` (estimated after code exists)
- `status/blocked`, `status/needs-spike`, `status/ready`

## Epic A — Package standard and secure install [P0]

**A1 Schema and fixtures** — Define formal `glint.json` JSON Schema for theme metadata/surfaces/capabilities, with valid/invalid examples. _Accept:_ schema validator gives stable diagnostics; Rust and TS fixtures agree.

**A2 `.glint` pack/unpack** — ZIP builder/reader with path normalization and resource budgets. _Accept:_ deterministic-enough package round-trip; malicious fixture suite rejected.

**A3 Package origin/provenance** — Track source, digest and version in local lock state. _Accept:_ same ID from two origins doesn't silently overwrite.

**A4 Unified installer** — Read official/custom registry, Git and local archive via common staged package. _Accept:_ same manifest installed regardless of source type.

**A5 Rollback journal** — Atomic package state, apply snapshots. _Accept:_ injected interruption leaves previous theme active and intact.

## Epic B — Daemon/state/IPC [P0]

**B1 Rust workspace/bootstrap** — binaries, crates, CI and path handling. _Accept:_ builds on target OS runners, no hardcoded HOME paths.

**B2 Per-user daemon single instance** — lock/IPC, clean shutdown. _Accept:_ second invocation connects instead of corrupting DB.

**B3 Profile store and composition** — base + surface override resolution. _Accept:_ deterministic fixtures and missing-package fallback.

**B4 Versioned typed IPC** — request/response errors + subscription snapshots. _Accept:_ reconnect and denial tests.

**B5 Capability service** — dynamic availability/permission statuses. _Accept:_ generic mock adapter can exercise all cases.

## Epic C — Surface renderer and SDK [P0/P1]

**C1 Surface contract** — standard IDs, placement, focus, monitor attachment. _Accept:_ parse and normalize all standard surface definitions.

**C2 Safe preview host** — mock data, immutable asset origin, no native actions. _Accept:_ sample theme renders without system changes.

**C3 Native host spikes** — Windows appbar/WebView2, Linux layer-shell/WebKitGTK, macOS NSWindow/WKWebView. _Accept:_ screenshots, setup, code and pass/fail matrix in repo.

**C4 Permission-enforced bridge** — renderer cannot call unauthorized daemon method. _Accept:_ malicious JS fixture blocked at runtime.

**C5 SDK hooks** — clock, battery, volume, workspaces, capabilities. _Accept:_ React test renderer changes correctly on supported/denied/unavailable events.

**C6 Default safe theme** — panel, launcher and simple widget. _Accept:_ shipped renderer always falls back to these if user theme broken.

## Epic D — Platform adapters [P1]

**D1 Windows wallpaper + appbar** — default wallpaper restore; appbar work-area coexistence and scaling. _Accept:_ integration tests/multi-monitor screenshots.

**D2 Linux session/compositor detection** — supports Wayland/X11 distinction, layer-shell presence, Hyprland socket discovery. _Accept:_ correct matrix and reconnect after compositor restart.

**D3 Linux system providers** — audio/UPower/network/MPRIS as available. _Accept:_ mock and real provider tests, absent-service fallback.

**D4 macOS wallpaper and overlay** — AppKit main-thread host + wallpaper. _Accept:_ import and preview, non-activating overlay with manual tests.

**D5 KDE/GNOME scope research** — documented adapter options, version matrix, non-destructive prototype. _Accept:_ design review and opt-in plan; not all features promised.

## Epic E — Creator UX and library [P1]

**E1 `glint theme create/dev/build/validate/pack`** — scaffold and bundler. _Accept:_ clean sample theme builds/runs and has no network runtime dependency.

**E2 Manager gallery and file association** — list installed and drag/drop `.glint`. _Accept:_ import offline without CLI.

**E3 Official library index** — public repo, generated registry metadata, PR CI. _Accept:_ 2 sample themes discoverable and downloadable.

**E4 Arbitrary Git/custom registry** — Git URL and registry source support, caching. _Accept:_ test with nonofficial public repo and pinned commit.

**E5 Package update UX** — manual check, preview, permission diff and rollback. _Accept:_ old version preserved on rejection.

## Epic F — Quality/performance/accessibility [continuous]

**F1 Rendering budget audit** — counts webviews, idle CPU/memory and animation perf.

**F2 Cross-platform device matrix** — published tested versions and known limitations.

**F3 Keyboard/a11y controls** — focus, Escape, reduced motion, scaling.

**F4 Safe mode and diagnostics** — crashes, blocked permissions, source metadata and clean support bundle.

## Dependencies / critical path

A1 -> A2 -> A4 -> E3/E4. B1 -> B2/B4 -> C2/C4 -> C5/C6 -> D1/D2/D4. B3 + A3/A5 -> profile apply. C3 native spikes can run early and independently of full runtime; **this is a critical feasibility gate**.

## Definition of done per issue

One independently testable change, relevant unit/integration tests, observable behavior in documentation, no new privileged bypass, platform statement, recovery/rollback check for native changes, and code review. Don't close an issue based only on PR description; tests and actual code change are evidence.
