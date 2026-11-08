import { describe, expect, it } from "vitest";
import { dayWords, droughtLine, funnelInsight, stages } from "./funnel";
import type { TipFunnel } from "./types";

const f = (o: Partial<TipFunnel> = {}): TipFunnel => ({
  days: 7, tasks: 20, kind_known: 5, tip_found: 1, tips: 3, waiting: 0, expired: 1, opened: 0, answered: 0,
  no_tip: [{ code: "no_match", words: "kind of task not recognised", count: 15 }],
  held: [], shown: [{ code: "copilot-cli", words: "Copilot CLI", count: 2 }], ...o,
});
// 2026-11-08 12:00 UTC.
const NOW = Date.UTC(2026, 10, 8, 12);

describe("tip funnel", () => {
  it("says how many tasks had a tip and the main reason the rest didn't", () => {
    expect(funnelInsight(f())).toBe("1 of 20 new tasks had a tip ready this week. Main reason the others didn't: kind of task not recognised (15).");
    expect(funnelInsight(f({ tasks: 0 }))).toBe("No new tasks this week yet.");
    expect(funnelInsight(f({ tasks: 2, tip_found: 2, no_tip: [] }))).toBe("2 of 2 new tasks had a tip ready this week.");
  });

  it("marks the step before the tip that lost the most tasks", () => {
    expect(stages(f()).biggestLoss).toBe(1);
    expect(stages(f({ kind_known: 19, tip_found: 2 })).biggestLoss).toBe(2);
    expect(stages(f({ tasks: 3, kind_known: 3, tip_found: 3 })).biggestLoss).toBeNull();
    expect(stages(f()).stages.map((s) => s.n)).toEqual([20, 5, 1, 2, 0]);
  });

  it("names a dry day as yesterday or by its date", () => {
    expect(dayWords("2026-11-07", NOW, 0)).toBe("yesterday");
    expect(dayWords("2026-11-05", NOW, 0)).toBe("on Thu 5 Nov");
    expect(dayWords("2026-11-07", Date.UTC(2026, 10, 8, 3), -480)).toBe("on Sat 7 Nov"); // still the 7th in Pacific
    expect(droughtLine({ day: "2026-11-07", tasks: 6, main_reason: { code: "no_match", words: "kind of task not recognised", count: 4 } }, NOW, 0))
      .toBe("No tips yesterday across 6 tasks. Main reason: kind of task not recognised.");
  });
});
