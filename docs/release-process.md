# Release process, signing, and distribution

> **Current release status:** **No application binary, installer, signed release artifact, SBOM, SDK package, or published theme exists yet.** Neither `Astraive/Glint` nor `Astraive/glint-library` has configured production signing key custody, and no release or pre-release has been published. This document describes the intended process; it is not a record of completed releases.

## Scope and release independence

- **`Glint` (application, daemon, shell hosts, CLI, SDK)** and **`glint-library` (official public theme library and registry index)** release independently.
- Library releases declare the manifest schema, package format and SDK entrypoint versions they target rather than tracking moving branch tips.
- Every release artifact is accompanied by an explicit statement of verified platforms and exact environments, checksums, and third-party notices, never blanket OS claims. Linux and macOS are compile-only in CI until real hardware verification exists; Windows 11 is the only current verification host.

## Semantic versioning and draft formats

Version numbers track each compatibility surface independently, so incrementing the application version does not implicitly break portable formats. The authoritative table is in [VERSIONING.md](../VERSIONING.md):

| Surface | Versioning rule | Current status |
|---|---|---|
| Application and CLI (`glint`) | SemVer (`0.x` prototypes, alpha, beta, then `1.0.0` at the end of the credible-first-release phases) | Unimplemented |
| `.glint` package format | Integer `schemaVersion`; frozen only after conformance evidence | Draft |
| `glint.json` manifest | Integer schema version | Draft |
| SDK (`@glint/sdk`) | SemVer plus `entrypointVersion` | Draft |
| Registry index, profile, config schemas | Integer schema versions with migrations | Draft |
| Local IPC protocol | Protocol version | Draft |

## CI separation and signing key custody

- **Untrusted PR workflows:** run formatting, repository and link checks, portable tests, and schema checks with `contents: read`, SHA-pinned actions, and **no** production secrets or privileged `pull_request_target` execution.
- **Protected release and signing jobs:** separated from untrusted PR caches and artifacts; require maintainer review and approval on protected tags or commits before signing, uploading immutable assets, or generating build provenance attestations.
- **Key custody status:** production code-signing certificates, Apple Developer ID and notarization credentials, and any registry or package signing keys are **currently unconfigured**. Branch protection will not require nonexistent signing checks until real key custody and release workflows exist. Trust and signing design remains an open decision in the plan.
- **Offline limitations:** because Glint is local-first, disconnected machines cannot learn about revoked or updated registry entries until they refresh; this limitation must be documented alongside any trust feature.

## Rights, fonts, and dependency notices

Both repositories start from a public **MIT** `LICENSE` baseline. Any future release distribution must record:

- **Rust and JavaScript dependencies:** committed lockfiles, a license inventory, and advisory checks (a clean scan is not proof of zero vulnerabilities). `REL-009` owns dependency freshness.
- **Fonts, icons and artwork:** distinguish built-in redistributable assets from user-supplied or system assets, retain required notices, and never ship proprietary brand assets without rights. No font binaries or artwork assets are shipped today.
- **Official themes:** `glint-library` keeps per-asset provenance and license records.

## Platform distribution lifecycles

Platform artifacts are not yet defined or built. Each platform's packaging, signing, installer, and uninstall behavior must be designed and verified on real hardware before a release claims support, and each must record exact tested environments. Linux packaging and macOS signing and notarization are blocked on hardware access.

## Manual native verification and rollback

Before signing off the final release gate of a phase:

- Execute clean-machine offline install, first launch, import of a `.glint` package, preview, activation, upgrade, non-destructive downgrade inspection, and uninstall while running and after a crash.
- Verify uninstall removes owned startup entries and stops owned hosts without deleting user data or overwriting an externally changed desktop wallpaper or setting.
- Verify immutable package versions and prior release hashes remain recoverable if an update fails.
- Verify safe mode and the default theme recover the desktop after a faulty theme.

## Private vulnerability reporting

Releases publish a private security contact and hash-based advisory policy without silent crash upload or data collection. See [SECURITY.md](../SECURITY.md). `.github/release.yml` defines release-note categories.
