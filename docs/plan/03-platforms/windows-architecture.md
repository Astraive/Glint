# Windows backend — native integration without fragile shell assumptions

## Target and boundary

Start with **Windows 11** as recommended test baseline (Windows 10 compatibility must be researched separately). Use standard user-level APIs wherever possible; avoid replacing Explorer, editing undocumented taskbar implementation details or modifying protected system files as normal Glint behavior.

## Candidate native components

- `windows` Rust crate for Win32, COM, shell, display and supported system APIs.
- `IDesktopWallpaper` for monitor-aware wallpaper management, slideshow and background options.
- `SHAppBarMessage`/application desktop toolbar integration for a Glint-owned docked panel that reserves desktop space. This is **not** rewriting Explorer's taskbar.
- Windows App SDK/WinRT or Win32 for application enumeration and safe launching, where applicable.
- Core Audio APIs for endpoint volume; appropriate interfaces for devices/network/power.
- WebView2 for rendering programmable surfaces hosted inside native windows.
- Standard notification API integration where available, but **not** promise intercepting/replacing the native Windows Notification Center.

## Proposed adapter structure

```text
crates/glint-windows/
├── src/lib.rs
├── src/capabilities.rs
├── src/wallpaper.rs
├── src/monitors.rs
├── src/appbar.rs
├── src/overlay.rs
├── src/audio.rs
├── src/power.rs
├── src/network.rs
├── src/applications.rs
├── src/windows.rs
├── src/hotkeys.rs
└── tests/windows_integration.rs
```

## Surface-host behavior

1. Create owned borderless window with correct DPI awareness and monitor coordinates.
2. For appbar mode, register with Windows appbar APIs, negotiate work area, and handle taskbar/work-area events.
3. For floating popups, use ordinary top-level windows with focus rules and positioning; do not steal focus on notifications.
4. Use WebView2 in restricted contexts to display themed UI; restrict navigation and external code.
5. Shell host should not crash Explorer or leave it hidden on exit.

## Features needing special care

- Explorer taskbar can coexist with a Glint panel. Hiding native taskbar is **opt-in**, reversible and cannot be assumed robust after OS updates.
- Control Panel and Settings app appearance generally cannot be redesigned through the Glint SDK. A Glint control-center can offer its own buttons that call approved system APIs.
- System tray forwarding is complex and OS-version sensitive; treat native tray emulation as research/P2, and support Glint-provided indicators from system providers first.
- Brightness APIs depend on hardware, display connection and privileges; external monitor control may need DDC/CI and should not be a core assumption.
- Native Alt-Tab, secure input and login UIs are not replaced.
- Wallpapers may be per-monitor; record previous values before applying profile.

## Example feature status

| Feature | Target stage | Condition |
|---|---|---|
| Set wallpaper | P0 | `IDesktopWallpaper` permissions/support |
| Custom floating launcher | P1 | Standard owned window |
| Edge appbar panel | P1 | Appbar registration/work area negotiation |
| Glint control center | P1 | Glint popup; actions capability-gated |
| Notification overlay | P1 | Own overlay, not native history replacement |
| Explorer taskbar replacement | Research only | Not a required milestone |
| Deep file explorer skins | Out of scope | Different target/product/security risk |

## Test cases

- Single/multi-monitor, mixed DPI, rotations and taskbar alignment.
- Shell restart / Explorer restart while Glint is running.
- Windows update changes appbar/work area behavior.
- Fullscreen game and screen recording; overlays should not trap focus.
- Rapid sleep/resume, login/logoff, monitor hot-plug.
- Failure mid-apply restores native wallpaper and doesn't leave invisible click-blocking window.

## Grounded docs

- Appbars: https://learn.microsoft.com/en-us/windows/win32/shell/application-desktop-toolbars
- SHAppBarMessage: https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shappbarmessage
- IDesktopWallpaper: https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-idesktopwallpaper
