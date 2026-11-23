# Glint platform — full design specification

**Date:** 2026-11-23  
**State:** Proposed technical design based on user-confirmed product requirements; implementation approval not yet given.  
**Scope:** Multi-OS desktop experience framework + theme ecosystem + open, local-first package distribution.

## 1. Intent and why

Glint transforms desktop customization from disconnected scripts and OS-specific skinning into a documented, portable system. The user originally requested Rust-based deep Windows customization inspired by Nothing OS; the project deliberately grew into a **cross-platform desktop theme engine** with custom-coded screens, owned/public official theme library, independent distribution and offline `.glint` imports. Visual originality, creative freedom and openness matter as much as deep integration.

## 2. Product goals

- Ship an application and CLI across **Windows, Linux and macOS**.
- Use Rust for the shared native core, OS/system providers and package logic.
- Implement complete **Glint-controlled** desktop surfaces (panels, docks, launchers, notification surfaces, widgets, control center etc.), not merely palette changes.
- Give theme authors a documented programmable UI runtime and SDK (React/TS recommended first); support simpler declarative themes.
- Distribute via project-owned public `glint-library` default registry, **or** arbitrary user Git repos, **or** standalone `.glint` file imports/exports.
- Support capability-aware, versioned themes, per-surface composition, per-monitor layouts, offline operation, preview and rollback.
- Expose native theme integration **only** when actually supported and opted into.

## 3. Non-goals and anti-goals

- No new OS, compositor, window manager or mandatory Explorer replacement.
- No guarantee that Windows Settings, GNOME Shell, macOS Notification Center, UAC/login/lock screens, or every third-party app can be reskinned.
- No required cloud, marketplace account, remote themes at runtime or proprietary distribution lock-in.
- No arbitrary shell scripts/native binaries executed from `.glint` packages.
- No unconditional claims of identical visual capabilities across OS families.

## 4. User experience

First install includes a working default theme and local manager. The official catalog uses bundled registry config but is not required for operation. User can import a local `.glint` by drag-and-drop/CLI, see author/permissions/support, preview without native changes, activate full theme or individual surfaces, mix overrides, and revert. A creator scaffolds theme code, runs mocked preview, builds, validates, packs and shares it wherever desired.

## 5. Approaches considered

**A: three native apps/themes** — deepest native integration, expensive and no common programmable UI format.  
**B: one webview customization utility** — easy shared frontend but insufficient native panel/desktop layers.  
**C: shared Rust services + host/renderer adapters (reference recommendation)** — portable surface and package contracts with platform-specific window/integration code. Tradeoff: harder isolation/rendering and cross-OS QA, but best fits product's actual constraints.

**Status:** C is recommended based on discussion, not a claim that the user explicitly ratified an ADR.

## 6. System decomposition

- Trusted native session: `glint-daemon` (configuration, packages, profiles, system events, permissions, adapters).
- Manager: `glint-desktop` (Tauri 2 + React/TS proposed) is on-demand.
- Shell: `glint-shell` creates OS-appropriate native surface windows/layers, supervises renderer processes and handles keyboard/window lifecycle.
- Renderer: isolated, precompiled UI bundles, no broad native APIs; accesses allowed SDK bridge only.
- CLI: user and theme creator operations, using same daemon/format library.
- Storage: local SQLite for metadata/grants/locks; immutable package files and user settings; caches can be evicted.

## 7. Standard surface contract

Standard IDs: `panel`, `dock`, `launcher`, `control-center`, `notification`, `notification-center`, `desktop`, `calendar`, `power-menu`, `workspace-switcher`, `window-switcher`, `volume-osd`, `brightness-osd`, `media-overlay`. Custom namespaced IDs allowed. Each has an entrypoint, placement policy, focus/input rules, permissions/capabilities and fallback. The native host positions/draws the shell window; the author controls its UI content.

## 8. OS adaptation

- **Windows:** Win32/COM, WebView2, `IDesktopWallpaper`, application appbars and supported system controls. No generic Explorer taskbar replacement.
- **Linux:** identify Wayland/X11 and actual compositor/DE; use layer-shell where supported, Hyprland IPC, freedesktop system provider interfaces, KDE/GNOME-specific bridges. Wayland webview embedding must be spiked.
- **macOS:** AppKit/WKWebView, NSWorkspace wallpaper, NSStatusItem and owned overlays. No native menu bar or Notification Center replacement.
- All backends report capability state, not just OS name or vague true/false.

## 9. Theme/package contract

- `glint.json` versioned manifest with `kind`, ID, semver, author/license, compatibility, required/optional capabilities, tokens/assets, surface entrypoints and platform integration intents.
- `.glint` is a portable versioned ZIP, self-contained compiled assets; offline import/export, strict extraction limits and path checks.
- Source resolvers: official/custom registry, arbitrary Git repo/ref/subpath, local archive/directory, optionally artifact URL. Every source normalizes to staged `ThemePackage`; installer is source-agnostic.
- Profiles lock package version/hash, combine base and surface overrides, user preferences and per-monitor settings.
- Theme runtime is sandboxed; no arbitrary native execution, direct OS commands or unsanctioned network access.

## 10. Official and community ecosystem

`glint` is independent of `glint-library`; the latter is a public official catalog, configured by default and maintained by Glint's team. Creators need neither inclusion in that repo nor GitHub to publish their own Git or `.glint` packages. Registry metadata improves discovery but is not the security boundary or the only installer input.

## 11. Safety and reliability

- Validate before changing live state; preview uses simulated data and disabled native actions by default.
- Run untrusted JS in restricted renderers and authorize each bridge call against package grants and OS capabilities.
- Journal native mutations, preserve old profile/surfaces and attempt safe rollback; report non-atomic OS side effects.
- Ship always-available safe mode with built-in default theme and a recovery command.
- Test hostile ZIPs, malicious UI requests, permission revocation, multi-monitor hotplug and renderer crashes.

## 12. Identity, visual direction and licensing

Name: Glint. Brand direction accepted: monochrome Obsidian/Graphite/Gunmetal/Titanium/Silver/Chrome/Frost/White/Muted; **no green**. A Nothing-inspired sample theme is valid only with original or properly licensed icons/fonts/assets. Brand colors are not public theme constraints; authors can build any visual identity.

## 13. Acceptance criteria for a credible first release

- Can import a valid `.glint` offline on all initial supported OS installers, preview and safely remove it.
- Creator can build one React panel and one launcher, package them, and consumers can activate without Node/npm.
- Runtime respects capability denied/unavailable states and never reports fake action success.
- At least one custom Glint-owned surface actually displays on each target OS, with documented differences and tested builds.
- At least one native wallpaper integration works per tested platform; unsupported functions are clearly surfaced.
- Official library browsing plus direct Git/standalone archive installation are supported.
- Theme crash leaves native desktop usable and recovery returns to default.

## 14. Major open decisions

Exact Linux layer-shell renderer; minimum supported OS versions; SDK version/entrypoint schema details; signing/trust; package source identity collisions; optional store/marketplace scope; native integrations release order. These require prototypes/ADRs, not invented answers.

## 15. Specification references

See detailed docs: `01-product/`, `02-architecture/`, `03-platforms/`, `04-themes/`, `05-distribution/`, `06-engineering/`. Technical source URLs are collected in `08-research/official-references.md`.

## 16. Approval status

The user requested this **documentation archive**, not product implementation. No external repository, GitHub issue, binary, installation or API was changed. Treat this spec as a review draft; do not start executing code tasks until the user approves a plan.
