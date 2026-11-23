# Glint Theme Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow a theme author to code complete Glint-owned React surfaces (panel, launcher, control center), package them, run safely and preview them with native actions disabled by default.

**Architecture:** Keep surface manifest contracts platform-neutral; host runtime isolates executable UI and exposes a narrowly permissioned SDK bridge. Build a default theme that uses the same API as third-party themes. Native shell-placement adapters are delivered in separate platform plan.

**Tech Stack:** Rust, `glint-surfaces`, `glint-renderer`, TypeScript/React, Vite or equivalent build tool, `@glint/sdk`, per-OS system WebViews (feasibility-gated).

**Spec:** `docs/superpowers/specs/2026-11-23-glint-platform-design.md`

## Global Constraints

- Every public surface API must define fallback and capability semantics on Windows, Linux and macOS.
- Untrusted theme JS cannot access daemon's native commands except allowlisted SDK methods authorized per package/surface.
- Consumer installs only compiled assets; no Node runtime or network required.
- Surface crash cannot crash OS-owned shell or erase the known-good profile.
- No false claim of native taskbar/control center replacement.

## Review Focus

- Untrusted theme code tries an unauthorized IPC command; Task 2 must return permission denial.
- Renderer navigates to external remote JS or tries arbitrary file URI; Task 2 must block it.
- Theme missing optional surface; Task 4 must use default implementation.
- Theme renderer crashes five times; Task 4 must stop restarting and enter safe mode rather than loop.
- Hook transitions from supported to unavailable mid-session; Task 3 must update controls immediately.

---

### Task 1: Surface manifest and host interface

**Files:** Create `crates/glint-surfaces/src/lib.rs`, `src/placement.rs`, `src/catalog.rs`, `tests/surface_manifest.rs`, `packages/types/src/surfaces.ts`; update theme schema.

**Interfaces:** `SurfaceKind`, `SurfaceDefinition {id, kind, entrypoint, placement, behavior, capabilities}`; `SurfaceBackend::{create,show,hide,destroy}`; `SurfaceCatalog::resolve(profile,kind)`.

- [ ] Write failing tests for standard surface ID, custom namespaced ID, invalid entrypoint, missing surface, monitor placement constraints.
- [ ] Run `cargo test -p glint-surfaces`; expect fail.
- [ ] Implement parser/placement contracts and default fallback lookup with no OS-specific branches in core.
- [ ] Run crate tests, typegen fixture check; expect pass.
- [ ] Commit `feat(surfaces): define portable theme-owned surface contracts`.

### Task 2: Restricted renderer and SDK bridge

**Files:** Create `crates/glint-renderer/src/lib.rs`, `src/asset_origin.rs`, `src/bridge.rs`, `src/supervisor.rs`, `crates/glint-permissions/src/lib.rs`, `tests/bridge_security.rs`.

**Interfaces:** `RendererHost::mount(surface, immutable_assets, grants) -> Result<RendererHandle, HostError>`; `Bridge::authorize(token, method, payload) -> Result<ApprovedCall, PermissionError>`; `RendererSupervisor::on_crash(surface) -> RecoveryAction`.

- [ ] Write failing tests for invalid/expired token, undeclared capability, `file://` navigation, remote script/asset attempt, rate-limit exhaustion and cross-package grant confusion.
- [ ] Run `cargo test -p glint-renderer -p glint-permissions`; expect fail.
- [ ] Implement local asset origin, CSP/navigation policy, permission-aware narrow IPC, process supervision and crash counter.
- [ ] Run security tests; review actual host environment limitations before declaring sandbox sufficient.
- [ ] Commit `feat(renderer): sandbox theme UI and scope native access`.

### Task 3: SDK subscriptions, hooks and simulated providers

**Files:** Create `packages/sdk/src/client.ts`, `packages/sdk/src/react.ts`, `packages/sdk/src/types.ts`, `packages/sdk/tests/react-hooks.test.tsx`, `crates/glint-events/src/lib.rs`, `crates/glint-system/src/mock.rs`.

**Interfaces:** `glint.capabilities.get(id)`, `glint.surfaces.toggle(id)`, `useClock`, `useAudio`, `useBattery`, `useWorkspaces`, `useCapabilities`; `SystemEvent` and provider snapshot/subscribe.

- [ ] Write failing hook tests for ready/permission-denied/unavailable/reconnect; assert no unauthorized action from read-only state.
- [ ] Run `pnpm --filter @glint/sdk test`; expect fail.
- [ ] Implement typed bridge client with subscription cleanup, snapshots, request IDs, mock data and event coalescing.
- [ ] Run package tests, Rust event tests and TypeScript `tsc --noEmit`; expect pass.
- [ ] Commit `feat(sdk): capability-aware React hooks and event client`.

### Task 4: Default theme, theme crash fallback, preview

**Files:** Create `themes/default/glint.json`, `themes/default/src/panel.tsx`, `launcher.tsx`, `control-center.tsx`, `src/theme.css`, `apps/desktop/src/pages/preview.tsx`, `crates/glint-core/src/profile_apply.rs`, tests `tests/e2e/theme_fallback.rs`.

**Interfaces:** `ThemeRuntime::preview(package_id, capability_fixture)`, `ThemeRuntime::activate_profile(profile_id)`, `ThemeRuntime::fallback(surface_id)`.

- [ ] Write failing tests: missing `notification` uses default, broken `panel` resets to default, repeated crash enters safe mode, preview never mutates OS state.
- [ ] Run relevant Rust and frontend test suites; expect fail.
- [ ] Build a minimal accessible default theme and preview screen with mock data and recovery controls.
- [ ] Run end-to-end preview + crash tests, visual smoke on one host and CI; expect pass.
- [ ] Commit `feat(theme): ship safe default surfaces and preview`.

### Task 5: Creator toolchain

**Files:** Create `packages/create-glint-theme/templates/react-full-shell/`, `apps/cli/src/commands/theme_dev.rs`, `theme_build.rs`, `theme_validate.rs`, `theme_pack.rs`, `examples/react-theme/`, tests `tests/e2e/creator_roundtrip.rs`.

**Interfaces:** `glint theme create NAME --template react`, `dev`, `build`, `validate`, `pack`; compiled `dist/<surface>/index.html` entrypoints; package reader from foundation plan.

- [ ] Write failing integration test scaffolding a theme, building it, verifying all declared outputs exist, packing, importing offline without Node at consume time.
- [ ] Run `cargo test -p glint-cli --test creator_roundtrip`; expect fail.
- [ ] Implement scaffold/build wrappers, HMR preview, manifest entry rewriting and validation errors.
- [ ] Run clean scaffold build and independent offline consumer import on available OS runners; expect pass.
- [ ] Commit `feat(creator): programmable theme scaffold and package workflow`.

## Exit criteria

A third party can write different JSX for a launcher and control-center, build and distribute the result as `.glint`; the runtime displays it safely through Glint-owned surfaces, guards OS actions and shows fallback when unavailable. No requirement for native taskbar patching.
