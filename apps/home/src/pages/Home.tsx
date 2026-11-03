// Home (spec US-H2, US-H3; mockup "home"). The rung, the rung shares, the split and the kinds of task with a
// next step come from the rung judge (design rung-judge, coaching-from-rungs D9); Uncertain judgments are shown
// on Tasks but never counted on a rung or in the split here. Until a task is judged it says so; nothing is
// guessed. Model-written text renders as plain text only (TR-5).
import { useState } from "react";
import { api } from "../api";
import { KindList, ProgressList, judgedBy, pct } from "../coach";
import { RUNGS, day, folderName, plural, rungLabel, sentence, toolName, when } from "../format";
import { AccuracyPaused, Banners, ProjectPicker, Title, UpdateLine, type Ctx } from "../ui";
import type { Project } from "../types";

export function Home(ctx: Ctx) {
  const { snap, go, announce, refresh } = ctx;
  const include = async (folder: string) => {
    const r = await api.includeFolder(folder);
    announce(r.ok ? `AIQ will watch sessions in ${folderName(folder)} again.` : r.message);
    if (r.ok) refresh();
  };
  const h = snap.home;
  const [open, setOpen] = useState<number | null>(null);

  if (h.tasks_in_window === 0 && snap.data.events === 0 && !snap.status.error) {
    return (
      <div className="page">
        <Banners {...ctx} />
        <UpdateLine {...ctx} />
        <header className="group">
          <Title>AIQ is watching quietly</Title>
          <p className="mute">Keep working as usual. Patterns show after about 10 tasks of the same kind. Tips can start sooner.</p>
        </header>
        <div className="card" style={{ maxWidth: 520 }}>
          <p>AIQ watches Claude Code, Copilot CLI and VS Code agent mode once they're connected. Setup connects the ones it finds.</p>
          <p className="meta">
            <button className="btnlink" onClick={() => go("data")}>See what AIQ has captured</button> ·{" "}
            <button className="btnlink" onClick={() => go("setup")}>See which tools are connected</button>
          </p>
        </div>
      </div>
    );
  }

  const max = Math.max(1, ...h.weeks.map((w) => w.tasks));
  const mc = h.most_common_rung;
  const onRung = h.rungs.reduce((a, b) => a + b, 0);
  const rungMax = Math.max(1, ...h.rungs);
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
      <Title sr>Home</Title>
      <ProjectPicker {...ctx} />
      {one && (
        <p className="meta" data-testid="project-note">
          Showing {one.name} only. Tips and Set aside cover all projects.
        </p>
      )}
      <section
        className="mainrung"
        role="img"
        aria-label={`Your most common rung: ${mc ? `${rungLabel(mc)}. ${RUNGS[mc - 1].means}` : "not judged yet"}. The ladder runs Assisted, Directed, Delegated, Orchestrated, Factory.`}
      >
        <div className="lbl" aria-hidden="true">
          <span className="mute">Your most common rung</span>
          {mc ? (
            <span style={{ font: "var(--t-heading)" }}>{rungLabel(mc)}</span>
          ) : (
            <span className="meta">Not judged yet</span>
          )}
          <span className="meta">
            {h.judged > 0 ? `${plural(h.judged, "task")} judged in ${h.window_days} days` : "No task judged yet"}
          </span>
          {mc && <span className="meta means">{RUNGS[mc - 1].means}</span>}
        </div>
        <div className="scale" aria-hidden="true">
          {RUNGS.map((_, i) => (
            <span key={i} className={mc === i + 1 ? "on" : undefined}>{rungLabel(i + 1)}</span>
          ))}
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
                  <b>{pct(sp.up, sp.tasks)}%</b> could move up a rung, most often to {rungLabel(sp.up_to)}.
                </p>
              )}
              <p className="meta">
                {sp.finished} of {plural(sp.tasks, "judged task")} finished by agents
                {sp.could > 0 && <> · {sp.could} could be</>} · {sp.needs} need you
                {sp.uncertain > 0 && <> · {sp.uncertain} unsure, not counted</>}
              </p>
            </>
          ) : (
            <>
              <div className="hl">
                <span style={{ font: "var(--t-title)" }}>{h.judged > 0 ? "Not enough judged tasks yet" : "Not judged yet"}</span>
              </div>
              <p className="mute">
                {h.judged > 0
                  ? `${sp.tasks} of ${c.split_min} judged tasks needed to show what agents finished${sp.uncertain > 0 ? ` (${plural(sp.uncertain, "task")} AIQ isn't sure about ${sp.uncertain === 1 ? "doesn't" : "don't"} count)` : ""}.`
                  : snap.settings.judge === "auto"
                    ? "AIQ judges your finished tasks in the background, through your own Claude Code or Copilot CLI."
                    : "Judging is off, so AIQ can't place your tasks on a rung."}
              </p>
              {h.judged === 0 && snap.settings.judge !== "auto" && (
                <p className="meta">
                  <button className="btnlink" onClick={() => go("settings")}>Turn judging on in Settings</button>
                </p>
              )}
              {h.judged > 0 && (
                <progress max={c.split_min} value={Math.min(sp.tasks, c.split_min)} aria-label={`${sp.tasks} of ${c.split_min} judged tasks`} />
              )}
            </>
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
                const say = ready && ws ? (few ? "too few judged tasks to say" : `${ws.finished} of ${ws.tasks} judged finished by agents`) : null;
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

      <section className="group" aria-labelledby="pg">
        <div className="head">
          <h2 id="pg">Is each kind of task moving up?</h2>
          <span className="meta">
            Typical rung of your judged tasks, last {c.progress_days} days against the {c.progress_days} before. Counts are measured; rungs are the judge's reading.
          </span>
        </div>
        {c.progress.length === 0 ? (
          <p className="mute">
            {h.judged === 0
              ? "Progress shows here once tasks are judged."
              : `Progress shows once AIQ knows the kind of your tasks from the last ${2 * c.progress_days} days.`}
          </p>
        ) : (
          <ProgressList c={c} open={ctx.openKind} />
        )}
      </section>

      <section className="group" aria-labelledby="ns">
        <div className="head">
          <h2 id="ns">Where agents could do more</h2>
          <span className="meta">Kinds of task you repeat, with one next step each. Most often first.</span>
        </div>
        {c.opportunities.length === 0 ? (
          <p className="mute">
            {h.judged === 0
              ? "Next steps show here once tasks are judged."
              : c.kinds.length === 0
                ? "Next steps show once AIQ knows the kind of each task. Tasks judged before this version don't have one yet."
                : "No next step yet: your kinds of task are at their right rung, too few to say, or capped by something outside your control."}
          </p>
        ) : (
          <KindList label="Kinds of task with a next step" rows={c.opportunities} open={(k) => ctx.openKind(k.job, k.task_type)} />
        )}
        {c.kinds.length > 0 && (
          <p className="meta">
            <button className="btnlink" onClick={() => go("tasks")}>All kinds of task</button>
          </p>
        )}
      </section>

      {!one && snap.projects.length > 1 && <Projects {...ctx} />}

      <section className="group" aria-labelledby="rg">
        <div className="head">
          <h2 id="rg">How you work with agents</h2>
          <span className="meta">
            {onRung > 0
              ? `Your judged tasks on each rung, last ${h.window_days} days${h.uncertain > 0 ? ` (${h.uncertain} uncertain not counted)` : ""}.`
              : "The five rungs. Your share on each shows once tasks are judged."}
          </span>
        </div>
        <div className="rungs">
          {RUNGS.map((_, i) => i).reverse().map((i) => (
            <div key={i}>
              <button
                className="rr"
                aria-label={`${rungLabel(i + 1)}, ${onRung > 0 ? plural(h.rungs[i], "task") : "not judged yet"}`}
                aria-expanded={open === i} aria-controls={`rx${i}`} onClick={() => setOpen(open === i ? null : i)}>
                <span className="chev" aria-hidden="true">›</span>
                <span>{rungLabel(i + 1)}</span>
                <span className="track" aria-hidden="true">
                  {h.rungs[i] > 0 && (
                    <span className="now" style={{ width: `${(100 * h.rungs[i]) / rungMax}%`, background: "var(--primary)" }} />
                  )}
                </span>
                <span className="v">{onRung > 0 ? plural(h.rungs[i], "task") : "Not judged yet"}</span>
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
              <button className="btn" disabled={!snap.status.core} onClick={() => include(f.folder)} aria-label={`Include ${folderName(f.folder)}`}>
                Include
              </button>
            </div>
          ))
        )}
        {h.set_aside.length > 0 && <p className="meta">If a folder is your own work, select Include and AIQ watches its sessions again.</p>}
      </section>
    </div>
  );
}

/** "Your projects" (design project-filter P5b): one row per project, most room to grow first. Selecting a name
 * shows that project; Rename keeps a name the person chose (empty goes back to the folder's name). */
function Projects(ctx: Ctx) {
  const { snap, setProject, announce, refresh } = ctx;
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const min = snap.coaching.split_min;
  const save = async (p: Project) => {
    const r = await api.setSetting(`project_name.${p.key}`, draft);
    announce(r.ok ? (draft.trim() ? `Renamed to ${draft.trim()}.` : "AIQ names this project from its folder again.") : r.message);
    if (r.ok) {
      setEditing(null);
      refresh();
    }
  };
  return (
    <section className="group" aria-labelledby="pj">
      <div className="head">
        <h2 id="pj">Your projects</h2>
        <span className="meta">
          A project is the folder you worked in. Most tasks that could move up a rung first. Counts are measured; rungs are the judge's reading.
        </span>
      </div>
      <div className="tablewrap">
        <table aria-label="Your projects">
          <thead>
            <tr>
              <th>Project</th>
              <th className="r">Tasks</th>
              <th>Most common rung</th>
              <th className="r">Agents finished</th>
              <th className="r">Could move up a rung</th>
              <th><span className="sr">Rename</span></th>
            </tr>
          </thead>
          <tbody>
            {snap.projects.map((p) => {
              const sp = p.coaching.split;
              const enough = sp.tasks >= min;
              return (
                <tr key={p.key}>
                  <td title={p.key || "Tasks from before AIQ recorded the folder, or from tools that don't send it"}>
                    {editing === p.key ? (
                      <form
                        className="row"
                        onSubmit={(e) => {
                          e.preventDefault();
                          save(p);
                        }}
                      >
                        <input
                          className="input"
                          aria-label={`New name for ${p.name}`}
                          maxLength={60}
                          value={draft}
                          placeholder={folderName(p.key)}
                          autoFocus
                          onChange={(e) => setDraft(e.target.value)}
                          onKeyDown={(e) => e.key === "Escape" && setEditing(null)}
                        />
                        <button className="btn pri" type="submit" disabled={!snap.status.core}>Save</button>
                        <button className="btn" type="button" onClick={() => setEditing(null)}>Cancel</button>
                      </form>
                    ) : (
                      <button
                        className="btnlink"
                        aria-label={`Show ${p.name} only`}
                        onClick={() => {
                          setProject(p.key);
                          // Back to the top, on the picker, so the person sees the whole page change.
                          requestAnimationFrame(() => document.getElementById("projsel")?.focus());
                        }}
                      >
                        {p.name}
                      </button>
                    )}{" "}
                    {p.by_tool.map(([t]) => (
                      <span className="chip" key={t}>{toolName(t)}</span>
                    ))}
                  </td>
                  <td className="r num">{p.tasks_in_window}</td>
                  <td className={p.most_common_rung ? undefined : "mute"}>{p.most_common_rung ? rungLabel(p.most_common_rung) : "Not judged yet"}</td>
                  <td className={`r num${enough ? "" : " mute"}`}>{enough ? `${pct(sp.finished, sp.tasks)}%` : p.judged > 0 ? "Too few judged" : "—"}</td>
                  <td className={`r num${enough ? "" : " mute"}`}>
                    {enough ? `${sp.up} (${pct(sp.up, sp.tasks)}%)` : p.judged > 0 ? "Too few judged" : "—"}
                  </td>
                  <td>
                    {p.key !== "" && editing !== p.key && (
                      <button
                        className="btnlink"
                        disabled={!snap.status.core}
                        aria-label={`Rename ${p.name}`}
                        onClick={() => {
                          setEditing(p.key);
                          setDraft(p.renamed ? p.name : "");
                        }}
                      >
                        Rename
                      </button>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <p className="meta">
        "Could move up a rung" counts judged tasks not finished by agents where the judge sees a next step at least one rung up. A project needs {min} judged tasks before AIQ shows its numbers.
      </p>
    </section>
  );
}
