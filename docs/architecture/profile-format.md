# Profile and profile lock formats

Status: **proposed** (SCH-006). This page records the `profile.json` v1 and
`profile.lock` v1 rules implemented by `crates/glint-schema`
(`src/profile.rs`, tests in `tests/profile.rs`, fixtures in
[`tests/profile-fixtures/`](../../tests/profile-fixtures/README.md)). The JSON
Schemas are [`schemas/glint-profile-v1.schema.json`](../../schemas/glint-profile-v1.schema.json)
and [`schemas/glint-profile-lock-v1.schema.json`](../../schemas/glint-profile-lock-v1.schema.json).
Like the manifest format, both may change until stabilized by tests and an
ADR; the parser is authoritative where a schema disagrees.

Sources: [Profile example](../plan/04-themes/theme-composition-layout-and-variants.md#profile-example),
[Update conflicts](../plan/04-themes/theme-composition-layout-and-variants.md#update-conflicts)
and [Profile resolution / composition](../plan/02-architecture/renderer-and-state.md#profile-resolution--composition).
Release identity follows the *proposed* [ADR 0006](../adr/0006-package-identity-and-origin-collisions.md).

## Four distinct documents

| Document | Describes | Schema |
|---|---|---|
| `glint.json` | One self-contained package | `glint-theme-v1` |
| `profile.json` | The user's active composition | `glint-profile-v1` |
| `profile.lock` | The exact releases a profile resolves to | `glint-profile-lock-v1` |
| `config.toml` | The application ([application config](application-config.md)) | `glint-config-v1` |

None embeds another: a profile names themes by `id` and exact version; only
the lock records origin and digest.

## `profile.json`

| Field | Required | Rule |
|---|---|---|
| `schemaVersion` | yes | `1` |
| `id` | yes | Local profile id: dot-separated segments of `a-z`, `0-9` and inner hyphens, at most 128 characters. Accepts both `work-setup` (as named by `config.toml`) and `profile.custom-mix` (the plan example) |
| `name` | yes | Display text, at most 128 characters |
| `description` | no | Display text |
| `baseTheme` | no | `{ "id", "version" }`; absent means only the built-in default safe theme applies |
| `surfaces` | no | Surface ID (manifest surface rules, including `custom:<id>`) to `{ "theme", "version" }` |
| `wallpaper` | no | Exactly one of `{ "path" }` (private user file) or `{ "asset" }` (named asset of the base theme) |
| `monitors` | no | Monitor key, then surface ID, then a placement object. The placement shape is the surface/layout engine's (SRF) and is kept as raw JSON; it is the only open object |
| `tokenOverrides` | no | Token name (dot-separated segments starting with a letter) to a string or number |

Decisions where the plan is silent:

- **Versions are exact**, never ranges: a profile selects one release.
- **One version per theme**: referencing the same theme at two versions
  (base and an override) is `GLINT_PROFILE_CONFLICTING_VERSIONS`, because a
  profile activates one version of each package.
- **Unknown fields are rejected** everywhere except placement objects.
  Profiles are written by Glint, and an ignored field could hide an override
  the user expects.

## Local-only values and export

`Profile::local_only_refs` lists values that must not leave the machine;
`Profile::without_local_only` returns the default export copy without them.

| Value | Why local-only |
|---|---|
| `wallpaper.path` | A private user file. The path is relative to the profile's own directory and follows the package-path rules, so it can never name an absolute path, a drive, a share or a parent directory, and never leaks a user name |
| Monitor keys other than `primary` and `secondary` | They name a physical display (stable OS or EDID-like keys) rather than a portable role |

An exporter copies an explicitly selected asset itself; nothing is exported
by default.

## `profile.lock`

```json
{
  "schemaVersion": 1,
  "profile": "profile.custom-mix",
  "packages": [
    {
      "kind": "theme",
      "id": "official.mono",
      "version": "1.0.0",
      "origin": "registry:https://registry.example/glint-library",
      "digest": "sha256:<64 lowercase hex digits>"
    }
  ]
}
```

- Every entry needs all five fields. **A lock entry without `digest` is
  rejected** (`GLINT_PROFILE_MISSING_FIELD` at `/packages/<n>/digest`).
- `origin` must be in the canonical form of ADR 0006 and never contains
  credentials; a parsable but non-canonical spelling is refused, because
  locks are machine-written.
- A (`kind`, `id`) appears at most once (`GLINT_PROFILE_DUPLICATE_PACKAGE`).
- No branch names, ranges or timestamps: a moving Git branch cannot change
  what a locked profile loads.

`verify_profile_lock(profile, lock)` reports every disagreement with a stable
code: `GLINT_PROFILE_LOCK_PROFILE_MISMATCH`, `..._UNLOCKED` (a referenced theme
is not pinned), `..._VERSION_MISMATCH` and `..._UNUSED` (a stale pin). Whether
each pinned release is installed with that origin and digest is the
installer's check (INS).

## Error codes

All `GLINT_PROFILE_*` codes are listed by `ProfileErrorCode::ALL`. They share
the manifest's document rules (1 MiB limit, no duplicate keys, object root,
optional BOM) and point at the offending value with an RFC 6901 JSON Pointer.
