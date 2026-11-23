# Glint Library and Open Distribution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make theme discovery/install work through the project-owned public `glint-library`, independent Git repositories and offline `.glint` files with one secure installer.

**Architecture:** Maintain `glint-library` as a **separate Git repository** containing curated packages/index metadata. `glint` holds generic registry/Git/archive source adapters and caches; no data or permission assumption depends on official repo ownership.

**Tech Stack:** Rust package/source/registry crates, Git HTTPS fetch with optional sparse/partial strategy or prebuilt artifacts, JSON registry index, GitHub Actions for official CI, Tauri manager UI/CLI.

**Spec:** `docs/superpowers/specs/2026-11-23-glint-platform-design.md`

## Global Constraints

- `glint-library` is official/public but never mandatory for users or publishers.
- `.glint` must install/export fully offline and remain usable if GitHub fails.
- Author can host a single theme in any compatible Git repository or independent multi-theme registry.
- No fetching and executing arbitrary installation shell scripts; all package paths/integrity validated.
- No full repository clone for simple browsing; updates opt-in by default.

## Review Focus

- Registry offline and no cache -> installed themes still work, browse reports offline (Task 2).
- External Git repo has root `glint.json` without `registry.json` -> treat as direct theme source (Task 3).
- Git branch moves between index and download -> digest/commit pin prevents confused install (Task 3).
- Same package ID from different origins -> provenance preserved, no silent replacement (Task 4).
- Imported local `.glint` has no upstream -> no automatic remote update (Task 4).

---

### Task 1: Create separate official `glint-library` project

**Files:** In `glint-library` repo create `README.md`, `registry.json`, `themes/mono/glint.json`, `themes/mono/preview.png` (original licensed asset), `CONTRIBUTING.md`, `.github/workflows/validate-themes.yml` and `scripts/build-registry.*`.

**Interfaces:** `RegistryIndex {schemaVersion, registryId, name, packages[]}` where entries include ID, version, path, preview and generated integrity digest.

- [ ] Write a failing index validator test for broken path, missing package manifest, duplicate ID/version and unsupported license metadata.
- [ ] Run official-library validation script on intentionally invalid fixtures; expect fail.
- [ ] Implement small original sample themes and CI index generation/validation based on manifests.
- [ ] Run validator and build clean index twice; verify stable output and links.
- [ ] Commit `feat(library): publish official curated registry schema`.

### Task 2: Registry resolution and metadata cache

**Files:** In `glint` create `crates/glint-registry/src/lib.rs`, `src/index.rs`, `src/cache.rs`, `src/config.rs`, tests `crates/glint-registry/tests/registry.rs`, user config defaults at `config/default.toml`.

**Interfaces:** `RegistryClient::list(name) -> Result<Vec<RegistryEntry>, RegistryError>`, `RegistryClient::refresh(name)`, `RegistryConfig {default, entries}`.

- [ ] Write failing tests for default `official` resolution, custom registry addition, offline cache list, invalid registry schema and unavailable remote.
- [ ] Run `cargo test -p glint-registry`; expect fail.
- [ ] Implement validated metadata fetch/cache and stale/offline status without cloning all theme assets.
- [ ] Run registry tests with local HTTP fixtures and disconnect simulation; expect pass.
- [ ] Commit `feat(registry): discover official and custom registries`.

### Task 3: Direct Git and theme source installer

**Files:** Create `crates/glint-sources/src/git.rs`, `src/artifact.rs`, tests `tests/git_source.rs`, update `crates/glint-installer/src/install.rs` for provenance.

**Interfaces:** `GitSource::stage(url,ref,subpath) -> Result<StagedPackage,...>` returning resolved commit and content digest; `ThemeSource::Git` and `Registry` both feed existing `PackageCandidate`.

- [ ] Write failing tests for root theme repo, registry subfolder, invalid/ref missing, no `glint.json`, malicious symlink, and moving branch versus pinned commit.
- [ ] Run `cargo test -p glint-sources`; expect fail.
- [ ] Implement Git HTTPS fetch (partial/sparse where supported; fallback prebuilt/download), temporary staging, pinned commit provenance and strict path restrictions.
- [ ] Run tests with temporary local Git servers or fixtures, including failed network and large repo path selection; expect pass.
- [ ] Commit `feat(sources): install independent git-hosted themes`.

### Task 4: Update, provenance and custom source CLI

**Files:** Modify `apps/cli/src/commands/registry.rs`, `theme.rs`, `crates/glint-storage/src/migrations/002_origin.sql`, `crates/glint-installer/src/updates.rs`, tests `tests/e2e/source_update.rs`.

**Interfaces:** `glint registry add/sync/list`, `glint theme install official/name`, `glint theme install https://...`, `glint theme import file.glint`, `glint theme update --check`; update engine returns change set including permission deltas.

- [ ] Write failing tests for source origin collision, local archive no-update behavior, manual update with changed permissions and preserve-old-version rollback.
- [ ] Run installer/CLI tests; expect fail.
- [ ] Implement source lock and manual update pipeline with approval API and clear CLI errors.
- [ ] Run tests and a no-account/no-internet local import smoke; expect pass.
- [ ] Commit `feat(distribution): multi-source installs and safe updates`.

### Task 5: GUI discovery and contribution docs

**Files:** Create `apps/desktop/src/pages/discover.tsx`, `src/pages/installed.tsx`, `src/components/theme-card.tsx`, `src/components/source-install.tsx`, tests `apps/desktop/src/pages/discover.test.tsx`; add `docs/publishing.md`.

**Interfaces:** Gallery lists cached/remote packages with platform compatibility badges; author repo install, `.glint` drag/drop import; source disclosure and explicit permission confirmation.

- [ ] Write failing UI tests for offline state, external source install, preview before applying, unsupported platform badge and author/source labeling.
- [ ] Run UI tests; expect fail.
- [ ] Implement gallery and multi-source installation dialogs through the same daemon API, with accessible keyboard/focus semantics.
- [ ] Run UI tests and manual GUI install of public Git theme and local `.glint`; expect pass.
- [ ] Commit `feat(discover): open theme distribution GUI`.

## Exit criteria

A theme is browsable from `glint-library` by default, installable from arbitrary Git without official approval, installable offline from `.glint`, and updateable with provenance/permission diff/revert. The app remains functional when the official library is offline.
