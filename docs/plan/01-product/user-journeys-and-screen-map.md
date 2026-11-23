# User journeys, Glint manager UI and surface catalog

## Journey A: New user applies a theme

1. Install Glint and launch the manager. Setup detects OS, display server/compositor, monitors and capabilities.
2. Show preinstalled safe default theme so the application works offline.
3. Official `glint-library` index is available when internet is permitted; browse screenshots + feature capability badges.
4. Select a theme and preview it with representative clock/network/media/workspace mock data.
5. Review **supported/unsupported** surface behavior on the current OS; request individual permissions for relevant controls.
6. Stage/install package, select whole-profile or chosen surfaces, and apply.
7. Observe shell rendering and any OS preference changes. Provide “Revert” and “Restore previous profile” paths.
8. Closing manager leaves Glint daemon/shell active; disabling Glint should leave the native desktop usable.

## Journey B: Creator publishes a themed control center

1. `glint theme create nova --template react`, choose display label/ID.
2. Implement `src/surfaces/control-center.tsx` with `useAudio`, `useNetwork`, `useCapabilities` SDK calls.
3. `glint theme dev` runs sandboxed preview with fake system state; can opt into actual read-only data and explicit action permission.
4. `glint theme validate` checks schema, surfaces, import paths, assets, accessibility, compatibility declarations and package limits.
5. `glint theme build`, `glint theme pack` produce self-contained `nova.glint`.
6. Share the file, publish a Git repo/Release, or submit it as a PR to the official `glint-library`.
7. Any user can import; gallery displays author and required permissions; installs offline with no build tooling.

## Journey C: Mix-and-match

- Install `official/mono`, `community/glassdock` and a private `my-notifications.glint`.
- Base appearance from Mono; override `dock` with Glassdock; override `notification` with own theme; use a custom wallpaper asset.
- Profile editor shows resolution order, exact package versions and missing fallback items.
- Theme updates do not silently overwrite the profile; apply version migration only after preview.

## Journey D: Multiple monitors

- Monitor A: dock bottom, panel top, desktop clock top-right.
- Monitor B: smaller panel, no dock, media widget on desktop.
- When docking/undocking changes monitor IDs, map by stable identity and fall back to primary display; never spawn invisible unreachable dialogs.
- Surface preferences resolve in logical coordinates and adjust for fractional scaling, rotation and work areas.

## Glint manager screen map (recommended)

| Screen | Main functions |
|---|---|
| Home | Active theme/profile, compatibility, quick preview, recent actions |
| Discover | Official registry, additional indexes, categories, filter by OS/capabilities |
| Theme detail | Screenshots, manifest, publisher source, version, sizes, permissions, changelog |
| Installed | Installed packages, updates, origins, pin/remove/export |
| Theme editor | Semantic tokens, assets, surface assignments |
| Layout editor | Drag/resize widgets, monitor selector, snapping, layers, safe bounds |
| Preview | Virtual desktop and simulated system state, compare revisions |
| Profiles | Create/duplicate/export/activate/restore |
| Integrations | OS features, required permissions, broken adapter diagnostics |
| Settings | Startup, telemetry opt-in, sources, cache, recovery, debug |

## Theming surfaces vs app screens

The manager screen map above is Glint's **own application UI**. A theme's programmable **shell surfaces** are separate and include:

`panel`, `dock`, `launcher`, `control-center`, `notification`, `notification-center`, `desktop`, `calendar`, `power-menu`, `workspace-switcher`, `window-switcher`, `volume-osd`, `brightness-osd`, `media-overlay` and custom namespaced surfaces. The native host may not support all on every OS.

## Empty/error states

- Registry inaccessible: show cached list and local import option.
- Incompatible theme: explain which required capability is missing and preview with fallback if allowed.
- Broken JS: kill that renderer, restore known-good surface; recovery shortcut always accessible.
- Permission denied: specific control disabled with an explanation; do not pretend toggle succeeded.
- Package invalid: retain previous theme, clean staging, offer diagnostic export.
