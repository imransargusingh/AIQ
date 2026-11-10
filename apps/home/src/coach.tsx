// Shared pieces for coaching from rung results (design coaching-from-rungs, UI design table): the kinds of
// task list (Home's top 3, Tasks' full list) and the wording of each state. Every value comes from the
// snapshot; nothing is computed here but percentages and labels.
import { useState } from "react";
import { RUNGS, jobName, plural, rungLabel } from "./format";
import { homeLine } from "./milestones";
import { kindsOn, ladder, mixHeadline, type Five, type LadderRow, type Shift } from "./mix";
import type { Coaching, KindRow, KindState, TaskRow } from "./types";
import { Chip, Rung } from "./ui";

export const KIND_STATE: Record<KindState, string> = {
  coach: "Next step ready",
  too_few: "Too few tasks yet",
  right_rung: "Right rung",
  blocked: "Capped by a blocker",
  not_sure: "AIQ isn't sure yet",
  flagged: "You said it doesn't fit",
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
  return seen.length ? `Analyzed by your ${seen.join(" and ")}` : null;
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

/** "1 of 3 kinds of task moved up a rung", and how many moved down. Only kinds with a reading in both halves. */
export function moved(c: Coaching): { up: number; down: number; read: number } {
  const read = c.progress.filter((r) => r.trend !== "too_few");
  return { up: read.filter((r) => r.trend === "up").length, down: read.filter((r) => r.trend === "down").length, read: read.length };
}

const COLW = 76;
const H = 30;
const cx = (r: number) => COLW * (r - 0.5);

/** One kind on the five-rung scale: filled = now, ring + arrow = where it was when it moved, dashed = next step. */
function Track({ r }: { r: LadderRow }) {
  const y = H / 2;
  const up = r.before != null && r.now > r.before;
  const col = up ? "var(--success)" : "var(--warning)";
  return (
    <svg className="lad-svg" viewBox={`0 0 ${COLW * 5} ${H}`} preserveAspectRatio="none" aria-hidden="true">
      {[1, 2, 3, 4].map((i) => (
        <line key={i} x1={COLW * i} y1={4} x2={COLW * i} y2={H - 4} className="lad-grid" />
      ))}
      <line x1={cx(1)} y1={y} x2={cx(5)} y2={y} className="lad-base" />
      {r.next != null && (
        <>
          <line x1={cx(r.now) + 8} y1={y} x2={cx(r.next) - 9} y2={y} className="lad-next" />
          <circle cx={cx(r.next)} cy={y} r={7} className="lad-next-dot" />
        </>
      )}
      {r.before != null && (
        <>
          <line x1={cx(r.before) + (up ? 7 : -7)} y1={y} x2={cx(r.now) + (up ? -9 : 9)} y2={y} stroke={col} strokeWidth={2} />
          <path d={`M${cx(r.now) + (up ? -9 : 9)} ${y} l${up ? -5 : 5} -4 v8 z`} fill={col} />
          <circle cx={cx(r.before)} cy={y} r={6} className="lad-before" />
        </>
      )}
      <circle cx={cx(r.now)} cy={y} r={7} className="lad-now" />
    </svg>
  );
}

function ladderLabel(r: LadderRow, d: number): string {
  const parts = [r.label, `${rungLabel(r.now)} in the last ${d} days`];
  if (r.before != null) parts.push(`${r.now > r.before ? "moved up from" : "at a lower rung than"} ${rungLabel(r.before)}`);
  else if (r.recent != null) parts.push("same rung");
  if (r.next != null) parts.push(`next step reaches ${rungLabel(r.next)}`);
  parts.push(r.recent != null ? `${r.recent} tasks in the last ${d} days, ${r.earlier} before` : `${r.tasks} tasks, too few in one half to say if it moved`);
  return parts.join(", ");
}

function LadderRows({ rows, d, open, label }: { rows: LadderRow[]; d: number; open: (job: string, type: string) => void; label: string }) {
  const packs = [...new Set(rows.map((r) => r.job))];
  return (
    <ul className="lad" aria-label={label}>
      {packs.map((p) => (
        <li key={p}>
          <span className="lad-pack">{jobName(p)}</span>
          <ul aria-label={jobName(p)}>
            {rows
              .filter((r) => r.job === p)
              .map((r) => (
                <li key={`${r.job}/${r.task_type}`}>
                  <button className="lad-row" onClick={() => open(r.job, r.task_type)} aria-label={ladderLabel(r, d)}>
                    <span className="lad-name" title={r.label}>{r.label}</span>
                    <Track r={r} />
                    <span className="lad-n num">
                      {r.before != null ? (
                        r.now > r.before ? <b className="up">↑ up</b> : <b className="dn">↓ down</b>
                      ) : r.recent != null ? (
                        <span title={`${r.recent} in the last ${d} days · ${r.earlier} before`}>{r.recent} · {r.earlier}</span>
                      ) : (
                        <span title="Too few analyzed tasks in one half to say if it moved">{r.tasks} {r.tasks === 1 ? "task" : "tasks"}</span>
                      )}
                    </span>
                  </button>
                </li>
              ))}
          </ul>
        </li>
      ))}
    </ul>
  );
}

/** "Is each kind of task moving up?" as a ladder map: one row per kind on the five rungs, grouped by pack. */
export function LadderMap({ c, open }: { c: Coaching; open: (job: string, type: string) => void }) {
  const [all, setAll] = useState(false);
  const d = c.progress_days;
  const { shown, more } = ladder(c);
  const m = moved(c);
  return (
    <>
      <div className="lad-key meta" aria-hidden="true">
        <span><svg width="14" height="14"><circle cx="7" cy="7" r="6" className="lad-now" /></svg>Last {d} days</span>
        <span><svg width="14" height="14"><circle cx="7" cy="7" r="5" className="lad-before" /></svg>The {d} before, when it moved</span>
        <span><svg width="14" height="14"><circle cx="7" cy="7" r="5" className="lad-next-dot" /></svg>Where the next step gets you</span>
      </div>
      <div className="lad-cols meta" aria-hidden="true">
        <span />
        {RUNGS.map((r, i) => (
          <span key={i} title={r.means}>{r.name}</span>
        ))}
        <span className="lad-n" title={`Analyzed tasks, last ${d} days · the ${d} before`}>Tasks</span>
      </div>
      {shown.length > 0 && <LadderRows rows={shown} d={d} open={open} label="Rung progress by kind of task" />}
      {all && more.length > 0 && <LadderRows rows={more} d={d} open={open} label="Kinds of task with too few tasks to say" />}
      {more.length > 0 && (
        <p className="meta">
          {more.length} {shown.length > 0 ? "more " : ""}
          {more.length === 1 ? "kind" : "kinds"} of task: too few to say if {more.length === 1 ? "it" : "they"} moved. Each needs at least {c.progress_min} analyzed tasks in both halves.{" "}
          <button className="btnlink" aria-expanded={all} onClick={() => setAll(!all)}>{all ? "Hide" : "Show"}</button>
        </p>
      )}
      {m.down > 0 && <p className="sr">{m.down} {m.down === 1 ? "kind" : "kinds"} of task at a lower rung.</p>}
    </>
  );
}

/** The top kinds with a next step, one line each (the old "Where agents could do more" cards). */
export function NextSteps({ rows, open }: { rows: KindRow[]; open: (k: KindRow) => void }) {
  return (
    <ul className="nsteps" aria-label="Kinds of task with a next step">
      {rows.map((k) => (
        <li key={`${k.job}/${k.task_type}`}>
          <button className="step-row" onClick={() => open(k)} aria-label={`${k.label}: ${k.next ? `Next step: ${k.next.habit}` : KIND_STATE[k.state]}`}>
            <span className="k">{k.label}</span>
            <span className="h">{k.next ? k.next.habit : KIND_STATE[k.state]}</span>
            {k.potential != null && k.today != null && k.potential > k.today && (
              <span className="meta to">→ <Rung r={k.potential} /></span>
            )}
          </button>
        </li>
      ))}
    </ul>
  );
}

/** "How you work with agents": one sentence, the share on each rung with its change, the weekly mix, and the
 *  kinds of task on the rung you pick. Counts are measured; rungs are the judge's reading. */
export function RungMix(props: {
  rungs: Five;
  shift: Shift | null;
  shiftDays: number;
  shiftMin: number;
  /** Shares show from this many judged tasks; below it, counts only (Home never shows a % under split_min). */
  shareMin: number;
  weeks: { start_ms: number; label: string; rungs: Five }[];
  weekMin: number;
  tasks: TaskRow[];
  uncertain: number;
  seeUncertain: () => void;
}) {
  const { rungs, shift: s } = props;
  const total = rungs.reduce((a, b) => a + b, 0);
  const highest = rungs.reduce((hi, v, i) => (v > 0 ? i : hi), 0);
  const [pick, setPick] = useState<number>(highest);
  const name = (i: number) => RUNGS[i].name;
  const shares = total >= props.shareMin;
  const share = (v: number) => Math.round((100 * v) / total);
  const big = (v: number) => (v === 0 ? "—" : shares ? `${share(v)}%` : String(v));
  const on = kindsOn(props.tasks, pick);
  const label = rungs.map((v, i) => `${name(i)} ${v === 0 ? "none" : shares ? `${share(v)}%` : plural(v, "task")}`).join(", ");
  return (
    <>
      <p className="mix-head">{mixHeadline(rungs, shares ? s : null, name, shares)}</p>
      <div className="mix-wrap">
        <div className="group" style={{ gap: "var(--s3)" }}>
          <div className="rmix" role="img" aria-label={label}>
            {rungs.map((v, i) =>
              v > 0 ? (
                <span key={i} className={`rg-${i + 1}`} style={{ flex: v }} title={`${rungLabel(i + 1)}: ${plural(v, "task")}${shares ? `, ${share(v)}%` : ""}`}>
                  {v / total >= 0.04 ? i + 1 : ""}
                </span>
              ) : null,
            )}
          </div>
          <div className="mix-lab" role="group" aria-label="Rungs: select one to see its kinds of task">
            {rungs.map((v, i) => {
              const p = s?.points[i] ?? 0;
              return (
                <button key={i} className="mix-r" aria-pressed={pick === i} disabled={v === 0} onClick={() => setPick(i)}
                  aria-label={`${rungLabel(i + 1)}: ${plural(v, "task")}${v > 0 && shares ? `, ${share(v)}%` : ""}${s && (v > 0 || p !== 0) ? `, ${p === 0 ? "same share" : `${p > 0 ? "up" : "down"} ${Math.abs(p)} points`} on the ${props.shiftDays} days before` : ""}`}>
                  <span><i className={`rg-${i + 1}`} aria-hidden="true">{i + 1}</i><span title={RUNGS[i].means}>{name(i)}</span></span>
                  <b className="num">{big(v)}</b>
                  <span className="num">
                    {shares || v === 0 ? plural(v, "task") : v === 1 ? "task" : "tasks"}
                    {s && (v > 0 || p !== 0) && (p === 0 ? " · same" : ` · ${p > 0 ? "▲" : "▼"} ${Math.abs(p)} pts`)}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
        <div className="mix-weeks" role="group" aria-label="Rung mix each week">
          <span className="meta">Each week</span>
          <div className="mix-cols">
            {props.weeks.map((w) => {
              const n = w.rungs.reduce((a, b) => a + b, 0);
              const few = n < props.weekMin;
              return (
                <div key={w.start_ms} className={`mix-col${few ? " few" : ""}`} role="img"
                  aria-label={`Week of ${w.label}: ${few ? "too few analyzed tasks to say" : w.rungs.map((v, i) => (v ? `${name(i)} ${Math.round((100 * v) / n)}%` : "")).filter(Boolean).join(", ")}`}>
                  {!few && w.rungs.map((v, i) => (v > 0 ? <span key={i} className={`rg-${i + 1}`} style={{ height: `${(100 * v) / n}%` }} /> : null))}
                </div>
              );
            })}
          </div>
        </div>
      </div>
      {on.length > 0 && (
        <p className="mix-kinds">
          <span className="meta">On {name(pick)}:</span>
          {on.slice(0, 4).map((k) => (
            <Chip key={k.label}>{k.label} · {k.n}</Chip>
          ))}
          {on.length > 4 && <span className="meta">and {on.length - 4} more</span>}
        </p>
      )}
      {!s && total > 0 && (
        <p className="meta">The change shows once the last {props.shiftDays} days and the {props.shiftDays} before each have {props.shiftMin} analyzed tasks.</p>
      )}
      {props.uncertain > 0 && (
        <p className="meta">
          {props.uncertain} {props.uncertain === 1 ? "task" : "tasks"} AIQ wasn't sure about {props.uncertain === 1 ? "is" : "are"} left out ·{" "}
          <button className="btnlink" onClick={props.seeUncertain}>See them</button>
        </p>
      )}
    </>
  );
}

/** Under the ladder map: how many habits changed by more than chance, the steps before a kind moves a rung.
 *  The habits themselves live in Habits and tips. Nothing when none changed. */
export function HabitChangeLine({ c, go }: { c: Coaching; go: () => void }) {
  const up = c.habit_changes.filter((h) => h.dir === "up").length;
  const down = c.habit_changes.length - up;
  if (up + down === 0) return null;
  const parts = [up > 0 && `${up} ${up === 1 ? "habit is" : "habits are"} growing`, down > 0 && `${down} less often lately`].filter(Boolean);
  return (
    <p className="hc-line">
      <span className="hc-dot" aria-hidden="true" />
      <span>
        {parts.join(" · ")}: the steps that move a kind of task up.{" "}
        <button className="btnlink" onClick={go}>See them in Habits and tips</button>
      </span>
    </p>
  );
}

/** Under the ladder map: the latest milestone, or the closest one before the first (Milestones page). */
export function MilestoneLine({ c, nowMs, short, go }: { c: Coaching; nowMs: number; short: (ms: number) => string; go: () => void }) {
  const l = homeLine(c, nowMs, short);
  if (!l) return null;
  return (
    <p className="ms-home">
      <span className={`ms-rb rg-${l.rung}`} aria-hidden="true">{l.rung}</span>
      <span className="t"><b>{l.lead}:</b> {l.text}</span>
      <button className="btnlink" onClick={go}>{l.link}</button>
    </p>
  );
}
