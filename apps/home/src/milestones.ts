// Milestones (thread "Design a rung improvements record", Imran 2026-11-10, "Build as mocked"): the record of steps
// up each kind of task took. The core finds them (aiq-domain coach::milestones); this turns them into what the page
// shows. Informational, never a prize: no badges, streaks or scores, and each entry says what changed in words.
import { RUNGS } from "./format";
import type { Coaching, Milestone } from "./types";

const DAY = 86_400_000;
/** An entry earned this recently carries "New". */
export const NEW_MS = 7 * DAY;

/** What a task at each rung looked like, as the end of "3 tasks within 30 days …". Index = rung - 1. */
const DID: string[] = [
  "were Assisted",
  "had a clear goal and limits, and you checked the result",
  "went to the agent as a whole outcome, with done criteria",
  "ran from a written spec across several agents or steps",
  "ran as a saved, versioned workflow",
];
/** The hero's words for reaching a rung. Index = rung - 1. */
const REACHED: string[] = [
  "Assisted",
  "Directed: a clear goal and limits, and a checked result",
  "Delegated: handed to the agent as a whole outcome",
  "Orchestrated: a written spec driving several agents or steps",
  "Factory: a saved workflow runs the work",
];

export interface Step {
  rung: number;
  text: string;
  ms: number;
}

export interface Entry {
  key: string;
  m: Milestone;
  title: string;
  line: string;
  /** The way here: lower rungs this kind reached, its first task at this rung, the day it held. Empty for a move. */
  steps: Step[];
  isNew: boolean;
  biggest: boolean;
}

export interface MilestoneRecord {
  entries: Entry[];
  /** Highest rung any kind reached, with how many kinds have it as their highest, and the first such task. */
  top: { rung: number; kinds: number; first_ms: number } | null;
  /** Kinds by the highest rung each reached, rungs 2 to 5. */
  byRung: { rung: number; kinds: number }[];
}

const name = (r: number) => RUNGS[r - 1]?.name ?? `Rung ${r}`;

/** One entry per kind for its highest reached rung (lower rungs fold into its steps), plus every typical-rung move.
 *  Newest first, as the core sends them. The biggest step is the move that climbed most (newest on a tie). */
export function record(c: Coaching, nowMs: number, short: (ms: number) => string): MilestoneRecord {
  const kindKey = (m: Milestone) => `${m.job}/${m.task_type}`;
  const reached = c.milestones.filter((m) => m.kind === "reached");
  const highest = new Map<string, Milestone>();
  for (const m of reached) {
    const h = highest.get(kindKey(m));
    if (!h || m.rung > h.rung) highest.set(kindKey(m), m);
  }
  const moves = c.milestones.filter((m) => m.kind === "moved_up");
  let biggest: Milestone | null = null;
  for (const m of moves) {
    const climb = m.rung - (m.from_rung ?? m.rung);
    if (!biggest || climb > biggest.rung - (biggest.from_rung ?? biggest.rung)) biggest = m;
  }
  const entries: Entry[] = [];
  for (const m of c.milestones) {
    const isNew = nowMs - m.earned_ms < NEW_MS;
    if (m.kind === "moved_up") {
      const from = m.from_rung ?? 1;
      entries.push({
        key: `move-${kindKey(m)}-${from}-${m.rung}`,
        m,
        title: `${m.label} moved up to ${name(m.rung)}`,
        line: `In the ${c.progress_days} days to ${short(m.earned_ms)}, most of these tasks ${DID[m.rung - 1]}. In the ${c.progress_days} before, most were ${name(from)}.`,
        steps: [],
        isNew,
        biggest: m === biggest,
      });
      continue;
    }
    if (highest.get(kindKey(m)) !== m) continue;
    // A lower rung held on the same tasks (the same day or later) adds nothing; the rest go in date order.
    const lower = reached
      .filter((x) => kindKey(x) === kindKey(m) && x.rung < m.rung && x.earned_ms < m.earned_ms)
      .map((x) => ({ rung: x.rung, text: `${name(x.rung)} ${short(x.earned_ms)}`, ms: x.earned_ms }));
    const own =
      m.first_ms < m.earned_ms
        ? [{ rung: m.rung, text: `first ${name(m.rung)} ${short(m.first_ms)}`, ms: m.first_ms }, { rung: m.rung, text: `held ${short(m.earned_ms)}`, ms: m.earned_ms }]
        : [{ rung: m.rung, text: `${name(m.rung)} ${short(m.earned_ms)}`, ms: m.earned_ms }];
    const steps = [...lower, ...own].sort((a, b) => a.ms - b.ms || a.rung - b.rung);
    entries.push({
      key: `reached-${kindKey(m)}-${m.rung}`,
      m,
      title: `${m.label} reached ${name(m.rung)}`,
      line: `${c.milestone_tasks} tasks within ${c.milestone_days} days ${DID[m.rung - 1]}.`,
      steps,
      isNew,
      biggest: false,
    });
  }
  const tops = [...highest.values()];
  const topRung = tops.reduce((a, m) => Math.max(a, m.rung), 0);
  const atTop = tops.filter((m) => m.rung === topRung);
  return {
    entries,
    top: topRung >= 2 ? { rung: topRung, kinds: atTop.length, first_ms: Math.min(...atTop.map((m) => m.first_ms)) } : null,
    byRung: [2, 3, 4, 5].map((rung) => ({ rung, kinds: tops.filter((m) => m.rung === rung).length })),
  };
}

/** "5 kinds of task reached Delegated: handed to the agent as a whole outcome." */
export function headline(top: NonNullable<MilestoneRecord["top"]>): string {
  return `${top.kinds === 1 ? "1 kind of task" : `${top.kinds} kinds of task`} reached ${REACHED[top.rung - 1]}.`;
}

/** Month groups for the timeline, newest first: "November", or "November 2025" outside the current year. */
export function byMonth(entries: Entry[], nowMs: number, offsetMin: number): { label: string; entries: Entry[] }[] {
  const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
  const local = (ms: number) => new Date(ms + offsetMin * 60_000);
  const year = local(nowMs).getUTCFullYear();
  const out: { label: string; entries: Entry[] }[] = [];
  for (const e of entries) {
    const d = local(e.m.earned_ms);
    const label = d.getUTCFullYear() === year ? MONTHS[d.getUTCMonth()] : `${MONTHS[d.getUTCMonth()]} ${d.getUTCFullYear()}`;
    const last = out[out.length - 1];
    if (last && last.label === label) last.entries.push(e);
    else out.push({ label, entries: [e] });
  }
  return out;
}

/** Home's one line under the ladder map: the latest milestone, or the closest one before the first. Null when there
 *  is neither (no line rather than an empty promise). */
export function homeLine(c: Coaching, nowMs: number, short: (ms: number) => string): { lead: string; rung: number; text: string; link: string } | null {
  const r = record(c, nowMs, short);
  const e = r.entries[0];
  if (e) return { lead: "Latest milestone", rung: e.m.rung, text: `${e.title}, ${short(e.m.earned_ms)}.`, link: r.entries.length === 1 ? "See your milestone" : `See all ${r.entries.length} milestones` };
  const n = c.milestones_near[0];
  if (n) return { lead: "Closest milestone", rung: n.rung, text: `${n.label} at ${name(n.rung)}: ${n.have} of ${c.milestone_tasks} tasks.`, link: "See what counts" };
  return null;
}
