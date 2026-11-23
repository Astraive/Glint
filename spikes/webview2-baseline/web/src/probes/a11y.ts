// In-page accessibility evidence. The browser's real accessibility tree is
// not readable from script without CDP; the host reads it through Windows UI
// Automation instead (see src/win.rs). Here we record what the page can see:
// the authored ARIA, whether the experimental computedRole/computedName
// accessors exist, ARIA reflection, and user-preference media queries.

/** Names the host expects to find through UI Automation; keep in sync with
 * EXPECTED_A11Y in spikes/webview2-baseline/src/logic.rs. */
export const EXPECTED = [
  { name: 'Start', role: 'button' },
  { name: 'Workspace 1', role: 'button' },
  { name: 'Wi-Fi', role: 'switch' },
  { name: 'Volume', role: 'slider' },
  { name: 'Battery', role: 'progressbar' },
  { name: 'Open control center', role: 'button' },
  { name: 'Glint probe panel', role: 'toolbar' },
  { name: 'Quick settings', role: 'region' },
];

type Computed = Element & { computedRole?: string | null; computedName?: string | null };

export interface A11yReport {
  authored: { name: string; role: string; found: boolean; computedRole: string | null; computedName: string | null }[];
  computedAccessorsAvailable: boolean;
  ariaReflection: boolean;
  elementInternalsRole: boolean;
  inert: boolean;
  popover: boolean;
  dialog: boolean;
  tabbableCount: number;
  media: Record<string, string>;
}

const query = (q: string) => {
  const m = matchMedia(q);
  return m.media === 'not all' ? 'unsupported' : String(m.matches);
};

export function runA11yProbes(): A11yReport {
  const authored = EXPECTED.map(({ name, role }) => {
    const el = [...document.querySelectorAll<HTMLElement>('[aria-label]')].find((e) => e.getAttribute('aria-label') === name) as Computed | undefined;
    const authoredRole = el ? el.getAttribute('role') ?? el.tagName.toLowerCase() : '';
    return {
      name,
      role,
      found: !!el && authoredRole === role,
      computedRole: el?.computedRole ?? null,
      computedName: el?.computedName ?? null,
    };
  });
  return {
    authored,
    computedAccessorsAvailable: 'computedRole' in Element.prototype,
    ariaReflection: 'ariaLabel' in Element.prototype,
    elementInternalsRole: typeof ElementInternals !== 'undefined' && 'role' in ElementInternals.prototype,
    inert: 'inert' in HTMLElement.prototype,
    popover: 'popover' in HTMLElement.prototype,
    dialog: typeof HTMLDialogElement !== 'undefined',
    tabbableCount: document.querySelectorAll('button, [tabindex="0"], input, a[href]').length,
    media: {
      'prefers-color-scheme: dark': query('(prefers-color-scheme: dark)'),
      'prefers-reduced-motion: reduce': query('(prefers-reduced-motion: reduce)'),
      'prefers-contrast: more': query('(prefers-contrast: more)'),
      'forced-colors: active': query('(forced-colors: active)'),
      'prefers-reduced-transparency: reduce': query('(prefers-reduced-transparency: reduce)'),
      'dynamic-range: high': query('(dynamic-range: high)'),
      'color-gamut: p3': query('(color-gamut: p3)'),
    },
  };
}
