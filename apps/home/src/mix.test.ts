import { describe, expect, it } from "vitest";
import { DAY, kindsOn, ladder, mixHeadline, rungCounts, shift, type Five } from "./mix";
import type { Snapshot, TaskRow } from "./types";
import coaching from "../tests/fixtures/coaching.json";
import full from "../tests/fixtures/full.json";
import progress from "../tests/fixtures/progress.json";

const FX = { coaching, full, progress } as unknown as Record<string, Snapshot>;
const fx = (n: string) => FX[n];
const NAMES = ["Assisted", "Directed", "Delegated", "Orchestrated", "Factory"];
const name = (i: number) => NAMES[i];

describe("rung mix agrees with the core", () => {
  for (const f of ["full", "coaching", "progress"]) {
    it(`${f}: rung counts over the window equal home.rungs, and the kinds on each rung add up to it`, () => {
      const s = fx(f);
      const n = rungCounts(s.tasks, s.now_ms - s.home.window_days * DAY, s.now_ms + 1);
      expect(n).toEqual(s.home.rungs);
      n.forEach((v, i) => expect(kindsOn(s.tasks, i).reduce((a, k) => a + k.n, 0)).toBe(v));
    });
  }
});

const t = (first_ms: number, current: number, state = "judged", label: string | null = "Writing tests"): TaskRow =>
  ({ first_ms, judgment: { current, state, task_type_label: label } }) as unknown as TaskRow;

describe("shift", () => {
  const now = 100 * DAY;
  const recent = [...Array(8)].map(() => t(now - DAY, 1)).concat([t(now - DAY, 3), t(now - DAY, 3)]);
  const earlier = [...Array(10)].map(() => t(now - 20 * DAY, 1));
  it("says how many share points each rung moved, last half against the one before", () => {
    expect(shift([...recent, ...earlier], now, 14, 10)).toEqual({ points: [-20, 0, 20, 0, 0], recent: 10, earlier: 10 });
  });
  it("says nothing while a half has too few counted tasks, and never counts Uncertain", () => {
    expect(shift([...recent, ...earlier.slice(1), t(now - 20 * DAY, 2, "uncertain")], now, 14, 10)).toBeNull();
  });
});

describe("mixHeadline", () => {
  const r: Five = [218, 19, 33, 0, 0];
  it("names the main rung, then the biggest move of 3 points or more", () => {
    expect(mixHeadline(r, null, name)).toBe("Most of your work is Assisted.");
    expect(mixHeadline(r, { points: [-5, -1, 6, 0, 0], recent: 1, earlier: 1 }, name)).toBe("Most of your work is Assisted. Delegated is growing: up 6 points.");
    expect(mixHeadline(r, { points: [2, -2, 0, 0, 0], recent: 1, earlier: 1 }, name)).toBe("Most of your work is Assisted.");
    expect(mixHeadline([3, 6, 6, 0, 0], { points: [0, 4, -4, 0, 0], recent: 1, earlier: 1 }, name)).toBe("Directed is your most common rung. Delegated is shrinking: down 4 points.");
    expect(mixHeadline([0, 0, 0, 0, 0], null, name)).toBe("");
    expect(mixHeadline([1, 2, 0, 0, 0], null, name, false)).toBe("Directed is your most common rung so far.");
  });
});

describe("ladder", () => {
  it("shows kinds with a reading or a next step, movers first in their pack, the rest as more", () => {
    const { shown, more } = ladder(fx("progress").coaching);
    expect(shown.map((r) => [r.label, r.now, r.before, r.next])).toEqual([
      ["Writing tests", 3, 2, 4],
      ["Adding a feature", 3, null, 4],
      ["Fixing a bug or failing test", 1, null, null],
      ["Writing a spec or PRD", 2, 3, 3],
    ]);
    expect(more.map((r) => r.label)).toEqual(["Refactoring or cleaning up"]);
    // Adding a feature's typical rung went 1 -> 3 by chance only: drawn where it is, no ring, no arrow.
    expect(shown.find((r) => r.label === "Adding a feature")!.trend).toBe("same");
  });
  it("a kind with a next step but too few tasks to say still shows, from its typical rung", () => {
    const { shown, more } = ladder(fx("coaching").coaching);
    expect(shown.map((r) => [r.label, r.now, r.before, r.next, r.recent])).toEqual([
      ["Writing tests", 3, null, 4, null],
      ["Writing a spec or PRD", 2, null, 3, null],
    ]);
    expect(more.map((r) => r.label).sort()).toEqual(["Fixing a bug or failing test", "Refactoring or cleaning up"]);
  });
});
