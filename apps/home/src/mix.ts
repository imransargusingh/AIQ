// The rung mix and the ladder map on Home (thread "Redesign the progress view"). Pure derivations over the
// snapshot: the 30-day rung counts stay the core's (`home.rungs`); the change, the weekly shares and the kinds
// on a rung are read from the same task list, so a project filter scopes them with no extra data.
import type { Coaching, KindRow, ProgressRow, TaskRow } from "./types";

export type Five = [number, number, number, number, number];
export const DAY = 86_400_000;
const zero = (): Five => [0, 0, 0, 0, 0];
const counted = (t: TaskRow) => t.judgment != null && t.judgment.state !== "uncertain";
const at = (t: TaskRow) => Math.min(5, Math.max(1, t.judgment!.current)) - 1;

/** Judged, not-Uncertain tasks per rung whose first prompt is in [from, to). */
export function rungCounts(tasks: TaskRow[], from: number, to: number): Five {
  const n = zero();
  for (const t of tasks) if (counted(t) && t.first_ms >= from && t.first_ms < to) n[at(t)] += 1;
  return n;
}

export interface Shift {
  /** Share points each rung gained (+) or lost (−), last `days` against the `days` before. */
  points: Five;
  recent: number;
  earlier: number;
}

/** The change in each rung's share, or null while either half has fewer than `min` counted tasks. */
export function shift(tasks: TaskRow[], now: number, days: number, min: number): Shift | null {
  const mid = now - days * DAY;
  const r = rungCounts(tasks, mid, now + 1);
  const e = rungCounts(tasks, mid - days * DAY, mid);
  const [rn, en] = [r, e].map((x) => x.reduce((a, b) => a + b, 0));
  if (rn < min || en < min) return null;
  return { points: r.map((v, i) => Math.round((100 * v) / rn - (100 * e[i]) / en)) as Five, recent: rn, earlier: en };
}

/** Kinds of task on one rung (index 0 = Assisted), most tasks first. Sums to that rung's count. */
export function kindsOn(tasks: TaskRow[], rung: number): { label: string; n: number }[] {
  const by = new Map<string, number>();
  for (const t of tasks) {
    if (!counted(t) || at(t) !== rung) continue;
    const k = t.judgment!.task_type_label ?? "No kind yet";
    by.set(k, (by.get(k) ?? 0) + 1);
  }
  return [...by].map(([label, n]) => ({ label, n })).sort((a, b) => b.n - a.n || a.label.localeCompare(b.label));
}

/** "Most of your work is Assisted. Delegated is growing: up 6 points." Names only what was counted. */
export function mixHeadline(rungs: Five, s: Shift | null, name: (i: number) => string, shares = true): string {
  const total = rungs.reduce((a, b) => a + b, 0);
  if (total === 0) return "";
  const top = rungs.indexOf(Math.max(...rungs));
  const share = Math.round((100 * rungs[top]) / total);
  if (!shares) return `${name(top)} is your most common rung so far.`;
  let out = share >= 50 ? `Most of your work is ${name(top)}.` : `${name(top)} is your most common rung.`;
  if (s) {
    let best = -1;
    s.points.forEach((p, i) => {
      if (Math.abs(p) >= 3 && (best < 0 || Math.abs(p) > Math.abs(s.points[best]) || (Math.abs(p) === Math.abs(s.points[best]) && i > best))) best = i;
    });
    if (best >= 0) {
      const p = s.points[best];
      out += ` ${name(best)} is ${p > 0 ? "growing" : "shrinking"}: ${p > 0 ? "up" : "down"} ${Math.abs(p)} ${Math.abs(p) === 1 ? "point" : "points"}.`;
    }
  }
  return out;
}

export interface LadderRow {
  job: string;
  task_type: string;
  label: string;
  /** Typical rung now (1–5). */
  now: number;
  /** Typical rung in the earlier half; set only when the kind moved. */
  before: number | null;
  /** Where the kind's next step reaches; set only when it is above `now`. */
  next: number | null;
  trend: ProgressRow["trend"] | null;
  /** Judged tasks last half and the half before; null when progress has no reading for the kind. */
  recent: number | null;
  earlier: number | null;
  tasks: number;
}

const key = (j: string, t: string) => `${j}/${t}`;
export const PACK_ORDER = ["code_change", "write_doc", "ask_research", "run_ship", "unclassified"];

/** Rows for the ladder map: kinds with a reading in both halves or a next step are shown; the rest are `more`.
 *  Within a pack, kinds that moved come first, then the most tasks. */
export function ladder(c: Coaching): { shown: LadderRow[]; more: LadderRow[] } {
  const kinds = new Map<string, KindRow>(c.kinds.map((k) => [key(k.job, k.task_type), k]));
  const prog = new Map<string, ProgressRow>(c.progress.map((p) => [key(p.job, p.task_type), p]));
  const keys = [...new Set([...prog.keys(), ...kinds.keys()])];
  const coach = new Set(c.opportunities.map((k) => key(k.job, k.task_type)));
  const rows: (LadderRow & { show: boolean })[] = [];
  for (const k of keys) {
    const p = prog.get(k);
    const kr = kinds.get(k);
    const now = p?.recent_rung ?? kr?.today ?? null;
    if (now == null) continue;
    const read = p != null && p.trend !== "too_few";
    const moved = read && (p.trend === "up" || p.trend === "down") && p.earlier_rung != null;
    const next = kr && kr.state === "coach" && kr.potential != null && kr.potential > now ? kr.potential : null;
    rows.push({
      job: p?.job ?? kr!.job,
      task_type: p?.task_type ?? kr!.task_type,
      label: p?.label ?? kr!.label,
      now,
      before: moved ? p.earlier_rung : null,
      next,
      trend: p?.trend ?? null,
      recent: read ? p.recent : null,
      earlier: read ? p.earlier : null,
      tasks: kr?.tasks ?? (p ? p.recent + p.earlier : 0),
      show: read || coach.has(k),
    });
  }
  const rank = (r: LadderRow) => (r.before != null ? 0 : 1);
  rows.sort(
    (a, b) =>
      PACK_ORDER.indexOf(a.job) - PACK_ORDER.indexOf(b.job) ||
      rank(a) - rank(b) ||
      (b.recent ?? b.tasks) - (a.recent ?? a.tasks) ||
      a.label.localeCompare(b.label),
  );
  const strip = ({ show: _s, ...r }: LadderRow & { show: boolean }) => r;
  return { shown: rows.filter((r) => r.show).map(strip), more: rows.filter((r) => !r.show).map(strip) };
}
