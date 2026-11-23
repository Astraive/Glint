# Glint SDK / React authoring specification

**Recommended authoring language:** TypeScript + React for programmable themes; optional `@glint/ui` primitives. The SDK is a **proposed API**. No names below imply published modules today.

## SDK principles

- Pure presentation code may run in isolated webviews; it never imports Rust OS adapters.
- System queries and actions use a **typed, permission-scoped async bridge**.
- Hooks support loading, unavailable, read-only and permission-denied states; don't return fake values.
- Data subscriptions are reference-counted by daemon providers; hidden surfaces suspend expensive work.
- APIs are versioned; TypeScript declarations generated from canonical schema/Rust types where practical.
- Package can style DOM using any library/CSS, so UI kit is optional.

## Proposed namespaces

```text
@glint/sdk
├── system.audio / brightness / battery / network / bluetooth / power
├── system.media / system.stats / system.clock
├── desktop.applications / windows / workspaces / monitors / wallpapers
├── surfaces.open / close / toggle / focus
├── notifications / assets / theme / settings
├── events.subscribe / events.snapshot
├── permissions.request / capabilities.get
└── diagnostics.log
```

## Example public TypeScript contract

```ts
export type Availability =
  | { state: 'supported' }
  | { state: 'read-only' }
  | { state: 'permission-required'; permission: string }
  | { state: 'experimental'; detail: string }
  | { state: 'unavailable'; reason: string };

export type CapabilityId =
  | 'system.audio.read'
  | 'system.audio.write'
  | 'desktop.workspaces.read'
  | 'desktop.workspaces.switch'
  | 'surface.panel'
  | 'surface.launcher'; // generated union expands as backend grows

export interface AudioState {
  volume: number; // 0..100
  muted: boolean;
}

export interface GlintApi {
  capabilities: { get(id: CapabilityId): Promise<Availability> };
  audio: {
    get(): Promise<AudioState>;
    setVolume(percent: number): Promise<void>;
  };
  surfaces: { toggle(id: string): Promise<void> };
}
```

Actual React hook `useAudio()` returns `{state, capability, setVolume, error}` rather than assuming permission or device presence. `useWorkspaces()` returns `Unavailable` state on sessions without workspace APIs.

## Example panel code

```tsx
import { useClock, useWorkspaces, useGlint } from '@glint/sdk/react';
import './panel.css';

export default function Panel() {
  const clock = useClock({ format: 'HH:mm' });
  const spaces = useWorkspaces();
  const glint = useGlint();
  return (
    <nav className="panel" aria-label="Desktop panel">
      <div className="workspace-list">
        {spaces.state === 'ready' && spaces.items.map(ws => (
          <button key={ws.id} aria-current={ws.active ? 'page' : undefined}
            onClick={() => spaces.switchTo(ws.id)}>{ws.label}</button>
        ))}
      </div>
      <time>{clock.text}</time>
      <button onClick={() => glint.surfaces.toggle('control-center')}>
        Controls
      </button>
    </nav>
  );
}
```

This example assumes an API to be implemented. It is a **reference shape**, not tested against a published SDK.

## Example safe control center

```tsx
import { useAudio, useCapabilities } from '@glint/sdk/react';

export function ControlCenter() {
  const audio = useAudio();
  const caps = useCapabilities();
  const canWrite = caps.isWritable('system.audio.write');
  return (
    <section aria-label="Quick settings">
      <label htmlFor="vol">Volume</label>
      <input id="vol" type="range" min={0} max={100}
        disabled={!canWrite || audio.state !== 'ready'}
        value={audio.value?.volume ?? 0}
        onChange={event => audio.setVolume(Number(event.target.value))}/>
    </section>
  );
}
```

SDK implementation should avoid blasting a native audio request on every micro-step; use throttling/coalescing and confirm final OS state.

## Theme-owned shared state

- `theme.storage` for small settings **namespaced to package ID**; not unrestricted filesystem.
- `theme.tokens` for active semantic tokens.
- `events` subscriptions for updates and lifetime cleanup.
- Cross-surface state via daemon-backed namespaced key-value store or event channels; never global JS access across arbitrary origins.
- Runtime context: platform family, monitor size, scale, host bounds, reduced-motion, localization, accessible contrast, grants.

## Third-party dependencies

Theme author can import any build-time JS/React packages subject to license and static bundling, but the packaged runtime may not fetch remote executable JS. Native addons are out of scope. App developer should document supported browser language/feature baseline across WebView2/WKWebView/WebKitGTK.

## API documentation requirements

Every method: input/output, capability, permission, OS variations, error codes, subscription rate, examples, privacy notes and fallbacks. Provide simulated SDK dev host fixtures for offline previews.
