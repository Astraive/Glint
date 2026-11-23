# Testing strategy, cross-platform QA and measurable acceptance

## Test pyramid

**Pure unit:** manifest parser/schema, path checks, package hash, profile composition, capability negotiation, permission engine, source URL parsing, asset lookup, layout calculations.

**Component/integration:** package installer staging + SQLite, Git resolver fixtures, IPC serialization/reconnection, system provider mock backends, renderer permission denial, surface host lifecycle and snapshot/rollback.

**Actual OS integration:** Windows appbar/wallpaper/WebView2, Hyprland socket + Wayland layer-shell, KDE/GNOME specific behavior, macOS AppKit/WKWebView/status items/wallpaper.

**End-to-end:** install -> preview -> activate -> switch surface -> export -> import offline -> revert; renderer crash safe mode; multi-display hot-plug.

## Non-negotiable package security tests

- `../outside` file extraction rejected.
- Absolute POSIX/Windows paths, UNC paths and device paths rejected.
- Symlinks and ZIP extra attributes cannot escape extraction destination.
- Duplicate case-folding archive path conflicts rejected.
- Compression bomb and too-large extracted assets rejected.
- HTML entrypoints cannot load arbitrary remote scripts.
- Package with native executable/shell script never executes it on import.
- Invalid/corrupted manifest does not change live profile.
- Unknown capability cannot silently bypass checks.
- Untrusted renderer cannot call unauthorized daemon commands, even after UI tampering.

## Capability tests

Mock three states for every public SDK action: supported, denied and unavailable. Test transitions live (network disappearing, audio endpoint removed, workspace destroyed). Confirm hook and UI update appropriately. Treat Linux compositors as distinct test targets.

## OS matrix

| Platform | Environments (initial suggested) | Test focus |
|---|---|---|
| Windows | Windows 11, two display/DPI combinations | Appbar layout, taskbar coexistence, Explorer restart, WebView2 |
| Linux | Arch Hyprland Wayland | Layer-shell, IPC, widgets, workspaces |
| Linux | KDE Plasma Wayland | App surface behavior, theme bridge and work areas |
| Linux | GNOME Wayland | Overlay fallback, extension boundary |
| Linux | Sway/Niri | Protocol detection; compositor adapters |
| Linux | One X11 session | Window hints, desktop layers |
| macOS | Intel/Apple Silicon if available; several macOS versions | AppKit, wallpaper, status bar, Spaces |

Only claim an environment is supported after the actual test passes. Some matrix rows can remain pending for early releases; show “experimental” clearly.

## User experience quality gates

- No unbounded CPU consumption in a dormant clock-only theme; use event timers and subscriptions.
- UI responds to monitor hotplug and fractional scaling without invisible windows or truncated controls.
- Full keyboard navigation for manager and built-in surfaces; Escape dismisses popup; screen reader labels present.
- Offline import/export round-trip preserves package assets/hash and intended appearance.
- Theme crashes never take down Explorer/compositor or disable basic system access.
- System modifications revert or report exactly which native preference couldn't be restored.
- Missing permissions produce visible disabled states, never simulated “success”.

## Performance measurements to establish through spikes

Cold daemon start, first panel paint, theme switch time, memory per renderer, aggregate idle CPU, animation frame pacing on integrated GPU, battery impact, archive pack/install time, registry browse latency with 1k/10k themes. **Do not publish arbitrary SLA targets before baseline data**.

## Release checklist

Platform tests, security fixture suite, package backwards compatibility, clean signed installers, rollback smoke, manifest schema version test, offline boot, changelog, permission audit, accessibility pass, source integrity and license review.
