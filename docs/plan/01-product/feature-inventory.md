# Complete feature inventory with priority and platform caveats

Priorities: **P0** basic shippable foundation, **P1** early product, **P2** expansion, **P3** experimental. They are recommendations, not commitments.

| Area | Feature | Pri | Notes |
|---|---|---:|---|
| Themes | Browse official library and installed list | P0 | Cached registry metadata |
| Themes | Install from arbitrary Git URL | P0 | Pin commit/ref, verify content |
| Themes | Import/export `.glint` archive | P0 | Offline, self-contained |
| Themes | Manifest schema and compatibility validator | P0 | Strict paths and capabilities |
| Themes | Preview without applying native settings | P0 | Simulated system data |
| Themes | Activate/disable/revert to default | P0 | Crash-safe recovery |
| Themes | Base theme + per-surface overrides | P1 | Stable resolution rules |
| Themes | Per-monitor and per-workspace profiles | P1 | Monitor ID mapping |
| Themes | Versioned updates, pinning and rollback | P1 | No silent upgrades by default |
| Appearance | Color palette, semantic tokens | P0 | Theme-owned colors separate from Glint product branding |
| Appearance | Fonts, spacing, radius, border, blur | P0 | Ship compatible font licenses |
| Appearance | Light/dark variants, adaptive tokens | P1 | Accessibility/contrast controls |
| Appearance | Motion/transition profiles | P1 | Reduced motion preference |
| Wallpaper | Static image, scaling, per-monitor | P0 | OS adapters and restore points |
| Wallpaper | Slideshow, schedule | P1 | Event-driven not constant polling |
| Wallpaper | Animated/live wallpaper | P2 | Rendering/power budget |
| Wallpaper | Cursor-reactive wallpaper, shader effects | P3 | Platform-specific desktop/background surface |
| Shell | Top/side/bottom panel | P0 | Glint-owned surface; native behavior varies |
| Shell | App launcher/search | P0 | App enumeration and launch permissions |
| Shell | Dock/taskbar-like surface | P1 | Does not necessarily remove native taskbar |
| Shell | Control center | P1 | Backend capabilities for toggles |
| Shell | System tray | P1 | OS-specific bridging protocols |
| Shell | Notification popups | P1 | Own notifications before replacing native history |
| Shell | Notification history center | P2 | Requires provider integration/consent |
| Shell | Volume/brightness OSD | P1 | Avoid double OSD where native appears |
| Shell | Workspace/window switcher | P2 | Some OSes limit enumerating/controlling windows |
| Shell | Power menu | P2 | Confirmations and privileged OS calls |
| Widgets | Clock/date/calendar | P0 | First reference widget |
| Widgets | Media playback | P1 | System provider bridge |
| Widgets | Audio, network, battery, Bluetooth | P1 | Read-only fallback where needed |
| Widgets | CPU/memory/storage graph | P1 | Rate limits and permissions |
| Widgets | Weather | P2 | Internet permission/data provider |
| Widgets | Glyph/dot-matrix visualizations | P2 | Nothing-inspired theme example |
| Widgets | Third-party widgets | P2 | Permission-scoped sandbox |
| System | App list/window focus/workspace | P1 | Capability-specific |
| System | Brightness/volume/notifications | P1 | Permission checks; absent on some hardware |
| System | Keyboard shortcuts / gestures | P2 | OS-reserved shortcuts excluded |
| System | OS theme bridges | P2 | KDE/GTK/cursor/system accent when possible |
| Native | Windows appbar, overlays | P1 | Win32 integration, not Explorer replacement |
| Native | Linux layer shell + Hyprland IPC | P1 | Verified supported compositor |
| Native | macOS desktop overlay/status item | P1 | AppKit, not macOS menu replacement |
| Creator | `create`, `dev`, `build`, `pack`, `validate` | P0 | TSX and declarative templates |
| Creator | Hot module replacement | P1 | Isolated dev renderer |
| Creator | SDK hooks/events/commands | P0 | Stable versioned API |
| Creator | Theme composition inspector | P2 | Debug cascade, deps, perms |
| Distribution | Official index + source abstraction | P0 | Public `glint-library` by default |
| Distribution | Custom registries + URL install | P0 | No central account |
| Distribution | Theme catalog filters/rating/popularity | P2 | Only if ethically sourced and hosted |
| Distribution | Marketplace purchase/payment | P3 | Optional separate strategy |
| Reliability | Default theme fallback, safe mode | P0 | Desktop still operable without Glint |
| Reliability | Package validation / journal / rollback | P0 | Avoid partial modification |
| Reliability | Diagnostics, debug logs | P0 | Redact secrets |
| Accessibility | Keyboard navigation/focus | P0 | Every Glint UI surface |
| Accessibility | Contrast/reduced motion/scaling | P1 | WCAG-influenced goals |
| Security | Untrusted JS isolation | P0 for programmable | Do not rely on manifest `permissions` alone |

## Product scope warning

“Controls” and “notifications” can mean either **Glint-rendered replacements/overlays** or **OS-managed system UI**. Only the first is under universal Glint control. A theme may *request* native integration, which the adapter may deny. Document this clearly in any marketplace listing to avoid misleading installers.
