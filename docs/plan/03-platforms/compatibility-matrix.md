# Cross-platform feature matrix and product contract

**Critical rule:** This is a **design feasibility matrix, not a verified shipping support matrix**. Every green/checkmark equivalent needs an executable integration test on a named OS/DE/compositor release. “Glint-managed” is distinct from “replaces native OS UI”.

Legend: **G** = generally achievable with own app/window; **I** = integration-dependent; **E** = experimental/restricted; **N** = not a universal supported claim.

| Capability | Windows | Linux/Hyprland | Linux/KDE | Linux/GNOME | macOS |
|---|---|---|---|---|---|
| App settings / theme browser | G | G | G | G | G |
| Import `.glint` / Git source | G | G | G | G | G |
| Static wallpaper set | G | I | I | I | G |
| Own widget windows | G | G/I | G/I | G/I | G |
| Own floating launcher | G | G | G | G/I | G |
| Own control-center popup | G | G | G | G/I | G |
| Edge-attached Glint panel | I (appbar) | G (layer-shell) | I | I/N | I (overlay) |
| Desktop-layer widgets | I | G/I | I | I | I |
| Replace native taskbar/panel | E/N | I (custom compositor config) | I (Plasma extensions) | I (Shell extension) | N |
| Modify OS-native notification UI | N | I (daemon/provider) | I | I (Shell extension) | N |
| Read audio/battery/network | I | I | I | I | I |
| Write volume/brightness | I | I | I | I | I |
| Workspaces read/switch | I | G via IPC | I | I/restricted | E |
| Third-party application window styling | N | I via GTK/KDE theme bridges | I | I | N |
| OS login/UAC/auth screens | N | N as generic theme | N as generic theme | N | N |

## Shared guarantees Glint should be able to keep

1. **Portable package format** and manifest parse/validation.
2. **Manager UI** and local theme browsing/installed list.
3. **Themes render Glint-owned content** in an appropriate host even if placement degrades to ordinary window/overlay.
4. **Capability introspection** and precise unsupported feedback.
5. **Safe mode, fallback theme and local package export/import**.
6. **Multi-monitor-aware layout** to the extent native APIs report displays.
7. **No need for account or cloud** after package installation.

## Things that will vary by OS

Native panel replacement, system tray semantics, notification interception, taskbar docking, window/global shortcuts, deep icon/cursor modifications, controlling brightness on all monitors, Bluetooth toggling, shell workspaces, third-party application GTK/Qt styling, login/lock-screen integration. Developers must not infer these from target OS name alone.

## Packaging cross-platform themes

Allow `compatibility.platforms` to list a subset. An author can provide shared React UI and optional platform-specific `integrations`. `requiredCapabilities` are hard blockers unless an explicitly defined fallback is included; `optionalCapabilities` are runtime-gated. The gallery labels supported OSes **and** specific integration limits. A theme with a Windows-specific panel hack is not automatically “works on macOS”.

## Test environment policy

Pin OS version, compositor/DE build, display server, hardware (internal/external displays), multi-monitor setup, scale factor, power conditions and session permissions. A compatibility claim is attached to a tested matrix row, not to an entire OS family forever.

## Current research grounding

- Windows supports custom **application desktop toolbars** via `SHAppBarMessage`; not identical to replacing Explorer taskbar.
- Hyprland provides IPC event sockets; layer-shell supports compositor-managed desktop layers when advertised.
- KDE provides Global Theme/plugin systems; GNOME customization is generally mediated by Shell extensions.
- Apple documents status bar items and desktop wallpaper APIs; no universal external API for replacing its system-wide UI.

Source URLs are in `08-research/official-references.md`.
