# Recovered conversation knowledge dump

This is the compact but broad record of **every Glint-specific idea and correction recoverable from the preceding conversation and the earlier project chat**. It deliberately separates original asks from assistant proposals. Not a verbatim chat export.

## Original intent

- A Rust application able to drastically theme a Windows desktop: “whole windows theme,” “widgets,” taskbar, control panel, notifications and other UI; Nothing OS-style look and feel.
- Evaluate whether it is technologically possible and practical, including marketplace publication of a themed experience instead of writing one hardcoded custom OS shell.
- Inspired by Caelestia Shell and potentially other rich desktop shell ecosystems.
- Scope broadened to all major desktop OSes; Windows-only or Linux-only architecture is unacceptable as the long-term product.
- Name exploration rejected complicated names, overly common names and unnecessary `shell` suffix. Selected `Glint`.
- Explicit brand constraint: **no green tones**. Chosen palette: Obsidian `#0B0C0F`, Graphite `#181A20`, Gunmetal `#2B2F38`, Titanium `#727A89`, Silver `#B9C0CC`, Chrome `#D7DBE3`, Frost `#F5F6F8`, White `#FFFFFF`, Muted `#9298A3`.

## Core and OS architecture discussion

- Rust as primary system engine and native integration language.
- Assistant recommended Tauri 2 + React + TS for desktop settings; Rust `Tokio` daemon, platform adapter traits, event bus, state/config/asset managers, layout engine, permissions, IPC and package installer.
- Process division: `glint-desktop` settings, `glint-daemon` state/service manager, `glint-shell` actual Glint-generated desktop surfaces, and `glint` CLI.
- Windows: wallpaper APIs, appbars, overlays, system controls, optional shell integration; no guarantee of replacing Explorer taskbar or system-owned UIs.
- Linux: Wayland/X11 plus compositor-specific bridges; Hyprland socket IPC, layer-shell for supporting compositors, GNOME Shell extensions/KDE Plasma integrations when appropriate.
- macOS: AppKit, status items, desktop overlays, wallpaper APIs; full native menu bar/Notification Center replacement unrealistic.
- Proposal that Glint should not build a new compositor/window manager. Theme code uses abstractions and capability detection rather than platform-specific syscalls.

## Public library and community distribution

- `glint-library` is a **second repository maintained by us**, publicly available, default set of themes. `glint` app ships a default config pointing to its repo.
- The `glint theme` command should browse/sync/install and fetch a selected theme. Optimize against cloning the entire library on every operation; reuse cache, index and sparse/partial Git fetching when appropriate.
- Crucial user correction: any user can create **their own GitHub/Git repo**, publish a packaged theme, or distribute a `.glint` file and directly import it into the application. Official-library inclusion is never required.
- Recommended concept: portable ZIP-based `.glint` package with one versioned manifest, local/offline import, compiled assets, no build tool required on consumers; registry metadata separate from theme manifest.
- Recommended CLI: `glint theme browse`, `search`, `install official/name`, `install <git-url>`, `import file.glint`, `use`, `list`, `update`, `create`, `build`, `validate`, `pack`, `export`; `glint registry add`, `sync`.
- Sources normalized through common staging, validation, compatibility checks, install and rollback pipeline. Track original source, version, commit and content hash.

## Full programmable screens / surfaces

- User explicitly rejected a mere “color tokens + wallpaper” theme system; wanted authors to **write screen code for all UI**.
- Proposed category name **surface** for a Glint-owned screen/region, distinct from widgets.
- Proposed surface identifiers: `panel`, `dock`, `launcher`, `control-center`, `notification`, `notification-center`, `desktop`, `calendar`, `power-menu`, `workspace-switcher`, `window-switcher`, `volume-osd`, `brightness-osd`, `media-overlay`, plus custom surfaces.
- Recommended React + TS, CSS for programmable theme source, compiled into static bundles. Illustrative hooks: `useClock()`, `useSystem()`, `useAudio()`, `useBrightness()`, `useNetwork()`, `useWorkspaces()`, `useCapabilities()`, `useGlint()`.
- Theme manifest maps surface IDs to entries, positions and sizes. Shell renderer creates OS-specific host and provides system data, event subscriptions and scoped actions.
- Declarative/no-code themes coexist with programmable themes, use same packaging/import pipeline.
- Built-in default theme fills missing surfaces; optional surface-by-surface overrides allow mix-and-match: base aesthetic from Mono, dock from Glass, notifications from another, wallpaper custom.
- Separate security boundaries for untrusted script content; no arbitrary native execution when a `.glint` is imported.

## Naming and repositories

- Main repo: `glint` containing `apps/desktop`, `apps/daemon`, `apps/shell`, `apps/cli`, Rust crates, SDK/packages, default theme, examples and docs.
- Official repo: `glint-library` containing `registry.json`, themes and previews, workflows and submission docs.
- Earlier speculation included Glint Studio, Glint Hub and Glint SDK naming; only main Glint name and separate library are binding.

## Visual and ecosystem inspiration

- Nothing-like styling: high contrast monochrome, modular/dot glyphs, bold typography, sharp visual geometry. **Glint itself is not locked to this style**; its settings brand may use selected monochrome palette while user themes can have any palette.
- Caelestia Shell shows polished dynamic Linux desktop UX; Quickshell demonstrates programmable Wayland shell concepts; KDE's native Global Theme system illustrates deep system styling via supported OS/desktop APIs.
- Separate Nothing UI component-library conversation explored 13×13 and 25×25 circular glyph matrices and dotted icons; treat as optional theme design inspiration, not an automatic Glint package requirement.

## Product qualities proposed

- Offline-installed themes, no central backend or required accounts, open theme format, custom Git registries.
- Theme preview before application; per-monitor settings; safe fallback and restore points.
- Asset dedup/cache, event-driven updates, subscription-based providers, native permissions, developer HMR, runtime logging and diagnostics.
- Future registry/creator economy possible without imposing one provider.

## Unanswered / not decided

- Mandatory React vs additional engines; exact renderer technology on Wayland; minimum versions, budget/time/team, final app UX, theme update policy defaults, license, package signing model, access control, which native OS modules ship when.
- These are tracked in `00-history/decisions-and-open-questions.md` and engineering risk docs; no invented approvals.
