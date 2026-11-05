// Rung guide (Imran 2026-11-05, option B): every habit AIQ coaches, as a grid. Rung steps run down, kinds of work
// across; pick a habit to read why, what to try and its starter beside the grid. The habits are the pack's moves
// (pack text only, never model text). Where you are comes from the kinds of task (`coaching.kinds`): a job's cell
// at its most common rung is marked "You're here", and a habit that is a kind's current next step says so.
import { useState } from "react";
import { RUNGS, jobName, sentence } from "../format";
import type { GuideMove, Snapshot } from "../types";
import { Banners, Rung, Title, type Ctx } from "../ui";

const JOBS = ["code_change", "write_doc", "ask_research", "run_ship"];
const STEPS = [1, 2, 3, 4];

const inCell = (g: GuideMove[], job: string, step: number) => g.filter((m) => m.step === step && (m.job === job || m.job === "any"));

/** A job's most common rung over its analyzed kinds of task (weighted by tasks; ties go lower), or null. */
export function jobRung(snap: Snapshot, job: string): number | null {
  const n = [0, 0, 0, 0, 0];
  for (const k of snap.coaching.kinds) if (k.job === job && k.today != null) n[k.today - 1] += k.tasks;
  const max = Math.max(...n);
  return max === 0 ? null : n.indexOf(max) + 1;
}

/** Kinds of task in `job` whose next step today is this habit. */
export const nextFor = (snap: Snapshot, m: GuideMove, job: string) =>
  snap.coaching.kinds.filter((k) => k.job === job && k.state === "coach" && k.next?.move_id === m.move_id);

const lower = (s: string) => s.charAt(0).toLowerCase() + s.slice(1);

export function Guide(ctx: Ctx) {
  const { snap, announce } = ctx;
  const g = snap.guide;
  // Open on your own next step if there is one, else the first habit of a step you're on, else the first habit.
  const first = (): [GuideMove, string] | null => {
    for (const j of JOBS) for (const m of g) if ((m.job === j || m.job === "any") && nextFor(snap, m, j).length) return [m, j];
    for (const j of JOBS) {
      const r = jobRung(snap, j);
      const m = r != null && r < 5 ? inCell(g, j, r)[0] : undefined;
      if (m) return [m, j];
    }
    return g[0] ? [g[0], g[0].job === "any" ? JOBS[0] : g[0].job] : null;
  };
  const [sel, setSel] = useState(first);
  const [copied, setCopied] = useState(false);
  const pick = (m: GuideMove, j: string) => {
    setSel([m, j]);
    setCopied(false);
  };
  const copy = async () => {
    if (!sel) return;
    try {
      await navigator.clipboard.writeText(sel[0].starter);
      setCopied(true);
      announce("Starter copied. Fill in the parts in angle brackets before you send it.");
    } catch {
      announce("Couldn't copy. Select the starter text and copy it yourself.");
    }
  };
  const [m, mj] = sel ?? [null, ""];
  const yours = m ? nextFor(snap, m, mj) : [];
  return (
    <div className="page wide">
      <Banners {...ctx} />
      <header className="group">
        <Title>Rung guide</Title>
        <p className="mute">The habits that move work up each rung, for each kind of work. Pick one to see how to try it.</p>
      </header>
      {g.length === 0 ? (
        <p className="mute">The guide shows once AIQ's core has started.</p>
      ) : (
        <div className="gd">
          <div className="card gd-grid" role="table" aria-label="Habits by rung step and kind of work">
            <div role="row" className="gd-gr gd-gh">
              <span role="columnheader" className="meta">Step</span>
              {JOBS.map((j) => {
                const r = jobRung(snap, j);
                return (
                  <span role="columnheader" key={j} className="gd-jh">
                    <b>{jobName(j)}</b>
                    <span className="meta">{r ? <>Mostly <Rung r={r} /></> : "Not analyzed yet"}</span>
                  </span>
                );
              })}
            </div>
            {STEPS.map((s) => (
              <div role="row" className="gd-gr" key={s}>
                <span role="rowheader" className="gd-sh">
                  <span aria-hidden="true">
                    <span className={`gd-r rg-${s}`}>{s}</span> → <span className={`gd-r rg-${s + 1}`}>{s + 1}</span>
                  </span>
                  <span className="meta">To {RUNGS[s].name}</span>
                  <span className="sr">{RUNGS[s - 1].name} to {RUNGS[s].name}</span>
                </span>
                {JOBS.map((j) => {
                  const here = jobRung(snap, j) === s;
                  return (
                    <span role="cell" key={j} className={here ? "gd-cell here" : "gd-cell"}>
                      <span className="gd-jn meta" aria-hidden="true">{jobName(j)}</span>
                      {here && <span className="gd-here meta">You're here</span>}
                      {inCell(g, j, s).map((x) => {
                        const on = m?.move_id === x.move_id && mj === j;
                        const next = nextFor(snap, x, j).length > 0;
                        return (
                          <button
                            key={x.move_id}
                            className={`gd-pill${on ? " on" : ""}${next ? " next" : ""}`}
                            aria-pressed={on}
                            aria-controls="gd-detail"
                            onClick={() => pick(x, j)}
                          >
                            {x.habit}
                            {next && <span className="gd-tag">Your next step</span>}
                          </button>
                        );
                      })}
                    </span>
                  );
                })}
              </div>
            ))}
          </div>
          {m && (
            <aside id="gd-detail" className="card gd-side" aria-label="The habit you picked" aria-live="polite">
              <span className="meta">
                {jobName(mj)} · <Rung r={m.step} /> → <Rung r={m.step + 1} />
              </span>
              <h2 style={{ margin: 0 }}>{m.habit}</h2>
              <p style={{ margin: 0 }}>{sentence(m.why)}</p>
              <p style={{ margin: 0 }}>
                <b>Try this:</b> {m.try_this}
              </p>
              <span id="gd-st" className="meta">A starter prompt. Fill in the parts in angle brackets; AIQ never sends it.</span>
              <pre className="starter" aria-labelledby="gd-st">{m.starter}</pre>
              <div className="row">
                <button className="btn pri" onClick={copy}>{copied ? "Copied" : "Copy starter"}</button>
              </div>
              {yours.length > 0 && (
                <p className="meta" style={{ margin: 0 }}>
                  Your next step for {yours.map((k) => lower(k.label)).join(", ")}.{" "}
                  {yours.length === 1 && (
                    <button className="btnlink" onClick={() => ctx.openKind(yours[0].job, yours[0].task_type)}>Open it</button>
                  )}
                </p>
              )}
            </aside>
          )}
        </div>
      )}
    </div>
  );
}
