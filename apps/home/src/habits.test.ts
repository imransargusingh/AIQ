import { describe, expect, it } from "vitest";
import { habitsByRung, reaches } from "./pages/Tips";
import type { GuideMove, HabitRow } from "./types";

const g = (move_id: string, step: number): GuideMove => ({ move_id, step, job: "any", habit: move_id, why: "", try_this: "", starter: "" });
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
