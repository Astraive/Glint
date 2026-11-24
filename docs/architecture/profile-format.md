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

## Composition and precedence

Status: **proposed** (PRF-001). `crates/glint-profiles` resolves a parsed
profile into a deterministic plan with `resolve(profile, themes,
default_theme, host)`. It is pure and OS-free: the caller (the daemon, the
only writer of profile state) loads each referenced release (`ThemeInput`:
`PackageRelease`, validated manifest, parsed `tokens.json`) and hands it in
through a `ThemeProvider` such as `ThemeSet`. The ticket proposed
`crates/glint-theme/src/resolve.rs`; the resolver lives in its own crate
because it consumes `glint-schema` and `glint-surfaces` and nothing else.

Layers, lowest first. A later layer wins; each plan value records the layer
that won (`Layer`: `default`, `base-theme`, `surface-override`, `user`,
`monitor`, `host-restriction`):

| # | Layer | Contributes |
|---|---|---|
| 1 | Default | The bundled default theme (`official.default`): tokens, its surfaces, its `wallpaper` asset |
| 2 | Base theme | Its tokens (the whole palette, registry-complete), its surfaces and its `wallpaper` asset |
| 3 | Surface override | Only the named surface's definition, from the named release. Never its tokens, wallpaper or other surfaces |
| 4 | User | `tokenOverrides` (validated with `glint_schema::validate_token_value` against the closed registry) and the `wallpaper` choice |
| 5 | Monitor | Per monitor and surface: `visible` (boolean) and placement keys laid over the theme's own `placement`, then validated by `glint-surfaces` with the theme's `behavior` |
| 6 | Host restriction | Reduced motion (`motion.duration.*` = `0ms`), contrast safety and capability enforcement. Always wins |

Rules where the plan is silent:

- **Surfaces in the plan** are the union of the default theme's, the base
  theme's and the profile's `surfaces` keys. For each, the highest usable
  layer wins: an override theme that is unavailable, does not declare the
  surface or declares an invalid one is reported and the base theme, then the
  default theme, applies.
- **No import.** A surface's entrypoint is always a path inside the release
  that declares it. Naming a theme in an override never pulls code, tokens,
  assets or other surfaces from it; this is not a cascade over native apps.
- **Tokens are profile-wide.** Override themes supply implementations, not
  palettes, so one coherent palette keeps its contrast guarantees. A base
  theme without a `tokens` file keeps the default palette. Per-surface token
  scopes (the plan's "surfaces that opt into those tokens") do not exist in
  v1.
- **Colour scheme.** The host's preference if the token source defines it,
  otherwise the source's default (dark when defined), reported as
  `GLINT_RESOLVE_SCHEME_UNAVAILABLE`. A colour override applies to the
  resolved scheme.
- **Contrast safety.** After user overrides, every `CONTRAST_REQUIREMENTS`
  pair is re-checked; overrides in a failing (or translucent) pair are
  reverted to the theme value with `GLINT_RESOLVE_TOKEN_LOW_CONTRAST` until
  every pair passes. Theme values passed `parse_tokens`, so the result always
  meets the contrast contract.
- **Capability enforcement.** A surface whose required capability is in
  `HostEnvironment::unusable_capabilities` is `blocked`; no user or monitor
  override can show it.
- **Wallpaper.** User file, then the chosen base-theme asset, then the base
  theme's `wallpaper` asset, then the default theme's.
- **Determinism.** Every map is ordered; diagnostics come in a fixed order
  (base theme, surfaces, tokens, wallpaper, monitors, host restrictions).
  `ResolvedPlan::to_json` is the stable JSON shape used by the golden
  fixtures in `crates/glint-profiles/tests/fixtures/`.

The resolver never fails: whatever it cannot honour is a `Diagnostic` with a
stable code, a severity (`error`: something asked for is not applied;
`warning`: part of the profile ignored or replaced; `info`: a restriction
working as intended), a JSON Pointer into `profile.json` (`""` when the
default theme itself is at fault) and the surface concerned.

| Code | Severity | Meaning |
|---|---|---|
| `GLINT_RESOLVE_THEME_UNAVAILABLE` | error | A referenced release is missing or corrupt; its layer is skipped |
| `GLINT_RESOLVE_THEME_MISMATCH` | error | The provider returned another id or version; treated as unavailable |
| `GLINT_RESOLVE_SURFACE_NOT_PROVIDED` | error | The override theme does not declare the surface |
| `GLINT_RESOLVE_SURFACE_INVALID` | error | A theme's surface fails `glint-surfaces` validation |
| `GLINT_RESOLVE_SURFACE_UNRESOLVED` | error | No layer provides a surface the profile names |
| `GLINT_RESOLVE_TOKENS_UNAVAILABLE` | error | Neither the base nor the default theme has tokens |
| `GLINT_RESOLVE_SURFACE_BLOCKED` | warning | A required capability is unusable on this host |
| `GLINT_RESOLVE_TOKEN_UNKNOWN` | warning | Override names a token outside the registry |
| `GLINT_RESOLVE_TOKEN_BRAND_NAME` | warning | Override uses a brand constant as a name |
| `GLINT_RESOLVE_TOKEN_INVALID_VALUE` | warning | Override value does not match the token grammar |
| `GLINT_RESOLVE_TOKEN_LOW_CONTRAST` | warning | Override reverted to keep a contrast requirement |
| `GLINT_RESOLVE_MONITOR_SURFACE_UNKNOWN` | warning | Monitor override for a surface not in the plan |
| `GLINT_RESOLVE_MONITOR_OVERRIDE_INVALID` | warning | Malformed `visible` or an invalid resulting placement |
| `GLINT_RESOLVE_WALLPAPER_ASSET_MISSING` | warning | The chosen asset is not in the base theme |
| `GLINT_RESOLVE_SCHEME_UNAVAILABLE` | info | The host's colour scheme is not offered |
| `GLINT_RESOLVE_OVERRIDE_RESTRICTED` | info | A user override was superseded by a host restriction |

Not yet wired: the daemon still selects one active release at boot
(`select_theme`); calling the resolver there, mapping installed releases to
`ThemeInput`s and the capability snapshot to `HostEnvironment`, is a
follow-up (COR). Safe areas, real monitor discovery and unknown monitor keys
are the surface host's (SRF-003); widget instances and layouts are not in
the v1 profile.

## Error codes

All `GLINT_PROFILE_*` codes are listed by `ProfileErrorCode::ALL`. They share
the manifest's document rules (1 MiB limit, no duplicate keys, object root,
optional BOM) and point at the offending value with an RFC 6901 JSON Pointer.
