# Application config and merge policy

Status: **proposed** (SCH-007). This page records the `config.toml` v1 format
and how its layers merge. The reference implementation is
`crates/glint-schema` (`src/config.rs`, tests in `tests/config_merge.rs`);
the bundled defaults are [`config/default.toml`](../../config/default.toml)
and the JSON Schema is [`schemas/glint-config-v1.schema.json`](../../schemas/glint-config-v1.schema.json).
The format may change until stabilized by tests and an ADR; the parser is
authoritative where the schema disagrees.

Sources: [Shipped default](../plan/07-examples/default-config-and-registry.md#shipped-default-configdefaulttoml),
[User override](../plan/07-examples/default-config-and-registry.md#user-override-optional),
[Registry versus theme manifest](../plan/07-examples/default-config-and-registry.md#registry-versus-theme-manifest)
and [Default config in Glint](../plan/05-distribution/registry-and-glint-library.md#default-config-in-glint).

## Layers

| Order | Layer | Source | Applied? |
|---|---|---|---|
| 1 | Bundled defaults | `config/default.toml`, shipped read-only and embedded in the binary | Always; must set every key |
| 2 | User config | The user-level `config.toml` (its location is the core's decision, never inside installed app resources) | Each key it sets overrides the default |
| - | Package requests | Settings a theme or other package asks for | **Never.** Every key is ignored with a diagnostic |

`resolve_config(defaults, user, package_requests)` returns the effective
`AppConfig` and every diagnostic. Layers are treated by position, so a layer
passed as a package request is ignored whatever it claims to be.

An admin or enterprise policy layer above the user would be a new ADR; it is
not part of v1.

## Merge rules

- **Keys override one by one.** Setting `[theme] active_profile` leaves the
  other `[theme]` keys at their defaults.
- **Registries merge by name.** A user `[registries.friends]` is added next
  to the default `official`. A user entry with an existing name changes only
  the fields it sets (for example just `branch`). A new name must set `type`
  and `url`; `branch` is optional and absent means the repository's default
  branch. A registry cannot be removed in v1.
- `registry.default` must name a configured registry after merging.
- **Security changes by the user are applied but reported.** Moving a
  security setting to a weaker value than the bundled default produces
  `GLINT_CONFIG_WARN_SECURITY_WEAKENED`, so the manager and CLI can show it.
  Keeping or tightening a value produces nothing.
- **Packages can change nothing.** A package request yields
  `GLINT_CONFIG_WARN_PACKAGE_SECURITY_OVERRIDE_IGNORED` for a security
  setting or any registry, and `GLINT_CONFIG_WARN_PACKAGE_SETTING_IGNORED`
  for any other key. In particular `allow_native_scripts = false`,
  `confirm_new_permissions = true` and `untrusted_network_default = "deny"`
  can never be weakened by a theme. The v1 `glint.json` has no settings field
  at all: a manifest carrying `packages`, `security` or `config` is rejected
  with `GLINT_MANIFEST_UNKNOWN_FIELD`.

## Keys

| Key | Type | Default | Security-relevant (safe direction) |
|---|---|---|---|
| `schema_version` | integer, optional | `1` | |
| `app.startup` | `"user-session"` or `"manual"` | `"user-session"` | |
| `app.locale` | `"system"` or a language tag | `"system"` | |
| `registry.default` | registry name | `"official"` | |
| `registry.auto_sync` | boolean | `true` | |
| `registry.cache_ttl` | `<n>s/m/h/d`, at most `365d` | `"24h"` | |
| `registries.<name>.type` | `"git"` | `"git"` | registries cannot be added by packages |
| `registries.<name>.url` | `https` URL, no credentials, query or fragment | placeholder | |
| `registries.<name>.branch` | safe Git branch name, optional | `"main"` | |
| `theme.active_profile` | profile id | `"default"` | |
| `theme.auto_update` | boolean | `false` | yes (`false`) |
| `theme.preview_before_apply` | boolean | `true` | yes (`true`) |
| `packages.allow_external_git` | boolean | `true` | yes (`false`) |
| `packages.allow_local_archives` | boolean | `true` | yes (`false`) |
| `packages.verify_integrity` | boolean | `true` | yes (`true`) |
| `packages.allow_native_scripts` | boolean | `false` | yes (`false`) |
| `security.confirm_new_permissions` | boolean | `true` | yes (`true`) |
| `security.untrusted_network_default` | `"deny"`, `"ask"` or `"allow"` | `"deny"` | yes (`deny` safest) |
| `shell.use_default_fallback` | boolean | `true` | yes (`true`) |
| `shell.safe_mode_after_crashes` | integer 1 to 100 | `3` | yes (lower) |

A registry URL is also the registry's package origin under the *proposed*
[ADR 0006](../adr/0006-package-identity-and-origin-collisions.md): pointing a
registry name at a different URL makes its packages a different origin.

## Strictness

Unknown keys are **rejected** (`GLINT_CONFIG_UNKNOWN_FIELD`) at the top level,
as unknown tables, in `[packages]`, `[security]` and in every
`[registries.<name>]`: an older Glint that ignored them could silently drop a
setting the user relies on, and a misspelt security key must not pass
unnoticed. Unknown keys in `[app]`, `[registry]`, `[theme]` and `[shell]`
are ignored with `GLINT_CONFIG_WARN_UNKNOWN_FIELD`. TOML syntax errors and
repeated keys are `GLINT_CONFIG_INVALID_TOML`; dates and times are not part
of the format. Files over 256 KiB are refused.

## Decisions where the plan is ambiguous

- The plan's two examples disagree (`[theme]` plus `[packages]` in
  `07-examples`, `[themes]` in `05-distribution`). v1 follows `07-examples`,
  which the ticket cites and which holds the security keys.
- `app.startup` adds `"manual"` next to the plan's `"user-session"`;
  `security.untrusted_network_default` adds `"ask"` between `deny` and
  `allow`.
- The official registry URL is a placeholder on the reserved `.invalid`
  domain until the library location is decided (maintainer decision).
- The ticket proposes `crates/glint-core/src/config.rs`. No `glint-core`
  crate exists yet, and parsing and merging are pure, so they live in
  `glint-schema` next to the other formats; file locations, reloading and
  applying the policy belong to the future core.

## Distinct documents

`config.toml` configures the application, `registry.json` indexes packages,
`glint.json` describes one package and `profile.json` describes the active
composition. Each has its own schema and parser and none is read through
another's (see [profile format](profile-format.md)).
