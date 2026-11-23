# Spike: Windows appbar + WebView2 host (SPK-001)

Status: **code spike complete; visual verification pending (SPK-002)**.
Result: **pass** for everything a process can observe about itself on one
Windows 11 machine. Nothing here was checked by looking at the screen.

This answers [Spike 2: Windows appbar + WebView2](../../plan/08-research/feasibility-spikes.md#spike-2-windows-appbar--webview2)
and the [surface-host behavior](../../plan/03-platforms/windows-architecture.md#surface-host-behavior)
list for the first time in code. The spike is throwaway: it lives in
[`spikes/windows-appbar`](../../../spikes/windows-appbar/src/main.rs), is never shipped and
is never imported by product code.

## Environment

| Item | Value |
|---|---|
| OS | Windows 11 Home Single Language, version 10.0.26300 (build 26300) |
| `explorer.exe` | 10.0.26100.9818 |
| Display | 1 monitor, 1920x1080 physical, 120 DPI (125% scale); Explorer taskbar at the bottom |
| WebView2 Runtime | Evergreen 157.0.4341.60 (system install, detected at start) |
| Rust | 1.95.0 (pinned), edition 2024, MSVC target |
| Code revision | `6b924e8` on the SPK-001 branch (built from `41926c5`) |

## What it does

- **Appbar.** A `WS_POPUP` window (`WS_EX_TOOLWINDOW | WS_EX_TOPMOST`, so not in
  the taskbar or Alt+Tab) registers with `SHAppBarMessage(ABM_NEW)`, then
  follows the documented `ABM_QUERYPOS` -> adjust -> `ABM_SETPOS` -> move
  sequence for the top edge of the primary monitor. The bar is 40 DIP tall,
  scaled by `GetDpiForMonitor` (process is per-monitor-v2 aware). It handles
  `ABN_POSCHANGED` (re-position), `ABN_FULLSCREENAPP` (drop to `HWND_BOTTOM`,
  close the popup, restore `HWND_TOPMOST` afterwards), `WM_ACTIVATE` /
  `WM_WINDOWPOSCHANGED` (forwarded as `ABM_ACTIVATE` / `ABM_WINDOWPOSCHANGED`),
  `WM_DPICHANGED` / `WM_DISPLAYCHANGE` (re-position) and the registered
  `TaskbarCreated` message (re-register after an Explorer restart).
- **Always unregister.** `ABM_REMOVE` is idempotent (an atomic flag) and is sent
  from `WM_DESTROY`, `WM_ENDSESSION`, a console control handler (Ctrl+C,
  Ctrl+Break, console close, logoff, shutdown), a panic hook, a drop guard on
  every error path, and once more after the message loop ends.
- **WebView2.** One environment (user data in
  `%TEMP%\glint-spike-windows-appbar\webview2-data`) and two controllers: the
  panel is a child of the appbar HWND, the control center is a child of a
  hidden owned popup window.
- **Bundled content.** Five files (`panel.html/js`, `control-center.html/js`,
  `spike.css`) are embedded with `include_str!` and served **from memory** on
  `https://glint.spike/` by a `WebResourceRequested` handler registered for
  `*`. Nothing is written to disk and no request reaches the network: unknown
  paths on the origin get 404, any other URL gets 403.
- **Popup toggle.** The panel's button posts `toggle-control-center` through
  `window.chrome.webview.postMessage`; the host checks the message `Source` is
  the bundled origin, parses it against a fixed list and shows or hides the
  popup, then posts `control-center:open|closed` back so the button state
  follows. The control center closes itself with a button or Escape.

### Policy applied to both webviews

| Control | How |
|---|---|
| Only the bundled origin | `NavigationStarting` and `FrameNavigationStarting` cancel any URL not starting with `https://glint.spike/` |
| No new windows | `NewWindowRequested` is marked handled without a window |
| No remote script/style/connect | CSP sent as a response header and repeated in a `<meta>` tag: `default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'` (+ `frame-ancestors 'none'` in the header). No inline script |
| No network fallback | every request is answered by the host (200 from memory, 404, or 403) |
| Permissions | `PermissionRequested` always denies |
| Browser chrome | context menus, status bar, zoom, host objects, script dialogs, built-in error page, autofill and password save off; DevTools and browser accelerator keys on **only in debug builds** |
| Host messages | only from the bundled origin, only fixed strings; everything else is counted and dropped |

## Crates and versions

| Crate | Version | Why |
|---|---|---|
| `windows` | 0.62.2 | Microsoft's Win32 bindings (appbar, windows, DPI, console, process memory) |
| `webview2-com` | 0.39.1 | WebView2 COM bindings and callback helpers; it depends on `windows` 0.62, so the spike uses the same minor to share types |
| `webview2-com-sys` | 0.39.1 (transitive) | Links Microsoft's `WebView2LoaderStatic.lib` on MSVC, so no `WebView2Loader.dll` ships next to the binary |

`wry` was not used. The spike needs the webview to be a child of an HWND
whose window procedure we own (appbar callbacks, `TaskbarCreated`, DPI
messages), and full access to `WebResourceRequested`, `NewWindowRequested`
and `PermissionRequested`. `webview2-com` exposes exactly the COM surface
with no event loop of its own. `wry` remains a candidate if the product wants
one webview layer across Windows, Linux and macOS; that is a WIN-*/RND-*
decision, not settled here.

Both dependencies are under `[target.'cfg(windows)'.dependencies]`. On other
hosts the binary compiles to a stub that prints `windows-only spike:
unsupported on this platform` and exits 2 (checked locally with
`cargo clippy --target x86_64-linux-android`; real Linux and macOS results come
from CI).

## How to run

```powershell
# interactive: stays docked until the panel's "Quit spike", Ctrl+C or WM_CLOSE
cargo run -p spike-windows-appbar
# self-test: probes, prints observations, unregisters, exits 0 on pass, 1 on fail
cargo run -p spike-windows-appbar --release -- --smoke 9
```

Exit codes: 0 success, 1 failure (or a failed smoke check), 2 bad arguments or
unsupported platform, 3 WebView2 Runtime unavailable.

## Evidence

All numbers come from `--smoke` output and from a separate PowerShell process
reading `SystemParametersInfo(SPI_GETWORKAREA)` with DPI awareness on.

### Smoke run (debug build, `--smoke 9`)

```text
smoke.webview2_runtime = 157.0.4341.60
smoke.dpi_awareness = per-monitor-v2
smoke.monitor_dpi = 120
smoke.window_dpi = 120
smoke.primary_monitor_rect = (0,0)-(1920,1080) 1920x1080
smoke.registered_rect = (0,0)-(1920,50) 1920x50
smoke.window_rect = (0,0)-(1920,50) 1920x50
smoke.work_area_before = (0,0)-(1920,1020) 1920x1020
smoke.work_area_during = (0,50)-(1920,1020) 1920x970
smoke.work_area_after_remove = (0,0)-(1920,1020) 1920x1020
smoke.navigation_completed.panel = 2267ms success=true web_error_status=0
smoke.navigation_completed.control-center = 2287ms success=true web_error_status=0
smoke.served_requests = 6
smoke.csp_violations = panel script-src-elem https://example.com/glint-spike-remote.js | (same, reported twice)
smoke.blocked_navigations = panel top-level https://example.com/glint-spike-navigation
smoke.blocked_new_windows = panel https://example.com/glint-spike-popup
smoke.blocked_requests = panel https://glint.spike/favicon.ico | panel https://example.com/glint-spike-navigation
smoke.popup = opened=1 closed=1 visible_after_open=true
smoke.shell_events = ABN_POSCHANGED=2 ABN_FULLSCREENAPP=0 TaskbarCreated=0
smoke.result = PASS   (10 of 10 checks)
```

The 40 DIP bar is 50 px at 120 DPI, as expected. The bottom of the work area
stays at 1020 (the Explorer taskbar is untouched); only its top moves from 0
to 50 while the spike is registered and back to 0 after `ABM_REMOVE`.

### Startup and memory (release build, three `--smoke 6` runs)

| Run | `ABM_NEW` done | Environment ready | Panel `NavigationCompleted` | Control center `NavigationCompleted` |
|---|---|---|---|---|
| 1 | 373 ms | 400 ms | 1351 ms | 1441 ms |
| 2 | 361 ms | 374 ms | 2016 ms | 2389 ms |
| 3 | 202 ms | 233 ms | 1097 ms | 1121 ms |

Times are from process start. Interactive runs measured 0.78-0.95 s to the
first `NavigationCompleted`. Most of the time is controller creation (browser
and renderer process start), not the appbar.

Memory at the end of a debug run, with both webviews loaded:

| Process | Working set | Private |
|---|---|---|
| host (`spike-windows-appbar.exe`) | 23.1 MiB | 3.6 MiB |
| WebView2 browser | 125.9 MiB | 44.1 MiB |
| WebView2 GPU | 69.6 MiB | 68.9 MiB |
| WebView2 renderers (panel, control center) | 56.9 + 51.7 MiB | 24.3 + 20.8 MiB |
| WebView2 utilities (2) | 41.1 + 18.9 MiB | 13.2 + 7.7 MiB |
| **Total** | **387.2 MiB** | **182.5 MiB** |

Release runs measured 376-388 MiB working set and 174-177 MiB private.
Working sets double-count shared pages; private bytes are the better budget
signal. Two trivial pages already cost about 175 MiB private, mostly fixed
WebView2 overhead (browser, GPU, utilities). That is the baseline the PERF-*
budget has to start from.

### Termination paths

| Path | Observed |
|---|---|
| `--smoke` self-exit | work area restored right after `ABM_REMOVE` (sampled 750 ms later) |
| `WM_CLOSE` posted from another process | exit code 0, `WM_DESTROY: appbar unregistered (ABM_REMOVE)` logged, work area back to `0,0,1920,1020` at +0 ms |
| `Stop-Process -Force` (TerminateProcess, no cleanup can run) | work area back to `0,0,1920,1020` by the first sample (+0 ms); no `msedgewebview2.exe` with the spike's data folder left after 3 s |
| WebView2 Runtime missing (simulated with `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` pointing to an empty folder) | `error: unavailable: Microsoft Edge WebView2 Runtime: no installed runtime found (... 0x80070002); install the Evergreen runtime from https://developer.microsoft.com/microsoft-edge/webview2/`, exit 3, no window created and no appbar registered |

The forced-kill result means this Explorer build reclaims the space of an
appbar whose window was destroyed. That is observed behaviour on one build,
not a documented guarantee, so the product must still unregister explicitly.

## Findings

1. **The appbar API does what the plan needs.** Registration, work-area
   reservation and restoration all behave as documented, and the Explorer
   taskbar is left alone. The spike never calls anything that hides, moves or
   restyles the taskbar.
2. **In-memory serving works for a made-up https origin.** A `*`
   `WebResourceRequested` filter answered every request for
   `https://glint.spike/`, including the top-level document, without disk
   files or `SetVirtualHostNameToFolderMapping`. The product can serve verified
   theme packages straight from memory.
3. **Navigation cancel is not the only line of defence.** After
   `NavigationStarting` cancelled `https://example.com/glint-spike-navigation`,
   the resource handler still saw a request for that URL about 5 ms later and
   answered 403. The navigation then completed with `success=false
   web_error_status=14` (operation cancelled). The product should keep both
   layers: cancel the navigation and refuse the request.
4. **CSP violations are reported once per policy.** The header and the
   `<meta>` tag both enforce, so one blocked script fires two
   `securitypolicyviolation` events. Pick one source of truth (the header) in
   the product, or dedupe.
5. **`window.open` from script reaches `NewWindowRequested`** even without a
   user gesture, so no popup blocker stands in front of the handler; the host
   must always handle it.
6. **`ABN_POSCHANGED` arrived twice** in the smoke run with no other appbar or
   taskbar change made on purpose. The re-position code is re-entrancy guarded
   and idempotent, which is what the product needs; why the shell sent them
   was not investigated.
7. **Single UI thread is enough.** Both controllers, all callbacks and the
   appbar ran on one STA thread. No borrow is held across calls that pump
   messages.

## Limitations (not verified)

- **Visual result.** Nothing in this report looked at pixels: whether the
  panel renders, whether text is crisp at 125%, whether the popup appears in
  the right place, and how the bar looks next to the taskbar are unverified.
- **Multi-monitor.** Only one monitor was attached. The spike always docks on
  the primary monitor; per-monitor bars, mixed DPI, hot-plug and monitor
  rotation are untested.
- **Auto-hide.** Neither an auto-hide Explorer taskbar nor an auto-hide Glint
  bar (`ABM_SETAUTOHIDEBAREX`) was tried.
- **Fullscreen apps.** The `ABN_FULLSCREENAPP` handler exists but was never
  triggered (count 0). Games, video players and borderless-fullscreen apps
  need a human test.
- **Explorer restart.** The `TaskbarCreated` re-registration is implemented but
  was not exercised; restarting Explorer was out of bounds for an unattended
  run.
- **Z-order.** The bar is `HWND_TOPMOST` and the popup is a topmost owned
  window. Interaction with other topmost windows, Start, Search, Snap layouts,
  Widgets and notification toasts is unknown.
- **Focus.** Showing the popup calls `SetForegroundWindow`; focus theft rules
  and keyboard behaviour are not checked.
- **Ctrl+C / console close / logoff / shutdown / panic.** The handlers are
  present and use the same idempotent `ABM_REMOVE`, but only `WM_CLOSE`, the
  smoke exit, a forced kill and a missing runtime were actually run.
- **Sleep/resume, user switching, Windows 10, ARM64, high-contrast,
  accessibility tools** were not tested.
- The WebView2 user data folder is a fixed folder under `%TEMP%`; that is
  fine for a spike and wrong for the product.

## For SPK-002 (human, real hardware)

Use `cargo run -p spike-windows-appbar` and record screenshots or a short
video for each item:

1. A 50 px (at 125%) dark bar appears at the top of the primary monitor, and
   maximised windows stop below it. The Explorer taskbar is unchanged.
2. The bar shows "Glint spike", a clock and two buttons, sharp at the current
   scale. Right-click shows no context menu; F12 and Ctrl+Shift+I do nothing in
   a `--release` build.
3. "Control center" opens a 360x440 DIP popup under the right end of the bar;
   clicking again, its Close button or Escape hides it. The button highlights
   while it is open.
4. "Quit spike" and Ctrl+C in the console both remove the bar, and maximised
   windows grow back to the top.
5. Restart Explorer (`taskkill /f /im explorer.exe`, then start it again):
   the bar should come back docked (`TaskbarCreated`).
6. Start a fullscreen video or game: the bar should drop behind it and return
   afterwards (`ABN_FULLSCREENAPP`).
7. With a second monitor at a different scale: the bar stays on the primary
   monitor with the right height; changing the primary's scale re-sizes it.
8. Turn on taskbar auto-hide and move the taskbar if the OS allows: note what
   happens to the bar and the work area.

## Recommendations for WIN-* tickets

- **Keep `windows` + `webview2-com`** for the Windows host, and pin
  `webview2-com` with the `windows` minor it depends on; upgrade them together.
- **Appbar module (`glint-windows::appbar`).** Reuse the pattern here: one
  idempotent unregister reachable from destroy, console control, panic hook,
  session end and drop; always run `QUERYPOS` -> `SETPOS` -> move together;
  re-register on `TaskbarCreated`; guard re-position against re-entrancy;
  treat `ABN_FULLSCREENAPP` as a z-order change. Add a startup check that
  removes a stale registration left by a crashed previous instance.
- **Serve theme content from memory** through `WebResourceRequested` on a
  fixed, non-resolvable https origin per theme, with the CSP as a response
  header and a `*` filter that refuses everything else. Keep
  `NavigationStarting` / `FrameNavigationStarting` / `NewWindowRequested` /
  `PermissionRequested` handlers as a second layer.
- **Typed runtime detection.** Check `GetAvailableCoreWebView2BrowserVersionString`
  before creating any window and map a missing runtime to `unavailable` with
  install guidance, as here. Decide in a WIN-* ticket whether the installer
  bootstraps the Evergreen runtime.
- **Host messages.** Check the message source origin, accept only a typed,
  versioned message set, and never expose host objects. This becomes the
  bridge contract for RND-*.
- **Budget.** Expect about 175 MiB private memory for the first two surfaces
  before any theme code; share one environment and browser process across all
  surfaces, and measure each extra renderer in PERF-*.
- **Per-surface user data.** Put the WebView2 profile under the Glint per-user
  data directory, not `%TEMP%`, and decide whether themes share one profile.
