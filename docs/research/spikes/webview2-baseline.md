# Spike: WebView2 rendering baseline for portable theme CSS (SPK-008)

Status: **code spike complete on Windows; WebKitGTK and WKWebView rows are UNVERIFIED until SPK-009**.
Result: **pass** for the WebView2 measurement. One React panel bundle ran in
WebView2 on the Windows 11 host. Every number below comes from that run
through the page's probes, the host's process counters, UI Automation or a
screen capture. Nothing here was measured on Linux or macOS.

This answers the Windows half of [Spike 5: portable theme rendering consistency](../../plan/08-research/feasibility-spikes.md#spike-5-portable-theme-rendering-consistency)
and gives a first answer to the "supported browser language/feature baseline"
that [SDK and React authoring](../../plan/04-themes/sdk-and-react-authoring.md#third-party-dependencies)
asks for. The spike is throwaway: it lives in
[`spikes/webview2-baseline`](../../../spikes/webview2-baseline/src/main.rs), is never shipped and is
never imported by product code. The ticket's proposed paths
(`spikes/rendering-baseline/`, `rendering-consistency.md`) were renamed
because this spike covers one engine only; the cross-engine comparison belongs to
SPK-009.

## Environment

| Item | Value |
|---|---|
| OS | Windows 11 Home Single Language, version 10.0.26300; `explorer.exe` 10.0.26100.9818 |
| Display | 1 monitor, 1920x1080 physical, 120 DPI (125%, `devicePixelRatio` 1.25); rAF cadence measured at ~144 Hz |
| GPU (as WebView2 reports it) | `ANGLE (NVIDIA, NVIDIA GeForce RTX 3050 6GB Laptop GPU Direct3D11 vs_5_0 ps_5_0, D3D11)`; the machine also has Intel UHD Graphics |
| CPU | 20 logical processors (`navigator.hardwareConcurrency`) |
| WebView2 Runtime | Evergreen **157.0.4341.60** (Chromium 157.0.8103.80, UA-CH `platformVersion` 19.0.0) |
| Rust | 1.95.0 (pinned), edition 2024, MSVC target; `webview2-com` 0.39.1, `windows` 0.62.2 |
| Node / pnpm | Node 24.16.0, pnpm 10.18.0 (Corepack) |
| Bundle | React 19.3.1 + react-dom 19.3.1, built by esbuild 0.28.2 (`target: chrome120`, minified), type-checked with TypeScript 5.9.3 |
| Probe images | Pillow 12.2.0 (WebP + AVIF enabled), via `web/scripts/generate-images.py` |
| Code revision | SPK-008 branch built on `b41b7a7`; release build |
| Date | 2026-11-23 |

## What it does

- **One React panel bundle.** [`web/src/main.tsx`](../../../spikes/webview2-baseline/web/src/main.tsx)
  renders a small theme: a 40 px status bar (start button, four workspace
  toggles, a Wi-Fi switch, a clock, a control-center button) over a gradient
  wallpaper, and a control center whose card grid uses a container query.
  The cards use frosted glass, there are 24 animated tiles, a masked gradient,
  the four probe images and a `<picture>` element. Styling is
  [`assets/baseline.css`](../../../spikes/webview2-baseline/assets/baseline.css),
  written the way the proposed baseline below asks authors to write it.
- **Probes in the page** ([`web/src/probes/`](../../../spikes/webview2-baseline/web/src/probes/css.ts)):
  system-font availability from canvas text metrics, layout boxes, 49 CSS
  feature probes, three rAF timing phases, image decode with pixel checks,
  in-page accessibility facts, engine facts and a React re-render timing. The
  page posts `report:<json>` through `window.chrome.webview.postMessage`.
- **Host** ([`src/win.rs`](../../../spikes/webview2-baseline/src/win.rs)): a normal
  top-level window (not an appbar) with `WS_EX_NOREDIRECTIONBITMAP`, the DWM
  transient-window (acrylic) backdrop and the frame extended into the client
  area. The WebView2 controller gets a transparent default background
  (`ICoreWebView2Controller2::SetDefaultBackgroundColor`, alpha 0). The SPK-001
  policy applies: only `https://glint.spike/`, assets served from memory with
  a CSP header, no new windows, every permission denied, no network.
- **`--smoke N`**: after the page report arrives, the host samples the memory of
  every process in the WebView2 tree (`GetProcessInfos` + `K32GetProcessMemoryInfo`).
  It then walks the accessibility tree through **UI Automation** on a separate
  MTA thread, samples memory again, and with `--out DIR` also writes
  `webview.png` (`CapturePreview`), `behind.bmp` (screen pixels before the window
  is shown) and `screen.bmp` (screen pixels under the client area after the
  run). It prints one JSON document to stdout and exits 0. Exit 4 means a
  timeout, 3 means the runtime is unavailable, 1 means failure. The run never
  reports success without a page report.

### How to run

```powershell
# from the repository root (Windows 11 with the Evergreen WebView2 Runtime)
cargo run -p spike-webview2-baseline --release -- --smoke 60 --out <dir> > report.json
cargo run -p spike-webview2-baseline --release            # interactive window
```

Rebuild the committed bundle only after changing `web/src` (the Rust build
embeds `assets/app.js` with `include_bytes!` and never needs Node):

```powershell
cd spikes/webview2-baseline/web
corepack pnpm@10.18.0 install --ignore-workspace --frozen-lockfile
corepack pnpm@10.18.0 --ignore-workspace run typecheck
corepack pnpm@10.18.0 --ignore-workspace run build      # writes ../assets/app.js
python scripts/generate-images.py                         # only to regenerate probe images
```

`web/` is deliberately **outside** the pnpm workspace (it has its own
lockfile), so CI's `pnpm install --frozen-lockfile` and the generated
[third-party inventory](../../legal/third-party-inventory.md) do not include
it. Its dependencies are React, react-dom and scheduler (MIT, bundled into
`app.js` with their `@license` headers kept at the end of the file), plus the
build-only tools esbuild (MIT), TypeScript (Apache-2.0), `@types/react` and
`@types/react-dom` (MIT). Nothing in `web/` ships.

## Evidence

Six smoke runs were made, all exit 0. The committed
[report.json](webview2-baseline/report.json) and the screen capture of the
panel ([screen.png](webview2-baseline/screen.png)) come from the final run of
the committed code. Ranges below cover all six runs. Runs 1 to 5 used earlier
builds of the same probes; the fixes between them were the aspect-ratio
fixture, reading paint entries at the end of the run, and the `--activate`
flag.

### Startup and engine

| Measure | Value |
|---|---|
| `NavigationCompleted` (from process start) | 460-733 ms |
| First paint / first contentful paint (page clock) | 332-408 ms |
| React first commit after the bundle starts | 46-63 ms |
| React: 50 synchronous re-renders of a 200-item list | 8.9-18.1 ms |
| Page report posted (includes ~600 sampled frames) | 5.1-5.7 s |
| JS heap (`performance.memory`) | 2.2 MiB used / 3.2-3.5 MiB total |
| WebGPU exposed (`navigator.gpu`) | yes |
| `performance.measureUserAgentSpecificMemory` | absent (needs cross-origin isolation) |

### Fonts

Availability comes from canvas metrics (48 px test string, compared against three
generic fallbacks). `document.fonts.check()` cannot be used for this because it
returns true for any family that has no `@font-face` rule.

| Family | WebView2 (this host) | Width (px) |
|---|---|---|
| Segoe UI Variable Text / Display / Small | present | 860.18 / 846.77 / 885.84 |
| Segoe UI | present | 867.87 |
| Segoe UI Emoji, Segoe Fluent Icons, Segoe MDL2 Assets | present | 867.87 / 714.28 / 712.45 |
| Cascadia Mono, Cascadia Code, Consolas | present | 787.5 / 787.5 / 738.94 |
| Arial, Calibri | present | 861.38 / 809.58 |
| Cantarell, Adwaita Sans, Noto Sans, DejaVu Sans, SF Pro Text, Helvetica Neue | **absent** | n/a |

Generic families, same string (default serif = 813.28):

| Generic | Width | Reading |
|---|---|---|
| `system-ui` | 867.87 | Resolves to **Segoe UI**, not Segoe UI Variable |
| `sans-serif` / `monospace` | 861.38 / 738.94 | Arial / Consolas |
| `ui-sans-serif`, `ui-serif`, `ui-monospace`, `ui-rounded`, `emoji` | 813.28 | **Not mapped**: they fall back to the default serif face |
| `cursive`, `fantasy`, `math` | 876.3 / 782.84 / 858.38 | Mapped to some installed face |

Sample `Glint 12:34 Wi-Fi` at 13 px: Segoe UI Variable Text 92.85 px,
`system-ui` 94.94 px, Cascadia Mono 129.49 px. At 600 14px, Segoe UI Variable
Display measures 101.8 px. These are the reference widths SPK-009 compares
against.

### Layout (CSS px)

`rem` is 13 px (the page sets `html { font: 13px/1.4 ... }`). The viewport is
520x760, which is 650x950 physical at 125%. The panel box is 520x40 at (0,0). The
brand is 30.31x18.4, the clock 60.94x18.2 at x=413.39, and the control-center
button 31.68x26.2 at x=480.33. The control center is 520 wide, so the
container query switched the card grid to `245px 245px`. A tile is 39.4x39.4
(`aspect-ratio: 1` in an `auto-fill` grid).

### CSS features

`supports` is `CSS.supports()` and `applied` is a computed-style read-back of a
real rule in `baseline.css`. A dash means that check does not exist for the
feature (for example, nesting has no `CSS.supports` form).

| Group | Feature | WebView2 157 | supports / applied | Measured value |
|---|---|---|---|---|
| effects | `backdrop-filter` | supported | yes / yes | `blur(8px)` |
| effects | `-webkit-backdrop-filter` | **unsupported** | no / no | Chromium ignores the prefix |
| effects | `filter: blur()` | supported | yes / yes | `blur(4px)` |
| effects | `box-shadow` (multi, inset) | supported | yes / yes | two layers kept |
| effects | `text-shadow` | supported | yes / yes | |
| effects | `mask-image` | supported | yes / yes | |
| effects | `-webkit-mask-image` | supported | yes / yes | alias of `mask-image` |
| effects | `clip-path: path()` | supported | yes / yes | |
| effects | `mix-blend-mode` | supported | yes / yes | |
| gradients | linear / radial / conic | supported | yes / yes | |
| gradients | interpolation `in oklch` | supported | yes / yes | |
| selectors | `:has()` | supported | yes / yes | |
| selectors | CSS nesting (`&`) | supported | - / yes | |
| selectors | `:is()`, `:focus-visible` | supported | yes / yes, - | |
| cascade | `@layer` | supported | - / yes | |
| cascade | `@property` | supported | - / yes | `42deg` initial value |
| cascade | `@scope` | supported | - / yes | |
| cascade | `@starting-style` | supported | - / yes (API) | |
| container | size queries | supported | yes / yes | |
| container | `cqi` units | supported | yes / yes | `150px` of 300 |
| container | style queries (custom property) | supported | - / yes | |
| color | `color-mix()` | supported | yes / yes | `color(srgb 0.5 0 0.5)` |
| color | `light-dark()` | supported | yes / yes | `rgb(1, 2, 3)` under `color-scheme: light` |
| color | `oklch()` | supported | yes / yes | |
| color | relative colour syntax | supported | yes / yes | |
| color | `color(display-p3 ...)` | supported | yes / yes | display reports `color-gamut: p3` false |
| layout | `subgrid` | supported | yes / yes | |
| layout | `aspect-ratio` | supported | yes / yes | `20px` for 40px at 2/1 |
| layout | `inset`, `dvh`, `translate` | supported | yes / yes | |
| layout | anchor positioning | supported | yes / yes | |
| layout | `content-visibility`, `field-sizing`, `interpolate-size` | supported | yes / yes | |
| text | `text-wrap: balance` | supported | yes / yes | |
| text | `font-variation-settings` | supported | yes / yes | |
| text | `-webkit-line-clamp` | supported | yes / yes | |
| scrolling | `scrollbar-color`, `scrollbar-gutter` | supported | yes / yes, - | |
| animation | scroll-driven animations | supported | yes / yes | |
| animation | Web Animations API, View Transitions API | supported | - / yes | |
| host | `-webkit-app-region` | supported | yes / yes | Chromium only; never portable |
| host | `accent-color`, `color-scheme` | supported | yes / - | |

### Translucency

| Layer | Result |
|---|---|
| In-page glass (`backdrop-filter` over page content) | **Works.** The `@supports` enhancement applied (`blur(20px) saturate(1.4)` on panel and cards, background `rgba(32, 32, 36, 0.72)`), and [screen.png](webview2-baseline/screen.png) shows the wallpaper blurred under the panel and the glass card |
| WebView2 transparent background | **Works.** `SetDefaultBackgroundColor` alpha 0 returned `S_OK`; in the `CapturePreview` PNG the uncovered right third of the page has alpha 28-63 (only the card shadows), so transparent pixels reach the host |
| DWM backdrop behind the webview, **inactive window** | `DWMWA_SYSTEMBACKDROP_TYPE = DWMSBT_TRANSIENTWINDOW` and `DwmExtendFrameIntoClientArea(-1)` both returned `S_OK`. On screen, though, the transparent region is a **flat grey**: the screen reads 63-75 where the webview alpha is 62-28, which works back to an underlying ~84 grey at every sampled point, while the desktop captured there beforehand was 22-41. An inactive window shows DWM's solid fallback, not the desktop |
| DWM backdrop, **active window** | **Not measured.** `--activate` calls `SetForegroundWindow`, which Windows refused because the spike was not started by the foreground process (`foregroundAtReport: false`). Whether the acrylic backdrop shows through WebView2 while the window is active is unverified |

### Animation (rAF frame timing)

Per run: 120 idle frames, then 240 frames while 24 tiles animate transform +
opacity, then 240 frames while a `backdrop-filter` card slides and cards
animate `box-shadow` + `filter`. A dropped frame is an interval longer than
1.5x the median.

| Phase | Mean | p50 | p95 | p99 | Max | Dropped (6 runs) |
|---|---|---|---|---|---|---|
| idle | 6.95 ms | 6.9-7.0 ms | 7.1-7.3 ms | 7.3-8.0 ms | 7.5-8.1 ms | 0, 0, 0, 0, 0, 0 |
| transform | 6.95 ms | 6.9-7.0 ms | 7.1 ms | 7.2-7.5 ms | 7.3-7.7 ms | 0, 0, 0, 0, 0, 0 |
| effects (blur + shadow + filter) | 6.95-7.79 ms | 6.9-7.0 ms | 7.1-7.2 ms | 7.1-27.7 ms | 7.3-97.2 ms | 6, 4, 0, 0, 0, 0 |

No long tasks were observed and the page was `visible` throughout (the
window is topmost during a smoke run, so occlusion never throttles rAF). The
paint-heavy phase held the 144 Hz cadence at p95, but the first two runs
dropped 6 and 4 frames, with a 97.2 ms worst frame. The cause is not
identified (it could be GPU clock ramp-up or a cold shader cache). This is one
fast discrete-GPU laptop, and the numbers are not a performance budget.

### Images

The probe images are repository-owned 16x16 quadrant images (red, green,
white, and blue at alpha 128), generated by
[`generate-images.py`](../../../spikes/webview2-baseline/web/scripts/generate-images.py).
Each one is decoded, drawn to a canvas and sampled at the four quadrant
centres.

| Format | Decoded | Decode time | Pixels match | Note |
|---|---|---|---|---|
| PNG (108 B) | yes | 4.6-5.4 ms | exact | decoded first, so the time includes decoder warm-up |
| SVG (398 B) | yes | 0-0.1 ms | exact | alpha via `fill-opacity` |
| WebP lossless (46 B) | yes | 0.6-1.3 ms | exact | |
| AVIF 4:4:4 q100 (491 B) | yes | 0-0.3 ms | within 2/255 | `<picture>` picked `probe.avif` first |

`createImageBitmap` is available. Canvas encoding works for PNG, WebP and JPEG
but **not AVIF** (`toDataURL('image/avif')` falls back to PNG).

### Accessibility

UI Automation (`IUIAutomation::FindAll`, descendants of the host HWND) found
all 8 expected names on the first attempt in 67-100 ms (the very first run
needed a second attempt, 631 ms in total, while Chromium built its tree). It
saw 39 elements in total.

| Name | Authored | UIA control type | Localized type | `AriaRole` |
|---|---|---|---|---|
| Start | `button` | Button (50000) | button | button |
| Workspace 1 | `button` + `aria-pressed` | Button (50000) | toggle button | button |
| Wi-Fi | `role="switch"` | Button (50000) | switch | switch |
| Volume | `input type=range` | Slider (50015) | slider | slider |
| Battery | `role="progressbar"` | ProgressBar (50012) | progress indicator | progressbar |
| Open control center | `button` | Button (50000) | button | button |
| Glint probe panel | `role="toolbar"` | ToolBar (50021) | tool bar | toolbar |
| Quick settings | `section role="region"` | Group (50026) | region | region |

The headings, the `<time>` element and every `<img alt>` (control type Image)
also appeared with their names. In the page, `Element.computedRole` and
`computedName` are **not exposed**, but ARIA reflection, `ElementInternals.role`,
`inert`, `popover` and `<dialog>` all exist. There are 8 tabbable elements.
User preferences as reported: `prefers-color-scheme: dark` true;
reduced-motion, more contrast, forced colors and reduced transparency all
false; `dynamic-range: high` false; `color-gamut: p3` false. The
`prefers-reduced-transparency` query is recognised. This run checks names and
roles only. Keyboard traversal and screen-reader output were not run; they
belong to A11Y.

### Memory

Working set (WS) and private bytes per process, sampled after UI Automation
(range over runs 2-6; run 1 was in range except UIA needed two attempts):

| Process | Working set | Private |
|---|---|---|
| Spike host | 24.6-25.4 MiB | 4.0-4.2 MiB |
| WebView2 browser | 122.3-128.9 MiB | 37.9-41.5 MiB |
| WebView2 renderer (the page) | 105.0-110.7 MiB | 55.1-59.0 MiB |
| WebView2 GPU | 107.0-115.8 MiB | 278.1-303.7 MiB |
| WebView2 utility x2 | ~20 + ~37-41 MiB | ~7.7 + ~12.3-12.8 MiB |
| **Total** | **415.7-440.5 MiB** | **399.3-424.4 MiB** |

Turning on UI Automation added 13-19 MiB WS and 5-8 MiB private across the
tree. Working sets double-count shared pages. The GPU process's private
commit includes driver allocations and dominates the total. The page's own
cost is the renderer, about 56 MiB private. One webview costs about 400 MiB of
private commit on this machine, so a theme with several surfaces must share
one environment and browser process (see recommendations).

## Proposed portable baseline (lowest common feature set)

> **UNVERIFIED for WebKitGTK and WKWebView.** Only the WebView2 column is
> measured. The tier assignments below are a hypothesis for SPK-009 to
> confirm or move on real WebKitGTK and WKWebView hosts. No WebKit row may be
> cited as supported until SPK-009 records it.

| Tier | Features | WebView2 157 | WebKitGTK | WKWebView |
|---|---|---|---|---|
| **0: rely on** | Flexbox, grid, `gap`, custom properties, `calc()`, linear/radial/conic gradients, `box-shadow`, `text-shadow`, `filter`, transforms + opacity transitions and keyframes, `aspect-ratio`, `inset`, `:is()`/`:where()`, `:focus-visible`, `prefers-color-scheme` / `prefers-reduced-motion` media, PNG/SVG/WebP images, system-font stacks ending in a CSS generic, ES2022 JS | measured: supported | UNVERIFIED (SPK-009) | UNVERIFIED (SPK-009) |
| **1: use with a fallback** | `backdrop-filter` (always with `-webkit-backdrop-filter` and a solid background), `mask-image` (with `-webkit-mask-image`), container size queries and `cq*` units, `:has()`, CSS nesting (prefer build-time flattening), `color-mix()`, `oklch()`, `light-dark()`, `@layer`, `@property`, `subgrid`, `dvh`, AVIF images (via `<picture>` with a PNG/WebP fallback), `text-wrap: balance`, `scrollbar-gutter`, `prefers-reduced-transparency` | measured: supported | UNVERIFIED (SPK-009) | UNVERIFIED (SPK-009) |
| **2: outside the baseline** | Container style queries, `@scope`, `@starting-style`, anchor positioning, scroll-driven animations, View Transitions, relative colour syntax, `field-sizing`, `interpolate-size`, `scrollbar-color`, `color(display-p3)` as the only value, `ui-*` generic font families, `-webkit-app-region` | measured: supported (`ui-*` fall back to serif) | UNVERIFIED (SPK-009) | UNVERIFIED (SPK-009) |

Tier 2 still works in WebView2, but a theme that depends on it is not
portable until SPK-009 says otherwise, so the SDK should warn about it.

### Fallback CSS pattern

Write the plain value first, put the enhancement after it or inside
`@supports`, and end every font stack with a real CSS generic. This is the
pattern `baseline.css` uses:

```css
:root {
  /* ui-* generics are unmapped in WebView2 (they render as serif): never end a stack with them. */
  --font-ui: "Segoe UI Variable Text", "Segoe UI", system-ui, -apple-system, "Cantarell", "Adwaita Sans", sans-serif;
  --font-mono: "Cascadia Mono", Consolas, "SF Mono", ui-monospace, monospace;
  --surface: rgba(32, 32, 36, 0.72);
  --surface-solid: rgb(32, 32, 36);
  --accent: rgb(76, 140, 255);
  --accent-soft: rgba(76, 140, 255, 0.25);          /* plain sRGB fallback */
}

@supports (color: color-mix(in srgb, red 50%, blue)) {
  :root { --accent-soft: color-mix(in srgb, var(--accent) 25%, transparent); }
}

.panel { background: var(--surface-solid); }      /* readable everywhere */

@supports (backdrop-filter: blur(1px)) or (-webkit-backdrop-filter: blur(1px)) {
  .panel {
    background: var(--surface);
    -webkit-backdrop-filter: blur(20px) saturate(1.4); /* WebKit; ignored by Chromium */
    backdrop-filter: blur(20px) saturate(1.4);
  }
}

@media (prefers-reduced-transparency: reduce) {
  .panel { background: var(--surface-solid); -webkit-backdrop-filter: none; backdrop-filter: none; }
}

.cards { display: grid; grid-template-columns: 1fr; }   /* single column without container queries */
.center { container-type: inline-size; }
@container (min-width: 420px) { .cards { grid-template-columns: 1fr 1fr; } }

.fade {
  -webkit-mask-image: linear-gradient(90deg, black 60%, transparent);
  mask-image: linear-gradient(90deg, black 60%, transparent);
}

@media (prefers-reduced-motion: reduce) {
  * { animation-duration: 1ms !important; animation-iteration-count: 1 !important; transition-duration: 1ms !important; }
}
```

```html
<picture>
  <source srcset="art.avif" type="image/avif">
  <img src="art.webp" alt="...">  <!-- WebP/PNG is the floor; AVIF is the enhancement -->
</picture>
```

## Blockers and limits

- **WebKitGTK and WKWebView: not run** (no hardware; CI compiles this crate
  on Linux and macOS to a stub that exits 2 with "unsupported"). Every WebKit
  cell above is UNVERIFIED pending SPK-009.
- **Active-window acrylic: not run.** Windows refused to make the spike the
  foreground window, so we do not know whether WebView2 alpha shows the DWM
  acrylic when the window is active.
- **Visual check:** the screen and CapturePreview images were inspected by the
  agent that ran the spike. No person looked at the live window, so the
  test-matrix "Visual" column stays `not-run`.
- One machine, one display, one GPU, one runtime version. Frame timing and
  memory are not budgets.
- The accessibility check covers names and roles only (no keyboard
  navigation, no Narrator output).

## Recommendations

- **SDK (SDK-007):** publish the tier table as the supported-feature
  contract and ship the fallback pattern in the template. Add a
  `glint theme validate` lint that flags Tier 2 features and `ui-*` generics
  as the last family in a stack. Flatten CSS nesting at build time so Tier 1
  nesting never reaches an engine that lacks it. Target `chrome120` plus the
  Safari versions SPK-009 measures for JS output.
- **THM (default theme):** use only Tier 0, plus Tier 1 behind
  `@supports`. Do not depend on DWM translucency: design the panel to read
  correctly on the solid fallback, because a docked panel is normally
  inactive and this run showed the inactive window gets a flat grey. Use
  Segoe UI Variable explicitly; `system-ui` gives Segoe UI.
- **RND / WIN:** share one `ICoreWebView2Environment` (one browser and GPU
  process) across all surfaces of a theme. A webview costs about 400 MiB
  private across the tree here, with the page itself about 56 MiB. Desktop
  see-through glass for an inactive panel needs a different composition path
  (for example a DirectComposition-hosted controller); add it to the
  WIN/RND spike list rather than promising it to themes. Keep transparent
  controller backgrounds: they work.
- **A11Y:** UI Automation exposes ARIA names and roles faithfully (switch,
  toggle button, slider, progressbar, toolbar, region), so an automated
  UIA-based check is feasible on Windows. `computedRole` is not available in
  the page, so do not build in-page checks on it.
- **PERF:** take rAF p95/p99 under a paint-heavy phase as the budget metric;
  this spike's harness (`frames.ts`) can be reused. Investigate the
  first-run hitches (6 and 4 dropped frames) before setting budgets.
- **SPK-009:** run the same bundle unchanged (it has no WebView2-specific
  code except the optional `postMessage`) in WebKitGTK and WKWebView and fill
  the UNVERIFIED cells. Compare the font widths, layout boxes and image
  samples recorded in [report.json](webview2-baseline/report.json).
