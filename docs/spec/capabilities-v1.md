# Capability registry v1

- **Status:** frozen for SDK v1 by [ADR 0009](../adr/0009-frozen-v1-capability-registry.md) (Proposed; maintainer sign-off pending)
- **Ticket:** CAP-005
- **Source of truth:** [`crates/glint-platform/src/registry.rs`](../../crates/glint-platform/src/registry.rs) (`glint_platform::registry::REGISTRY`)
- **Generated from it:** the table below and [`packages/types/src/capabilities.ts`](../../packages/types/src/capabilities.ts), both drift-checked by `cargo test -p glint-platform --test registry`

This page lists every capability ID that exists in Glint SDK v1, what using it
does, and how a new ID is added. It answers question 4 of
[Open design choices requiring evidence](../plan/06-engineering/risks-feasibility-and-tradeoffs.md#open-design-choices-requiring-evidence)
for capability names. Permission names and grant scopes are CAP-004's.

## What an ID means

A capability ID is `<namespace>.<name>[.<name>...]` in one of five namespaces
(the syntax is checked by `glint-schema` and `CapabilityId::parse`). Being
well formed is not enough: an ID must also be in the registry.

| Namespace | Covers |
|---|---|
| `surface.*` | Showing a Glint-owned surface. One ID per standard surface kind in the [surface catalog](../architecture/surfaces.md), plus `surface.custom` |
| `system.*` | System state and actions: audio, brightness, network, Bluetooth, battery, power, media |
| `desktop.*` | Windows and workspaces |
| `native.*` | Changes to OS-managed appearance |
| `host.*` | Host window mechanisms a surface backend may use |

Every entry has:

- **Access**, the read/write split. `read` observes state and includes change
  events for it; `write` changes state or performs an action; `present` shows
  Glint-owned UI or uses a host window mechanism and neither reads user data
  nor changes OS settings. Reading and writing the same thing are always
  separate IDs (`system.audio.read` / `system.audio.write`), so a display-only
  widget never needs write access.
- **Sensitivity**: `low` (no personal data, no effect outside Glint),
  `moderate` (a reversible change to OS state, or state that hints at user
  activity), `high` (personal data such as window titles, the SSID or media
  metadata, or a disruptive or persistent change). High-sensitivity values are
  redacted in diagnostics. Sensitivity informs prompts; it never grants
  anything.
- **Effect**: `glint-owned-ui` or `os-managed-setting` (the
  `IntegrationEffect` from CAP-001).
- **Platform notes**: what is evidenced on Windows 11 (the only hands-on host,
  [ADR 0002](../adr/0002-windows-11-only-hands-on-test-host.md)) and what is
  planned elsewhere. Availability is always reported by the host adapter at
  runtime; a note never makes a capability available.

## Registry

<!-- BEGIN GENERATED: capability registry (crates/glint-platform/src/registry.rs) -->

| ID | Namespace | Access | Sensitivity | Effect | Summary | Platform notes |
|---|---|---|---|---|---|---|
| `surface.panel` | surface | present | low | glint-owned-ui | Show the panel surface (pinned desktop toolbar). | Glint-owned window; which placements the host can realise is reported by the surface backend (SRF-002). |
| `surface.dock` | surface | present | low | glint-owned-ui | Show the dock surface (running and pinned apps). | Glint-owned window; which placements the host can realise is reported by the surface backend (SRF-002). |
| `surface.launcher` | surface | present | low | glint-owned-ui | Show the launcher surface (searchable app picker). | Glint-owned window; which placements the host can realise is reported by the surface backend (SRF-002). |
| `surface.control-center` | surface | present | low | glint-owned-ui | Show the Glint control center surface (quick settings); it never replaces the OS one. | Glint-owned window; which placements the host can realise is reported by the surface backend (SRF-002). |
| `surface.notification` | surface | present | low | glint-owned-ui | Show toast notifications for Glint-visible events. | Glint-owned window; which placements the host can realise is reported by the surface backend (SRF-002). |
| `surface.notification-center` | surface | present | low | glint-owned-ui | Show the notification history surface inside Glint. | Glint-owned window; which placements the host can realise is reported by the surface backend (SRF-002). |
| `surface.desktop` | surface | present | low | glint-owned-ui | Show the desktop surface (background canvas and widgets). | Glint-owned window; which placements the host can realise is reported by the surface backend (SRF-002). |
| `surface.calendar` | surface | present | low | glint-owned-ui | Show the calendar popup surface. | Glint-owned window; which placements the host can realise is reported by the surface backend (SRF-002). |
| `surface.power-menu` | surface | present | low | glint-owned-ui | Show the power menu surface; the actions need `system.power.*`. | Glint-owned window; which placements the host can realise is reported by the surface backend (SRF-002). |
| `surface.workspace-switcher` | surface | present | low | glint-owned-ui | Show the workspace switcher surface. | Glint-owned window; which placements the host can realise is reported by the surface backend (SRF-002). |
| `surface.window-switcher` | surface | present | low | glint-owned-ui | Show the window switcher overlay surface. | Glint-owned window; which placements the host can realise is reported by the surface backend (SRF-002). |
| `surface.volume-osd` | surface | present | low | glint-owned-ui | Show the volume on-screen display. | Glint-owned window; which placements the host can realise is reported by the surface backend (SRF-002). |
| `surface.brightness-osd` | surface | present | low | glint-owned-ui | Show the brightness on-screen display. | Glint-owned window; which placements the host can realise is reported by the surface backend (SRF-002). |
| `surface.media-overlay` | surface | present | low | glint-owned-ui | Show the media overlay surface. | Glint-owned window; which placements the host can realise is reported by the surface backend (SRF-002). |
| `surface.custom` | surface | present | low | glint-owned-ui | Show author-defined `custom:<namespace>.<name>` surfaces. | Glint-owned window; which placements the host can realise is reported by the surface backend (SRF-002). |
| `system.audio.read` | system | read | low | glint-owned-ui | Read the default output device's volume and mute state, and receive change and device-change events. | Windows: supported (Core Audio endpoint volume, SPK-010 evidence); no device is unavailable. Linux (PipeWire/PulseAudio), macOS: planned. |
| `system.audio.write` | system | write | moderate | os-managed-setting | Set the default output device's volume and mute state. | Windows: supported without a prompt for unpackaged processes (SPK-010); a denied write is permission-required. Linux, macOS: planned. |
| `system.brightness.read` | system | read | low | glint-owned-ui | Read display brightness. | Windows: not yet evidenced; external displays are often hardware-unavailable. Linux, macOS: planned. |
| `system.brightness.write` | system | write | moderate | os-managed-setting | Set display brightness. | Windows: not yet evidenced; external displays are often hardware-unavailable. Linux, macOS: planned. |
| `system.network.read` | system | read | low | glint-owned-ui | Read connectivity level, interface type and cost, and receive change events. Excludes the SSID. | Windows: supported (NLM and NetworkInformation, SPK-010). Linux (NetworkManager), macOS: planned. |
| `system.network.toggle` | system | write | high | os-managed-setting | Turn a network radio (Wi-Fi) on or off. | Windows: unavailable in SPK-010 (no toggle path; Windows.Devices.Radios not evaluated). Linux, macOS: planned. |
| `system.network.ssid` | system | read | high | glint-owned-ui | Read the connected Wi-Fi SSID. Reveals location; separate from system.network.read and redacted in diagnostics. | Windows: supported when location consent allows it, otherwise permission-required (SPK-010). Linux, macOS: planned. |
| `system.bluetooth.read` | system | read | moderate | glint-owned-ui | Read Bluetooth radio state and paired device names. | Windows: not yet evidenced. Linux (BlueZ), macOS: planned. |
| `system.bluetooth.toggle` | system | write | high | os-managed-setting | Turn the Bluetooth radio on or off. | Windows: not yet evidenced. Linux, macOS: planned. |
| `system.battery.read` | system | read | low | glint-owned-ui | Read battery level, charging state and estimates, and receive change events. | Windows: supported (GetSystemPowerStatus, PowerManager, SPK-010); a desktop without a battery is unavailable, not an error. Linux (UPower), macOS: planned. |
| `system.power.source.read` | system | read | low | glint-owned-ui | Read whether the device runs on AC power. | Windows: supported (ACLineStatus, SPK-010), also on desktops without a battery. Linux, macOS: planned. |
| `system.power.sleep` | system | write | high | os-managed-setting | Put the device to sleep. | Windows: not yet evidenced. Linux, macOS: planned. |
| `system.power.shutdown` | system | write | high | os-managed-setting | Shut down or restart the device. | Windows: not yet evidenced. Linux, macOS: planned. |
| `system.media.read` | system | read | high | glint-owned-ui | Read the current media session's track metadata and playback state. | Windows: not yet evidenced (system media transport controls). Linux (MPRIS), macOS: planned. |
| `system.media.control` | system | write | moderate | os-managed-setting | Play, pause and skip in the current media session. | Windows: not yet evidenced. Linux (MPRIS), macOS: planned. |
| `desktop.windows.list` | desktop | read | high | glint-owned-ui | List top-level windows with titles and app names, and receive window events. | Windows: not yet evidenced. Linux: compositor-dependent. macOS: needs the accessibility permission (permission-required). |
| `desktop.windows.focus` | desktop | write | moderate | os-managed-setting | Bring a listed window to the foreground. | Windows: not yet evidenced (foreground-lock rules apply). Linux: compositor-dependent. macOS: needs the accessibility permission. |
| `desktop.workspaces.read` | desktop | read | moderate | glint-owned-ui | List workspaces (virtual desktops) and the active one, and receive change events. | Windows: not yet evidenced (virtual desktops have no public enumeration API). Linux: compositor-dependent. macOS: planned. |
| `desktop.workspaces.switch` | desktop | write | moderate | os-managed-setting | Switch the active workspace. | Windows: not yet evidenced. Linux: compositor-dependent. macOS: planned. |
| `native.wallpaper.set` | native | write | high | os-managed-setting | Set the desktop wallpaper per display, with restore. | Windows: planned (IDesktopWallpaper). Linux: desktop-dependent. macOS: planned. |
| `native.system-theme.apply` | native | write | high | os-managed-setting | Apply OS light/dark mode or accent colour. | Windows: planned. Linux: desktop-dependent. macOS: planned. |
| `native.taskbar.modify` | native | write | high | os-managed-setting | Change supported OS taskbar settings (auto-hide, alignment). | Windows: planned, supported settings only; the native taskbar is never replaced. Linux, macOS: unsupported or desktop-dependent. |
| `native.status-item.create` | native | write | moderate | os-managed-setting | Create a notification-area (tray) or menu-bar status item. | Windows: planned (notification area). Linux (StatusNotifierItem), macOS (menu bar): planned. |
| `host.layer-shell` | host | present | low | glint-owned-ui | Place surfaces with the Wayland layer-shell protocol. | Linux Wayland only, when the compositor advertises it; unsupported-platform on Windows and macOS. |
| `host.appbar` | host | present | low | glint-owned-ui | Reserve a screen edge as an application desktop toolbar. | Windows only (SHAppBarMessage); unsupported-platform elsewhere. |
| `host.desktop-layer` | host | present | low | glint-owned-ui | Place a surface on the desktop layer, below normal windows. | Windows: planned (WorkerW). Linux: layer-shell background or X11 desktop type. macOS: planned. |
| `host.transparent-overlay` | host | present | low | glint-owned-ui | Show transparent, optionally click-through overlay windows. | Windows: planned (layered windows). Linux, macOS: planned. |
| `host.global-hotkey` | host | present | moderate | glint-owned-ui | Register system-wide keyboard shortcuts that open Glint surfaces. | Windows: planned (RegisterHotKey). Linux: portal or compositor-dependent. macOS: planned. |

<!-- END GENERATED: capability registry -->

## Unknown IDs fail closed

An ID that is not in the table is unknown everywhere. Nothing ignores it,
warns and continues, or grants it.

| Where | What happens | Test |
|---|---|---|
| Package reader and packer (`glint-package`, also used by `glint theme validate`) | The manifest is refused with `GLINT_PACKAGE_UNKNOWN_CAPABILITY`, required or optional | `glint-package` `tests/references.rs`, `tests/security.rs`, `tests/roundtrip.rs` |
| Capability service | An adapter report containing an unregistered ID is refused as `adapter-failure`; like any failed discovery, every known capability turns `temporary-unavailable` | `glint-platform` `tests/registry.rs` |
| Snapshot lookups | `availability` is `unavailable` / `not-advertised`; `require` and `require_writable` return `unknown-capability` | `tests/registry.rs` |
| Activation gate | Activation is refused (`theme-incompatible`) with an `unknown-capability` blocker for each unknown ID, required **or optional** | `tests/registry.rs` |
| Surface catalog (`glint-surfaces`) | Every catalog capability must be registered | `glint-surfaces` `tests/registry.rs` |

`glint-schema` stays a syntax-only parser (it does not depend on
`glint-platform`); the JSON Schema `schemas/glint-theme-v1.schema.json`
likewise checks syntax only. The registry check is applied by every consumer
above.

## Freeze rules

1. The registry version is `1`. Within v1 an ID is never removed, renamed or
   given a different meaning, and its access never changes.
2. IDs may be **added** within v1. Each addition needs an ADR.
3. Removing, renaming or splitting an ID, or changing its access, needs
   registry v2: a new ADR, a manifest `schemaVersion` decision and a
   migration note.
4. A sensitivity, summary or platform note may be corrected without an ADR
   when evidence changes (for example a spike result); the change is listed in
   the changelog. Raising sensitivity is always allowed; lowering it needs an
   ADR.
5. The test `registry_matches_the_frozen_v1_list` pins the full list, so an
   accidental edit fails CI.

## Adding a capability

1. Write an ADR (copy [the template](../adr/0000-template.md)) that names the
   ID, its access, sensitivity, effect and evidence for at least one
   platform, and explains why an existing ID does not cover it. Prefer a
   separate `read` / `write` pair; give personal data its own ID (as
   `system.network.ssid` is separate from `system.network.read`).
2. Append a `CapabilitySpec` to `REGISTRY` in
   `crates/glint-platform/src/registry.rs`, in its namespace group, and
   append the ID to `FROZEN_V1` in `crates/glint-platform/tests/registry.rs`.
3. Regenerate: `GLINT_UPDATE_TYPES=1 cargo test -p glint-platform --test registry`
   (rewrites the table above and `packages/types/src/capabilities.ts`).
4. If a surface uses it, add it to the surface catalog and regenerate the
   surface types (`GLINT_UPDATE_TYPES=1 cargo test -p glint-surfaces --test typegen`).
5. Update the [threat model](../security/threat-model.md) when the
   capability exposes personal data or changes OS state, and add a changelog
   entry.

Package acceptance, the runtime checks and the TypeScript `CapabilityId` type
follow automatically.

## Differences from the plan list

The plan's [Capability namespaces](../plan/02-architecture/capabilities-permissions-and-errors.md#capability-namespaces)
list is the baseline; [ADR 0009](../adr/0009-frozen-v1-capability-registry.md)
records the differences:

- `surface.notification-overlay` and `surface.desktop-widget` are not
  registered; the implemented surface catalog names them `surface.notification`
  and `surface.desktop`. The other surface kinds from the catalog are added.
- `system.network.ssid`, `system.battery.read` and
  `system.power.source.read` are added on the evidence of the
  [Windows providers spike](../research/spikes/windows-system-providers.md).
- Event subscriptions are part of the matching `read` ID (no separate
  `*.events` IDs), and network details are part of `system.network.read`.
