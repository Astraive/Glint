# License policy

This page records which licenses Glint uses for its own code, which licenses it accepts from dependencies, and what must be shipped with a release. It implements REL-002. It is a project policy, not legal advice.

## Glint's own licenses

| Component | License | Status |
|---|---|---|
| Core (this repository: crates, apps, schemas, docs) | MIT, Copyright (c) 2026 Astraive | Decided. See [LICENSE](../../LICENSE); workspace crates set `license = "MIT"`. |
| SDK (`@glint/*` published packages) | Not chosen | **Decision O11: proposed, pending maintainer decision.** |
| Glint library (glint-library repository) | Not chosen | **Decision O11: proposed, pending maintainer decision.** |
| Individual themes, including the default theme | Not chosen | **Decision O11: proposed, pending maintainer decision.** |

[Decision O11](../plan/00-history/decisions-and-open-questions.md#open-product-decisions) ("Which license?") is recorded in the frozen plan as undecided: core, SDK, library and individual themes may have different licenses. Nothing in `docs/plan/` states a choice for the SDK, library or themes, so this page does not invent one. Until the maintainers decide and record it in an ADR:

- do not add a `LICENSE` to an SDK, library or theme package that differs from MIT silently;
- workspace packages that are private and unpublished keep `"license": "MIT"` as they do today;
- theme authors choose their own license for their own themes; the manifest carries a license field (see the plan's manifest description), and Glint does not relicense third-party themes.

Recording the O11 outcome needs an ADR under [docs/adr](../adr/README.md), because it picks a license policy for public artifacts.

## Dependency licenses

The rule applies to anything that is bundled into a Glint release or linked into a shipped binary. Build, lint and test tooling that is not redistributed is held to the same list so that nothing needs a special case later, with the exceptions noted below.

### Allowed

| License (SPDX) | Why it is allowed |
|---|---|
| MIT | Permissive; compatible with MIT core. |
| Apache-2.0 | Permissive with a patent grant; requires notice preservation (see below). |
| Apache-2.0 WITH LLVM-exception | Apache-2.0 with a linking exception; strictly less restrictive. Rust crates only. |
| BSD-2-Clause, BSD-3-Clause | Permissive; attribution only. |
| ISC | Functionally equivalent to MIT. |
| Zlib | Permissive; attribution in source only. |
| Unicode-3.0 | Permissive data and code license used by Unicode crates. |
| MPL-2.0 | **File-level** copyleft. Allowed because it does not extend to the rest of the program: using an unmodified MPL crate as a dependency imposes no obligation beyond shipping the notice. If Glint ever modifies an MPL-licensed file it must publish those modified files. Vendoring and patching an MPL crate therefore needs review first. |
| 0BSD, BlueOak-1.0.0, CC0-1.0 | npm only. Permissive or public-domain-like; allowed so common tooling trees do not need exceptions. |
| Python-2.0 | npm only (`argparse` 2.0.1, a development-only dependency of `json-schema-to-typescript`). PSF-style permissive license; it does not ship. |

### Denied

- GPL (any version), AGPL (any version) and LGPL (any version) for **bundled** dependencies. Strong and network copyleft would impose terms on Glint's distribution that are incompatible with the MIT core, and LGPL's relinking requirements are impractical for statically linked Rust binaries.
- Licenses with no SPDX identifier, "UNKNOWN", proprietary or non-commercial terms (for example CC-BY-NC), and "see license file" entries. These fail the check until a maintainer reviews them.
- Packages with no declared license.

### Dual-licensed dependencies

An `OR` expression passes when at least one alternative is on the allow-list. Glint then relies on that permissive alternative. Today one crate declares `MIT OR Apache-2.0 OR LGPL-2.1-or-later`; Glint uses it under MIT or Apache-2.0, never under the LGPL option. An `AND` expression needs every part allowed.

### Exceptions

There are none. Adding an exception (a `[[licenses.exceptions]]` entry in `deny.toml`, or a change to the npm allow-list) needs a comment naming the package, the license, why it is acceptable and who approved it, and an update to this page in the same change.

## Apache-2.0 NOTICE and attribution

Apache-2.0 section 4 requires that a redistribution include a copy of the license and keep upstream copyright, patent, trademark and attribution notices, including the contents of any `NOTICE` file the upstream work provides. Glint therefore treats every Apache-2.0 dependency as needing attribution in a release.

### UI SDK

Glint's manager and settings UI uses UI SDK per [ADR 0003](../adr/0003-ui-sdk-for-glint-manager-ui.md): `@ui-sdk/react` and `@ui-sdk/core`, licensed Apache-2.0, pinned exactly to `0.1.0-beta.13`.

| Package | Version | License | Status |
|---|---|---|---|
| `@ui-sdk/react` | 0.1.0-beta.13 | Apache-2.0 | **Planned.** Not a dependency yet; the manager app (MGR epic) has not been created. |
| `@ui-sdk/core` | 0.1.0-beta.13 | Apache-2.0 | **Planned.** Not a dependency yet. |

Because they are not installed, they appear only in the "planned" section of the [third-party inventory](third-party-inventory.md) and are not covered by the automated checks yet. When the manager app adds them:

1. Add them to the workspace with the exact pin; the npm license check and the generated inventory then include them automatically.
2. Ship UI SDK's `LICENSE` text and its `NOTICE` file, if the package has one, in the release's third-party notices (the planned `THIRD_PARTY_NOTICES.md`, owned by the release epic REL-009 for the release bundle).
3. Do not modify or strip copyright headers from UI SDK code. If any UI SDK file is ever modified, mark it as changed as section 4(b) requires.
4. Do not use UI SDK's name or marks to imply endorsement of Glint (section 6).

## What is checked automatically

| Check | Covers | Run |
|---|---|---|
| `cargo deny check` with [deny.toml](../../deny.toml) | Rust: licenses (allow-list above), RustSec advisories and yanked crates, wildcard version requirements, registry sources (crates.io only, no git dependencies) | CI job "Dependency policy" in `.github/workflows/license-and-dependency-policy.yml` |
| `pnpm licenses:check` | npm: every installed package, production and development, against the same allow-list; `OR`/`AND` expressions evaluated; unknown or missing licenses fail | CI job "License inventory" |
| `pnpm licenses:inventory:check` | The committed [third-party inventory](third-party-inventory.md) matches `Cargo.lock` and the installed pnpm tree | CI job "License inventory" |

To refresh the inventory after a dependency change, run `pnpm install --frozen-lockfile` and `pnpm licenses:inventory`, then commit the result.

### Limits

- The inventory records declared package metadata. It does not read license files, so a package that declares the wrong license is not caught.
- It is not an SBOM in SPDX or CycloneDX format. Producing one for a release is planned work under REL-009.
- Advisory data comes from the live RustSec database, so the "Dependency policy" job can start failing without any change in this repository; it also runs on a weekly schedule for that reason.
- Only dependencies that exist are checked. Dependencies of the future manager app, SDK and library are not covered until added.
