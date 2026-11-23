// CSS feature probes. Each probe is checked two ways where possible:
//  - `supports`: what CSS.supports() claims (parse-level only);
//  - `applied`: a rule in baseline.css targets a .pr-* fixture, and the
//    computed style proves the engine actually applied it.
// Status: supported = both agree on yes; partial = only one says yes (or only
// a vendor-prefixed form works); unsupported = neither.

export type Status = 'supported' | 'partial' | 'unsupported';

export interface CssResult {
  id: string;
  group: string;
  status: Status;
  supports: boolean | null;
  applied: boolean | null;
  value: string | null;
  note?: string;
}

interface Probe {
  id: string;
  group: string;
  supports?: string;
  /** Fixture class (created under #probes); `kid` adds a child to read from. */
  fixture?: { cls: string; kid?: string; wrap?: string };
  read?: (el: HTMLElement) => { applied: boolean; value: string };
  /** For probes that have no computed-style evidence (JS API presence). */
  api?: () => { applied: boolean; value: string };
  note?: string;
}

const cs = (el: Element) => getComputedStyle(el);
const prop = (name: string, unset: string | RegExp) => (el: HTMLElement) => {
  const value = cs(el).getPropertyValue(name).trim();
  const applied = typeof unset === 'string' ? value !== '' && value !== unset : !unset.test(value);
  return { applied, value };
};
const order7 = (el: HTMLElement) => {
  const value = cs(el).order;
  return { applied: value === '7', value };
};
const colorNotBlack = (el: HTMLElement) => {
  const value = cs(el).color;
  return { applied: value !== 'rgb(0, 0, 0)', value };
};

export const PROBES: Probe[] = [
  // Effects / translucency
  { id: 'backdrop-filter', group: 'effects', supports: 'backdrop-filter: blur(1px)', fixture: { cls: 'pr-backdrop' }, read: prop('backdrop-filter', 'none') },
  { id: '-webkit-backdrop-filter', group: 'effects', supports: '-webkit-backdrop-filter: blur(1px)', fixture: { cls: 'pr-backdrop-webkit' }, read: prop('-webkit-backdrop-filter', /^(none|)$/), note: 'WebKit historically needed the prefix; keep both in theme CSS' },
  { id: 'filter: blur()', group: 'effects', supports: 'filter: blur(1px)', fixture: { cls: 'pr-filter' }, read: prop('filter', 'none') },
  { id: 'box-shadow (multi, inset)', group: 'effects', supports: 'box-shadow: inset 0 0 0 1px red', fixture: { cls: 'pr-box-shadow' }, read: prop('box-shadow', 'none') },
  { id: 'text-shadow', group: 'effects', supports: 'text-shadow: 0 1px 2px black', fixture: { cls: 'pr-text-shadow' }, read: prop('text-shadow', 'none') },
  { id: 'mask-image', group: 'effects', supports: 'mask-image: linear-gradient(black, transparent)', fixture: { cls: 'pr-mask' }, read: prop('mask-image', 'none') },
  { id: '-webkit-mask-image', group: 'effects', supports: '-webkit-mask-image: linear-gradient(black, transparent)', fixture: { cls: 'pr-mask-webkit' }, read: prop('-webkit-mask-image', 'none') },
  { id: 'clip-path: path()', group: 'effects', supports: 'clip-path: path("M0 0 L1 0 Z")', fixture: { cls: 'pr-clip-path' }, read: prop('clip-path', 'none') },
  { id: 'mix-blend-mode', group: 'effects', supports: 'mix-blend-mode: multiply', fixture: { cls: 'pr-blend' }, read: prop('mix-blend-mode', 'normal') },
  // Gradients
  { id: 'linear-gradient', group: 'gradients', supports: 'background-image: linear-gradient(red, blue)', fixture: { cls: 'pr-linear' }, read: prop('background-image', 'none') },
  { id: 'radial-gradient', group: 'gradients', supports: 'background-image: radial-gradient(red, blue)', fixture: { cls: 'pr-radial' }, read: prop('background-image', 'none') },
  { id: 'conic-gradient', group: 'gradients', supports: 'background-image: conic-gradient(red, blue)', fixture: { cls: 'pr-conic' }, read: prop('background-image', 'none') },
  { id: 'gradient interpolation (in oklch)', group: 'gradients', supports: 'background-image: linear-gradient(in oklch, red, blue)', fixture: { cls: 'pr-grad-oklch' }, read: prop('background-image', 'none') },
  // Selectors and cascade
  { id: ':has()', group: 'selectors', supports: 'selector(:has(> a))', fixture: { cls: 'pr-has', kid: 'pr-has-kid' }, read: (el) => order7(el.parentElement as HTMLElement) },
  { id: 'CSS nesting (&)', group: 'selectors', fixture: { cls: 'pr-nest', kid: 'pr-nest-kid' }, read: order7, note: 'no CSS.supports() form exists for nesting; computed check only' },
  { id: ':is()', group: 'selectors', supports: 'selector(:is(a))', fixture: { cls: 'pr-is', kid: 'pr-is-kid' }, read: order7 },
  { id: ':focus-visible', group: 'selectors', supports: 'selector(:focus-visible)' },
  { id: '@layer', group: 'cascade', api: () => ({ applied: 'CSSLayerBlockRule' in window, value: String('CSSLayerBlockRule' in window) }), fixture: { cls: 'pr-layer' }, read: order7 },
  { id: '@property', group: 'cascade', fixture: { cls: 'pr-layer' }, read: (el) => { const v = cs(el).getPropertyValue('--pr-angle').trim(); return { applied: v === '42deg', value: v || '(empty)' }; } },
  { id: '@scope', group: 'cascade', fixture: { cls: 'pr-scope', kid: 'pr-scope-kid' }, read: order7 },
  { id: '@starting-style', group: 'cascade', api: () => ({ applied: 'CSSStartingStyleRule' in window, value: String('CSSStartingStyleRule' in window) }) },
  // Container queries
  { id: 'container size queries', group: 'container', supports: 'container-type: inline-size', fixture: { cls: 'pr-cq', kid: 'pr-cq-kid' }, read: order7 },
  { id: 'container query units (cqi)', group: 'container', supports: 'width: 50cqi', fixture: { cls: 'pr-cq', kid: 'pr-cq-unit' }, read: (el) => { const value = cs(el).width; return { applied: value === '150px', value }; } },
  { id: 'container style queries', group: 'container', fixture: { cls: 'pr-csq', kid: 'pr-csq-kid' }, read: order7, note: 'custom-property style() queries only' },
  // Colour
  { id: 'color-mix()', group: 'color', supports: 'color: color-mix(in srgb, red 50%, blue)', fixture: { cls: 'pr-color-mix' }, read: colorNotBlack },
  { id: 'light-dark()', group: 'color', supports: 'color: light-dark(red, blue)', fixture: { cls: 'pr-light-dark' }, read: (el) => { const value = cs(el).color; return { applied: value === 'rgb(1, 2, 3)', value }; } },
  { id: 'oklch()', group: 'color', supports: 'color: oklch(0.7 0.15 250)', fixture: { cls: 'pr-oklch' }, read: colorNotBlack },
  { id: 'relative color syntax', group: 'color', supports: 'color: rgb(from red r g b)', fixture: { cls: 'pr-relative' }, read: colorNotBlack },
  { id: 'color(display-p3)', group: 'color', supports: 'color: color(display-p3 1 0 0)', fixture: { cls: 'pr-p3' }, read: colorNotBlack },
  // Layout
  { id: 'subgrid', group: 'layout', supports: 'grid-template-columns: subgrid', fixture: { cls: 'pr-subgrid', kid: '' }, read: prop('grid-template-columns', 'none') },
  { id: 'aspect-ratio', group: 'layout', supports: 'aspect-ratio: 2 / 1', fixture: { cls: 'pr-aspect' }, read: (el) => { const value = cs(el).height; return { applied: value === '20px', value }; } },
  { id: 'inset shorthand', group: 'layout', supports: 'inset: 1px', fixture: { cls: 'pr-inset' }, read: (el) => { const value = cs(el).left; return { applied: value === '4px', value }; } },
  { id: 'dynamic viewport units (dvh)', group: 'layout', supports: 'height: 10dvh', fixture: { cls: 'pr-dvh' }, read: prop('height', /^(auto|0px|)$/) },
  { id: 'individual transform (translate)', group: 'layout', supports: 'translate: 1px 1px', fixture: { cls: 'pr-translate' }, read: prop('translate', 'none') },
  { id: 'anchor positioning', group: 'layout', supports: 'anchor-name: --a', fixture: { cls: 'pr-anchor' }, read: prop('anchor-name', 'none') },
  { id: 'content-visibility', group: 'layout', supports: 'content-visibility: auto', fixture: { cls: 'pr-content-vis' }, read: prop('content-visibility', 'visible') },
  { id: 'field-sizing', group: 'layout', supports: 'field-sizing: content', fixture: { cls: 'pr-field-sizing' }, read: prop('field-sizing', /^(fixed|)$/) },
  { id: 'interpolate-size', group: 'layout', supports: 'interpolate-size: allow-keywords', fixture: { cls: 'pr-interpolate-size' }, read: prop('interpolate-size', /^(numeric-only|)$/) },
  // Text
  { id: 'text-wrap: balance', group: 'text', supports: 'text-wrap: balance', fixture: { cls: 'pr-text-wrap' }, read: (el) => { const s = cs(el); const value = s.getPropertyValue('text-wrap-style') || s.getPropertyValue('text-wrap'); return { applied: /balance/.test(value), value }; } },
  { id: 'font-variation-settings', group: 'text', supports: 'font-variation-settings: "wght" 650', fixture: { cls: 'pr-font-var' }, read: prop('font-variation-settings', 'normal') },
  { id: '-webkit-line-clamp', group: 'text', supports: '-webkit-line-clamp: 2', fixture: { cls: 'pr-line-clamp' }, read: prop('-webkit-line-clamp', 'none') },
  // Scrolling / animation
  { id: 'scrollbar-color', group: 'scrolling', supports: 'scrollbar-color: red blue', fixture: { cls: 'pr-scrollbar-color' }, read: prop('scrollbar-color', 'auto') },
  { id: 'scrollbar-gutter', group: 'scrolling', supports: 'scrollbar-gutter: stable' },
  { id: 'scroll-driven animations', group: 'animation', supports: 'animation-timeline: scroll()', fixture: { cls: 'pr-scroll-timeline' }, read: prop('animation-timeline', /^(auto|)$/) },
  { id: 'Web Animations API', group: 'animation', api: () => ({ applied: typeof Element.prototype.animate === 'function', value: typeof Element.prototype.animate }) },
  { id: 'View Transitions API', group: 'animation', api: () => ({ applied: 'startViewTransition' in document, value: String('startViewTransition' in document) }) },
  // Host-integration CSS
  { id: '-webkit-app-region', group: 'host', supports: '-webkit-app-region: drag', fixture: { cls: 'pr-app-region' }, read: prop('-webkit-app-region', /^(none|)$/), note: 'Chromium-only drag regions; never a portable theme feature' },
  { id: 'accent-color', group: 'host', supports: 'accent-color: red' },
  { id: 'color-scheme', group: 'host', supports: 'color-scheme: light dark' },
];

export function buildFixtures(root: HTMLElement): Map<string, HTMLElement> {
  const targets = new Map<string, HTMLElement>();
  for (const probe of PROBES) {
    if (!probe.fixture) continue;
    const host = document.createElement('div');
    host.className = probe.fixture.cls;
    host.textContent = probe.fixture.kid === undefined ? 'Probe text' : '';
    root.appendChild(host);
    let target = host;
    if (probe.fixture.kid !== undefined) {
      const kid = document.createElement('div');
      if (probe.fixture.kid) kid.className = probe.fixture.kid;
      kid.textContent = 'Probe text';
      host.appendChild(kid);
      target = kid;
    }
    targets.set(probe.id, target);
  }
  return targets;
}

export function runCssProbes(root: HTMLElement): CssResult[] {
  const targets = buildFixtures(root);
  return PROBES.map((probe) => {
    const supports = probe.supports ? CSS.supports(probe.supports) : null;
    let applied: boolean | null = null;
    let value: string | null = null;
    const target = targets.get(probe.id);
    if (probe.read && target) {
      const r = probe.read(target);
      applied = r.applied;
      value = r.value;
    }
    if (probe.api) {
      const r = probe.api();
      applied = applied === null ? r.applied : applied && r.applied;
      value = value === null ? r.value : `${value}; api=${r.value}`;
    }
    const votes = [supports, applied].filter((v): v is boolean => v !== null);
    const yes = votes.filter(Boolean).length;
    const status: Status = yes === 0 ? 'unsupported' : yes === votes.length ? 'supported' : 'partial';
    return { id: probe.id, group: probe.group, status, supports, applied, value, note: probe.note };
  });
}
