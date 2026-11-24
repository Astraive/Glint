# Design tokens

Status: **proposed** (THM-004). The contract may change until it is stabilized by tests and an ADR. The Rust parser in [`crates/glint-schema/src/tokens.rs`](../../crates/glint-schema/src/tokens.rs) is the authority; [`schemas/glint-tokens-v1.schema.json`](../../schemas/glint-tokens-v1.schema.json) is the matching JSON Schema and a test keeps its token names equal to the parser's registry.

Source requirement: public theme tokens use **semantic** names (`background.default`, `text.primary`, `surface.raised`, `focus.ring`, `panel.height`), never Glint brand constants such as `Obsidian` ([brand and design language](../plan/01-product/brand-and-design-language.md#style-system-versus-platform-style)). The SDK defines the contract; the active theme supplies values.

## File

A theme points at its tokens file with the manifest's `tokens` path (`theme/tokens.json` in the default theme). Format:

```json
{
  "schemaVersion": 1,
  "colorSchemes": {
    "dark":  { "background.default": "#141518", "...": "..." },
    "light": { "background.default": "#ebecee", "...": "..." }
  },
  "tokens": { "space.md": "12px", "panel.height": "36px", "...": "..." }
}
```

- `colorSchemes` has `dark`, `light` or both. Every declared scheme defines **every** colour token.
- `tokens` holds every non-colour token; they do not vary by scheme.
- `$schema` and `$comment` are accepted and ignored. Any other top-level field is refused (`GLINT_TOKENS_UNKNOWN_FIELD`).
- The registry is **closed and complete**: an unknown token is refused (`GLINT_TOKENS_UNKNOWN_TOKEN`), a missing one is refused (`GLINT_TOKENS_MISSING_TOKEN`), and a colour in `tokens` (or the reverse) is `GLINT_TOKENS_INVALID_VALUE`. A theme that wants fewer than all tokens does not exist in v1; per-user changes are the profile's `tokenOverrides` ([profile format](profile-format.md)).
- Duplicate keys, files over 256 KiB and non-object roots are refused like every other Glint JSON document.

## Token groups

| Group | Tokens | Kind |
|---|---|---|
| `background` | `default`, `subtle` | colour |
| `surface` | `default`, `raised`, `overlay` | colour |
| `text` | `primary`, `secondary`, `muted`, `on-accent` | colour |
| `border` | `subtle`, `default` (colour), `width` | colour, dimension |
| `accent` | `default`, `subtle` | colour |
| `focus` | `ring` (colour), `width`, `offset` | colour, dimension |
| `font` | `family.base`, `family.mono`, `size.small`, `size.base`, `size.large`, `weight.regular`, `weight.strong`, `line-height.base` | typography |
| `space` | `xs`, `sm`, `md`, `lg`, `xl` | dimension |
| `radius` | `sm`, `md`, `lg` | dimension |
| `elevation` | `low`, `medium`, `high` | shadow |
| `motion` | `duration.fast`, `duration.normal`, `duration.slow`, `easing.standard`, `easing.emphasized` | duration, easing |
| `panel` | `height` | dimension |

Full names are the group plus the leaf (`surface.raised`, `font.size.base`). The machine-readable list with descriptions is `TOKEN_REGISTRY`.

There is deliberately no status palette (success, warning, danger) yet; see [Gaps](#gaps).

## Values

Values are small closed grammars so that a token can never fetch a resource or inject CSS:

| Kind | Grammar |
|---|---|
| colour | `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa` (stored lowercase). No named colours, `rgb()`, `hsl()`, `var()` or `url()`. |
| dimension | `0`, or a non-negative number (at most 10000) with `px`, `rem` or `em`. |
| font family | at most 8 comma-separated entries, each a bare identifier or a double-quoted name of letters, digits, space, `-`, `_`; the last must be a generic family (`sans-serif`, `monospace`, `system-ui`, ...). CSS-wide keywords are refused. Fonts are system fonts; the package cannot ship or reference web fonts through tokens. |
| font weight | JSON integer 1 to 1000. |
| number | JSON number 0.5 to 3 (line height, unitless). |
| duration | `<number>ms` or `<number>s`, at most 5 s. |
| easing | `linear`, `ease`, `ease-in`, `ease-out`, `ease-in-out`, or `cubic-bezier(x1, y1, x2, y2)` with `x1`, `x2` in 0..1 and `y1`, `y2` in -4..4. |
| shadow | `none`, or one to four comma-separated layers `<x> <y> <blur> [<spread>] <hex colour>`; offsets and spread may be negative, blur may not. |

Each value is at most 256 bytes. Wrong shape or value is `GLINT_TOKENS_INVALID_VALUE` or `GLINT_TOKENS_INVALID_TYPE`, pointing at the value.

## Brand names are never public

Any `.`/`-`/`_`-separated word of a token name that is a Glint brand constant (`BRAND_NAMES`: `obsidian`, `graphite`, `gunmetal`, `titanium`, `silver`, `chrome`, `frost`, `glint`, case-insensitive) is refused with the dedicated code `GLINT_TOKENS_BRAND_NAME`, checked before the unknown-token check so authors get a precise message. The lint applies in both sections. The default theme's palette therefore appears only as *values* (`#141518`), never as names; the provisional THM-001 layout (`color.graphite-950`, `semantic.surface`) is not valid v1. The registry itself is also tested to contain no brand word.

The Glint app's own monochrome identity (graphite and silver, no green) is a property of the default theme's values, not of the contract: installed themes may use any palette.

## Contrast

`CONTRAST_REQUIREMENTS` lists WCAG 2.x pairs that **every declared colour scheme** must meet; a miss is `GLINT_TOKENS_LOW_CONTRAST` pointing at the foreground token.

| Foreground | Background | Minimum |
|---|---|---|
| `text.primary` | `background.default`, `surface.default`, `surface.raised`, `surface.overlay` | 4.5:1 |
| `text.secondary` | `background.default`, `surface.default`, `surface.raised` | 4.5:1 |
| `text.muted` | `background.default`, `surface.default` | 4.5:1 |
| `text.on-accent` | `accent.default` | 4.5:1 |
| `focus.ring` | `background.default`, `surface.default`, `surface.raised` | 3:1 |

Colours in a pair must be opaque (alpha `ff`); a translucent one is `GLINT_TOKENS_INVALID_VALUE`, because contrast against an unknown backdrop is undefined. `contrast_ratio` and `Rgba::relative_luminance` implement the WCAG formula and are public for editors (MGR/CRT). Borders are not checked (WCAG has no AA requirement for decorative separators); controls whose boundary is the only affordance should use `focus.ring` or `border.default` and rely on the user's forced-colours mode, which surfaces must respect (`@media (forced-colors: active)`).

## CSS custom property mapping

`TokenFile::to_css()` is the reference mapping; the renderer injects its output (RND) and the SDK reads the same names.

1. **Name:** `--glint-` plus the token name with every `.` replaced by `-`: `background.default` becomes `--glint-background-default`, `text.on-accent` becomes `--glint-text-on-accent`, `font.line-height.base` becomes `--glint-font-line-height-base`. A test proves the mapping is collision free across the registry.
2. **Values:** emitted verbatim as normalised by the parser (colours lowercase, numbers shortest form). No `calc()`, no `var()`.
3. **Default scheme:** `:root` gets `color-scheme: dark;` (or `light` when only light is defined) and every token for the default scheme (dark when defined).
4. **Second scheme** (only when both are defined):
   - `@media (prefers-color-scheme: <other>) { :root:not([data-glint-scheme]) { ... } }` follows the system preference when the host has not chosen;
   - `:root[data-glint-scheme="dark"]` and `:root[data-glint-scheme="light"]` always win; the host sets the attribute from the user's explicit choice.
   Only the colour tokens and `color-scheme` are repeated.
5. **Reduced motion:** under `@media (prefers-reduced-motion: reduce)` and under `:root[data-glint-reduced-motion="true"]` (the host's runtime accessibility restriction, which wins over theme values per the [precedence rule](../plan/02-architecture/renderer-and-state.md)), every `--glint-motion-duration-*` is `0ms`. Easing is unchanged because with zero duration it has no effect. Surfaces must express all transitions and animations with the duration variables so that this single rule disables them; a token cannot opt out.
6. **Forced colours:** tokens are not applied under `forced-colors: active`; surfaces use system colours there (as the default panel does).

Only baseline (tier 0) features of the [WebView2 baseline](../research/spikes/webview2-baseline.md) are used: custom properties, media queries and attribute selectors. `light-dark()` and `color-mix()` (tier 1) are deliberately not emitted; surfaces that want derived colours use `@supports (color: color-mix(...))` themselves with a token as the fallback.

Non-CSS consumers (the SDK's `theme.tokens`) use the same names without the prefix and with `.`, for example `tokens["surface.raised"]`.

## Diagnostics

| Code | Meaning |
|---|---|
| `GLINT_TOKENS_INVALID_JSON` | not well-formed JSON |
| `GLINT_TOKENS_TOO_LARGE` | over 256 KiB |
| `GLINT_TOKENS_DUPLICATE_KEY` | an object repeats a key |
| `GLINT_TOKENS_NOT_AN_OBJECT` | root is not an object |
| `GLINT_TOKENS_MISSING_SCHEMA_VERSION` | no `schemaVersion` |
| `GLINT_TOKENS_UNSUPPORTED_SCHEMA_VERSION` | `schemaVersion` is not 1 |
| `GLINT_TOKENS_UNKNOWN_FIELD` | unknown top-level field |
| `GLINT_TOKENS_INVALID_TYPE` | wrong JSON type |
| `GLINT_TOKENS_INVALID_VALUE` | value outside its grammar, translucent contrast colour or token in the wrong section |
| `GLINT_TOKENS_UNKNOWN_TOKEN` | name not in the contract |
| `GLINT_TOKENS_BRAND_NAME` | name uses a Glint brand constant |
| `GLINT_TOKENS_MISSING_TOKEN` | a required token (or the `tokens` object) is absent |
| `GLINT_TOKENS_INVALID_COLOR_SCHEME` | `colorSchemes` missing, empty or not `dark`/`light` |
| `GLINT_TOKENS_LOW_CONTRAST` | a contrast requirement is not met |

Every error carries an RFC 6901 JSON Pointer; the parser stops at the first problem. Codes are only added, never renamed.

## Default theme

[`themes/default/theme/tokens.json`](../../themes/default/theme/tokens.json) implements the contract with neutral graphite, silver and white in both schemes (no green). Tests in `crates/glint-schema/tests/tokens.rs` check that it validates, that every colour is a neutral grey, that it meets every contrast pair, and that the static fallback panel's CSS declares the same values under the contract's variable names.

## Gaps

- No status colours (success, warning, danger, info) and no data-visualisation palette; adding them is a minor, additive change once surfaces need them.
- `tokenOverrides` in `profile.json` still parse with any dotted name; the profile resolver (`glint-profiles`, PRF-001) validates each one with `validate_token_value` against `TOKEN_REGISTRY` and brand names, ignores invalid ones with a `GLINT_RESOLVE_TOKEN_*` diagnostic and reverts any that would break a contrast requirement ([profile format](profile-format.md#composition-and-precedence)).
- `glint theme validate` and `glint-package` do not yet run `parse_tokens` on the `tokens` file; wiring is a PKG follow-up.
- No TypeScript types for tokens yet (the SDK work in THM-002 will generate them from the schema, following the `@glint/theme-schema` drift-check pattern).
- Per-surface token scopes and theme-defined extension tokens are not in v1 (the registry is closed on purpose).
