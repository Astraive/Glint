# Research spikes before committing to a renderer or compatibility claims

These experiments are *deliberately small throwaways*. They answer open architectural questions without building the whole app.

## Spike 1: Linux native surface hosting (highest priority)

**Question:** Can a Rust app host a local WebKitGTK/wry rendered React panel on a Wayland `wlr-layer-shell` surface with correct compositing, keyboard focus, input transparency and CPU usage?

**Experiment:** Arch Linux/Hyprland; 44px top panel, transparent rounded region; embedded JS showing a clock/workspace list; no privileged theme operations. Test on scale 1.0, 1.25 and dual monitors.

**Measurements:** stable event loop, frame pacing, memory, focus, click-through, exclusive zone reservation, app fullscreen behavior, repaint under compositing, clean exit.

**Pass condition:** The host is sufficiently robust for a real panel with a documented setup and no compositor crash. **Fail path:** prototype a GTK4 native shell renderer with a portable theme DOM alternative, or test Qt/QML renderer contract; do not promise React webviews on layer-shell until proven.

## Spike 2: Windows appbar + WebView2

Create one Win32 appbar registered with `SHAppBarMessage`, with a WebView2 child showing bundled HTML and a toggleable control-center window. Test DPI scale and Explorer restart, panel coexistence with taskbar, multiple monitors, fullscreen behavior, restore on crash.

## Spike 3: macOS overlay + WKWebView

Create simple AppKit NSWindow host with WKWebView and wallpaper setter; verify desktop layering, Spaces, focus, external monitors, status menu extras, app sandbox/signing implications. Document what is fundamentally unavailable.

## Spike 4: package security

Build `.glint` ZIP parser/validator against malicious fixtures: traversal, symlinks, giant compression ratios, duplicates, path case collisions, remote JS dependencies, unknown capabilities and permissions. Prove untrusted UI cannot reach direct privileged commands.

## Spike 5: portable theme rendering consistency

Build same `panel` React code for Windows WebView2, Linux WebKitGTK and macOS WKWebView; compare fonts/layout, translucency, CSS effects, animation, images, accessibility and memory. Identify lowest feasible browser feature baseline and fallback CSS.

## Spike 6: native provider adapters

Audio/network/battery/workspaces on at least one environment per OS. Return precise `supported`, `read-only`, `permission-required`, `unavailable` statuses. Simulate provider restart and stale subscriptions.

## Spike 7: registry performance

Construct synthetic official registry with 1k/10k theme metadata entries and large image assets, compare index-only search, sparse clone, Git archive and `.glint` artifact fetch. Count bytes/time for first list, first install and update; choose based on evidence.

## Decision outputs

Each spike writes `docs/research/spikes/<name>.md` with environment, exact library versions, code revision, screenshots/log snippets, measured results, pass/fail, blocker and recommended architecture decision. No one should merge a native host abstraction based solely on an API doc.
