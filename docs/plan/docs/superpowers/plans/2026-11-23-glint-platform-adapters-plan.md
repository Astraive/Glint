# Glint Cross-Platform Adapters Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver honest, tested desktop integration on Windows, Linux and macOS for Glint-managed surfaces, wallpaper and basic system state.

**Architecture:** Use `glint-platform` typed interfaces and one Rust adapter per OS/session; make each feature capability-gated. Validate native rendering host strategy in tiny spikes before generalizing. A single theme UI bundle is reused where feasible; native OS-owned UI stays untouched.

**Tech Stack:** Rust, Windows Win32/COM/WebView2, Linux Wayland/X11/GTK/WebKitGTK/D-Bus/Hyprland IPC, macOS AppKit/WKWebView; platform test harnesses.

**Spec:** `docs/superpowers/specs/2026-11-23-glint-platform-design.md`

## Global Constraints

- OS family is insufficient; detect actual compositor/DE/session, permissions, display setup and available protocols.
- Do not hide/replace Explorer taskbar or GNOME/macOS system UI by default.
- Every native preference change includes snapshot/restore where possible; failures are reported.
- A platform-specific integration must not make independent `.glint` packages unportable.
- Native renderer quality and resources are verified on physical OS builds before marking support.

## Review Focus

- Missing layer-shell on GNOME -> fallback to non-layer surface with an explanation (Task 2).
- Explorer restart with appbar active -> no ghost work area or permanent taskbar hiding (Task 1).
- macOS permission denied on app/window control -> disabled control, not silent success (Task 3).
- Display hotplug/fractional DPI -> surface remains reachable on all three hosts (Task 4).
- External monitor brightness absent -> appropriate unsupported/read-only state (Tasks 1–3).

---

### Task 1: Windows API adapter and appbar host

**Files:** Create `crates/glint-windows/src/lib.rs`, `wallpaper.rs`, `appbar.rs`, `display.rs`, `audio.rs`, `capabilities.rs`; `platform/windows/packaging/`; `crates/glint-windows/tests/native_smoke.rs`.

**Interfaces:** `WindowsAdapter::discover() -> CapabilityMap`, `WallpaperBackend::{get,set,restore}`, `SurfaceBackend::create()` for `Panel/Overlay`, `AudioBackend::volume()`, `set_volume()` when available.

- [ ] Write platform-only failing unit/mocked API tests for wallpaper get/set/restore and appbar registration/work area lifecycle.
- [ ] Run `cargo test -p glint-windows --target x86_64-pc-windows-msvc`; expect fail until implementation, executed on Windows CI runner.
- [ ] Implement documented Win32 COM wallpaper and appbar methods with DPI awareness; integrate restricted WebView2 host after spike.
- [ ] Run Windows manual matrix: two monitors, Explorer restart, fullscreen and recovery; record results.
- [ ] Commit `feat(windows): wallpaper and owned panel surfaces`.

### Task 2: Linux host and session/compositor adapter

**Files:** Create `crates/glint-linux/src/session.rs`, `wayland/layer_shell.rs`, `compositors/hyprland.rs`, `providers/audio.rs`, `providers/network.rs`, `capabilities.rs`; tests in `crates/glint-linux/tests/`; spike report `docs/research/spikes/linux-layer-webview.md`.

**Interfaces:** `LinuxAdapter::discover_session()`, `LayerSurfaceHost::create()`, `HyprlandProvider::snapshot/subscribe`, `CapabilityMap`.

- [ ] Run **spike first**: GTK4 layer-shell + restricted bundled webview content, transparency and input semantics, document pass/fail in research report.
- [ ] Write failing tests for compositor detection, missing layer-shell, Hyprland socket reconnect and unavailable system provider.
- [ ] Implement compositor/session discovery and IPC client, then native layer host only for proven supported path; provide ordinary window fallback.
- [ ] Run `cargo test -p glint-linux`, then actual Arch/Hyprland and KDE/GNOME fallback smoke tests with screenshots/logs.
- [ ] Commit `feat(linux): compositor-aware surfaces and providers`.

### Task 3: macOS owned window and wallpaper adapter

**Files:** Create `crates/glint-macos/src/lib.rs`, `appkit.rs`, `windows.rs`, `wallpaper.rs`, `status_bar.rs`, `capabilities.rs`, tests `crates/glint-macos/tests/native_smoke.rs`; spike report.

**Interfaces:** `MacosAdapter::discover()`, `WallpaperBackend::{get,set,restore}`, `SurfaceBackend::create()`, and `StatusItemBackend` separate from shell surface capabilities.

- [ ] Run small NSWindow + WKWebView spike including multiple Spaces and menu bar safe areas.
- [ ] Write failing tests for per-screen wallpaper, denied permission state and host close/reopen.
- [ ] Implement AppKit main-thread host and valid native methods; leave unexposed methods as unavailable, never fake implementation.
- [ ] Run macOS integration/visual tests on available Intel/Apple Silicon machines; document platform version and signing.
- [ ] Commit `feat(macos): owned windows, status item and wallpaper`.

### Task 4: Unified end-to-end platform contract

**Files:** Modify `crates/glint-platform/src/registry.rs`, `crates/glint-surfaces/src/placement.rs`, `crates/glint-core/src/bootstrap.rs`; create `tests/e2e/platform_capabilities.rs`, `docs/platform-compatibility.md`.

**Interfaces:** `PlatformRegistry::select(session) -> Arc<dyn PlatformAdapter>`, `CapabilityMap` continuously updated, `SurfacePlacement::resolve(displays, requested)`.

- [ ] Write cross-platform failing tests showing theme compatibility and fallbacks for Windows, Linux (Hyprland/GNOME/KDE), macOS, plus monitor unplug and denied capabilities.
- [ ] Run mock matrix tests; expect fail.
- [ ] Wire concrete adapters, capability subscriptions, responsive placement and user-visible unsupported reasons.
- [ ] Verify on physical OS runners and update matrix **only for tested environments**; run all mock/e2e tests.
- [ ] Commit `feat(platform): capability-driven native host selection`.

## Exit criteria

Owned surfaces and wallpaper/system actions work to the degree documented on every tested OS, with explicit differences and safe fallback. Secure UI and native system shell remain intact. Anything not proven becomes experimental or unavailable, not a promise.
