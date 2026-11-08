import { describe, expect, it } from "vitest";
import { dayKey, daySummary, lanes, taskDays, timeline } from "./day";
import type { TaskJudgment, TaskRow } from "./types";

const H = 3_600_000;
const D0 = 20_000 * 86_400_000; // a UTC midnight
const j = (current: number, label: string | null = null) => ({ current, potential: current + 1, task_type_label: label }) as TaskJudgment;
const t = (id: string, session: string, first: number, last: number, judgment: TaskJudgment | null = null, tool = "copilot-cli"): TaskRow => ({
  task_id: id, tool, folder: "c:\\repo", first_ms: first, last_ms: last, prompts: 2, origin: "live", session, prompt_ms: [first, last],
  job: judgment ? "code_change" : null, rung: judgment?.current ?? null, judgment,
});

describe("day view", () => {
  it("groups one day's tasks into a lane per tool session, in start order", () => {
    const ts = [
      t("a", "s1", D0 + 9 * H, D0 + 10 * H, j(1, "Fixing a bug")),
      t("b", "s2", D0 + 8 * H, D0 + 8 * H),
      t("c", "s1", D0 + 14 * H, D0 + 15 * H, j(1, "Adding a feature")),
      t("d", "s1", D0 + 16 * H, D0 + 16 * H, j(2, "Fixing a bug")),
      t("e", "s1", D0 + 9 * H, D0 + 9 * H, null, "claude-code"),
      t("f", "s1", D0 + 30 * H, D0 + 30 * H),
    ];
    const ls = lanes(ts, dayKey(D0, 0), 0);
    expect(ls.map((l) => l.key)).toEqual(["copilot-cli/s2", "claude-code/s1", "copilot-cli/s1"]); // same start: by key
    const s1 = ls[2];
    expect(s1.tasks.map((x) => x.task_id)).toEqual(["a", "c", "d"]);
    expect([s1.prompts, s1.not_judged, s1.usual_rung]).toEqual([6, 0, 1]);
    expect(s1.kinds).toEqual(["Fixing a bug", "Adding a feature"]);
    expect(ls[0]).toMatchObject({ not_judged: 1, usual_rung: null, kinds: [] });
  });

  it("days follow the core's offset, newest first", () => {
    const ts = [t("a", "s", D0 + 23 * H, D0 + 23 * H), t("b", "s", D0 + 2 * H, D0 + 2 * H)];
    expect(taskDays(ts, 0)).toEqual([20_000]);
    expect(taskDays(ts, 120)).toEqual([20_001, 20_000]); // 23:00 UTC is 01:00 next day at UTC+2
  });

  it("the axis keeps stretches of work and shrinks quiet gaps over 30 minutes to one break", () => {
    const M = 60_000;
    const { stretches, pos } = timeline([
      { first_ms: D0 + 9 * H, last_ms: D0 + 9 * H + 20 * M },
      { first_ms: D0 + 9 * H + 40 * M, last_ms: D0 + 10 * H }, // 20 min after: same stretch
      { first_ms: D0 + 15 * H, last_ms: D0 + 15 * H }, // 5 h later: a new stretch, 10 units of room
    ]);
    // 60 + 10 units of work, one 15-unit break: 85 units.
    expect(stretches.map((s) => [s.a, s.b])).toEqual([[D0 + 9 * H, D0 + 10 * H], [D0 + 15 * H, D0 + 15 * H]]);
    expect(stretches[0].from).toBe(0);
    expect(stretches[0].to).toBeCloseTo((60 / 85) * 100);
    expect(stretches[1].from).toBeCloseTo((75 / 85) * 100);
    expect(pos(D0 + 9 * H + 30 * M)).toBeCloseTo((30 / 85) * 100);
    expect(pos(D0 + 15 * H)).toBeCloseTo((75 / 85) * 100);
    expect(timeline([]).pos(D0)).toBe(0);
  });

  it("sums the day: tasks, kinds, usual rung without Uncertain, minutes inside tasks, not judged", () => {
    const u = { ...j(5, "Writing tests"), state: "uncertain" } as TaskJudgment;
    const s = daySummary([
      t("a", "s", D0, D0 + H, { ...j(3, "Writing tests"), state: "judged" } as TaskJudgment),
      t("b", "s", D0 + 2 * H, D0 + 2 * H + 30 * 60_000, { ...j(2, "Fixing a bug"), state: "judged" } as TaskJudgment),
      t("c", "s", D0 + 3 * H, D0 + 3 * H, u),
      t("d", "s", D0 + 4 * H, D0 + 4 * H),
    ]);
    expect(s).toEqual({ tasks: 4, kinds: 2, usual: 2, minutes: 90, not_judged: 1 });
  });
});
