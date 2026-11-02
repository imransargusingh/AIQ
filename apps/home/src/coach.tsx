// Shared pieces for coaching from rung results (design coaching-from-rungs, UI design table): the kinds of
// task list (Home's top 3, Tasks' full list) and the wording of each state. Every value comes from the
// snapshot; nothing is computed here but percentages and labels.
import { jobName, rungLabel } from "./format";
import type { KindRow, KindState, TaskRow } from "./types";
import { Chip } from "./ui";

export const KIND_STATE: Record<KindState, string> = {
  coach: "Next step ready",
  too_few: "Too few tasks yet",
  right_rung: "Right rung",
  blocked: "Capped by a blocker",
  not_sure: "AIQ isn't sure yet",
  flagged: "You flagged the step",
};

/** Whole percent of `part` in `n`. */
export const pct = (part: number, n: number) => (n > 0 ? Math.round((100 * part) / n) : 0);

/** "1.6 a week", "less than once a week". Times a week is a measured count over the window. */
export function perWeek(n: number): string {
  if (n < 1) return "less than once a week";
  const v = Number.isInteger(n) ? String(n) : n.toFixed(1);
  return `${v} a week`;
}

/** "Judged by your Copilot CLI and Claude Code", from the judgments on the tasks shown. */
export function judgedBy(tasks: TaskRow[]): string | null {
  const names = { "copilot-cli": "Copilot CLI", "claude-code": "Claude Code" } as const;
  const seen = [...new Set(tasks.flatMap((t) => (t.judgment ? [names[t.judgment.provider] ?? t.judgment.provider] : [])))].sort();
  return seen.length ? `Judged by your ${seen.join(" and ")}` : null;
}

/** One kind of task as a row: its label, job, rungs (names then numbers) and the next step's habit. */
export function KindList({ rows, open, label }: { rows: KindRow[]; open: (k: KindRow) => void; label: string }) {
  return (
    <ul className="list" aria-label={label} style={{ listStyle: "none", margin: 0, padding: 0 }}>
      {rows.map((k) => (
        <li key={`${k.job}/${k.task_type}`}>
          <button className="opp kindrow" onClick={() => open(k)} aria-label={`${k.label}: ${KIND_STATE[k.state]}`}>
            <span className="group" style={{ gap: "var(--s2)" }}>
              <span className="name">{k.label}</span>
              <span className="row">
                <Chip>{jobName(k.job)}</Chip>
                {k.today != null && k.potential != null && k.potential > k.today ? (
                  <Chip>
                    {rungLabel(k.today)} → {rungLabel(k.potential)}
                  </Chip>
                ) : k.today != null ? (
                  <Chip>{rungLabel(k.today)}</Chip>
                ) : null}
                {k.state !== "coach" && <Chip tone={k.state === "right_rung" ? "ok" : undefined}>{KIND_STATE[k.state]}</Chip>}
              </span>
            </span>
            <span className="freq num">
              {k.tasks} {k.tasks === 1 ? "task" : "tasks"} · {perWeek(k.per_week)}
            </span>
            {k.state === "coach" && k.next && <span className="next">Next step: {k.next.habit}</span>}
          </button>
        </li>
      ))}
    </ul>
  );
}
