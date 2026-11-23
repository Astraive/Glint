# Product vision, goals, alternatives and boundaries

## One-line idea

**Glint is a cross-platform, local-first desktop experience platform: a common theme runtime, deep OS-aware integrations and an open ecosystem where anyone can code, share and install entire desktop designs.**

## Why this exists

Existing theme ecosystems are generally tied to a compositor (Quickshell/Caelestia), a desktop environment (KDE Global Themes), an OS-specific widget framework, or simple wallpaper/icon preferences. Users wanting an intentional *whole-desktop look* often combine manual configuration, fragmented tools and fragile workarounds. Glint aims to provide a single creator-to-consumer lifecycle across Windows, Linux and macOS while preserving native limitations.

## Primary audiences

1. **Desktop enthusiasts:** install a cohesive desktop quickly; preview, customize and revert it.
2. **Theme creators:** code complete panels/launchers and distribute them without seeking approval from an app store.
3. **Widget designers:** ship isolated widgets/data visualizations reusable across themes.
4. **Power users/ricers:** configure per-monitor surfaces, shortcuts, icons, wallpaper, compositor-specific extras and mixed profiles.
5. **Developers of OS adaptations:** improve Glint adapters and capability coverage without changing author UI code.

## Outcomes

- A downloaded `.glint` file can be imported offline and immediately previewed.
- A theme creator can build a React/TypeScript top bar and control center without writing Win32, GTK or AppKit code.
- The same theme bundle can run on two operating systems when its required capabilities are available; missing capabilities are surfaced clearly.
- The theme package may target only Linux, only Windows, or only macOS. OS coverage is **optional per package**, but Glint itself targets all three.
- Users can compose a base theme + per-surface overrides and revert an unwanted change.
- Community theme publication is independent of the official `glint-library`.

## Full scope categories

1. Appearance (color, fonts, spacing, radius, shadows, blur, motion).
2. Wallpaper and dynamic/interactive backgrounds via safe supported providers.
3. Glint shell surfaces (panel, dock, launcher, control center, notifications, overlays, widgets).
4. Desktop data/actions (audio, battery, network, Bluetooth, media, applications, workspaces, window information, monitors).
5. Asset packs (wallpaper, icons, cursor themes, sounds) and supported native theming bridges.
6. Layout/workspace/profile saving and restoration.
7. Author tooling (CLI, HMR, simulator, manifest validator, packer).
8. Distribution (default public repo, third-party Git, portable files, optional custom indexes).
9. Creator discoverability (previews, browsing, tagged categories, curated recommendations).

## Non-goals (especially for v1)

- Become a fully independent operating system, display server or compositor.
- Replace Explorer, the Windows kernel shell, macOS Dock/Menu Bar/Notification Center, GNOME Shell, UAC, lock/login/authentication screens universally.
- Inject CSS/JS into arbitrary proprietary third-party applications.
- Run arbitrary theme-supplied native plugins or installation shell scripts.
- Operate a required cloud platform, authentication system or payment marketplace.
- Ship every desktop integration simultaneously on day one.
- Guarantee pixel-identical rendering across different native WebViews without a compatibility layer and QA.

## Product alternatives (Superpowers brainstorming tradeoffs)

**Approach A — Native OS-specific theme managers.** Three tightly integrated clients: strongest native feel but maximum implementation and maintenance cost; inconsistent package model. Rejected as primary architecture because cross-platform themes are a core requirement.

**Approach B — One webview app controlling only wallpapers/widgets.** Quick to ship with near-uniform UI; insufficient support for desktop-layer panels, workspaces and platform-level theming. Too shallow for stated vision.

**Approach C — Shared Rust runtime + managed surfaces + per-OS adapters (recommended).** A common SDK/package format plus OS-specific hosts and bridges. Harder engineering up front, but protects creator ergonomics and accommodates OS constraints without false parity.

**Choice status:** Approach C is the recurring proposal in chat, not an explicitly signed-off ADR. It is the reference architecture for these documents.

## Success criteria

- Time-to-first-theme: from install to preview/activation without running developer tools.
- Repeatability: import the same theme twice => same version/assets and idempotent installed state.
- Safety: untrusted themes cannot directly read arbitrary files or invoke privileged commands.
- Resilience: bad theme launch cannot strand user in a blank desktop; default Glint shell/recovery always available.
- Platform honesty: unsupported features are detected, explained and either omitted/fallbacked or require explicit acceptance.
- Community interoperability: theme author can host an ordinary public Git repo and export a `.glint` archive.
