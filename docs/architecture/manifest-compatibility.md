# Manifest compatibility and version negotiation

Status: **proposed** (SCH-003). This page specifies how a Glint host decides
whether it can load a `glint.json` theme manifest and how the format evolves.
It follows [Evolution/versioning](../plan/04-themes/manifest-and-glint-package-spec.md#evolutionversioning),
[SDK version negotiation](../plan/06-engineering/cli-and-developer-experience.md#sdk-version-negotiation)
and [Protocol evolution](../plan/02-architecture/process-model-and-ipc.md#protocol-evolution).
The reference implementation is `crates/glint-schema` (`src/compat.rs`,
`src/parse.rs`); its tests are `tests/compatibility.rs`. Like the v1 format
itself, these rules may change until stabilized by tests and an ADR.

## Independent version axes

A manifest declares four versions. They are **independent**: none implies
another, and a host compares each one separately.

| Axis | Manifest field | Kind | Host value | Absent in manifest |
|---|---|---|---|---|
| Package format | `schemaVersion` | integer | set of readable format versions (today `{1}`) | invalid (`GLINT_MANIFEST_MISSING_SCHEMA_VERSION`) |
| Glint application | `compatibility.glint` | semver requirement | running Glint version | invalid (`GLINT_MANIFEST_MISSING_FIELD`) |
| SDK | `compatibility.sdk` | semver requirement | bundled `@glint/sdk` version | axis skipped (declarative theme) |
| Renderer entrypoint contract | `renderer.entrypointVersion` | integer | set of loadable entrypoint versions (today `{1}`) | axis skipped (no programmable surfaces) |

Requirements use Cargo-style semver syntax (`>=0.1.0`, `^0.1.0`,
`>=0.1.0, <0.3.0`) and are matched with the `semver` crate. A pre-release host
(`0.2.0-alpha.1`) only matches requirements that name a pre-release of the
same `major.minor.patch`; a theme targeting pre-releases must say so.

## Negotiation order

`negotiate_theme_manifest(text, host)` runs three steps and stops at the first
that fails:

1. **Probe the envelope** (`probe_manifest_versions`). Document-level rules
   apply (size limit, strict JSON, no duplicate keys, object root), then only
   the four fields above are read. Every other field, known or not, is left
   alone, so a manifest written for a newer format can still be explained. A
   missing or malformed envelope is **invalid** (`ManifestError`).
2. **Compare every axis** (`check_versions`). If any axis fails, the result is
   **theme-incompatible** (`IncompatibilityReport`), listing *every* failing
   axis in the fixed order schema, glint, sdk, entrypoint. This happens
   before full validation, because syntax this parser does not know may be
   valid in the newer format the theme targets.
3. **Parse fully** (`parse_theme_manifest_with_diagnostics`). Any v1 rule
   violation is **invalid**; ignored non-sensitive content is returned as
   warnings.

Invalid and incompatible are different outcomes and must not be merged: an
invalid package is broken, an incompatible one may work on another Glint.

### The frozen envelope

To keep step 1 possible, every future `schemaVersion` **must** keep
`schemaVersion`, `compatibility.glint`, `compatibility.sdk` and
`renderer.entrypointVersion` at the same JSON Pointers, with the same types
and meaning. Changing any of them is not a format bump but a new file format.

## Incompatibility reasons

Each reason carries a stable code, the JSON Pointer of the field, what the
theme requires and what the host provides. The category is always
`theme-incompatible` (see
[Capability errors](../plan/02-architecture/capabilities-permissions-and-errors.md#capability-errors)):
block activation and keep the current theme.

| Code | Pointer | Example message |
|---|---|---|
| `GLINT_COMPAT_SCHEMA_VERSION` | `/schemaVersion` | theme requires package format schemaVersion 2; this Glint provides 1 |
| `GLINT_COMPAT_GLINT_VERSION` | `/compatibility/glint` | theme requires Glint >=0.5.0; this Glint provides 0.4.9 |
| `GLINT_COMPAT_SDK_VERSION` | `/compatibility/sdk` | theme requires @glint/sdk >=0.3.0; this Glint provides 0.1.0 |
| `GLINT_COMPAT_ENTRYPOINT_VERSION` | `/renderer/entrypointVersion` | theme requires renderer entrypointVersion 2; this Glint provides 1 |

Codes and pointers are stable; message wording is not. Codes are only added,
never renamed or reused.

## Unknown fields: forward compatibility

A v1 reader meets fields it does not know when a newer tool wrote the
manifest. Ignoring them is only safe where no Glint version will ever act on
them; anywhere else, an older reader that dropped a field could grant, load or
skip something a newer reader would enforce. Unknown fields are therefore
classified by **section**:

| Section | Sensitive | Unknown key | Why |
|---|---|---|---|
| Root object | yes | rejected, `GLINT_MANIFEST_UNKNOWN_FIELD` | New top-level keys introduce behaviour: `scripts`, `migrations`, `postInstall`, `integrations`, `dependencies`, a `capabilities` object |
| `compatibility` | yes | rejected | Controls whether the theme may load at all |
| `renderer` | yes | rejected | Controls what code runs and how (`nodeIntegration`, `allowNetwork`) |
| `surfaces.<id>` | yes | rejected | Controls which documents load and with what (`preload`) |
| `requiredCapabilities`, `optionalCapabilities` | yes | n/a (string arrays; unknown namespaces are `GLINT_MANIFEST_INVALID_CAPABILITY`) | Permission requests |
| `tokens`, `assets`, `layouts` | yes | n/a (paths; every value is a validated package path) | File access inside the package |
| `author` | **no** | ignored, warning `GLINT_MANIFEST_WARN_UNKNOWN_FIELD` | Display metadata; Glint never contacts, loads or acts on it |
| `surfaces.<id>.placement`, `surfaces.<id>.behavior` | yes, owned by SRF | kept as raw JSON by `glint-schema`; unknown keys rejected by `glint-surfaces` (`GLINT_SURFACE_UNKNOWN_FIELD`) | The surface engine (`crates/glint-surfaces`) validates their shape |

Rules that apply everywhere, including ignored sections: the document size
limit, strict JSON and the duplicate-key refusal. An ignored value is never
interpreted, dereferenced or forwarded to a renderer.

Warnings come from `parse_theme_manifest_with_diagnostics` (and
`negotiate_theme_manifest`); they never change acceptance.
`parse_theme_manifest` accepts and rejects exactly the same documents and
drops the warnings.

To make a currently sensitive section tolerant, or to add a new field that
older hosts must not ignore, bump `schemaVersion`. Moving a section from
non-sensitive to sensitive is also a `schemaVersion` bump.

## No package migrations

- Glint **never runs a migration script, hook or any other code shipped in a
  package** to make a manifest compatible. Fields that could carry such code
  (`scripts`, `migrations`, `postInstall`, ...) are unknown root keys and are
  rejected.
- A manifest is never rewritten on load. If a host cannot read a
  `schemaVersion`, the result is `theme-incompatible`, not an upgrade.
- Any future migration is host-owned, data-only (a pure function from one
  format version to the next, shipped in Glint and covered by fixtures), only
  added where the semantics are well defined, and still subject to full
  validation of its output.

## Not covered here

Platform support (`compatibility.platforms`) versus the running OS, surface
availability and runtime capability checks belong to the surfaces (SRF) and
capabilities (CAP) work and are not part of manifest negotiation.
