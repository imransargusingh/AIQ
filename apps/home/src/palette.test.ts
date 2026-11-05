// The rung ramp's checks (thread "Make the five rung colours easy to tell apart", 2026-11-05): one hue, lightness
// monotone in even visible steps, the light end clearing the canvas, and a number ink that reads on every fill.
// Thresholds are the dataviz skill's ordinal checks (ΔL ≥ 0.06, light end ≥ 2:1) plus the AIQ floors chosen here.
import { describe, expect, it } from "vitest";
import css from "./styles.css?raw";

const lin = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const rgb = (h: string) => {
  const x = h.length === 4 ? h.slice(1).split("").map((d) => d + d) : [h.slice(1, 3), h.slice(3, 5), h.slice(5, 7)];
  return x.map((v) => lin(parseInt(v, 16) / 255));
};
const lum = (h: string) => { const [r, g, b] = rgb(h); return 0.2126 * r + 0.7152 * g + 0.0722 * b; };
const contrast = (a: string, b: string) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };
const oklabL = (h: string) => {
  const [r, g, b] = rgb(h);
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return 0.2104542553 * l + 0.793617813 * m - 0.0040720468 * s;
};
const block = (sel: string) => {
  const i = css.indexOf(sel);
  return css.slice(i, css.indexOf("}", i));
};
const tok = (b: string, name: string) => b.match(new RegExp(`--${name}:(#[0-9a-fA-F]{3,6})`))![1];

for (const [theme, sel] of [["light", ":root{"], ["dark", '[data-theme="dark"]{']] as const) {
  describe(`rung ramp (${theme})`, () => {
    const b = block(sel);
    const ramp = [1, 2, 3, 4, 5].map((n) => tok(b, `rung-${n}`));
    const canvas = tok(b, "canvas");
    const L = ramp.map(oklabL);
    it("lightness is monotone, in even steps of at least 0.1", () => {
      const gaps = L.slice(1).map((v, i) => Math.abs(v - L[i]));
      const dir = Math.sign(L[4] - L[0]);
      L.slice(1).forEach((v, i) => expect(Math.sign(v - L[i])).toBe(dir));
      for (const g of gaps) expect(g).toBeGreaterThanOrEqual(0.1);
      expect(Math.max(...gaps) - Math.min(...gaps)).toBeLessThan(0.02);
    });
    it("the palest rung clears the canvas at 2:1", () => {
      // Rung 1 is the end nearest the canvas in both themes (palest in light, darkest in dark).
      expect(contrast(ramp[0], canvas)).toBeGreaterThanOrEqual(2);
    });
    it("each rung's number ink reads on its fill at 4.5:1", () => {
      [1, 2, 3, 4, 5].forEach((n) => expect(contrast(tok(b, `on-rung-${n}`), ramp[n - 1])).toBeGreaterThanOrEqual(4.5));
    });
  });
}
