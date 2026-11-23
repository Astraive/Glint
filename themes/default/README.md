# Glint Default theme

The bundled fallback theme (THM-001), package ID `official.default`. Glint
uses it when no other theme is installed, when `glint-library` or the network
is unavailable, and in safe mode. It is a normal `.glint` package: the same
`glint-package` reader that checks third-party themes validates it
(`cargo test -p glint-package --test default_theme`).

| File | Purpose |
|---|---|
| `glint.json` | Manifest, *proposed* v1 format |
| `theme/tokens.json` | Semantic design tokens (dark and light colour schemes, type, spacing, radius, elevation, motion) following the THM-004 contract |
| `surfaces/panel/index.html`, `panel.css` | Static top panel: no script, no data, no remote content, system fonts only |
| `assets/preview.svg` | Preview image for theme lists |
| `LICENSE` | MIT |

Design rules: works offline, references nothing outside the package, and
declares `surface.panel` as an *optional* capability so activation never
fails for lack of a capability. The SDK-driven panel, launcher, control
centre and clock replace the static surface in THM-002.

Provenance: every file here was written by hand for this repository. There
are no third-party fonts, images, icons or trademarks, and no personal data.
Licensed under the MIT license in [LICENSE](LICENSE), Copyright (c) 2026
Astraive.
