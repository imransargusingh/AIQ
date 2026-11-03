// Shared pieces for coaching from rung results (design coaching-from-rungs, UI design table): the kinds of
// task list (Home's top 3, Tasks' full list) and the wording of each state. Every value comes from the
// snapshot; nothing is computed here but percentages and labels.
import { jobName, rungLabel } from "./format";
import type { Coaching, KindRow, KindState, ProgressRow, TaskRow } from "./types";
import { Chip, Rung } from "./ui";

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
                    <span>
                      <Rung r={k.today} /> → <Rung r={k.potential} />
                    </span>
                  </Chip>
                ) : k.today != null ? (
                  <Chip><Rung r={k.today} /></Chip>
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

const TREND: Record<ProgressRow["trend"], string> = {
  up: "Moved up",
  down: "Moved down",
  same: "Same rung",
  too_few: "Too few tasks to say",
};

/** "1 kind of task moved up a rung. 1 moved down." The success measure in one line. */
export function progressSummary(rows: ProgressRow[]): string {
  const up = rows.filter((r) => r.trend === "up").length;
  const down = rows.filter((r) => r.trend === "down").length;
  const kinds = (n: number) => `${n} ${n === 1 ? "kind" : "kinds"} of task`;
  if (up === 0 && down === 0) return "No kind of task has moved a rung yet.";
  if (up === 0) return `No kind of task moved up. ${kinds(down)} moved down.`;
  return `${kinds(up)} moved up a rung.${down > 0 ? ` ${down} moved down.` : ""}`;
}

/** Rung progress per kind of task (the success measure): rows for kinds with enough tasks in both halves,
 *  and one line for the rest. Counts are measured; rungs are the judge's reading. */
export function ProgressList({ c, open }: { c: Coaching; open: (job: string, type: string) => void }) {
  const d = c.progress_days;
  const shown = c.progress.filter((r) => r.trend !== "too_few");
  const few = c.progress.length - shown.length;
  return (
    <>
      <p>{progressSummary(c.progress)}</p>
      {shown.length > 0 && (
        <ul className="list" aria-label="Rung progress by kind of task" style={{ listStyle: "none", margin: 0, padding: 0 }}>
          {shown.map((r) => {
            const moved = r.trend !== "same" && r.earlier_rung != null && r.recent_rung != null;
            return (
              <li key={`${r.job}/${r.task_type}`}>
                <button className="opp kindrow" onClick={() => open(r.job, r.task_type)} aria-label={`${r.label}: ${TREND[r.trend]}${moved ? `, ${rungLabel(r.earlier_rung!)} to ${rungLabel(r.recent_rung!)}` : r.recent_rung != null ? `, ${rungLabel(r.recent_rung)}` : ""}`}>
                  <span className="group" style={{ gap: "var(--s2)" }}>
                    <span className="name">{r.label}</span>
                    <span className="row">
                      <Chip>{jobName(r.job)}</Chip>
                      {moved ? (
                        <Chip>
                          <span>
                            <Rung r={r.earlier_rung!} /> → <Rung r={r.recent_rung!} />
                          </span>
                        </Chip>
                      ) : r.recent_rung != null ? (
                        <Chip><Rung r={r.recent_rung} /></Chip>
                      ) : null}
                      <Chip tone={r.trend === "up" ? "ok" : r.trend === "down" ? "warn" : undefined}>{TREND[r.trend]}</Chip>
                    </span>
                  </span>
                  <span className="freq num">
                    {r.recent} in the last {d} days · {r.earlier} before
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
      {few > 0 && (
        <p className="meta">
          Too few tasks to say for {few} {few === 1 ? "kind" : "kinds"} of task: each needs at least {c.progress_min} judged tasks in both halves.
        </p>
      )}
    </>
  );
}
