# Prior art: where Glint differs

## Caelestia Shell

Caelestia provides a fluid Linux/Hyprland shell with Quickshell/QtQuick/QML and a cohesive desktop experience. The earlier Glint discussion cited Caelestia as inspiration. It demonstrates the *quality bar* for a custom panel, launcher, control center and dynamic styling. But its reference configuration is primarily Linux/Hyprland-specific. **Glint differentiates on cross-platform packaging and portable theme authoring**, not by claiming Caelestia is inferior.

Source: https://github.com/caelestia-dots/shell

## Quickshell

Quickshell is a programmable toolkit for desktop shells with QML, integrated with Linux/Wayland/X11 concepts. It has fast iteration and shell-aware integrations. For Glint, it is a valuable model for surface lifecycle and data providers. Glint's recommended React TS theme authoring and Windows/macOS targets make it architecturally different.

Source: https://quickshell.org/about/

## Wayle

Wayle is a Rust + GTK4/Relm4 shell supporting top bar, notification center, OSD, wallpaper and device controls; a model for robust Rust/Linux native integration. Its dependence on compositor-advertised layer-shell demonstrates why Glint should test Linux native hosting early.

Source: https://wayle.app/guide/getting-started

## KDE Global Themes

KDE already supports panels/widgets/icon/cursor/wallpaper/app style/window decorations through a native theme ecosystem. Glint should **integrate where feasible** rather than overwrite the KDE experience, and its UI should explain that native KDE style coverage depends on KDE APIs, not generic React surfaces.

Source: https://develop.kde.org/docs/plasma/

## GNOME Shell Extensions

Extensions can alter GNOME Shell UI, but they are a distinctly privileged/version-specific mechanism compared with Glint packages. Glint shouldn't assume its `.glint` JS can be loaded as a GNOME extension or silently patch the Shell.

Source: https://extensions.gnome.org/about/

## OS-specific widget/theme projects

Many utilities deliver widgets, docks, live wallpaper, window overlays or taskbar changes on one OS. This is already a crowded capability set. Glint's core differentiator should be **author once, express compatibility, distribute freely, install safely, configure together** rather than “the first desktop customization app.”

## Product differentiation summary

| Dimension | Glint ambition |
|---|---|
| OS reach | Windows, Linux, macOS with actual capability matrix |
| Theme depth | Whole **Glint-owned** surface implementations, not only design tokens |
| Distribution | Official library + independent Git + offline `.glint` |
| Authoring | Declarative preset or programmable React/TS + SDK |
| Compatibility | Per-surface and per-action capability discovery |
| Trust | Sandboxed runtime, permission negotiation, rollback |
| Composition | Mix surface themes and profile/layout overrides |
| Cloud | Not required |

## Avoid misleading comparisons

Do not claim Glint universally replaces native taskbars/notifications everywhere or that it has all Quickshell/Wayle/KDE native integrations by virtue of using Rust. Deliverables must prove specific capabilities on named platforms.
