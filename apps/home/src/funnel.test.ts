import { describe, expect, it } from "vitest";
import { byReason, dayWords, droughtLine, funnelInsight, lostAt, otherLine, stages } from "./funnel";
import type { TipFunnel } from "./types";

const f = (o: Partial<TipFunnel> = {}): TipFunnel => ({
  days: 7, tasks: 20, kind_known: 5, tip_found: 1, tips: 3, waiting: 0, expired: 1, opened: 0, answered: 0,
  tip_shown: 1, tip_opened: 0, other_shown: 1, lost: [],
  no_tip: [{ code: "no_match", words: "kind of task not recognised", count: 15 }],
  held: [], shown: [{ code: "copilot-cli", words: "Copilot CLI", count: 2 }], ...o,
});
// 2026-11-08 12:00 UTC.
const NOW = Date.UTC(2026, 10, 8, 12);

describe("tip funnel", () => {
  it("says how many tasks had a tip and the main reason the rest didn't", () => {
    expect(funnelInsight(f())).toBe("1 of 20 new tasks had a tip ready this week. Most common reason: kind of task not recognised (15).");
    expect(funnelInsight(f({ tasks: 0 }))).toBe("No new tasks this week yet.");
    expect(funnelInsight(f({ tasks: 2, tip_found: 2, no_tip: [] }))).toBe("2 of 2 new tasks had a tip ready this week.");
  });

  it("marks the step before the tip that lost the most tasks", () => {
    expect(stages(f()).biggestLoss).toBe(1);
    expect(stages(f({ kind_known: 19, tip_found: 2 })).biggestLoss).toBe(2);
    expect(stages(f({ tasks: 3, kind_known: 3, tip_found: 3 })).biggestLoss).toBeNull();
    expect(stages(f()).stages.map((s) => s.n)).toEqual([20, 5, 1, 1, 0]);
  });

  it("keeps every bar within the one above, and names other shown tips apart (2026-11-09: Shown 1 under Tip ready 0)", () => {
    const laptop = f({ tasks: 6, kind_known: 0, tip_found: 0, tip_shown: 0, other_shown: 1, shown: [{ code: "copilot-cli", words: "Copilot CLI", count: 1 }] });
    const n = stages(laptop).stages.map((s) => s.n);
    expect(n).toEqual([6, 0, 0, 0, 0]);
    n.slice(1).forEach((x, i) => expect(x).toBeLessThanOrEqual(n[i]));
    expect(otherLine(laptop)).toBe("Also shown this week: 1 tip from earlier tasks or mid-task.");
    expect(otherLine(f({ other_shown: 0 }))).toBeNull();
  });

  it("lists the tasks lost at the marked bar", () => {
    const t = (id: string, kind_known: boolean) => ({ task_id: id, surface: "vscode", first_ms: NOW, code: "x", words: "x", kind_known });
    const g = f({ lost: [t("a", false), t("b", true), t("c", false)] });
    expect(lostAt(g, 1).map((x) => x.task_id)).toEqual(["a", "c"]);
    expect(lostAt(g, 2).map((x) => x.task_id)).toEqual(["b"]);
  });

  it("groups the listed tasks by reason, biggest group first, keeping each group newest first", () => {
    const t = (id: string, code: string) => ({ task_id: id, surface: "vscode", first_ms: NOW, code, words: code, kind_known: false });
    const g = byReason([t("1", "label_asked"), t("2", "label_unsure"), t("3", "label_unsure")]);
    expect(g.map((x) => [x.code, x.tasks.map((y) => y.task_id)])).toEqual([["label_unsure", ["2", "3"]], ["label_asked", ["1"]]]);
  });

  it("names a dry day as yesterday or by its date", () => {
    expect(dayWords("2026-11-07", NOW, 0)).toBe("yesterday");
    expect(dayWords("2026-11-05", NOW, 0)).toBe("on Thu 5 Nov");
    expect(dayWords("2026-11-07", Date.UTC(2026, 10, 8, 3), -480)).toBe("on Sat 7 Nov"); // still the 7th in Pacific
    expect(droughtLine({ day: "2026-11-07", tasks: 6, main_reason: { code: "no_match", words: "kind of task not recognised", count: 4 } }, NOW, 0))
      .toBe("No tips yesterday across 6 tasks. Main reason: kind of task not recognised.");
  });
});
