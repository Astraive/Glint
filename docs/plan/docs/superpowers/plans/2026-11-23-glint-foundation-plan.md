# Glint Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an offline-first Glint skeleton that can validate/import/export a `.glint` package, track installed themes and recover from invalid state.

**Architecture:** Greenfield Rust workspace with source-independent package pipeline, per-user storage, versioned state and a thin CLI. No native shell replacement in this plan; create mock adapters to keep all three OSes compileable.

**Tech Stack:** Rust stable, Cargo workspace, Serde/serde_json, ZIP reader with safe validation, SQLite (`rusqlite` or SQLx selected before start), Tokio IPC, TS schema generation later.

**Spec:** `docs/superpowers/specs/2026-11-23-glint-platform-design.md`

## Global Constraints

- Must target Windows, Linux and macOS; no Linux-only assumptions in core crates.
- `.glint` portable, offline, self-contained; no installation scripts or native executable loading.
- `glint-library` is optional default source; direct Git/.glint installers are first class.
- User can restore default theme after any failed import; do not modify protected OS shell.
- `glint.json`/API formats are *proposed* and should be stabilized by tests before public distribution.

## Review Focus

- Archive entries containing `..`, absolute, Windows drive/UNC or symlink paths are rejected by package tests in Task 2.
- ZIP bombs/oversized uncompressed packages are rejected by package tests in Task 2.
- Duplicate theme ID from a different origin is not overwritten silently in Task 4.
- Corrupt previous SQLite migration/transaction causes safe recovery and no active-theme loss in Task 4.
- Feature unavailable/permission required never masquerades as success in Task 3.

---

### Task 1: Cargo workspace, paths and shared schema

**Files:** Create `Cargo.toml`, `apps/cli/Cargo.toml`, `apps/cli/src/main.rs`, `crates/glint-schema/Cargo.toml`, `crates/glint-schema/src/lib.rs`, `crates/glint-platform/src/capabilities.rs`, `schemas/glint-theme-v1.schema.json`, `tests/package-fixtures/valid-minimal/glint.json`.

**Interfaces:** Produces `ThemeManifest {schema_version, kind, id, version, compatibility, surfaces}` and `parse_theme_manifest(&str) -> Result<ThemeManifest, ManifestError>`. Produces `Availability` enum matching spec.

- [ ] **Step 1: Write failing parser tests** for minimal valid manifest, absent ID, unsupported schemaVersion and incorrect kind; tests in `crates/glint-schema/tests/manifest.rs`.
- [ ] **Step 2: Run** `cargo test -p glint-schema`; expect tests fail before implementation.
- [ ] **Step 3: Implement manifest types, parser and semantic validation** in `crates/glint-schema/src/lib.rs`; initialize workspace and CLI crate.
- [ ] **Step 4: Run** `cargo fmt --all --check && cargo test -p glint-schema`; expect pass.
- [ ] **Step 5: Commit** `feat(schema): establish v1 theme manifest and workspace`.

### Task 2: Strict `.glint` ZIP reader and packer

**Files:** Create `crates/glint-package/Cargo.toml`, `src/lib.rs`, `src/reader.rs`, `src/writer.rs`, `src/limits.rs`, `tests/security.rs` and hostile ZIP fixture generation in tests (do not commit giant bombs).

**Interfaces:** Consumes `ThemeManifest`; produces `PackageCandidate {manifest, extracted_root, sha256}`. Functions `inspect_archive(path: &Path, limits: &PackageLimits) -> Result<PackageCandidate, PackageError>` and `pack_directory(src: &Path, dest: &Path) -> Result<PackageDigest, PackageError>`.

- [ ] **Step 1: Write failing tests** for valid round-trip and rejection of absolute/traversal/symlink entries, duplicate case-folded paths, decompression ratio and entry count limits.
- [ ] **Step 2: Run** `cargo test -p glint-package`; expect fail.
- [ ] **Step 3: Implement bounded extract and deterministic-enough writer** with SHA256, normalized paths, immutable staging and manifest reference validation.
- [ ] **Step 4: Run** `cargo test -p glint-package`; verify no writes outside temporary package directory and passing fixtures.
- [ ] **Step 5: Commit** `feat(package): safely import and export glint archives`.

### Task 3: Capability and package-source model

**Files:** Create `crates/glint-platform/src/lib.rs`, `crates/glint-platform/src/error.rs`, `crates/glint-sources/src/lib.rs`, `crates/glint-sources/src/local.rs`, `crates/glint-sources/tests/local_source.rs`.

**Interfaces:** `Availability::{Supported,ReadOnly,PermissionRequired,Experimental,Unavailable}`; `CapabilityId`; `ThemeSource::{Registry,Git,LocalArchive,LocalDirectory,ArtifactUrl}`; `PackageSource::stage(...) -> Result<StagedPackage, SourceError>`.

- [ ] **Step 1: Test** local archive resolve, missing source and unsupported capability response (not a fake success).
- [ ] **Step 2: Run** `cargo test -p glint-platform -p glint-sources`; expect fail.
- [ ] **Step 3: Implement shared errors, local resolver, and mock capability provider**; defer Git/HTTP backend to library plan.
- [ ] **Step 4: Run** tests and ensure core crate has no OS-specific dependency.
- [ ] **Step 5: Commit** `feat(core): add source and capability contracts`.

### Task 4: Installer store, provenance and rollback

**Files:** Create `crates/glint-storage/src/lib.rs`, `src/migrations/001_init.sql`, `crates/glint-installer/src/lib.rs`, `src/install.rs`, `src/rollback.rs`, `tests/transaction.rs`; use OS-resolved config/data/cache paths.

**Interfaces:** `Installer::install(candidate: PackageCandidate, origin: SourceOrigin) -> Result<InstalledPackage, InstallError>`, `Installer::remove(package_key)`, `Installer::recover_pending()`. `InstalledPackage` contains ID, version, source, digest, immutable path.

- [ ] **Step 1: Write tests** for idempotent install, same-ID conflicting origin, interrupted transaction, database migration failure and recovery of previously active theme.
- [ ] **Step 2: Run** `cargo test -p glint-installer -p glint-storage`; expect fail.
- [ ] **Step 3: Implement SQLite migrations, lock/journal and staged atomic store**, with safe cleanup and locking.
- [ ] **Step 4: Run** all relevant tests in temporary directories; verify installed archive survives offline and rollback leaves old state.
- [ ] **Step 5: Commit** `feat(installer): transactional offline package storage`.

### Task 5: CLI and visible safe recovery

**Files:** Modify `apps/cli/src/main.rs`; add `apps/cli/src/commands/theme.rs`, `commands/doctor.rs`, `tests/cli_roundtrip.rs`; create bundled `themes/default/glint.json` and minimal default assets.

**Interfaces:** `glint theme import PATH`, `list`, `export ID`, `remove ID`, `use default`, `glint safe-mode`, `glint doctor`. CLI invokes foundation crates; avoids native OS modifications.

- [ ] **Step 1: Write failing CLI integration test** import/list/export/reimport with verified manifest/hash and invalid archive not changing previous active package.
- [ ] **Step 2: Run** `cargo test -p glint-cli --test cli_roundtrip`; expect fail.
- [ ] **Step 3: Implement minimal commands and default theme fallback**, clear errors and OS path handling.
- [ ] **Step 4: Run** `cargo test --workspace` and smoke CLI on Windows/Linux/macOS runners; expect pass or record platform blockers before claiming support.
- [ ] **Step 5: Commit** `feat(cli): offline theme package workflow and safe mode`.

## Plan exit criteria

A valid `.glint` imports without internet, appears in local inventory, exports without personal data, malicious archives are blocked, collisions are handled, and safe-mode can always select default. This plan does **not** create executable shell windows or pretend to finish all-OS integration.

## Implementation handoff

Read full spec and decisions register; obtain user approval of this plan before writing product code. Run tasks in order with TDD, visible review of interfaces and a final cross-platform CI gate.
