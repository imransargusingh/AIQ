import { describe, expect, it } from "vitest";
import { STEPS, place } from "./tour";

const pop = { w: 320, h: 200 };

describe("tour coach mark placement", () => {
  it("goes below the target when it fits", () => {
    expect(place({ left: 400, top: 100, width: 600, height: 200 }, pop, 1280, 800)).toEqual({ left: 400, top: 322 });
  });
  it("goes above when below would leave the window", () => {
    expect(place({ left: 400, top: 500, width: 600, height: 200 }, pop, 1280, 800)).toEqual({ left: 400, top: 278 });
  });
  it("goes beside a target taller than the window, kept on screen", () => {
    expect(place({ left: 380, top: 40, width: 500, height: 900 }, pop, 1280, 800)).toEqual({ left: 902, top: 40 });
  });
  it("sits centred with no target", () => {
    expect(place(null, pop, 1280, 800)).toEqual({ left: 480, top: 300 });
  });
  it("never leaves the window on the right", () => {
    expect(place({ left: 1200, top: 100, width: 60, height: 40 }, pop, 1280, 800).left).toBe(1280 - 320 - 16);
  });
});

describe("tour steps", () => {
  it("are eight, one view each, short enough to read at a glance", () => {
    expect(STEPS).toHaveLength(8);
    expect(new Set(STEPS.map((s) => s.target)).size).toBe(8);
    for (const s of STEPS) expect(s.body.split(" ").length).toBeLessThanOrEqual(25);
  });
});
