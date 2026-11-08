// Day view (design day-view): one lane per session for one local day, each task a block placed by time.
// Pure functions over the snapshot's task rows, so the Day view and the task list always show the same tasks.
import type { TaskRow } from "./types";

const DAY = 86_400_000;
const MIN = 60_000;

/** The local day a moment falls in (days since the epoch, in the core's offset). */
export const dayKey = (ms: number, offsetMin: number) => Math.floor((ms + offsetMin * MIN) / DAY);

/** Minutes after local midnight of `key`. */
export const minuteOf = (ms: number, key: number, offsetMin: number) => (ms + offsetMin * MIN - key * DAY) / MIN;

/** Days that hold at least one task, newest first. */
export function taskDays(tasks: TaskRow[], offsetMin: number): number[] {
  return [...new Set(tasks.map((t) => dayKey(t.first_ms, offsetMin)))].sort((a, b) => b - a);
}

export interface Lane {
  key: string;
  tool: string;
  folder: string | null;
  tasks: TaskRow[];
  first_ms: number;
  last_ms: number;
  prompts: number;
  /** Kinds of task in the lane, judge's labels, first seen first. */
  kinds: string[];
  not_judged: number;
  /** The rung most of the lane's judged tasks sit at (lower wins a tie); null when none is judged. */
  usual_rung: number | null;
}

/** Tasks that started on day `key`, one lane per tool session, lanes in start order. */
export function lanes(tasks: TaskRow[], key: number, offsetMin: number): Lane[] {
  const by = new Map<string, TaskRow[]>();
  for (const t of tasks) {
    if (dayKey(t.first_ms, offsetMin) !== key) continue;
    const k = `${t.tool}/${t.session}`;
    by.set(k, [...(by.get(k) ?? []), t]);
  }
  const out: Lane[] = [];
  for (const [k, ts] of by) {
    ts.sort((a, b) => a.first_ms - b.first_ms);
    const kinds: string[] = [];
    const count = new Map<number, number>();
    for (const t of ts) {
      const label = t.judgment?.task_type_label;
      if (label && !kinds.includes(label)) kinds.push(label);
      if (t.judgment) count.set(t.judgment.current, (count.get(t.judgment.current) ?? 0) + 1);
    }
    let usual: number | null = null;
    for (const [r, n] of [...count].sort((a, b) => a[0] - b[0])) if (usual === null || n > count.get(usual)!) usual = r;
    out.push({
      key: k,
      tool: ts[0].tool,
      folder: ts[0].folder,
      tasks: ts,
      first_ms: ts[0].first_ms,
      last_ms: Math.max(...ts.map((t) => t.last_ms)),
      prompts: ts.reduce((n, t) => n + t.prompts, 0),
      kinds,
      not_judged: ts.filter((t) => !t.judgment).length,
      usual_rung: usual,
    });
  }
  return out.sort((a, b) => a.first_ms - b.first_ms || a.key.localeCompare(b.key));
}

/** A stretch of the day with work in it: tasks no more than `GAP_MIN` apart. */
export interface Stretch {
  a: number;
  b: number;
  /** Where it starts and ends on the axis, 0–100. */
  from: number;
  to: number;
}

/** Quiet gaps longer than this shrink to a fixed break on the Day view's axis. */
export const GAP_MIN = 30;
/** Axis room for a break, and the least room a stretch gets, in minutes of axis. */
const BREAK_UNITS = 15;
const MIN_UNITS = 10;

/** The Day view's axis (UI review 2026-11-04, item 5a): stretches of work keep their length, quiet gaps over
 *  `GAP_MIN` minutes become a short break, so a day of short tasks doesn't spread over empty hours. `pos` maps a
 *  moment on the day to 0–100. */
export function timeline(tasks: { first_ms: number; last_ms: number }[]): { stretches: Stretch[]; pos: (ms: number) => number } {
  const spans = [...tasks].map((t) => [t.first_ms, Math.max(t.first_ms, t.last_ms)] as const).sort((x, y) => x[0] - y[0]);
  const merged: [number, number][] = [];
  for (const [a, b] of spans) {
    const last = merged[merged.length - 1];
    if (last && a - last[1] <= GAP_MIN * MIN) last[1] = Math.max(last[1], b);
    else merged.push([a, b]);
  }
  const units = merged.map(([a, b]) => Math.max((b - a) / MIN, MIN_UNITS));
  const total = units.reduce((n, u) => n + u, 0) + BREAK_UNITS * Math.max(merged.length - 1, 0) || 1;
  let at = 0;
  const stretches: Stretch[] = merged.map(([a, b], i) => {
    const from = (at / total) * 100;
    at += units[i];
    const s = { a, b, from, to: (at / total) * 100 };
    at += BREAK_UNITS;
    return s;
  });
  // Inside a stretch a minute is one unit of axis, the same scale everywhere; a short stretch keeps spare room.
  const pos = (ms: number) => {
    const s = stretches.find((x) => ms <= x.b) ?? stretches[stretches.length - 1];
    if (!s || ms <= s.a) return s ? s.from : 0;
    return Math.min(s.to, s.from + ((ms - s.a) / MIN / total) * 100);
  };
  return { stretches, pos };
}

/** The day in one line (item 5a): tasks, kinds, the usual rung of judged tasks (Uncertain left out, lower wins a
 *  tie), minutes inside tasks (first to last prompt, measured), and how many aren't judged yet. */
export function daySummary(tasks: TaskRow[]): { tasks: number; kinds: number; usual: number | null; minutes: number; not_judged: number } {
  const count = new Map<number, number>();
  for (const t of tasks) if (t.judgment && t.judgment.state !== "uncertain") count.set(t.judgment.current, (count.get(t.judgment.current) ?? 0) + 1);
  let usual: number | null = null;
  for (const [r, n] of [...count].sort((a, b) => a[0] - b[0])) if (usual === null || n > count.get(usual)!) usual = r;
  return {
    tasks: tasks.length,
    kinds: new Set(tasks.flatMap((t) => (t.judgment?.task_type_label ? [t.judgment.task_type_label] : []))).size,
    usual,
    minutes: Math.round(tasks.reduce((n, t) => n + Math.max(t.last_ms - t.first_ms, 0), 0) / MIN),
    not_judged: tasks.filter((t) => !t.judgment).length,
  };
}
