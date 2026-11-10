// Milestones (thread "Design a rung improvements record", Imran 2026-11-10, "Build as mocked"): a dated record of
// each step up a kind of task took, for pride in the work. A step counts only when it held (3 tasks at the rung
// within 30 days) or when the kind's typical rung moved by more than chance; one lucky task never counts, and a
// milestone stays once earned. Informational, never a prize: no badges, streaks or scores. Rungs carry their name
// and number, not colour alone.
import { RUNGS, shortDay } from "../format";
import { byMonth, headline, record } from "../milestones";
import { Banners, Title, type Ctx } from "../ui";

export function Milestones(ctx: Ctx) {
  const { snap, go } = ctx;
  const c = snap.coaching;
  const fmt = (ms: number) => shortDay(ms, snap.offset_min);
  const r = record(c, snap.now_ms, fmt);
  const name = (n: number) => RUNGS[n - 1].name;
  return (
    <div className="page">
      <Banners {...ctx} />
      <header className="group">
        <Title>Milestones</Title>
        <p className="mute">
          Each step up one kind of task took. A step counts when {c.milestone_tasks} analyzed tasks of one kind reach a rung within{" "}
          {c.milestone_days} days, so one lucky task never counts. Milestones stay once earned.
        </p>
      </header>
      {r.top ? (
        <section className="ms-hero" aria-labelledby="ms-head">
          <div>
            <h2 id="ms-head">{headline(r.top)}</h2>
            <p className="mute">Before {fmt(r.top.first_ms)}, none had.</p>
          </div>
          <div className="ms-climb" role="img" aria-label={`Kinds of task by the highest rung each reached: ${r.byRung.map((b) => `${name(b.rung)} ${b.kinds}`).join(", ")}`}>
            {r.byRung.map((b) => (
              <div key={b.rung}>
                <span className="ms-n">{b.kinds > 0 ? b.kinds : " "}</span>
                <i className={`rg-${b.rung}${b.kinds > 0 ? " on" : ""}`} style={{ height: 24 + b.rung * 18 }} />
                <span>{name(b.rung)}</span>
              </div>
            ))}
          </div>
        </section>
      ) : (
        <section className="ms-empty" aria-labelledby="ms-head">
          <div>
            <h2 id="ms-head">
              {c.milestones_near.length > 0 ? "Your first milestone is close." : snap.home.judged === 0 ? "Milestones show here once tasks are analyzed." : "No milestones yet."}
            </h2>
            <p className="mute">
              A milestone comes when {c.milestone_tasks} tasks of one kind reach a higher rung within {c.milestone_days} days.
              {c.milestones_near.length > 0 && " Here is where you stand."}
            </p>
            <p>
              <button className="btnlink" onClick={() => go("guide")}>What moves a task up a rung</button>
            </p>
          </div>
          {c.milestones_near.length > 0 && (
            <ul className="ms-near" aria-label="Closest milestones">
              {c.milestones_near.map((n) => (
                <li key={`${n.job}/${n.task_type}`}>
                  <span>
                    {n.label} · {name(n.rung)} ({n.rung})
                  </span>
                  <span className="ms-pips" role="img" aria-label={`${n.have} of ${c.milestone_tasks} tasks`}>
                    {Array.from({ length: c.milestone_tasks }, (_, i) => (
                      <i key={i} className={i < n.have ? `rg-${n.rung}` : undefined} style={{ borderColor: `var(--rung-${n.rung})` }} />
                    ))}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}
      {byMonth(r.entries, snap.now_ms, snap.offset_min).map((g) => (
        <section className="group" key={g.label} aria-label={g.label}>
          <h3 className="ms-month">{g.label}</h3>
          <ul className="ms-list">
            {g.entries.map((e) => (
              <li key={e.key} className={`ms${e.biggest ? " top" : ""}`}>
                <span className={`ms-rb rg-${e.m.rung}`} aria-hidden="true">{e.m.rung}</span>
                <div>
                  <h4>
                    <button className="btnlink ms-kind" onClick={() => ctx.openKind(e.m.job, e.m.task_type)}>{e.title}</button>
                    {e.isNew && <span className="ms-tag new">New</span>}
                    {e.biggest && <span className="ms-tag">Biggest step</span>}
                  </h4>
                  <p className="mute">{e.line}</p>
                  {e.steps.length > 1 && (
                    <p className="ms-path">
                      {e.steps.map((s, i) => (
                        <span key={i}>
                          {i > 0 && <span aria-hidden="true">→ </span>}
                          <i className={`rg-${s.rung}`} aria-hidden="true" />
                          {s.text}
                        </span>
                      ))}
                    </p>
                  )}
                </div>
                <span className="meta ms-when">{fmt(e.m.earned_ms)}</span>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
