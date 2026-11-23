# macOS backend — AppKit-controlled content and native limits

## Boundary

Glint's own manager, widgets, floating launchers and supported overlay surfaces can run on macOS. Glint **cannot claim to replace** the system menu bar, Dock, Notification Center, lock/login screen or arbitrary app UIs. These are OS-owned and often protected by sandbox, entitlement and privacy rules.

## Candidate native frameworks

- AppKit for NSWindow, NSScreen, NSStatusBar/NSStatusItem, input/focus behavior and desktop app lifecycle.
- `NSWorkspace.setDesktopImageURL(_:for:options:)` for wallpaper per screen when permitted.
- WKWebView for Glint programmable React/TS UI in owned windows.
- CoreGraphics/display APIs for screen geometry and available display information.
- Scoped media/power/audio interfaces only where documented or permitted; do not expect all control center toggles to be accessible.
- Accessibility APIs where user explicitly grants permission for window/app interactions; no guaranteed arbitrary app control.

## Modules

```text
crates/glint-macos/
├── src/lib.rs
├── src/appkit.rs
├── src/status_bar.rs
├── src/windows.rs
├── src/wallpaper.rs
├── src/displays.rs
├── src/audio.rs
├── src/apps.rs
├── src/power.rs
├── src/capabilities.rs
└── tests/macos_integration.rs
```

## Surface behavior

- A Glint panel is likely an ordinary overlay/window or app-controlled status bar item, not a shell-level reserve-work-area taskbar equivalent.
- Interactive desktop widgets need careful window-level behavior, Mission Control/Spaces tests, multiple monitors and click-through input regions.
- Notches and menu bar safe areas matter; layout engine should use platform-reported screen visible frame and geometry.
- Permission-denied Accessibility/Automation controls return clear errors without trapping users in repeated permission prompts.
- AppKit UI work happens on the main thread; IPC data flow from Tokio must marshal safely.

## Packaging

Use native `.app` with signing/notarization where applicable for distribution. A packaged `.glint` theme cannot itself be a mechanism to bypass Gatekeeper or dynamically load unrestricted native code. Examine Mac App Store rules before promising that distribution channel; standalone signed distribution may be more practical for deep customization.

## Testing

- Intel vs Apple Silicon; screen notch and external monitor variants.
- Multi-Space and fullscreen transitions; Desktop/Stage Manager if relevant.
- Permissions before/after denial, status bar hidden/overflowed.
- Renderer crash with safe native app fallback.
- macOS version differences in WKWebView/NSWindow levels.

## Grounded docs

- NSStatusBar: https://developer.apple.com/documentation/appkit/nsstatusbar
- NSWorkspace wallpaper: https://developer.apple.com/documentation/appkit/nsworkspace/setdesktopimageurl%28_%3Afor%3Aoptions%3A%29
- Tauri WebView engines: https://tauri.app/reference/webview-versions/

## Scope choice

A first macOS build should at least support the settings UI, theme import, safe preview, wallpaper, user-owned widgets and an application launcher; more invasive OS controls should be behind measured R&D and explicit user consent.
