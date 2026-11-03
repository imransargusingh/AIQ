// Day view on the Tasks page (design day-view): one row per tool session for one day, each task a block
// coloured by its judged rung, prompts as ticks. Hover or focus a row or block for details; Enter or a click
// on a block opens the task. Rung is also named in text (tooltip and row summary), never colour alone.
import { useMemo, useState } from "react";
import { clock, day, jobOf, plural, rungLabel, stateLabel, toolName } from "../format";
import { hours, lanes, minuteOf, taskDays, type Lane } from "../day";
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
        "Not judged yet"
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
          {l.not_judged} not judged yet
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
  const [h0, h1] = hours(ls, key, off);
  const pct = (ms: number) => ((minuteOf(ms, key, off) - h0 * 60) / ((h1 - h0) * 60)) * 100;
  const step = h1 - h0 > 8 ? 2 : 1;
  const ticks: number[] = [];
  for (let h = h0; h <= h1; h += step) ticks.push(h);
  const i = days.indexOf(key);
  const tasks = ls.reduce((n, l) => n + l.tasks.length, 0);

  return (
    <section className="group" aria-labelledby="dayhd">
      <div className="head">
        <h2 id="dayhd">{day(key * 86_400_000, 0)}</h2>
        <div className="row">
          <span className="meta">
            {plural(ls.length, "session")} · {plural(tasks, "task")}
          </span>
          <button className="btn" disabled={i >= days.length - 1} onClick={() => setPicked(days[i + 1])} aria-label="Earlier day">
            ‹ Earlier
          </button>
          <button className="btn" disabled={i <= 0} onClick={() => setPicked(days[i - 1])} aria-label="Later day">
            Later ›
          </button>
        </div>
      </div>
      <div className="key" aria-hidden="true">
        {[1, 2, 3, 4, 5].map((r) => (
          <span key={r}>
            <i className={`dv-r${r}`} />
            {rungLabel(r)}
          </span>
        ))}
        <span>
          <i className="dv-none" />
          Not judged yet
        </span>
      </div>
      <div className="dv" role="list" aria-label={`Sessions on ${day(key * 86_400_000, 0)}`} style={{ ["--dv-hours" as string]: h1 - h0 }}>
        {ls.map((l) => {
          const name = projectName(snap, projectOf(l.tasks[0]));
          return (
            <div className="dv-lane" role="listitem" key={l.key}>
              <div className="dv-name" tabIndex={0} aria-label={`${toolName(l.tool)} session in ${name}, ${span(l.first_ms, l.last_ms, off)}, ${plural(l.tasks.length, "task")}`}>
                <span className="dv-t">
                  {toolName(l.tool)} · {name}
                </span>
                <span className="meta num">
                  {span(l.first_ms, l.last_ms, off)} · {plural(l.prompts, "prompt")}
                </span>
                <LaneTip l={l} name={name} off={off} />
              </div>
              <div className="dv-track">
                {l.tasks.map((t) => {
                  const a = pct(t.first_ms);
                  const w = Math.max(pct(t.last_ms) - a, 0);
                  const r = t.judgment?.current;
                  return (
                    <button
                      key={t.task_id}
                      className={`dv-task ${r ? `dv-r${r}` : "dv-none"}`}
                      style={{ left: `${a}%`, width: `max(8px, ${w}%)` }}
                      aria-label={`${t.judgment?.task_type_label ?? jobOf(t)}, ${span(t.first_ms, t.last_ms, off)}, ${plural(t.prompts, "prompt")}, ${t.judgment ? rungLabel(t.judgment.current) : "not judged yet"}. Open task`}
                      onClick={() => ctx.openTask(t.task_id)}
                    >
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
            {ticks.map((h) => (
              <span key={h} style={{ left: `${((h - h0) / (h1 - h0)) * 100}%` }}>
                {String(h % 24).padStart(2, "0")}:00
              </span>
            ))}
          </div>
        </div>
      </div>
      <p className="meta">One row per session. Hover or focus a row or block for details; select a block to open its task.</p>
    </section>
  );
}
