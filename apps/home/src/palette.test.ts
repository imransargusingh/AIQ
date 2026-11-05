// The rung colours (thread "Make the five rung colours easy to tell apart", 2026-11-05). Imran: shades of one
// green can't be told apart, so each rung has its own hue, the same in light and dark (colour follows the rung).
// Checks are the dataviz skill's categorical gates over ALL pairs (Day view blocks can sit next to any rung):
// OKLab ΔE×100 ≥ 15 for normal vision and ≥ 8 under protan and deutan simulation (Machado 2009, severity 1),
// every fill at least 2:1 on both canvases, and each rung's number ink at least 4.5:1 on its fill.
import { describe, expect, it } from "vitest";
import css from "./styles.css?raw";

type V3 = [number, number, number];
const lin = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const rgb = (h: string): V3 => {
  const x = h.length === 4 ? h.slice(1).split("").map((d) => d + d) : [h.slice(1, 3), h.slice(3, 5), h.slice(5, 7)];
  return x.map((v) => lin(parseInt(v, 16) / 255)) as V3;
};
const lum = (h: string) => { const [r, g, b] = rgb(h); return 0.2126 * r + 0.7152 * g + 0.0722 * b; };
const contrast = (a: string, b: string) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };
const MACHADO: Record<"protan" | "deutan", V3[]> = {
  protan: [[0.152286, 1.052583, -0.204868], [0.114503, 0.786281, 0.099216], [-0.003882, -0.048116, 1.051998]],
  deutan: [[0.367322, 0.860646, -0.227968], [0.280085, 0.672501, 0.047413], [-0.01182, 0.04294, 0.968881]],
};
const sim = (c: V3, k?: keyof typeof MACHADO): V3 =>
  k ? (MACHADO[k].map((row) => Math.min(1, Math.max(0, row[0] * c[0] + row[1] * c[1] + row[2] * c[2]))) as V3) : c;
const oklab = ([r, g, b]: V3): V3 => {
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return [0.2104542553 * l + 0.793617813 * m - 0.0040720468 * s, 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s, 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s];
};
const dE = (a: string, b: string, k?: keyof typeof MACHADO) => {
  const [x, y] = [a, b].map((h) => oklab(sim(rgb(h), k)));
  return 100 * Math.hypot(x[0] - y[0], x[1] - y[1], x[2] - y[2]);
};
const block = (sel: string) => { const i = css.indexOf(sel); return css.slice(i, css.indexOf("}", i)); };
const tok = (b: string, name: string) => b.match(new RegExp(`--${name}:(#[0-9a-fA-F]{3,6})`))?.[1];

const light = block(":root{");
const dark = block('[data-theme="dark"]{');
const ramp = [1, 2, 3, 4, 5].map((n) => tok(light, `rung-${n}`)!);
const pairs = ramp.flatMap((a, i) => ramp.slice(i + 1).map((b) => [a, b] as const));

describe("rung colours", () => {
  it("are the same five in dark mode (a rung keeps its colour across themes)", () => {
    for (const n of [1, 2, 3, 4, 5]) expect(tok(dark, `rung-${n}`)).toBeUndefined();
  });
  it("every pair is far apart for normal vision (ΔE ≥ 15)", () => {
    for (const [a, b] of pairs) expect(dE(a, b), `${a} vs ${b}`).toBeGreaterThanOrEqual(15);
  });
  it("every pair stays apart for red-green colour blindness (protan and deutan ΔE ≥ 8)", () => {
    for (const [a, b] of pairs) for (const k of ["protan", "deutan"] as const) expect(dE(a, b, k), `${a} vs ${b} ${k}`).toBeGreaterThanOrEqual(8);
  });
  it("every fill shows on both canvases (2:1)", () => {
    for (const c of ramp) for (const bg of [tok(light, "canvas")!, tok(dark, "canvas")!]) expect(contrast(c, bg), `${c} on ${bg}`).toBeGreaterThanOrEqual(2);
  });
  it("each rung's number ink reads on its fill (4.5:1)", () => {
    [1, 2, 3, 4, 5].forEach((n) => expect(contrast(tok(light, `on-rung-${n}`)!, ramp[n - 1])).toBeGreaterThanOrEqual(4.5));
  });
});
