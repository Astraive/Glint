# Glint surfaces, placement engine, desktop composition

## Surface contract

A **surface** is a Glint-controlled region or window with a declared role, host strategy, content entrypoint and geometry policy. A **widget** is a child UI module hosted within an existing surface. Themes can implement standard and custom surfaces.

| ID | Role and lifecycle | Default input/position considerations |
|---|---|---|
| `panel` | Pinned desktop toolbar | Edge anchor; can reserve work area where supported |
| `dock` | Running/pinned apps | Edge/floating; may need accessibility/window data |
| `launcher` | Searchable app picker | Toggle + keyboard focus + dismissal |
| `control-center` | Quick settings | Overlay/popover; no native control center replacement |
| `notification` | Toasts for Glint-visible events | Short-lived, stacking, avoids focus steal |
| `notification-center` | History inside Glint | Persistent flyout; provider-dependent capture |
| `desktop` | Background canvas + widgets | Below windows where host permits; click-through regions |
| `calendar` | Date/time popup | Anchored to trigger, multi-monitor aware |
| `power-menu` | Sleep/shutdown options | Confirmation and OS permission gating |
| `workspace-switcher` | Workspace UI | Only controls supported workspaces |
| `window-switcher` | Window selection overlay | Cannot guarantee native Alt-Tab interception |
| `volume-osd` | Volume visual feedback | Auto-hide; avoid duplicates where possible |
| `brightness-osd` | Brightness visual feedback | Hardware support-dependent |
| `media-overlay` | Track metadata/playback | Shared MPRIS/OS provider |
| `custom:creator.id` | Author-defined window | Namespaced; requires declared placement/focus policy |

## Surface definition (illustrative)

```json
{
  "id": "panel.primary",
  "kind": "panel",
  "entry": "surfaces/panel/index.html",
  "monitor": "primary",
  "placement": {"anchor": "top", "height": 44, "margin": 0},
  "behavior": {"reserveWorkArea": true, "focus": "non-activating"},
  "requiredCapabilities": ["surface.panel"]
}
```

Using HTML entrypoints per surface is recommended over raw JS filenames: bundlers can emit HTML+JS+CSS chunk graphs, and hosts know the exact local document to load. Manifests must never point to arbitrary `file://` or remote pages.

## Layout pipeline

1. Load theme defaults and profile user overrides.
2. Choose target monitors and work areas from adapter.
3. Resolve edges, anchors, max/min sizes, density and safe areas.
4. Account for scale factor, rotation, taskbar/dock/notch, reservation conflicts.
5. Resolve z-layer and click/focus behavior supported by host.
6. Mount renderer; notify it of logical size and safe bounds.
7. Store actual geometry and monitor mapping; recalc on topology changes.

## Per-monitor model

Monitor ID should use stable OS identity when possible and tolerate renaming/reconnection. Unknown monitors receive primary-display fallback, not an off-screen last-known coordinate. User configuration may override auto placement by screen name/EDID-like stable key (privacy-safe display metadata as available).

## Placement semantics

- `edge`: top/bottom/left/right; optionally reserve workspace.
- `floating`: point, relative anchor, snapping; respects safe bounds.
- `desktop`: render below normal windows only on supported compositor/OS technique.
- `overlay`: above normal windows as allowed; focusable or non-focusable.
- `popup`: anchored to trigger, closes on Escape/outside click as platform allows.

Do not promise exact z-order on every compositor; use capability flags and screenshots/QA.

## Conflict policy

- Never hide the OS taskbar automatically without explicit, reversible user permission.
- If a top bar reservation conflicts with another toolbar, offer overlay or relocated mode.
- If fullscreen gaming/video requires no overlays, allow per-app/per-workspace suppression and a panic-disable shortcut.
- One package may create several surfaces, but it must declare them and cannot spawn unbounded windows.

## Fallback rules

Surface requested but not supplied -> default theme surface. Surface supplied but incompatible -> compatible default (or safely absent if optional). Surface renderer crash -> unmount and recover default; diagnostic event. Package requiring a missing **hard** capability should fail activation before any OS state change, unless user explicitly chose partial application mode.

## Accessibility

Keyboard reachability, focus trap rules, screen reader labels and predictable Escape semantics are required in built-in surface contracts. Programmable themes can customize visuals but must pass basic lint/a11y checks for official library acceptance.
