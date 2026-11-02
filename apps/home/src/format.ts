// Pure formatting and ordering helpers. No DOM, so vitest covers them directly.
import type { TaskRow } from "./types";

export const RUNGS: { name: string; means: string }[] = [
  { name: "Assisted", means: "You ask AI for answers or pieces, and you do the work." },
  { name: "Directed", means: "You give the agent a clear goal, the context and the limits, and check what it gives you." },
  { name: "Delegated", means: "You hand the agent a whole outcome with done criteria. It makes the changes; you review." },
  { name: "Orchestrated", means: "A written spec drives several agents or steps, with automated checks between them." },
  { name: "Factory", means: "A saved, versioned workflow runs the work, for you and for others." },
];

/** Rung names first, numbers second (spec H4): "Directed (2)". */
export const rungLabel = (rung: number) => `${RUNGS[rung - 1].name} (${rung})`;

const TOOLS: Record<string, string> = { "claude-code": "Claude Code", "copilot-cli": "Copilot CLI", vscode: "VS Code" };
export const toolName = (t: string) => TOOLS[t] ?? t;

/** The folder's own name, for a table cell. The full path is the title. */
export function folderName(path: string | null): string {
  if (!path) return "—";
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

const DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const pad = (n: number) => String(n).padStart(2, "0");
const DAY = 86_400_000;

/** Local time as the core sees it: UTC ms plus the snapshot's offset. */
function local(ms: number, offsetMin: number) {
  return new Date(ms + offsetMin * 60_000);
}

export function clock(ms: number, offsetMin: number): string {
  const d = local(ms, offsetMin);
  return `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}`;
}

export function day(ms: number, offsetMin: number): string {
  const d = local(ms, offsetMin);
  return `${DAYS[d.getUTCDay()]} ${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]}`;
}

/** "Today 14:05", "Yesterday 09:12", "Sat 31 Oct 18:40". */
export function when(ms: number, nowMs: number, offsetMin: number): string {
  const dayOf = (x: number) => Math.floor((x + offsetMin * 60_000) / DAY);
  const diff = dayOf(nowMs) - dayOf(ms);
  const prefix = diff === 0 ? "Today" : diff === 1 ? "Yesterday" : day(ms, offsetMin);
  return `${prefix} ${clock(ms, offsetMin)}`;
}

export function bytes(b: number): string {
  if (b >= 1 << 20) return `${(b / (1 << 20)).toFixed(1)} MB`;
  if (b >= 1 << 10) return `${Math.round(b / (1 << 10))} KB`;
  return `${b} bytes`;
}

/** Tip text is written to follow "AIQ tip:" in a terminal, so it starts lower case; shown alone it needs a capital. */
export const sentence = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

export const plural = (n: number, one: string, many = `${one}s`) => `${n.toLocaleString("en-GB")} ${n === 1 ? one : many}`;

export type SortKey = "when" | "prompts" | "tool";
export type Dir = 1 | -1;

export function sortTasks(tasks: TaskRow[], key: SortKey, dir: Dir): TaskRow[] {
  const by: Record<SortKey, (t: TaskRow) => number | string> = {
    when: (t) => t.first_ms,
    prompts: (t) => t.prompts,
    tool: (t) => toolName(t.tool),
  };
  const f = by[key];
  return [...tasks].sort((a, b) => {
    const x = f(a), y = f(b);
    const c = x < y ? -1 : x > y ? 1 : 0;
    return c !== 0 ? c * dir : b.first_ms - a.first_ms;
  });
}

export const NOT_CLASSIFIED = "Not classified yet";
export const jobOf = (t: TaskRow) => t.job ?? NOT_CLASSIFIED;

/** Job filter options: "All" plus each job present. Buttons up to 4 jobs, a dropdown beyond (spec). */
export function jobOptions(tasks: TaskRow[]): { options: string[]; dropdown: boolean } {
  const jobs = [...new Set(tasks.map(jobOf))].sort();
  return { options: ["All", ...jobs], dropdown: jobs.length > 4 };
}

/** Where the person uses agents most: folder and tool, by number of tasks. */
export function topPlaces(tasks: TaskRow[], max = 3): { folder: string | null; tool: string; tasks: number; prompts: number }[] {
  const m = new Map<string, { folder: string | null; tool: string; tasks: number; prompts: number }>();
  for (const t of tasks) {
    const k = `${t.folder ?? ""}|${t.tool}`;
    const e = m.get(k) ?? { folder: t.folder, tool: t.tool, tasks: 0, prompts: 0 };
    e.tasks += 1;
    e.prompts += t.prompts;
    m.set(k, e);
  }
  return [...m.values()].sort((a, b) => b.tasks - a.tasks || b.prompts - a.prompts).slice(0, max);
}
