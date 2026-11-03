import { describe, expect, it } from "vitest";
import { dayKey, hours, lanes, taskDays } from "./day";
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

  it("the axis covers first to last prompt in whole hours, at least two, past midnight when a task runs over", () => {
    const k = dayKey(D0, 0);
    expect(hours(lanes([t("a", "s", D0 + 9.5 * H, D0 + 9.6 * H)], k, 0), k, 0)).toEqual([9, 11]);
    expect(hours(lanes([t("a", "s", D0 + 22 * H, D0 + 25 * H)], k, 0), k, 0)).toEqual([22, 26]);
    expect(hours([], k, 0)).toEqual([9, 17]);
  });
});
