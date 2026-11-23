# Glint product branding and design language

## Origin

The project began as a Nothing-inspired customization concept and evolved into a general-purpose theme platform. **The Glint app's own visual identity may use monochrome metallic tones; installed themes may be any style/color.** Do not hardcode product brand colors into public theme SDK semantics.

## Accepted palette from earlier discussion

| Token / name | Hex | Suggested role |
|---|---|---|
| Obsidian | `#0B0C0F` | Deep app background |
| Graphite | `#181A20` | Primary elevated surface |
| Gunmetal | `#2B2F38` | Borders, rails, muted panels |
| Titanium | `#727A89` | Secondary icon/text |
| Silver | `#B9C0CC` | Primary secondary content |
| Chrome | `#D7DBE3` | Brighter text / outlines |
| Frost | `#F5F6F8` | High-contrast light surfaces |
| White | `#FFFFFF` | Highest contrast |
| Muted | `#9298A3` | Supporting text and metadata |

**Hard user preference: no green accents** for Glint's brand palette. Alternate theme authors are free to use any palette; the global Glint app itself retains this brand identity.

## Visual properties (recommended)

- Neutral dark-first manager UI, high typography hierarchy, crisp panels and restrained use of rounding.
- Clear surfaces, material-like contrasts, hairline borders and occasional chrome/silver emphasis rather than neon green.
- Purposeful animation and transitions; accessibility control can reduce/disable motion.
- Zero privileged branding assumptions: no “Nothing” trademarks, unlicensed dot fonts, proprietary glyph packs or commercial artwork shipped by default without rights.
- Theme gallery: big preview composition, surfaces supported, platforms supported, creator and import source, permissions, size and version.

## Glint app areas (recommended)

1. **Discover** — official and custom registries; filters and previews.
2. **Installed** — themes, packages, updates, pinning.
3. **Customize** — token editor, surface assignments, layout/monitor workspace.
4. **Preview** — simulated safe desktop with capability emulation.
5. **Integrations** — OS-specific features and diagnostics.
6. **Developer** — active local theme, HMR, manifest diagnostics (can live under Settings).
7. **Settings** — app boot/autostart, permissions, cache, profiles, backups.

## Earlier related Nothing UI concept (do not merge by accident)

Another project explored a modular web/native Nothing-inspired component library and its own dotted icons and glyph-matrix components with 13×13 and 25×25 circular grids. A Glint theme could consume such a library **at build time** if licenses, bundling and compatibility allow. Glint should not depend on that separate package or force its visual style on all themes.

## Style system versus platform style

Public theme tokens should use **semantic** names (`background.default`, `text.primary`, `surface.raised`, `focus.ring`, `panel.height`), not Glint brand constants like `Obsidian`. The SDK defines a stable token contract; the active theme supplies values, and platform modules apply safe corresponding native colors where supported.

## Legal/brand consideration

“Inspired by Nothing OS” describes aesthetic intent. Official gallery should not imply affiliation, redistribute third-party icons/fonts/sounds without licenses, or advertise pixel-identical duplication of proprietary system screens. See security/trust and OSS review docs.
