import { describe, expect, it } from "vitest";
import { changeText, changesFor, habitsByRung, reaches } from "./pages/Tips";
import type { GuideMove, HabitChange, HabitRow } from "./types";

const g = (move_id: string, step: number): GuideMove => ({ move_id, step, job: "any", habit: move_id, why: "", try_this: "", starter: "", counts: "" });
const guide = [g("brief", 1), g("hand_over", 2), g("written_brief", 3)];
const h = (move_id: string, job: string, state: HabitRow["state"], seen = 0, eligible = 4): HabitRow => ({
  job, move_id, name: move_id, state, eligible, seen, moved_to: null, moved_ms: null, moved_back: false,
});

describe("habitsByRung", () => {
  it("names the rung a habit helps you reach: its guide step + 1, or null when the guide lacks the move", () => {
    expect(reaches(guide, "brief")).toBe(2);
    expect(reaches(guide, "written_brief")).toBe(4);
    expect(reaches(guide, "gone")).toBeNull();
  });

  it("groups by rung, lowest first, unknown moves last; trying, adopted, not yet inside; counts adopted", () => {
    const out = habitsByRung(
      [h("gone", "code_change", "adopted"), h("written_brief", "code_change", "not_yet"), h("hand_over", "code_change", "adopted"), h("brief", "code_change", "adopted"), h("hand_over", "write_doc", "tried")],
      guide,
    );
    expect(out.map((x) => x.rung)).toEqual([2, 3, 4, null]);
    expect(out[1].habits.map((x) => x.state)).toEqual(["tried", "adopted"]);
    expect(out.map((x) => x.adopted)).toEqual([1, 1, 0, 1]);
  });

  it("one line for a habit two jobs share in the same state, counts added", () => {
    const out = habitsByRung([h("written_brief", "code_change", "not_yet", 0, 8), h("written_brief", "write_doc", "not_yet", 0, 3)], guide);
    expect(out).toHaveLength(1);
    expect(out[0].habits).toHaveLength(1);
    expect(out[0].habits[0]).toMatchObject({ jobs: ["code_change", "write_doc"], eligible: 11 });
  });
});

describe("habit changes", () => {
  const hc = (job: string, move_id: string, dir: "up" | "down", es: number, ee: number, rs: number, re: number): HabitChange => ({
    job,
    task_type: "feature",
    label: "Adding a feature",
    move_id,
    habit: "",
    earlier_seen: es,
    earlier_eligible: ee,
    recent_seen: rs,
    recent_eligible: re,
    dir,
  });
  it("a habit line finds the changes of every job it stands for, and says them in words", () => {
    const rows: HabitRow[] = ["code_change", "write_doc"].map((job) => ({
      job,
      move_id: "written_brief",
      name: "Keep an instructions file in the repo and point to it",
      state: "not_yet",
      eligible: 3,
      seen: 0,
      moved_to: null,
      moved_ms: null,
      moved_back: false,
    }));
    const [g] = habitsByRung(rows, [{ move_id: "written_brief", step: 3 } as GuideMove]);
    const h = g.habits[0];
    const all = [hc("code_change", "written_brief", "up", 2, 12, 9, 14), hc("ask_research", "written_brief", "down", 1, 5, 0, 6)];
    expect(changesFor(h, all)).toEqual([all[0]]);
    expect(changeText(all[0])).toBe("↑ Growing in Adding a feature: 2 of 12 tasks, now 9 of 14");
    expect(changeText(hc("code_change", "brief", "down", 9, 12, 4, 15))).toBe("↓ Less often lately in Adding a feature: 9 of 12 tasks, now 4 of 15");
  });
});
