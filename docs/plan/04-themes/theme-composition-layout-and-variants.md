# Profiles, theme mixing, variants and native integration requests

## Why composition matters

A user may want a Monochrome palette from one theme, a glass dock from another, and a handmade notification renderer. Glint should support **surface-level composition** without treating every mix as a new standalone theme release.

## Proposed resolution order

1. Built-in **default safe theme** with required core surfaces.
2. Selected base theme contributes tokens, wallpaper and any surface implementations.
3. Profile's surface override packages replace only named surfaces.
4. User semantic token overrides and widget/layout choices apply to selected surfaces that opt into those tokens.
5. Per-monitor or workspace overrides refine geometry, visibility and widget instances.
6. Host accessibility and safety restrictions always win: reduced motion, safe areas, permission/capability enforcement.

This is *not* a CSS cascade over all native OS apps. It only applies to Glint's internal layout model and supported integration requests.

## Profile example

```json
{
  "schemaVersion": 1,
  "id": "profile.custom-mix",
  "name": "Custom Mix",
  "baseTheme": {"id": "official.mono", "version": "1.0.0"},
  "surfaces": {
    "dock": {"theme": "community.glassdock", "version": "1.1.0"},
    "notification": {"theme": "me.minimal-notifs", "version": "0.4.0"}
  },
  "wallpaper": {"path": "user-assets/wallpaper.png"},
  "monitors": {
    "primary": {"panel": {"edge": "top", "height": 44}},
    "secondary": {"dock": {"visible": false}}
  },
  "tokenOverrides": {"background.default": "#0B0C0F"}
}
```

Profile-specific private asset references are local-only; when exporting, Glint copies explicitly selected assets and removes any private paths/identifiers.

## Variants

A theme may ship light, dark, compact, touch, high-contrast, reduce-motion and platform variants. Do not add every combination as separate package IDs; model them as named manifest variants with selective overrides. Platform-specific CSS is author-supplied. Variant changes should not silently grant more permissions.

## Dependencies

Widgets/themes may depend on a Glint SDK minimum version or non-executable asset packages. Dependency resolution must detect cycles, incompatible versions and missing assets. For first MVP, simplify: **no executable cross-package dynamic imports**, just bundled compiled assets. Later registries can manage immutable shared packages with strict locks and provenance.

## Native integration requests

A theme may declare an integration intent such as:

```json
{
  "integrations": {
    "windows": {"wallpaper": "assets/wallpaper.png"},
    "linux": {"gtkColorScheme": "dark", "kdeColorScheme": "Monochrome"},
    "macos": {"wallpaper": "assets/wallpaper.png"}
  }
}
```

This declaration **does not execute system commands**. A trusted adapter checks whether intent is recognized, safe, reversible and supported; user consents to affecting native preferences. Unrecognized keys are ignored with diagnostics or rejected by strict schema (policy to decide), not blindly forwarded to a shell.

## Update conflicts

When base or override themes update, retain a `profile.lock` of package versions; preview new resolved layout before switching. If a deleted theme is referenced, retain installed locked version until user migrates or offer fallback. If incompatible version removes a surface, use default implementation and inform user.

## Resolution diagnostics

Profile inspector should explain each surface: `source: official/mono@1.0.0`, `override: glassdock@1.1.0`, `host: windows.appbar`, `capability: supported`, `permissions: system.audio.read`, and `fallback: none`. This makes cross-platform discrepancies diagnosable.
