import { describe, expect, it } from "vitest";
import { scoped } from "./project";
import type { Snapshot } from "./types";
import fixture from "../tests/fixtures/coaching.json";
import noRung from "../tests/fixtures/no-rung.json";

const snap = fixture as unknown as Snapshot;

describe("project filter", () => {
  it("all projects is the snapshot itself", () => {
    expect(scoped(snap, null)).toBe(snap);
    expect(scoped(snap, "no such folder")).toBe(snap);
  });
  it("one project swaps in its own numbers and tasks", () => {
    const p = snap.projects[1];
    const s = scoped(snap, p.key);
    expect(s.home.tasks_in_window).toBe(p.tasks_in_window);
    expect(s.tasks).toHaveLength(p.tasks_in_window);
    expect(s.tasks.every((t) => t.folder === p.key)).toBe(true);
    expect(s.coaching).toEqual({ ...p.coaching, milestones: snap.coaching.milestones, milestones_near: snap.coaching.milestones_near });
    expect(s.tips).toBe(snap.tips);
  });
  it("projects add up to every project", () => {
    expect(snap.projects.reduce((n, p) => n + p.tasks_in_window, 0)).toBe(snap.home.tasks_in_window);
  });
  // The tasks with no rung of their own are one of Home's numbers, so a project swaps in its own count: the
  // fixture's three are all in one project, and the other project has none.
  it("one project's analyzed tasks add up: on a rung, unsure, or with no rung of their own", () => {
    const all = noRung as unknown as Snapshot;
    const sum = (ns: number[]) => ns.reduce((a, b) => a + b, 0);
    expect(all.home.no_rung).toEqual({ progress_check: 2, release_notice: 1 });
    for (const p of all.projects) {
      const h = scoped(all, p.key).home;
      expect([p.name, sum(h.rungs) + h.uncertain + sum(Object.values(h.no_rung))]).toEqual([p.name, h.judged]);
      expect(h.no_rung).toEqual(p.coaching.split.no_rung);
    }
  });
  // The read model gives a project each of its own numbers under the name Home uses for it (its tally is taken
  // apart in full for both), so the fixture's own keys say what the picker must swap in.
  it("one project swaps in every number it carries under one of Home's names", () => {
    const all = noRung as unknown as Snapshot;
    for (const p of all.projects) {
      const [own, h] = [p as unknown as Record<string, unknown>, scoped(all, p.key).home as unknown as Record<string, unknown>];
      const shared = Object.keys(own).filter((k) => k in all.home);
      expect(shared).toContain("no_rung");
      for (const k of shared) expect([k, h[k]]).toEqual([k, own[k]]);
    }
  });
});
