// Home (spec US-H2, US-H3; mockup "home", judge-not-connected state). No rung judge exists yet, so the
// kept rate, the rung and the next steps say "not judged yet"; every number shown is a measured count.
import { useState } from "react";
import { RUNGS, day, folderName, plural, rungLabel, sentence, toolName, topPlaces, when } from "../format";
import { Banners, Title, type Ctx } from "../ui";

export function Home(ctx: Ctx) {
  const { snap, go } = ctx;
  const h = snap.home;
  const [open, setOpen] = useState<number | null>(null);

  if (h.tasks_in_window === 0 && snap.data.events === 0 && !snap.status.error) {
    return (
      <div className="page">
        <Banners {...ctx} />
        <header className="group">
          <Title>AIQ is watching quietly</Title>
          <p className="mute">Keep working as usual. Patterns show after about 10 tasks of the same kind. Tips can start sooner.</p>
        </header>
        <div className="card" style={{ maxWidth: 520 }}>
          <p>AIQ watches Claude Code, Copilot CLI and VS Code agent mode once they're set up with <code>aiq install</code>.</p>
          <p className="meta">
            <button className="btnlink" onClick={() => go("data")}>See what AIQ has captured</button>
          </p>
        </div>
      </div>
    );
  }

  const max = Math.max(1, ...h.weeks.map((w) => w.tasks));
  const latest = snap.tips[0];
  const places = topPlaces(snap.tasks);
  return (
    <div className="page">
      <Banners {...ctx} />
      <Title sr>Home</Title>
      <section className="mainrung" role="img" aria-label="Your most common rung: not judged yet. The ladder runs Assisted, Directed, Delegated, Orchestrated, Factory.">
        <div className="lbl" aria-hidden="true">
          <span className="mute">Your most common rung</span>
          <span className="meta">Not judged yet</span>
        </div>
        <div className="scale" aria-hidden="true">
          {RUNGS.map((_, i) => (
            <span key={i}>{rungLabel(i + 1)}</span>
          ))}
        </div>
      </section>

      <div className="top">
        <section className="card" aria-labelledby="kr">
          <h2 id="kr" className="mute" style={{ fontWeight: 400 }}>Tasks agents finished</h2>
          <div className="hl">
            <span style={{ font: "var(--t-title)" }}>Not judged yet</span>
          </div>
          <p className="mute">
            AIQ doesn't judge tasks yet, so it can't say which ones agents finished. Judging comes in a later version. Tips work without it.
          </p>
          <p className="meta">
            {plural(h.tasks_in_window, "task")} in the last {h.window_days} days
            {h.imported_in_window > 0 && <> · {plural(h.imported_in_window, "imported task")}</>} ·{" "}
            <button className="btnlink" onClick={() => go("tasks")}>See which</button>
          </p>
          <figure className="wk" role="group" aria-label="Tasks each week, last 4 weeks">
            <div className="wk-y" aria-hidden="true">
              {[0, max].map((t, i) => (
                <span key={i} style={{ top: `${i === 0 ? 100 : 0}%` }}>{t}</span>
              ))}
            </div>
            <div className="wk-plot">
              {h.weeks.map((w) => (
                <div
                  key={w.start_ms}
                  className="wk-col"
                  tabIndex={0}
                  role="img"
                  aria-label={`Week of ${day(w.start_ms, snap.offset_min)}: ${plural(w.tasks, "task")}`}
                >
                  <i style={{ flex: `${w.tasks} 1 0`, background: "var(--primary)", minHeight: w.tasks ? 18 : 0 }}>{w.tasks || ""}</i>
                  <i style={{ flex: `${max - w.tasks} 1 0` }} />
                  <span className="tip" aria-hidden="true">
                    <b>Week of {day(w.start_ms, snap.offset_min)}</b>
                    <br />
                    {plural(w.tasks, "task")}
                  </span>
                </div>
              ))}
            </div>
            <div className="wk-x" aria-hidden="true">
              {h.weeks.map((w) => (
                <span key={w.start_ms}>{day(w.start_ms, snap.offset_min).slice(4)}</span>
              ))}
            </div>
          </figure>
        </section>

        <section className="card" aria-labelledby="nx">
          <h2 id="nx" className="mute" style={{ fontWeight: 400 }}>Your latest tip</h2>
          {latest ? (
            <>
              <p style={{ font: "var(--t-heading)" }}>{sentence(latest.try_this)}</p>
              <p className="mute">{sentence(latest.noticed)}</p>
              <p className="meta">
                {when(latest.delivered_ms, snap.now_ms, snap.offset_min)} · {toolName(latest.tool)}
              </p>
            </>
          ) : (
            <p className="mute">No tips yet. AIQ shows one in your tool when it notices something worth changing.</p>
          )}
          <div className="row" style={{ marginTop: "auto" }}>
            <button className="btn pri" onClick={() => go("tips")}>All tips</button>
          </div>
        </section>
      </div>

      <section className="group" aria-labelledby="opps">
        <div className="head">
          <h2 id="opps">Where you use agents most</h2>
          <button className="btnlink" onClick={() => go("tasks")}>All tasks</button>
        </div>
        <p className="meta">Where agents could do more shows here once tasks are judged.</p>
        <div className="list">
          {places.map((p) => (
            <div className="opp" key={`${p.folder}|${p.tool}`}>
              <div className="group" style={{ gap: "var(--s2)" }}>
                <span className="name" title={p.folder ?? undefined}>{p.folder ? folderName(p.folder) : "Folder not recorded"}</span>
                <span className="row">
                  <span className="chip">{toolName(p.tool)}</span>
                  <span className="chip">Not judged yet</span>
                </span>
              </div>
              <span className="freq num">
                {plural(p.tasks, "task")} · {plural(p.prompts, "prompt")}
              </span>
            </div>
          ))}
        </div>
      </section>

      <section className="group" aria-labelledby="rg">
        <div className="head">
          <h2 id="rg">How you work with agents</h2>
          <span className="meta">The five rungs. Your share on each shows once tasks are judged.</span>
        </div>
        <div className="rungs">
          {RUNGS.map((_, i) => i).reverse().map((i) => (
            <div key={i}>
              <button className="rr" aria-expanded={open === i} aria-controls={`rx${i}`} onClick={() => setOpen(open === i ? null : i)}>
                <span className="chev" aria-hidden="true">›</span>
                <span>{rungLabel(i + 1)}</span>
                <span className="track" aria-hidden="true" />
                <span className="v">Not judged yet</span>
              </button>
              <div className="rx" id={`rx${i}`} hidden={open !== i}>
                <div>
                  <h3>What it means</h3>
                  <p>{RUNGS[i].means}</p>
                </div>
              </div>
            </div>
          ))}
        </div>
      </section>

      <div className="two">
        <section className="group" aria-labelledby="hb">
          <div className="head">
            <h2 id="hb">Set aside</h2>
          </div>
          {h.set_aside.length === 0 ? (
            <p className="mute">No folders set aside. AIQ sets a folder aside when an app or script drives its sessions.</p>
          ) : (
            h.set_aside.map((f) => (
              <div className="row" key={f.folder}>
                <span className="chip">Automated</span>
                <span title={f.folder}>{folderName(f.folder)}</span>
                <span className="meta">
                  {f.reason === "burst" ? "many sessions started at once" : f.reason === "template" ? "the same first prompt over and over" : f.reason}
                </span>
              </div>
            ))
          )}
          {h.set_aside.length > 0 && <p className="meta">If that is your own work: <code>aiq include "folder"</code></p>}
        </section>
        <section className="group" aria-labelledby="rt">
          <h2 id="rt">Recent tips</h2>
          {snap.tips.length === 0 && <p className="mute">No tips yet.</p>}
          {snap.tips.slice(0, 3).map((t) => (
            <div className="head" key={t.card_id}>
              <span>{sentence(t.noticed)}</span>
              <span className="meta">
                {when(t.delivered_ms, snap.now_ms, snap.offset_min)} · {toolName(t.tool)}
              </span>
            </div>
          ))}
        </section>
      </div>
    </div>
  );
}
