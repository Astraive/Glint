// System font availability by canvas text metrics. document.fonts.check()
// is useless here: it returns true for any family with no @font-face rule.
// A family is "present" when text set in `"Family", <generic>` measures
// differently from the bare generic for at least one of three generics.

export interface FontResult {
  family: string;
  present: boolean;
  widthPx: number | null;
}

export interface FontReport {
  families: FontResult[];
  generics: Record<string, number>;
  sample: { font: string; text: string; widthPx: number; ascent: number; descent: number }[];
  bodyFontFamily: string;
}

const TEST = 'mmmmmmmmmmlli WWW 0123456789';
const SIZE = 48;
const BASES = ['monospace', 'serif', 'sans-serif'];

// Windows families first; the rest are expected absent here and recorded so
// SPK-009 can fill the same rows on WebKitGTK and WKWebView.
export const FAMILIES = [
  'Segoe UI Variable Text',
  'Segoe UI Variable Display',
  'Segoe UI Variable Small',
  'Segoe UI',
  'Segoe UI Emoji',
  'Segoe Fluent Icons',
  'Segoe MDL2 Assets',
  'Cascadia Mono',
  'Cascadia Code',
  'Consolas',
  'Arial',
  'Calibri',
  'Cantarell',
  'Adwaita Sans',
  'Noto Sans',
  'DejaVu Sans',
  'SF Pro Text',
  'Helvetica Neue',
];

export const GENERICS = ['system-ui', 'ui-sans-serif', 'ui-serif', 'ui-monospace', 'ui-rounded', 'sans-serif', 'serif', 'monospace', 'cursive', 'fantasy', 'emoji', 'math'];

export function runFontProbes(): FontReport {
  const ctx = document.createElement('canvas').getContext('2d');
  if (!ctx) throw new Error('2d canvas unavailable');
  const width = (font: string) => {
    ctx.font = font;
    return ctx.measureText(TEST).width;
  };
  const base = Object.fromEntries(BASES.map((b) => [b, width(`${SIZE}px ${b}`)]));
  const families = FAMILIES.map((family) => {
    let present = false;
    let widthPx: number | null = null;
    for (const b of BASES) {
      const w = width(`${SIZE}px "${family}", ${b}`);
      if (Math.abs(w - base[b]) > 0.01) {
        present = true;
        widthPx = round(w);
        break;
      }
    }
    return { family, present, widthPx };
  });
  // A generic the engine does not know makes the whole font shorthand
  // invalid, so ctx.font keeps the previous value: reset first and record it.
  const generics: Record<string, number> = {};
  for (const g of GENERICS) {
    ctx.font = '10px serif';
    ctx.font = `${SIZE}px ${g}`;
    generics[g] = ctx.font.includes(g) ? round(ctx.measureText(TEST).width) : -1;
  }
  const sample = ['13px "Segoe UI Variable Text", system-ui', '600 14px "Segoe UI Variable Display", system-ui', '13px "Cascadia Mono", ui-monospace, monospace', '13px system-ui'].map((font) => {
    ctx.font = font;
    const text = 'Glint 12:34 Wi-Fi';
    const m = ctx.measureText(text);
    return { font, text, widthPx: round(m.width), ascent: round(m.actualBoundingBoxAscent), descent: round(m.actualBoundingBoxDescent) };
  });
  return { families, generics, sample, bodyFontFamily: getComputedStyle(document.body).fontFamily };
}

export const round = (n: number, digits = 2) => Math.round(n * 10 ** digits) / 10 ** digits;
