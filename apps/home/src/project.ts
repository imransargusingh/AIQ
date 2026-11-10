// The project filter (design project-filter P3–P5): the read model computes every project's numbers; this
// swaps them in so Home, Tasks and a kind of task's page render one project with the same code as all.
import type { Snapshot, TaskRow } from "./types";

/** The project key of a task: its folder, "" when none was recorded (matches `Project.key`). */
export const projectOf = (t: TaskRow) => t.folder ?? "";

/** The snapshot as one project sees it; `null` or a project no longer listed gives every project. */
export function scoped(s: Snapshot, key: string | null): Snapshot {
  const p = key === null ? undefined : s.projects.find((x) => x.key === key);
  if (!p) return s;
  return {
    ...s,
    home: {
      ...s.home,
      tasks_in_window: p.tasks_in_window,
      judged: p.judged,
      uncertain: p.uncertain,
      rungs: p.rungs,
      most_common_rung: p.most_common_rung,
      imported_in_window: p.imported_in_window,
      weeks: p.weeks,
      by_tool: p.by_tool,
    },
    // Milestones are the person's whole record: a project filter never hides one.
    coaching: {
      ...p.coaching,
      milestones: s.coaching.milestones,
      milestones_near: s.coaching.milestones_near,
      milestone_tasks: s.coaching.milestone_tasks,
      milestone_days: s.coaching.milestone_days,
    },
    tasks: s.tasks.filter((t) => projectOf(t) === p.key),
  };
}

/** A project's name by key, for the Tasks list. */
export function projectName(s: Snapshot, key: string): string {
  return s.projects.find((p) => p.key === key)?.name ?? "Folder unknown";
}
