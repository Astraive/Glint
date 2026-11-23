# Linux backend — Wayland/X11 plus compositor/desktop adapters

## Why “Linux support” is not one API

Linux has multiple layers: hardware/system services (D-Bus, PipeWire, NetworkManager, UPower etc.), display protocol (Wayland/X11), compositor (Hyprland, Sway, Niri, KWin etc.) and desktop environment (KDE Plasma, GNOME, others). Glint should detect **session and compositor**, not just `target_os=linux`.

## Proposed modules

```text
crates/glint-linux/
├── src/session.rs            # Wayland/X11, desktop and compositor detection
├── src/providers/            # audio, power, network, Bluetooth, media
├── src/wayland/              # surface protocol support
├── src/x11/                  # X11 window/desktop behavior
├── src/compositors/
│   ├── hyprland.rs
│   ├── sway.rs
│   ├── niri.rs
│   └── generic.rs
├── src/desktops/
│   ├── plasma.rs
│   └── gnome.rs
└── src/theme_integration/     # GTK, cursor, wallpapers, Plasma bridges
```

## Layer-shell / native surface hosting

`wlr-layer-shell` gives supporting Wayland compositors a mechanism for top/bottom/background/overlay layers, anchors, exclusive zones and keyboard interactivity. **Do not assume all compositors advertise it.** It is notably different from generic Wayland xdg-toplevel surfaces. Glint must negotiate the protocol and gracefully degrade or refuse a placement when absent.

For deep Linux shell UX, research `gtk4-layer-shell` with GTK4 and a compatible WebKitGTK embedding mechanism. The existence of `wry` on Linux does **not** prove it works in arbitrary layer-shell topologies. Validate event loop, transparency, input regions, rendering, scaling and GPU behavior in a test spike.

## Hyprland reference implementation

Hyprland exposes a command socket and an events socket (`.socket2.sock`); use the native async client with safe framing/reconnect. On reconnect query authoritative state, then stream events to Glint providers.

System capabilities include workspaces, focused monitor, windows, compositor-specific operations and window events. User-visible actions use typed commands instead of shelling out unvalidated `hyprctl` command strings.

## KDE Plasma integration

Plasma is a **first-class independent desktop environment** with existing widgets, native icon packs, panel layout templates and Global Themes. Use documented Plasma packaging/integration APIs or user-visible external theme application workflows. Avoid treating KWin as Hyprland, and do not replace native Plasma components without a tested approach and opt-in.

## GNOME integration

GNOME Shell's top bar, overview and message tray are owned by GNOME Shell. Deeper changes often require GNOME Shell extensions, which have version-specific APIs/lifecycle and security implications. Glint can still run ordinary windows/overlays and managed widgets. A GNOME extension should be separately versioned and tested, not installed implicitly by a random `.glint` theme.

## X11 integration

Use X11/EWMH/ICCCM where suitable for window enumeration, desktop layers, docks and work areas, but verify each window manager's actual behavior. X11 global desktop capabilities are not equivalent to Wayland. Treat access control and privacy differences explicitly.

## Shared system providers

Use D-Bus clients where supported for NetworkManager/BlueZ/UPower; PipeWire/PulseAudio integration for audio; MPRIS for media; freedesktop notifications/status notifier where present. Services may be absent or replaced, so capabilities must be probed rather than assumed by distro.

## Linux install realities

- GTK/WebKitGTK and other system libraries depend on distro packaging; installer must verify ABI and dependencies.
- Different Wayland compositors have different layer-shell protocol versions, focus and screenshot behavior.
- Packaging targets should be phased (e.g., Arch/AUR first for Hyprland users, then distro native packages/Flatpak subject to sandbox constraints).
- Flatpak sandbox can limit control over compositor IPC or host settings; do not promise identical behavior under every packaging mode.

## Matrix to test

Arch + Hyprland Wayland; Sway and Niri; KDE Plasma Wayland; GNOME Wayland; an X11 session. Each test needs compositor/DE release, scaling, monitor layout, notification and network/audio provider variants.

## Grounded docs

- Hyprland IPC: https://wiki.hypr.land/ipc/
- Layer shell: https://wayland.app/protocols/wlr-layer-shell-unstable-v1
- KDE: https://develop.kde.org/docs/plasma/
- GNOME Shell extensions: https://extensions.gnome.org/about/
- Wayle: https://wayle.app/guide/getting-started
