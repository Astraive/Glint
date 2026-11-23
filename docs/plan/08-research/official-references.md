# Verified technical references and further reading

**Research check:** 2026-11-23. URLs point to primary docs or official project repositories where possible. These sources support feasibility discussions, not proof that Glint itself exists or that every integration works. Reverify before implementation because API versions change.

## Runtime / framework

1. Tauri WebView engines: https://tauri.app/reference/webview-versions/ — Windows WebView2, macOS WKWebView, Linux WebKitGTK.
2. Tauri 2 capabilities: https://v2.tauri.app/security/capabilities/ — webview permissions must be narrowly configured. Glint also needs its own per-theme permission boundary.
3. Rust `wry` documentation: https://docs.rs/wry/latest/wry/ — embedded webviews and Linux GTK usage; **Wayland requires GTK-specific path**, ordinary child windows may support only X11.
4. `wry` WebViewBuilder: https://docs.rs/wry/latest/wry/struct.WebViewBuilder.html — platform-specific constraints.

## Windows

5. Microsoft Application Desktop Toolbars: https://learn.microsoft.com/en-us/windows/win32/shell/application-desktop-toolbars — custom appbars reserve desktop space; not Explorer taskbar replacement.
6. SHAppBarMessage: https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shappbarmessage — appbar registration/position/state.
7. IDesktopWallpaper: https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-idesktopwallpaper — get/set wallpaper per monitor and slideshow.

## Linux / Wayland / DEs

8. Hyprland IPC: https://wiki.hypr.land/ipc/ — control and live events UNIX sockets; ensure synchronous command socket connections are short-lived.
9. wlr-layer-shell protocol: https://wayland.app/protocols/wlr-layer-shell-unstable-v1 — desktop-layer role, anchors, exclusive zones and input semantics. **Compositor support varies**.
10. KDE Plasma development: https://develop.kde.org/docs/plasma/ — Plasma customization and global theme mechanisms.
11. KDE Global Theme user docs: https://docs.kde.org/stable_kf6/en/plasma-workspace/kcontrol/lookandfeel/ — scope of theme elements; verify current version when coding.
12. GNOME Shell Extensions: https://extensions.gnome.org/about/ — modifies GNOME Shell UI; extensions are platform/version specific.
13. Quickshell: https://quickshell.org/about/ — QtQuick/QML programmable shell toolkit with several system integrations.
14. Caelestia Shell: https://github.com/caelestia-dots/shell — polished Hyprland shell built around Quickshell; visual and interaction inspiration.
15. Wayle: https://wayle.app/guide/getting-started — Rust GTK4/Relm4 shell requiring supporting layer-shell compositor; useful native Rust comparison.

## macOS

16. Apple NSStatusBar: https://developer.apple.com/documentation/appkit/nsstatusbar — app-controlled menu bar items, not replacement of system menu bar.
17. Apple NSWorkspace wallpaper: https://developer.apple.com/documentation/appkit/nsworkspace/setdesktopimageurl%28_%3Afor%3Aoptions%3A%29 — wallpaper per NSScreen.

## Git / distribution

18. Git partial clone docs: https://git-scm.com/docs/partial-clone.html — avoid downloading every blob.
19. Git sparse checkout docs: https://git-scm.com/docs/git-sparse-checkout.html — materialize selected directories.

## Interpretation cautions

- A listed API's existence proves an integration *can be investigated*, not that it will work with Glint's chosen renderer or all OS versions.
- Official docs often omit compositor-specific quirks, sandbox rights and distribution restrictions; add physical-device tests.
- “System tray” and “notifications” mean different things under Windows, KDE, GNOME and macOS; do not present one generic promise.
- Source links were inspected during planning on 2026-11-23; ongoing maintenance must revisit them.
