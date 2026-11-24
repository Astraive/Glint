// Palette lint (MGR-001 acceptance): the manager's own sources contain no
// green colour value. Every hex, rgb() and hsl() colour in src/ and
// index.html is converted to HSL; a saturated hue between yellow-green and
// cyan-green fails, and so do green colour keywords. Neutral greys pass.

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const APP = fileURLToPath(new URL("..", import.meta.url));

function sources(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? sources(path) : /\.(css|tsx?|html)$/.test(name) ? [path] : [];
  });
}

type Rgb = [number, number, number];

function hexToRgb(hex: string): Rgb {
  let body = hex.slice(1);
  if (body.length === 3 || body.length === 4) body = [...body.slice(0, 3)].map((c) => c + c).join("");
  return [0, 2, 4].map((i) => Number.parseInt(body.slice(i, i + 2), 16)) as Rgb;
}

/** Hue in degrees and saturation 0..1 (HSL). */
function hueSaturation([r, g, b]: Rgb): { hue: number; saturation: number } {
  const [rn, gn, bn] = [r / 255, g / 255, b / 255];
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const delta = max - min;
  if (delta === 0) return { hue: 0, saturation: 0 };
  const lightness = (max + min) / 2;
  const saturation = delta / (1 - Math.abs(2 * lightness - 1));
  let hue: number;
  if (max === rn) hue = 60 * (((gn - bn) / delta) % 6);
  else if (max === gn) hue = 60 * ((bn - rn) / delta + 2);
  else hue = 60 * ((rn - gn) / delta + 4);
  return { hue: (hue + 360) % 360, saturation };
}

function isGreen(rgb: Rgb): boolean {
  const { hue, saturation } = hueSaturation(rgb);
  return saturation > 0.15 && hue >= 75 && hue <= 165;
}

const HEX = /#(?:[0-9a-f]{8}|[0-9a-f]{6}|[0-9a-f]{3,4})\b/gi;
const RGB = /rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)/gi;
const HSL = /hsla?\(\s*(\d+(?:\.\d+)?)(?:deg)?[\s,]+(\d+(?:\.\d+)?)%/gi;
const KEYWORDS = /\b(?:green|lime|limegreen|lawngreen|chartreuse|olive|olivedrab|seagreen|forestgreen|springgreen|teal|mintcream|honeydew)\b/i;

describe("palette lint", () => {
  it("detects green and passes neutral greys", () => {
    expect(isGreen(hexToRgb("#00ff00"))).toBe(true);
    expect(isGreen(hexToRgb("#2e7d32"))).toBe(true);
    expect(isGreen(hexToRgb("#7a8"))).toBe(true);
    for (const grey of ["#0b0c0f", "#181a20", "#727a89", "#b9c0cc", "#ffffff", "#565d6b"]) {
      expect(isGreen(hexToRgb(grey))).toBe(false);
    }
  });

  it("finds no green value in the manager sources", () => {
    const files = [...sources(join(APP, "src")), join(APP, "index.html")];
    expect(files.length).toBeGreaterThan(3);
    const offenders: string[] = [];
    for (const file of files) {
      const text = readFileSync(file, "utf8");
      for (const match of text.matchAll(HEX)) {
        if (isGreen(hexToRgb(match[0]))) offenders.push(`${file}: ${match[0]}`);
      }
      for (const match of text.matchAll(RGB)) {
        const rgb = [match[1], match[2], match[3]].map(Number) as Rgb;
        if (isGreen(rgb)) offenders.push(`${file}: ${match[0]}`);
      }
      for (const match of text.matchAll(HSL)) {
        const hue = Number(match[1]);
        const saturation = Number(match[2]) / 100;
        if (saturation > 0.15 && hue >= 75 && hue <= 165) offenders.push(`${file}: ${match[0]}`);
      }
      // Comments may say "no green"; declarations may not use it.
      const code = text.replace(/\/\*[\s\S]*?\*\//g, "");
      if (file.endsWith(".css") && KEYWORDS.test(code)) offenders.push(`${file}: green colour keyword`);
    }
    expect(offenders).toEqual([]);
  });
});
