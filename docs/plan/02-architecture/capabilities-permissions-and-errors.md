# Capability model — precise contracts instead of “works everywhere”

## Why booleans aren't enough

An OS can provide only read access to a value, require permission, permit only a custom overlay, or rely on experimental third-party integration. A simple `supportsControlCenter=true` is misleading because Glint **can** render a control center even when it **cannot** replace the OS's native control center.

## Proposed capability representation

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Availability {
    Supported,
    ReadOnly,
    PermissionRequired,
    Experimental,
    Unavailable { reason: String },
}

pub struct Capability {
    pub id: String,
    pub availability: Availability,
    pub scope: Option<String>, // e.g. compositor or hardware-dependent detail
}
```

A distinct `IntegrationEffect` should convey whether the action affects **Glint-owned UI** versus an **OS-managed setting**. Package authors can declare `requiredCapabilities` and `optionalCapabilities`.

## Capability namespaces

- `surface.panel`, `surface.launcher`, `surface.notification-overlay`, `surface.desktop-widget`.
- `system.audio.read`, `system.audio.write`, `system.brightness.read`, `system.brightness.write`.
- `system.network.read`, `system.network.toggle`, `system.bluetooth.read`, `system.bluetooth.toggle`.
- `system.power.sleep`, `system.power.shutdown`, `system.media.read`, `system.media.control`.
- `desktop.windows.list`, `desktop.windows.focus`, `desktop.workspaces.read`, `desktop.workspaces.switch`.
- `native.wallpaper.set`, `native.system-theme.apply`, `native.taskbar.modify`, `native.status-item.create`.
- `host.layer-shell`, `host.appbar`, `host.desktop-layer`, `host.transparent-overlay`, `host.global-hotkey`.

## Capability negotiation flow

1. Adapter discovery returns host capabilities and per-action authorization requirements.
2. Theme declares mandatory and optional abilities in manifest.
3. Installer validates and presents exact unavailable/permission-gated actions.
4. Profile resolver selects viable surfaces and disables unsupported interactive controls.
5. Runtime SDK exposes `capabilities.get('system.audio.write')`, normalized feature status and error hints.
6. If capability vanishes (monitor unplug, Bluetooth gone, OS permission revoked), emit change event and update UI.

## Capability errors

| Code | Example | UI guidance |
|---|---|---|
| `unsupported-platform` | macOS native notification center replacement | Fall back to Glint overlay |
| `unsupported-session` | layer shell not advertised by compositor | Choose ordinary window mode or disable panel |
| `permission-required` | application accessibility API not approved | Explain native OS permission flow |
| `hardware-unavailable` | brightness unavailable on external display | Render disabled control |
| `temporary-unavailable` | network manager restarting | Retry after event/reconnect |
| `conflict` | existing appbar occupies requested edge | Alternate placement/ask user |
| `theme-incompatible` | minimum SDK version higher than installed Glint | Block activation, keep current theme |
| `invalid-state` | switching workspace after it was removed | Refresh workspace list |

## Semantics for “theme supports all OSes”

A manifest claiming `windows`, `linux`, `macos` **only claims compatible theme content and a viable fallback for required features**. It cannot imply identical native taskbar integrations. The gallery displays per-surface support and honors optional packages targeting one or two OSes.

## User trust

- Display permissions before installing executable UI packages.
- Separate read data from actions; never grant full access for display-only widgets.
- Allow revocation without uninstalling the theme.
- Keep sensitive properties (SSID, foreground application names, window titles, calendar content, external network requests) behind appropriate scoping or privacy controls.
- A package's capability declarations do not themselves grant permission. Runtime enforcement is required.
