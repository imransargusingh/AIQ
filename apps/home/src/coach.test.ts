import { describe, expect, it } from "vitest";
import { moved } from "./coach";
import type { Coaching, ProgressRow } from "./types";

const r = (trend: ProgressRow["trend"]): ProgressRow => ({
  job: "code_change", task_type: "tests", label: "Writing tests", recent: 3, earlier: 3, uncertain: 0, recent_rung: 2, earlier_rung: 2, trend,
});
const c = (rows: ProgressRow[]) => ({ progress: rows }) as Coaching;

describe("moved", () => {
  it("counts kinds that moved up and down out of those with a reading, never a too-few kind", () => {
    expect(moved(c([r("up"), r("down"), r("same")]))).toEqual({ up: 1, down: 1, read: 3 });
    expect(moved(c([r("same"), r("too_few")]))).toEqual({ up: 0, down: 0, read: 1 });
    expect(moved(c([]))).toEqual({ up: 0, down: 0, read: 0 });
  });
});
