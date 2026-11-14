// Home (spec US-H2, US-H3; mockup "home"). The rung, the rung shares, the split and the kinds of task with a
// next step come from the rung judge (design rung-judge, coaching-from-rungs D9); Uncertain judgments are shown
// on Tasks but never counted on a rung or in the split here. Until a task is judged it says so; nothing is
// guessed. Model-written text renders as plain text only (TR-5).
import { api } from "../api";
import { HabitChangeLine, LadderMap, MilestoneLine, NextSteps, RungMix, judgedBy, moved, pct } from "../coach";
import { DAY, rungCounts, shift } from "../mix";
import { RUNGS, day, folderName, plural, sentence, shortDay, shortPath, toolName, when } from "../format";
import { AccuracyPaused, Banners, ProjectPicker, Rung, RungNo, Title, UpdateLine, type Ctx } from "../ui";
import type { SetAside } from "../types";
import { RightNow } from "./RightNow";
import { DroughtNote } from "../funnel";
import { GettingStartedCard, GettingStartedPage } from "./GettingStarted";
import { AnalysisCard, analysisLine } from "./Analysis";
import { SyncOffer } from "../computers";

export function Home(ctx: Ctx) {
  const { snap, go, announce, refresh } = ctx;
  const include = async (folder: string) => {
    const r = await api.includeFolder(folder);
    announce(r.ok ? `AIQ will include sessions in ${folderName(folder)} again.` : r.message);
    if (r.ok) refresh();
  };
  const h = snap.home;

  if (h.tasks_in_window === 0 && snap.data.events === 0 && !snap.status.error) {
    return (
      <div className="page">
        <Banners {...ctx} />
        <UpdateLine {...ctx} />
        <SyncOffer snap={snap} refresh={refresh} announce={announce} />
        <GettingStartedPage snap={snap} go={go} startTour={ctx.startTour} />
      </div>
    );
  }

  const max = Math.max(1, ...h.weeks.map((w) => w.tasks));
  const mc = h.most_common_rung;
  const onRung = h.rungs.reduce((a, b) => a + b, 0);
  // Shares only from the split minimum, as the rung mix below: a few tasks can't swing a percentage.
  const shares = onRung >= snap.coaching.split_min;
  const mv = moved(snap.coaching);
  const c = snap.coaching;
  const sp = c.split;
  const ready = sp.tasks >= c.split_min;
  const f = pct(sp.finished, sp.tasks);
  const fc = pct(sp.finished + sp.could, sp.tasks);
  const by = judgedBy(snap.tasks);
  const one = ctx.project === null ? undefined : snap.projects.find((p) => p.key === ctx.project);
  return (
    <div className="page">
      <Banners {...ctx} />
      <UpdateLine {...ctx} />
      <AccuracyPaused {...ctx} />
      <DroughtNote snap={snap} go={go} />
      <SyncOffer snap={snap} refresh={refresh} announce={announce} />
      <Title sr>Home</Title>
      <ProjectPicker {...ctx} />
      <RightNow {...ctx} />
      {one && (
        <p className="meta" data-testid="project-note">
          Showing {one.name} only. Tips and Automated, not counted cover all projects.
        </p>
      )}
      {ctx.project === null && !ready && <GettingStartedCard snap={snap} go={go} />}
      <section
        className="mainrung"
        role="img"
        aria-label={`Your most common rung: ${mc ? `${RUNGS[mc - 1].name}, rung ${mc} of 5. ${RUNGS[mc - 1].means} ${RUNGS.map((r, i) => `${r.name} ${plural(h.rungs[i] ?? 0, "task")}`).join(", ")}, last ${h.window_days} days.` : "not analyzed yet. The ladder runs Assisted, Directed, Delegated, Orchestrated, Factory."}`}
      >
        <div className="lbl" aria-hidden="true">
          <span className="mute">Your most common rung</span>
          {mc ? (
            <span style={{ font: "var(--t-heading)" }}>{RUNGS[mc - 1].name}</span>
          ) : (
            <span className="meta">{h.analysis.state === "off" ? "Analysis is off" : "Not analyzed yet"}</span>
          )}
          {mc && (
            <span className="meta" data-testid="rung-share">
              {shares ? `${pct(h.rungs[mc - 1], onRung)}%` : h.rungs[mc - 1]} of {plural(onRung, "task")} on a rung · last {h.window_days} days
            </span>
          )}
          <span className="meta">
            {(ctx.project === null && h.analysis.state !== "blocked" && analysisLine(h.analysis)) ||
              (h.judged > 0 ? `${plural(h.judged, "task")} analyzed in ${h.window_days} days` : "No task analyzed yet")}
            {h.uncertain > 0 && ` · ${h.uncertain} unsure, not on a rung`}
          </span>
          {mc && <span className="meta means">{RUNGS[mc - 1].means}</span>}
        </div>
        <div className="scale counted" aria-hidden="true">
          {RUNGS.map((r, i) => {
            const v = h.rungs[i] ?? 0;
            return (
              <span key={i} className={mc === i + 1 ? "on" : undefined} title={r.means}>
                <span className="step-bar"><i className={`rg-${i + 1}`} style={{ width: `${pct(v, onRung)}%` }} /></span>
                <span className="step-name"><RungNo r={i + 1} />{r.name}</span>
                <span className="step-n num">{v === 0 ? "none" : shares ? `${plural(v, "task")} · ${pct(v, onRung)}%` : plural(v, "task")}</span>
              </span>
            );
          })}
        </div>
      </section>

      <div className="top">
        <section className="card" aria-labelledby="kr">
          <h2 id="kr" className="mute" style={{ fontWeight: 400 }}>Tasks agents finished</h2>
          {ready ? (
            <>
              <div className="hl" role="img" aria-label={sp.could > 0 ? `Agents finished ${f}%, could be ${fc}%` : `Agents finished ${f}%`}>
                <span style={{ font: "var(--t-display)" }} aria-hidden="true">{f}%</span>
                {sp.could > 0 && (
                  <span className="of" aria-hidden="true">
                    could be <b>{fc}%</b>
                  </span>
                )}
              </div>
              <div className="splitbar" aria-hidden="true">
                <i style={{ width: `${f}%`, background: "var(--primary)" }} />
                {sp.could > 0 && <i style={{ width: `${fc - f}%`, background: "color-mix(in srgb,var(--primary) 35%,var(--surface))" }} />}
              </div>
              {sp.up > 0 && sp.up_to != null && (
                <p>
                  <b>{pct(sp.up, sp.tasks)}%</b> have an easy next step, most often to <Rung r={sp.up_to} />.
                </p>
              )}
              <p className="meta">
                {sp.finished} of {plural(sp.tasks, "analyzed task")} finished by agents
                {sp.could > 0 && <> · {sp.could} could be</>} · {sp.needs} need you
                {sp.uncertain > 0 && <> · {sp.uncertain} unsure, not counted</>}
              </p>
            </>
          ) : (
            <AnalysisCard {...ctx} />
          )}
          <details className="how">
            <summary className="meta">How AIQ counts</summary>
            {ready && <p className="meta">{c.rule}</p>}
            {ready && sp.could > 0 && <p className="meta">"Could be" adds the tasks of kinds whose next step would let agents finish them.</p>}
            <p className="meta">
              {by && <>{by} · </>}
              {plural(h.tasks_in_window, "task")} in the last {h.window_days} days
              {h.imported_in_window > 0 && <> · {plural(h.imported_in_window, "imported task")}</>} ·{" "}
              <button className="btnlink" onClick={() => go("tasks")}>See which</button>
            </p>
          </details>
          <figure className="wk" role="group" aria-label={ready ? "Tasks and tasks agents finished each week, last 4 weeks" : "Tasks each week, last 4 weeks"}>
            <figcaption className="wk-key">
              Tasks each week
              {ready && (
                <>
                  <span><i className="sw fin" aria-hidden="true" />Finished by agents</span>
                  <span><i className="sw rest" aria-hidden="true" />Other tasks</span>
                </>
              )}
            </figcaption>
            <div className="wk-plot">
              {h.weeks.map((w, n) => {
                const ws = c.weeks[n];
                const fin = ready && ws && ws.start_ms === w.start_ms ? ws.finished : 0;
                const few = ready && ws && ws.tasks < c.week_min;
                const say = ready && ws ? (few ? "too few analyzed tasks to say" : `${ws.finished} of ${ws.tasks} analyzed finished by agents`) : null;
                return (
                  <div
                    key={w.start_ms}
                    className="wk-col"
                    tabIndex={0}
                    role="img"
                    aria-label={`Week of ${day(w.start_ms, snap.offset_min)}: ${plural(w.tasks, "task")}${say ? `, ${say}` : ""}`}
                  >
                    {fin > 0 && !few && <i style={{ flex: `${fin} 1 0`, background: "var(--primary)", minHeight: 18 }}>{fin}</i>}
                    <i
                      style={{
                        flex: `${w.tasks - (few ? 0 : fin)} 1 0`,
                        background: ready ? "var(--wk-rest)" : "var(--primary)",
                        minHeight: w.tasks - (few ? 0 : fin) ? 18 : 0,
                        color: ready ? "var(--ink)" : undefined,
                      }}
                    >
                      {w.tasks - (few ? 0 : fin) || ""}
                    </i>
                    <i style={{ flex: `${max - w.tasks} 1 0` }} />
                    <span className="tip" aria-hidden="true">
                      <b>Week of {day(w.start_ms, snap.offset_min)}</b>
                      <br />
                      {plural(w.tasks, "task")}
                      {say && (
                        <>
                          <br />
                          {sentence(say)}
                        </>
                      )}
                    </span>
                  </div>
                );
              })}
            </div>
            <div className="wk-x" aria-hidden="true">
              {h.weeks.map((w) => (
                <span key={w.start_ms}>{day(w.start_ms, snap.offset_min).slice(4)}</span>
              ))}
            </div>
          </figure>
        </section>

        {/* Latest tips (design project-filter P6: all projects). Sized to its content, not stretched to the chart
            card beside it; with no tips it is one line. */}
        <section className={`card tips${snap.tips.length ? "" : " none"}`} aria-labelledby="nx">
          <div className="head">
            <h2 id="nx" className="mute" style={{ fontWeight: 400 }}>Your latest tips</h2>
            <button className="btnlink" onClick={() => go("tips")}>All tips</button>
          </div>
          {snap.tips.length === 0 ? (
            <p className="meta">None yet. AIQ shows one in your tool when it notices something worth changing.</p>
          ) : (
            <ul aria-label="Your latest tips">
              {snap.tips.slice(0, 4).map((t) => (
                <li key={t.card_id}>
                  <span>{sentence(t.try_this)}</span>
                  <span className="meta">
                    {when(t.delivered_ms, snap.now_ms, snap.offset_min)} · {toolName(t.tool)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </section>
      </div>

      <section className="group" aria-labelledby="rg">
        <div className="head">
          <h2 id="rg">How you work with agents</h2>
          <span className="meta">
            {onRung > 0
              ? `Analyzed tasks, last ${h.window_days} days · change: last ${c.shift_days} days against the ${c.shift_days} before`
              : "The five rungs. Your share on each shows once tasks are analyzed."}
          </span>
        </div>
        {onRung > 0 ? (
          <RungMix
            rungs={h.rungs}
            shift={shift(snap.tasks, snap.now_ms, c.shift_days, c.split_min)}
            shiftDays={c.shift_days}
            shiftMin={c.split_min}
            shareMin={c.split_min}
            weeks={h.weeks.map((w) => ({ start_ms: w.start_ms, label: day(w.start_ms, snap.offset_min), rungs: rungCounts(snap.tasks, w.start_ms, w.start_ms + 7 * DAY) }))}
            weekMin={c.week_min}
            tasks={snap.tasks}
            uncertain={h.uncertain}
            seeUncertain={() => go("tasks")}
          />
        ) : (
          <ul className="mix-lab" aria-label="The five rungs">
            {RUNGS.map((_, i) => (
              <li key={i}><Rung r={i + 1} /></li>
            ))}
          </ul>
        )}
      </section>

      <section className="group" aria-labelledby="pg">
        <div className="head">
          <h2 id="pg">Is each kind of task moving up?</h2>
          {mv.read > 0 && (
            <span className="hero">
              <b className="num">{mv.up}</b>
              <span className="meta">
                of {mv.read} {mv.read === 1 ? "kind" : "kinds"} moved up a rung{mv.down > 0 ? ` · ${mv.down} at a lower rung` : ""}
              </span>
            </span>
          )}
        </div>
        {c.progress.length === 0 && c.kinds.length === 0 ? (
          <p className="mute">
            {h.judged === 0
              ? "Progress shows here once tasks are analyzed."
              : `Progress shows once AIQ knows the kind of your tasks from the last ${2 * c.progress_days} days.`}
          </p>
        ) : (
          <>
            <LadderMap c={c} open={ctx.openKind} />
            <HabitChangeLine c={c} go={() => go("tips")} />
            <MilestoneLine c={c} nowMs={snap.now_ms} short={(ms) => shortDay(ms, snap.offset_min)} go={() => go("milestones")} />
          </>
        )}
        <div className="head">
          <h3 id="ns">Next steps</h3>
          {c.kinds.length > 0 && (
            <button className="btnlink meta" onClick={() => go("tasks")}>All kinds of task</button>
          )}
        </div>
        {c.opportunities.length === 0 ? (
          <p className="mute">
            {h.judged === 0
              ? "Next steps show here once tasks are analyzed."
              : c.kinds.length === 0
                ? "Next steps show once AIQ knows the kind of each task. Tasks analyzed before this version don't have one yet."
                : "No next step yet: your kinds of task are at their right rung, too few to say, or capped by something outside your control."}
          </p>
        ) : (
          <NextSteps rows={c.opportunities} open={(k) => ctx.openKind(k.job, k.task_type)} />
        )}
      </section>

      <section className="group" aria-labelledby="hb">
        <div className="head">
          <h2 id="hb">Automated, not counted</h2>
        </div>
        <SetAsideList {...ctx} include={include} />
      </section>
    </div>
  );
}

const WHY: Record<string, string> = {
  template: "Same first prompt each time",
  burst: "Many sessions at once",
  "aiq-check": "AIQ's own test",
};

/** Set aside (design aiq-home-app D11): what AIQ leaves out of coaching and why, with how much and when, so the
 * person can tell an app's folder from their own. AIQ's own test folders fold into one line nobody needs to open. */
function SetAsideList(ctx: Ctx & { include: (folder: string) => void }) {
  const { snap, include } = ctx;
  const all = snap.home.set_aside;
  if (all.length === 0) return <p className="mute">Nothing here yet. When an app or script drives the AI in a folder, AIQ doesn't count those runs.</p>;
  const mine = all.filter((f) => !f.own);
  const own = all.filter((f) => f.own);
  const ownRuns = own.reduce((n, f) => n + f.runs, 0);
  // Two lines a folder: which folder (name, then the path that tells same-named folders apart), then why and how much.
  const row = (f: SetAside, path: string) => {
    const last = f.last_ms !== null ? when(f.last_ms, snap.now_ms, snap.offset_min) : null;
    const facts = [
      f.own ? null : WHY[f.reason] ?? f.reason,
      f.runs > 0 ? `${plural(f.runs, "run")} left out` : null,
      last ? `last ${last.replace(/^(Today|Yesterday)/, (w) => w.toLowerCase())}` : null,
      f.kept > 0 ? `${plural(f.kept, "task")} of yours kept` : null,
    ].filter(Boolean);
    return (
      <li className="aside" key={f.folder}>
        <div className="aside-text">
          <span className="aside-name" title={f.folder}>
            <b>{f.own ? path : folderName(f.folder)}</b>
            {!f.own && <span className="meta"> {path}</span>}
          </span>
          {facts.length > 0 && <span className="meta">{facts.join(" · ")}</span>}
        </div>
        <button
          className="btn small"
          disabled={!snap.status.core}
          onClick={() => include(f.folder)}
          aria-label={`Count ${folderName(f.folder)}`}
          title="Count every session in this folder from now on"
        >
          Count this folder
        </button>
      </li>
    );
  };
  return (
    <>
      <p className="meta">AIQ leaves out runs an app or script started here. A session where you send a second prompt still counts.</p>
      {mine.length > 0 && <ul className="asides">{mine.map((f) => row(f, shortPath(f.folder)))}</ul>}
      {own.length > 0 && (
        <details className="folded">
          <summary>
            AIQ's own test folders ({own.length}{ownRuns > 0 ? `, ${plural(ownRuns, "run")}` : ""})
          </summary>
          <ul className="asides">{own.map((f) => row(f, f.folder.replace(/^.*?[\\/]te?mp[\\/]/i, "")))}</ul>
        </details>
      )}
    </>
  );
}
