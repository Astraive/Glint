# Theme renderer, persistence, profiles and rollback

## Renderer architecture

Recommended v1: programmable theme author compiles a static React/TS application into per-surface HTML/CSS/JS entrypoints. Glint loads the compiled content from an internal trusted asset origin into **isolated, restricted webview hosts**. OS window/layer creation remains native.

- Windows: native Win32 windows/appbars and WebView2.
- macOS: AppKit NSWindow/panels and WKWebView.
- Linux: GTK/WebKitGTK embedding on a compositor-compatible host if feasible; **must prototype GTK layer-shell + WebKitGTK/`wry` event-loop compatibility** before treating as solved. Native or alternate renderer backends may be needed.

Tauri 2 is recommended for the management app, not automatically every shell surface. The `wry` library supports embedded webviews but Wayland requires a GTK container approach; a plain child-window setup may be X11-only. See research links.

## Renderer isolation and asset origin

- Each untrusted theme gets an origin/scheme with no direct access to local OS files, app-private stores or arbitrary remote sites.
- Theme static files are copied into immutable content-addressed package store and served only through a path-normalized Glint resource handler.
- CSP blocks unintended remote script, cross-origin data exfiltration, insecure mixed content, and `eval` wherever possible.
- If WebView's security model cannot enforce the desired origin/IPC isolation, migrate untrusted JS to a sandboxed subprocess and narrow data bridge; do not compromise permissions to retain rendering choice.
- Reuse renderers per panel/surface group rather than spawning separate webviews for each widget.

## State stores

| Store | Examples | Update model |
|---|---|---|
| Shipped defaults | Official registry URL, safe default theme | Read-only app resources |
| User configuration | Registries, auto-start, theme update policy | Config file overrides defaults |
| SQLite DB | Package index, grants, profile metadata, install locks, migrations | Daemon only |
| Package files | Immutable installed `.glint` contents | Atomic install/remove |
| User assets | Custom wallpaper, screenshots, imports | Explicit file locations |
| Cache | Git clones, index thumbnails, downloaded artifacts | Evictable |
| Runtime state | Provider subscriptions, surface handles, live geometry | In memory, reconstructable |

## Suggested configuration locations

- Linux: `$XDG_CONFIG_HOME/glint/config.toml`, `$XDG_DATA_HOME/glint/`, `$XDG_CACHE_HOME/glint/` (with XDG defaults).
- Windows: `%APPDATA%\Glint\config.toml`, `%LOCALAPPDATA%\Glint\data` and `cache`.
- macOS: `~/Library/Application Support/Glint/config.toml`, appropriate `Caches/Glint`.

These should be resolved with a tested platform-aware path library, not hardcoded username paths.

## Profile resolution / composition

Snapshot includes base theme, surface overrides, widget instances and positions, package version locks, environment capability digest, optional native integrations, user token overrides, monitor mappings and authorization references. Profile sync through files is possible but **no account/cloud is required**.

Precedence (recommended): built-in default -> selected base theme -> surface-specific theme -> user token/layout overrides -> per-monitor overrides -> runtime accessibility restrictions. Never import executable source from a theme simply because an override names it.

## Apply transaction

1. Validate all installed packages and availability of required capabilities.
2. Create a **desired-state plan**, including native mutations; ask approval for changed OS preferences.
3. Record current Glint state and every native preference the adapter plans to alter.
4. Stage surfaces and resources; do not tear down all known-good UI first.
5. Apply native changes with action-specific confirmation/rollback callbacks.
6. Commit new active profile only if mandatory steps succeed; otherwise revert affected settings best-effort and re-open old surfaces.
7. Persist transaction journal and expose recovery on next startup after crash.

Not all OS preference changes are atomic or fully reversible. Mark adapters with explicit `canSnapshot/canRestore` semantics and present risks.

## Reliability targets (proposed to validate)

- No blank OS desktop when a theme crashes: Glint crashes leave the OS compositor/Explorer/Dock running.
- Cold launch and theme activation performance budgets to be measured after spikes (not arbitrary hard promises).
- Bound memory/CPU of theme renderers; suspend hidden surfaces; lazy-load gallery images.
- Single source of truth in daemon; UI reconnect restores subscription snapshots.
- Always preserve a recovery shortcut and CLI `glint safe-mode`/`glint theme use default`.
