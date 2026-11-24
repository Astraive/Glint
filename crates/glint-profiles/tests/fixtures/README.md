# Profile resolver fixtures

Repository-owned, synthetic test data (MIT, like the repository). No
personal data, proprietary fonts or third-party trademarks: every theme id is
under `dev.example.*` (built in `tests/common/mod.rs`) and the default theme
stand-in uses the real `themes/default/theme/tokens.json`.

| File | Used by | What it is |
|---|---|---|
| `custom-mix.profile.json` | `tests/resolve.rs` | The plan's profile example with synthetic ids |
| `custom-mix.plan.json` | `tests/resolve.rs` | Golden plan with no host restriction |
| `custom-mix.restricted.plan.json` | `tests/resolve.rs` | Golden plan with light scheme, reduced motion and an unusable `surface.notification` |

Regenerate the golden plans with `GLINT_BLESS=1 cargo test -p glint-profiles
--test resolve` and review the diff: a change is a change to the resolver's
observable output.
