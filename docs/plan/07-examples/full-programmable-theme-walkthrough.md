# Worked example — a full Nothing-inspired Glint theme

This is a **conceptual design + starter snippets**, not a tested theme. The project may use a Nothing-inspired aesthetic while avoiding proprietary naming/asset redistribution. Use original glyphs and licensed fonts/icons.

## Experience

- Monochrome dark theme with circular dot-matrix clock, high-contrast icons and minimal chrome.
- Top panel: workspace markers on left, clock in middle, system state on right.
- App launcher: centered search, applications grid, pinned commands.
- Control center: big tactile Wi-Fi/Bluetooth tiles, volume/brightness sliders, media card.
- Notifications: black-background compact toasts and an expandable list.
- Desktop: animated abstract wallpaper and 13×13 glyph clock widget; 25×25 matrix is optional for larger hero widget.
- All screens are **Glint-managed**, not rewritten native Windows Settings or macOS Control Center.

## Layout

```text
NOTHING-MONO (example theme, not project core)
├── panel.top
│   ├── workspaces
│   ├── clock
│   └── network, battery, controls button
├── dock.bottom
│   ├── pinned apps
│   └── running app indicators
├── launcher.center
│   ├── search input
│   └── app grid
├── control-center.top-right
│   ├── system toggle tiles
│   ├── volume / brightness sliders
│   └── music player
├── notification.top-right
└── desktop
    ├── wallpaper
    └── GlyphMatrix13 clock widget
```

## Source structure

```text
nothing-mono/
├── glint.json
├── src/surfaces/panel.tsx
├── src/surfaces/dock.tsx
├── src/surfaces/launcher.tsx
├── src/surfaces/control-center.tsx
├── src/surfaces/notification.tsx
├── src/widgets/glyph-matrix.tsx
├── src/styles/variables.css
├── assets/wallpaper.png
├── assets/icons/
└── dist/ # generated
```

## Top panel UI

```tsx
import { useClock, useWorkspaces, useBattery, useGlint } from '@glint/sdk/react';

export default function Panel() {
  const { text } = useClock({ format: 'HH:mm' });
  const workspaces = useWorkspaces();
  const battery = useBattery();
  const glint = useGlint();
  return (
    <header className="mono-panel">
      <div className="workspace-dots" aria-label="Workspaces">
        {workspaces.state === 'ready' ? workspaces.items.map(w => (
          <button key={w.id} aria-label={w.label}
            aria-current={w.active ? 'page' : undefined}
            onClick={() => workspaces.switchTo(w.id)}>
            {w.active ? '●' : '○'}
          </button>
        )) : <span>•</span>}
      </div>
      <time>{text}</time>
      <div className="indicators">
        <span>{battery.state === 'ready' ? `${battery.value.percent}%` : '—'}</span>
        <button onClick={() => glint.surfaces.toggle('control-center')}>
          Controls
        </button>
      </div>
    </header>
  );
}
```

## CSS visual tokens

```css
:root {
  --mono-bg: #0b0c0f;
  --mono-surface: #181a20;
  --mono-text: #f5f6f8;
  --mono-secondary: #b9c0cc;
  --mono-outline: #2b2f38;
}
.mono-panel {
  display: grid; grid-template-columns: 1fr auto 1fr;
  align-items: center; height: 48px; padding: 0 16px;
  background: var(--mono-bg); color: var(--mono-text);
  border-bottom: 1px solid var(--mono-outline);
}
.workspace-dots { display: flex; gap: 8px; }
.indicators { justify-self: end; display: flex; gap: 12px; }
```

## Compatibility detail

- Linux/Hyprland: workspace names from compositor adapter; layer-shell host for panel if available.
- Windows: if no real equivalent for Hyprland workspace semantics, show taskbar app groups or omit dots using an explicit component fallback; appbar/overlay host can display panel.
- macOS: surface is app-controlled overlay; do not claim to replace global menu bar. If `battery` is unavailable (desktop hardware), hide it.

## Deliverable validation

`glint theme build` emits static `dist/<surface>/index.html` bundles, `glint theme pack` writes `nothing-mono.glint`, importing offline opens a safe preview, activating on each tested OS either shows supported surfaces or explained fallbacks. No user must install Node to consume package.
