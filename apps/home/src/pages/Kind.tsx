// One kind of task (design coaching-from-rungs, UI table "Type detail"; aiq-home US-H4). The next step is the
// pack's text for a move AIQ derived from the judgments' facts, never the model's words. The judge's own words
// about the last task show as plain text (TR-5). "This doesn't fit" withdraws the step for this kind; Undo
// restores it. The rung is unchanged either way.
import { useState } from "react";
import { api, type FlagReason } from "../api";
import { KIND_STATE, perWeek } from "../coach";
import { jobName, plural, rungLabel, sentence, toolName, when } from "../format";
import { Banners, Chip, Title, type Ctx } from "../ui";

const REASONS: { v: FlagReason; label: string }[] = [
  { v: "evidence_wrong", label: "AIQ got these tasks wrong" },
  { v: "not_useful", label: "The step isn't useful for this work" },
  { v: "blocked", label: "Something outside my control stops me" },
  { v: "other", label: "Another reason" },
];

const NO_STEP: Record<string, string> = {
  too_few: "AIQ isn't sure yet. It suggests a step once you've done a few more tasks of this kind.",
  not_sure: "AIQ isn't sure yet. The judged tasks of this kind don't agree on one next step.",
  right_rung: "This is the right rung for this task.",
  blocked: "Something outside your control caps this kind of task, such as a missing test command or a policy. AIQ won't coach it.",
};

export function Kind(ctx: Ctx & { kindId: string }) {
  const { snap, announce, refresh, openTask, kindId } = ctx;
  const [job, type] = kindId.split("/");
  const k = snap.coaching.kinds.find((x) => x.job === job && x.task_type === type);
  const [asking, setAsking] = useState(false);
  const [reason, setReason] = useState<FlagReason | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  if (!k) {
    return (
      <div className="page">
        <Banners {...ctx} />
        <Title>Kind of task not found</Title>
        <p className="mute">AIQ has no judged tasks of this kind in the last {snap.home.window_days} days.</p>
        <p>
          <button className="btnlink" onClick={() => ctx.go("tasks")}>Back to Tasks</button>
        </p>
      </div>
    );
  }

  const flag = async (action: "flag" | "undo") => {
    if (!k.next) return;
    const r = await api.guidanceFlag(k.job, k.task_type, k.next.move_id, action, action === "flag" ? reason : null);
    if (r.ok) {
      setMsg(null);
      setAsking(false);
      setReason(null);
      announce(action === "flag" ? `AIQ won't suggest this step for ${k.label} any more. Undo is on this page.` : `The step for ${k.label} is back.`);
      refresh();
    } else {
      setMsg(r.message);
      announce(r.message);
    }
  };

  const copy = async () => {
    if (!k.next) return;
    try {
      await navigator.clipboard.writeText(k.next.starter);
      setCopied(true);
      announce("Starter copied. Fill in the parts in angle brackets before you send it.");
    } catch {
      announce("Couldn't copy. Select the starter text and copy it yourself.");
    }
  };

  const tasks = k.task_ids.map((id) => snap.tasks.find((t) => t.task_id === id)).filter((t) => t != null);

  return (
    <div className="page">
      <Banners {...ctx} />
      <header className="group">
        <p className="meta">
          <button className="btnlink" onClick={() => ctx.go("tasks")}>Tasks</button> / Kind of task
        </p>
        <Title>{k.label}</Title>
        <div className="row">
          <Chip>{jobName(k.job)}</Chip>
          <Chip>{plural(k.tasks, "task")} in {snap.home.window_days} days</Chip>
          <Chip>{perWeek(k.per_week)}</Chip>
          {k.today != null && <Chip>Today: {rungLabel(k.today)}</Chip>}
          {k.potential != null && k.today != null && k.potential > k.today && <Chip>Could be: {rungLabel(k.potential)}</Chip>}
          <Chip tone={k.state === "right_rung" ? "ok" : undefined}>{KIND_STATE[k.state]}</Chip>
        </div>
        {k.uncertain > 0 && <p className="meta">{plural(k.uncertain, "task")} AIQ isn't sure about {k.uncertain === 1 ? "is" : "are"} left out of this.</p>}
        {msg && <p className="errtext" role="alert">{msg}</p>}
      </header>

      {k.state === "coach" && k.next ? (
        <section className="card" aria-labelledby="ks">
          <h2 id="ks">Next step: {k.next.habit}</h2>
          <p>{sentence(k.next.why)}</p>
          <p>
            <b>Try this:</b> {k.next.try_this}
          </p>
          <div className="group" style={{ gap: "var(--s2)" }}>
            <span id="kst" className="mute">A starter prompt. Fill in the parts in angle brackets; AIQ never sends it.</span>
            <pre className="starter" aria-labelledby="kst">{k.next.starter}</pre>
            <div className="row">
              <button className="btn pri" onClick={copy}>{copied ? "Copied" : "Copy starter"}</button>
              {!asking && (
                <button className="btn" onClick={() => setAsking(true)}>
                  This doesn't fit
                </button>
              )}
            </div>
          </div>
          {asking && (
            <fieldset className="group" style={{ border: 0, padding: 0, margin: 0 }}>
              <legend style={{ padding: 0, marginBottom: "var(--s2)" }}>Why doesn't it fit? AIQ stops suggesting this step for {k.label}. Your rung stays the same.</legend>
              {REASONS.map((r) => (
                <label className="opt" key={r.v}>
                  <input type="radio" name="why" checked={reason === r.v} onChange={() => setReason(r.v)} />
                  <span>{r.label}</span>
                </label>
              ))}
              <div className="row">
                <button className="btn pri" disabled={!reason} onClick={() => flag("flag")}>Stop suggesting it</button>
                <button className="btn" onClick={() => setAsking(false)}>Cancel</button>
              </div>
            </fieldset>
          )}
          {k.ranked_last && <p className="meta">You marked this step not helpful twice lately, so Home lists it last.</p>}
        </section>
      ) : k.state === "flagged" && k.next ? (
        <section className="card" aria-labelledby="ks">
          <h2 id="ks">You flagged this step</h2>
          <p className="mute">AIQ doesn't suggest “{k.next.habit}” for {k.label} any more.</p>
          <div className="row">
            <button className="btn" onClick={() => flag("undo")}>Undo</button>
          </div>
        </section>
      ) : (
        <section className="card" aria-labelledby="ks">
          <h2 id="ks">{k.state === "right_rung" ? "No next step" : "No next step yet"}</h2>
          <p className="mute">{NO_STEP[k.state] ?? KIND_STATE[k.state]}</p>
        </section>
      )}

      {k.last_gap && (
        <section className="group" aria-labelledby="kg">
          <h2 id="kg">For your last task</h2>
          <p>{k.last_gap}</p>
          <p className="meta">
            The judge's own words, shown as written.{" "}
            {k.last_task_id && (
              <button className="btnlink" onClick={() => openTask(k.last_task_id!)}>Open that task</button>
            )}
          </p>
        </section>
      )}

      <section className="group" aria-labelledby="kt">
        <h2 id="kt">Tasks of this kind</h2>
        {tasks.length === 0 ? (
          <p className="mute">These tasks are older than the task list shows.</p>
        ) : (
          <ul className="list" style={{ listStyle: "none", margin: 0, padding: 0 }}>
            {tasks.map((t) => (
              <li key={t.task_id} className="head">
                <button className="btnlink" onClick={() => openTask(t.task_id)}>
                  {when(t.first_ms, snap.now_ms, snap.offset_min)} · {toolName(t.tool)}
                </button>
                <span className="meta">
                  {t.judgment ? `${rungLabel(t.judgment.current)}${t.judgment.state === "uncertain" ? " · Uncertain" : ""}` : "Not judged yet"}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
