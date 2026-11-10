import { describe, expect, it } from "vitest";
import { STEPS, place } from "./tour";
import type { Snapshot } from "./types";
import tour from "../tests/fixtures/tour.json";

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
  const ex = tour as unknown as Snapshot;
  it("are ten, one view each, short enough to read at a glance", () => {
    expect(STEPS).toHaveLength(10);
    expect(new Set(STEPS.map((s) => String(s.target))).size).toBe(10);
    for (const s of STEPS) expect(s.body(ex).split(" ").length).toBeLessThanOrEqual(25);
  });
  it("take every number from the snapshot, never typed in, so they follow the app's own windows and settings", () => {
    const odd = structuredClone(ex);
    odd.coaching.progress_days = 47;
    odd.coaching.shift_days = 23;
    odd.settings.daily_cap = 19;
    odd.coaching.milestone_tasks = 7;
    odd.coaching.milestone_days = 41;
    const said = STEPS.map((s) => s.body(odd)).join(" ");
    for (const n of ["47", "23", "19", "7", "41"]) expect(said).toContain(n);
    // With the numbers taken out, nothing numeric is left: any digit in a step was typed, and will drift.
    expect(said.replace(/\b(47|23|19|7|41)\b/g, "")).not.toMatch(/\d/);
  });
  it("name every tool that shows tips", () => {
    const tips = STEPS.find((s) => s.title === "Tips show where you work")!.body(ex);
    for (const t of ["Claude Code", "Copilot CLI", "VS Code"]) expect(tips).toContain(t);
  });
});
