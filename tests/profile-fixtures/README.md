# Profile fixtures

Synthetic `profile.json` and `profile.lock` inputs for the *proposed* profile
formats (SCH-006). They are read by `cargo test -p glint-schema --test profile`.

| Fixture | Purpose |
|---|---|
| `custom-mix/profile.json` | The profile example from [Profiles, theme mixing, variants](../../docs/plan/04-themes/theme-composition-layout-and-variants.md#profile-example): base theme, two surface overrides, a private wallpaper, two monitors and a token override |
| `custom-mix/profile.lock` | A lock that pins every theme `custom-mix` references, one per origin form (registry, Git with subpath, local) |
| `minimal/profile.json` | The smallest accepted profile: only the required fields |
| `lock-missing-digest/profile.lock` | A lock entry without `digest`; must be rejected with `GLINT_PROFILE_MISSING_FIELD` at `/packages/0/digest` |

Provenance: every file here is written by hand for this repository. Package
ids, hosts (`*.example`) and digests are placeholders and refer to nothing
real; the files contain no personal data, third-party trademarks, fonts or
images and are covered by the repository [MIT license](../../LICENSE).
