# Engineering risks, alternatives and explicit stop conditions

## Risk register

| Risk | Severity | Why it matters | Mitigation / decision gate |
|---|---|---|---|
| Linux layer-shell with embedded webview | Critical | GTK/WebKit event loops + layer surface semantics may conflict | Minimal spike before runtime selection; consider alternate native renderer |
| OS-native shell modification expectation | Critical | Users may expect Control Panel/Explorer/macOS UI restyled | Product copy separates owned surfaces from supported system integrations |
| Running untrusted JS with native bridge | Critical | Theme can steal data/execute actions if IPC too broad | Separate host origins, least-privilege bridge, per-package grants, malicious tests |
| Windows Explorer updates | High | Unofficial taskbar patching breaks easily | Do not use as P0 dependency; use documented appbars |
| GNOME/KDE variability | High | Each DE has unique lifecycle and permissions | Versioned adapters and test matrix; never advertise “all Linux” indiscriminately |
| Multi-monitor + fractional DPI | High | Wrong geometry leads to invisible/overlapping surfaces | Native display integration, visual regression, restore hotkey |
| Package supply chain | High | Malicious `.glint`/Git package | Strict extract/schema validation, signature roadmap, no scripts |
| Renderer memory/CPU footprint | High | Background desktop app must be efficient | Share providers and grouped webviews; monitor budgets |
| macOS permissions/signing | High | AppKit APIs and distribution restrictions | Standalone signed distribution research; early Mac spike |
| Inconsistent visual engines | Medium | WebView2/WebKitGTK/WKWebView differ | Define supported browser baseline; CI visual snapshots |
| Theme dependency churn | Medium | Version mismatches break UI | Pin locks, immutable packages and explicit SDK semver |
| Registry size/network outages | Medium | Massive clone unacceptable | Index-first, cache, partial/sparse clone or prebuilt artifacts |
| Licensing of assets/fonts | Medium | Rights/trade dress can block publishing | License metadata and official-library review |
| Incorrect system toggles | Medium | UI says “on” while native action failed | Await real acknowledgement; status/error UI |

## Stop conditions for any deep integration

Stop and redesign if it requires invasive kernel/root modifications, disabling OS security protections, injecting code into unrelated apps, breaking platform update paths, installing unreviewed native theme binaries, or using credentials collected by a Glint-themed login impersonator.

## Major architecture alternatives

**Pure native rendering:** GTK4/Relm4 on Linux, WinUI/Win32 on Windows, AppKit on macOS. Efficient/native but requires multiple UI code paths, weakening portable custom themes.

**Embedded webview renderer:** one React theme authoring language and compatible packaging; easier creator tools but memory/runtime and native layering complexity. **Recommended to prototype**, not blindly assume success on Linux Wayland.

**QtQuick/QML cross-platform:** attractive for shell-specific QML, Qt performance and Quickshell inspiration; would make React/TS theme API harder or require dual runtimes. Retain as fallback if webview shell spike fails.

**Custom Rust GPU renderer (wgpu):** maximal control and potential high performance but vastly more effort for text, browser-like widgets, accessibility and React compatibility. Not a pragmatic first phase.

**One process vs daemon+shell:** fewer binaries simplifies startup but combines trust/runtime failure domains. Separate daemon and untrusted renderer is safer.

## Open design choices requiring evidence

1. Which **Linux host/renderer** works on actual Hyprland+Wayland with transparency and input regions?
2. Do we support custom Linux GTK/Qt themes as first-class integration or compatibility plugin?
3. How to model source identity if two Git authors publish the same `id`?
4. Which permissions/capability names will be frozen for SDK v1?
5. Which macOS version and signing path can deliver overlay behavior reliably?
6. What counts as a platform-supported feature versus preview-only fallback?
7. Whether app surface composition may load UI from different publisher trust groups in one renderer process.
8. Whether Glint inherits users' independent installed apps themes or manages separate copies.

## Delivery risk rule

Never put a risky native-shell hack on the critical path for package portability or theme creation. The whole design should remain valuable if an OS can only support a Glint launcher, widget overlay, wallpaper and management app.
