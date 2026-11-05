// Day view on the Tasks page (design day-view; UI review 2026-11-04, item 5a): one row per tool session for one
// day, named by its kinds of task, each task a block with its rung number, coloured by rung, prompts as ticks.
// Quiet gaps over 30 minutes shrink to a break, and one line sums the day. Hover or focus a row or block for
// details; Enter or a click on a block opens the task. Rung is in text and number, never colour alone.
import { useMemo, useState } from "react";
import { clock, day, jobOf, plural, rungLabel, stateLabel, toolName } from "../format";
import { GAP_MIN, daySummary, lanes, taskDays, timeline, type Lane } from "../day";
import { projectName, projectOf } from "../project";
import type { Ctx } from "../ui";
import type { TaskRow } from "../types";

const span = (a: number, b: number, off: number) => (clock(a, off) === clock(b, off) ? clock(a, off) : `${clock(a, off)}–${clock(b, off)}`);

function TaskTip({ t, off }: { t: TaskRow; off: number }) {
  return (
    <span className="tip" role="tooltip">
      <b>{t.judgment?.task_type_label ?? jobOf(t)}</b>
      <br />
      {span(t.first_ms, t.last_ms, off)} · {plural(t.prompts, "prompt")}
      <br />
      {t.judgment ? (
        <>
          {rungLabel(t.judgment.current)}
          {t.judgment.potential > t.judgment.current && ` · could be ${rungLabel(t.judgment.potential)}`}
          {stateLabel(t.judgment.state) && ` · ${stateLabel(t.judgment.state)}`}
        </>
      ) : (
        "Not analyzed yet"
      )}
    </span>
  );
}

function LaneTip({ l, name, off }: { l: Lane; name: string; off: number }) {
  return (
    <span className="tip" role="tooltip">
      <b>
        {toolName(l.tool)} session in {name}
      </b>
      <br />
      {span(l.first_ms, l.last_ms, off)} · {plural(l.tasks.length, "task")} · {plural(l.prompts, "prompt")}
      {l.kinds.length > 0 && (
        <>
          <br />
          {l.kinds.join(", ")}
        </>
      )}
      {l.not_judged > 0 && (
        <>
          <br />
          {l.not_judged} not analyzed yet
        </>
      )}
      {l.usual_rung !== null && (
        <>
          <br />
          Mostly {rungLabel(l.usual_rung)}
        </>
      )}
    </span>
  );
}

export function DayView(ctx: Ctx) {
  const { snap } = ctx;
  const off = snap.offset_min;
  const days = useMemo(() => taskDays(snap.tasks, off), [snap.tasks, off]);
  const [picked, setPicked] = useState<number | null>(null);
  const key = picked !== null && days.includes(picked) ? picked : days[0];
  const ls = useMemo(() => (key === undefined ? [] : lanes(snap.tasks, key, off)), [snap.tasks, key, off]);
  if (key === undefined) return null;
  const all = ls.flatMap((l) => l.tasks);
  const { stretches, pos: pct } = timeline(all);
  const i = days.indexOf(key);
  const sum = daySummary(all);
  const tips = snap.tips.filter((t) => Math.floor((t.delivered_ms + off * 60_000) / 86_400_000) === key).length;
  const hm = sum.minutes >= 60 ? `${Math.floor(sum.minutes / 60)} h ${sum.minutes % 60} min` : `${sum.minutes} min`;
  // An axis label per stretch, skipped when it would sit on the one before.
  const labels = stretches.filter((s, n) => n === 0 || s.from - stretches[n - 1].from >= 7);

  return (
    <section className="group" aria-labelledby="dayhd">
      <div className="head">
        <h2 id="dayhd">{day(key * 86_400_000, 0)}</h2>
        <div className="row">
          <button className="btn" disabled={i >= days.length - 1} onClick={() => setPicked(days[i + 1])} aria-label="Earlier day">
            ‹ Earlier
          </button>
          <button className="btn" disabled={i <= 0} onClick={() => setPicked(days[i - 1])} aria-label="Later day">
            Later ›
          </button>
        </div>
      </div>
      <p className="card daysum" data-testid="day-summary">
        {plural(sum.tasks, "task")}
        {sum.kinds > 0 && <> · {plural(sum.kinds, "kind")} of task</>}
        {sum.usual !== null && (
          <>
            {" "}· mostly <b>{rungLabel(sum.usual)}</b>
          </>
        )}
        {sum.not_judged > 0 && <> · {sum.not_judged} not analyzed yet</>} · {hm} inside tasks · {plural(ls.length, "session")}
        {tips > 0 && <> · {plural(tips, "tip")} shown</>}
      </p>
      <div className="key" aria-hidden="true">
        {[1, 2, 3, 4, 5].map((r) => (
          <span key={r}>
            <i className={`dv-r${r}`} />
            {rungLabel(r)}
          </span>
        ))}
        <span>
          <i className="dv-none" />
          Not analyzed yet
        </span>
      </div>
      <div className="dv" role="list" aria-label={`Sessions on ${day(key * 86_400_000, 0)}`} >
        {ls.map((l) => {
          const name = projectName(snap, projectOf(l.tasks[0]));
          const title = l.kinds.length ? l.kinds.join(", ") : l.not_judged ? "Not analyzed yet" : "No kind yet";
          return (
            <div className="dv-lane" role="listitem" key={l.key}>
              <div className="dv-name" tabIndex={0} aria-label={`${toolName(l.tool)} session in ${name}, ${span(l.first_ms, l.last_ms, off)}, ${plural(l.tasks.length, "task")}`}>
                <span className="dv-t">
                  <b>{title}</b>
                </span>
                <span className="meta num dv-t">
                  {span(l.first_ms, l.last_ms, off)} · {toolName(l.tool)} · {name} · {plural(l.prompts, "prompt")}
                </span>
                <LaneTip l={l} name={name} off={off} />
              </div>
              <div className="dv-track">
                {stretches.slice(1).map((st) => (
                  <i key={st.a} className="dv-break" style={{ left: `${st.from}%` }} />
                ))}
                {l.tasks.map((t) => {
                  const a = pct(t.first_ms);
                  const w = Math.max(pct(t.last_ms) - a, 0);
                  const r = t.judgment?.current;
                  return (
                    <button
                      key={t.task_id}
                      className={`dv-task ${r ? `dv-r${r}` : "dv-none"}`}
                      style={{ left: `min(${a}%, calc(100% - 24px))`, width: `max(24px, ${w}%)` }}
                      aria-label={`${t.judgment?.task_type_label ?? jobOf(t)}, ${span(t.first_ms, t.last_ms, off)}, ${plural(t.prompts, "prompt")}, ${t.judgment ? rungLabel(t.judgment.current) : "not analyzed yet"}. Open task`}
                      onClick={() => ctx.openTask(t.task_id)}
                    >
                      <span className="dv-n" aria-hidden="true">{r ?? "–"}</span>
                      {t.prompt_ms.map((p, k) => (
                        <i key={k} className="dv-tick" style={{ left: w > 0 ? `${((pct(p) - a) / w) * 100}%` : "50%" }} />
                      ))}
                      <TaskTip t={t} off={off} />
                    </button>
                  );
                })}
              </div>
            </div>
          );
        })}
        <div className="dv-lane dv-axis" aria-hidden="true">
          <span />
          <div className="dv-track">
            {labels.map((st) => (
              <span key={st.a} style={{ left: `${st.from}%` }}>
                {clock(st.a, off)}
              </span>
            ))}
          </div>
        </div>
      </div>
      <p className="meta">
        One row per session, named by its kinds of task. Each block shows its rung number. Quiet gaps over {GAP_MIN} minutes shrink to a dotted break. Hover or focus a row or
        block for details; select a block to open its task.
      </p>
    </section>
  );
}
