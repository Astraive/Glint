# Discussion timeline — how Glint evolved

This timeline summarizes recoverable decisions and proposals. It is a **concept-level conversation reconstruction**, not a transcript or proof of implementation.

## 2026-11-18 — Windows customization to an OS-wide platform

- The initial request: assess feasibility of a **Rust application** to edit the entire Windows appearance: taskbar, control panel, widgets, notifications, and the rest, with a **Nothing UI-inspired** identity.
- The direction shifted from one hardcoded Nothing-style Windows replacement toward a **general-purpose theme application**. The user considered publishing a Nothing-inspired theme in a marketplace rather than making every Glint installation look identical.
- **Caelestia Shell** was identified as a useful inspiration for desktop shell richness and customization, but Caelestia is a Linux/Hyprland-oriented shell rather than a cross-platform solution.
- The user explicitly expanded scope to **multiple operating systems, including Linux and Windows**, and later macOS; avoid any design that only handles Hyprland.
- Naming explored short/shell-style variants. The user rejected convoluted/generic names and unnecessary shell suffixes and selected the standalone project name **Glint**.
- Brand direction: **no green tones**; a monochrome graphite/chrome palette was accepted. See `01-product/brand-and-design-language.md`.

## 2026-11-23 — Architecture discussion

- Agreed goal: a customizable desktop *experience*, not just wallpaper/color toggles.
- Proposed three responsibilities: `glint-desktop` (settings), `glint-daemon` (per-user background state and OS adapters), `glint-shell` (Glint-controlled panels, widgets and overlays).
- Proposed Rust core, OS adapters, Tauri 2 + React/TypeScript settings UI, separate shell surface hosting, local-first state, theme and package management, and multi-monitor support.
- Critical constraint surfaced: **the operating systems do not expose equivalent theme/shell APIs**. Cross-platform means a unified UX with honest capability detection, not claiming every privileged screen can be replaced everywhere.

## 2026-11-23 — Library and installer discussion

- The user established a **separate `glint-library` repository**, publicly readable and owned by the project, as the default source of official themes.
- Glint itself should ship a default configuration that points to that repository. `glint theme` can browse/discover/install, and the installer fetches only what it needs when possible.
- The user explicitly corrected the model: **community authors must not be forced into `glint-library`**. They can publish an independent GitHub/Git repo, upload a package release, or share a `.glint` file. Direct `.glint` import and export must work offline.
- Proposed common packaging rule: every source resolves to the same validated internal package representation. Package tracking records version, origin, hashes and update strategy.

## 2026-11-23 — Fully programmable themes

- The user asked how **all themes** can work: who writes code for every screen, not just swaps colors.
- Proposed theme-owned **surfaces**: panel, dock, app launcher, control center, notification popup, notification center, clock/calendar, power menu, workspace switcher, window switcher, volume/brightness OSD and desktop/widgets.
- Proposed two author modes: declarative token/layout themes and **programmable React + TypeScript themes** compiled to packaged JS/CSS and run in managed, restricted renderers.
- Glint SDK supplies system hooks, events, state and scoped actions; native OS code stays in trusted adapters.
- Proposed per-surface composition: users can mix different themes for panel, dock, wallpaper, notification and control center, with default-theme fallback.
- The user reiterated **multiple OS support**, so both the theme ABI and adapters must carry capability information.

## Corrected or refined ideas

| Earlier framing | Corrected framing |
|---|---|
| “Full Windows theme editor” | Cross-platform desktop experience platform; Windows native-shell replacement is limited and optional. |
| “Nothing theme built into app” | Glint is a general platform; Nothing-like themes are examples, not the only design. |
| “Official repo is the theme source” | Official **default** repository **plus** arbitrary community Git repos and `.glint` packages. |
| “Theme = tokens and wallpaper” | Theme may implement complete Glint-managed UI surfaces and integrations. |
| “Same exact feature everywhere” | Shared APIs with capability discovery, graceful fallback and per-platform overrides. |
| “All screens are replaced” | Glint-managed surfaces can be replaced; native and secure OS UI cannot universally be overridden. |

## Related, distinct project context

A separate *Nothing UI component library* discussion included React/web/native packages, dotted icons and 13×13/25×25 glyph matrices. This is useful **design inspiration for a possible Glint theme**, but that package is **not automatically a Glint dependency or part of Glint's architecture**. See `01-product/brand-and-design-language.md`.
