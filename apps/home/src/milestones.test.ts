import { describe, expect, it } from "vitest";
import { byMonth, headline, homeLine, record } from "./milestones";
import { scoped } from "./project";
import type { Coaching, Milestone, Snapshot } from "./types";
import full from "../tests/fixtures/full.json";
import tour from "../tests/fixtures/tour.json";

const DAY = 86_400_000;
const NOW = 400 * DAY;
const short = (ms: number) => `d${Math.round((NOW - ms) / DAY)}`;
const m = (task_type: string, kind: Milestone["kind"], rung: number, earnedAgo: number, firstAgo = earnedAgo, from_rung: number | null = null): Milestone => ({
  job: "code_change",
  task_type,
  label: `L ${task_type}`,
  kind,
  rung,
  from_rung,
  earned_ms: NOW - earnedAgo * DAY,
  first_ms: NOW - firstAgo * DAY,
});
const coaching = (milestones: Milestone[], near: Coaching["milestones_near"] = []): Coaching =>
  ({ milestones, milestones_near: near, milestone_tasks: 3, milestone_days: 30, progress_days: 30 }) as unknown as Coaching;

describe("milestone record", () => {
  // Newest first, as the core sends them.
  const c = coaching([
    m("release", "reached", 2, 2, 5),
    m("fix_bug", "reached", 3, 23, 60),
    m("feature", "moved_up", 3, 25, 70, 1),
    m("fix_bug", "reached", 2, 28, 61),
    m("tests", "moved_up", 2, 40, 50, 1),
    m("feature", "reached", 3, 70, 75),
  ]);
  const r = record(c, NOW, short);

  it("one entry per kind for its highest rung, lower rungs folded into its way here, every move kept", () => {
    expect(r.entries.map((e) => e.title)).toEqual([
      "L release reached Directed",
      "L fix_bug reached Delegated",
      "L feature moved up to Delegated",
      "L tests moved up to Directed",
      "L feature reached Delegated",
    ]);
    const fix = r.entries[1];
    expect(fix.steps.map((s) => s.text)).toEqual(["first Delegated d60", "Directed d28", "held d23"]);
    const same = record(coaching([m("x", "reached", 3, 5, 9), m("x", "reached", 2, 5, 9)]), NOW, short).entries[0];
    expect(same.steps.map((s) => s.text)).toEqual(["first Delegated d9", "held d5"]);
    expect(fix.line).toBe("3 tasks within 30 days went to the agent as a whole outcome, with done criteria.");
  });

  it("the biggest step is the move that climbed most; New marks the last 7 days", () => {
    expect(r.entries.filter((e) => e.biggest).map((e) => e.title)).toEqual(["L feature moved up to Delegated"]);
    expect(r.entries.filter((e) => e.isNew).map((e) => e.title)).toEqual(["L release reached Directed"]);
    expect(r.entries[2].line).toBe("In the 30 days to d25, most of these tasks went to the agent as a whole outcome, with done criteria. In the 30 before, most were Assisted.");
  });

  it("the hero counts kinds by their highest rung, never a kind twice", () => {
    expect(r.top).toEqual({ rung: 3, kinds: 2, first_ms: NOW - 75 * DAY });
    expect(r.byRung).toEqual([
      { rung: 2, kinds: 1 },
      { rung: 3, kinds: 2 },
      { rung: 4, kinds: 0 },
      { rung: 5, kinds: 0 },
    ]);
    expect(headline(r.top!)).toBe("2 kinds of task reached Delegated: handed to the agent as a whole outcome.");
    expect(headline({ rung: 2, kinds: 1, first_ms: 0 })).toBe("1 kind of task reached Directed: a clear goal and limits, and a checked result.");
  });

  it("months group the timeline, with the year only outside this year", () => {
    const g = byMonth(r.entries, NOW, 0);
    expect(g.flatMap((x) => x.entries)).toEqual(r.entries);
    expect(new Set(g.map((x) => x.label)).size).toBe(g.length);
    const old = byMonth(record(coaching([m("x", "reached", 2, 380)]), NOW, short).entries, NOW, 0);
    expect(old[0].label).toMatch(/ \d{4}$/);
  });

  it("Home's line: the latest milestone, else the closest, else nothing", () => {
    expect(homeLine(c, NOW, short)).toEqual({ lead: "Latest milestone", rung: 2, text: "L release reached Directed, d2.", link: "See all 5 milestones" });
    const near = coaching([], [{ job: "run_ship", task_type: "release", label: "Releasing", rung: 2, have: 2 }]);
    expect(homeLine(near, NOW, short)).toEqual({ lead: "Closest milestone", rung: 2, text: "Releasing at Directed: 2 of 3 tasks.", link: "See what counts" });
    expect(homeLine(coaching([]), NOW, short)).toBeNull();
    expect(record(coaching([]), NOW, short)).toEqual({ entries: [], top: null, byRung: [2, 3, 4, 5].map((rung) => ({ rung, kinds: 0 })) });
  });
});

describe("milestones from the Rust read model", () => {
  it("the tour store has a record; the standard store has none", () => {
    const t = tour as unknown as Snapshot;
    const r = record(t.coaching, t.now_ms, short);
    expect(r.entries.length).toBeGreaterThan(0);
    expect(r.entries.filter((e) => e.biggest).length).toBe(1);
    expect(record((full as unknown as Snapshot).coaching, NOW, short).entries).toEqual([]);
  });

  it("a project filter never hides a milestone", () => {
    const t = tour as unknown as Snapshot;
    for (const p of t.projects) {
      const s = scoped(t, p.key);
      expect(s.coaching.milestones).toEqual(t.coaching.milestones);
      expect(s.coaching.milestones_near).toEqual(t.coaching.milestones_near);
      expect(s.coaching.progress).toEqual(p.coaching.progress);
    }
  });
});
