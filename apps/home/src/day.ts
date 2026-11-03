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

/** Whole hours the axis spans: from the hour of the first prompt to the hour after the last, at least 2 hours.
 *  With no lanes, an empty working day. */
export function hours(ls: Lane[], key: number, offsetMin: number): [number, number] {
  if (ls.length === 0) return [9, 17];
  const a = Math.floor(Math.min(...ls.map((l) => minuteOf(l.first_ms, key, offsetMin))) / 60);
  const b = Math.ceil((Math.max(...ls.map((l) => minuteOf(l.last_ms, key, offsetMin))) + 1) / 60);
  // A task that runs past midnight extends the axis into the next day rather than being cut.
  return [a, Math.max(b, a + 2)];
}
