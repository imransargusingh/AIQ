import { describe, expect, it } from "vitest";
import { progressSummary } from "./coach";
import type { ProgressRow } from "./types";

const r = (trend: ProgressRow["trend"]): ProgressRow => ({
  job: "code_change", task_type: "tests", label: "Writing tests", recent: 3, earlier: 3, uncertain: 0, recent_rung: 2, earlier_rung: 2, trend,
});

describe("progressSummary", () => {
  it("leads with kinds that moved up, names moves down, never guesses a move", () => {
    expect(progressSummary([r("up"), r("down"), r("same")])).toBe("1 kind of task moved up a rung. 1 moved down.");
    expect(progressSummary([r("up"), r("up")])).toBe("2 kinds of task moved up a rung.");
    expect(progressSummary([r("down"), r("down")])).toBe("No kind of task moved up. 2 kinds of task moved down.");
    expect(progressSummary([r("same"), r("too_few")])).toBe("No kind of task has moved a rung yet.");
    expect(progressSummary([])).toBe("No kind of task has moved a rung yet.");
  });
});
