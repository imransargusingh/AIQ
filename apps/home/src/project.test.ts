import { describe, expect, it } from "vitest";
import { scoped } from "./project";
import type { Snapshot } from "./types";
import fixture from "../tests/fixtures/coaching.json";

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
    expect(s.coaching).toBe(p.coaching);
    expect(s.tips).toBe(snap.tips);
  });
  it("projects add up to every project", () => {
    expect(snap.projects.reduce((n, p) => n + p.tasks_in_window, 0)).toBe(snap.home.tasks_in_window);
  });
});
