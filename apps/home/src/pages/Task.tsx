// Task detail (spec US-H4; design aiq-home-deferred-pages E8): one task's facts and its prompts in order.
// The judgment (design rung-judge) shows the rung, why, one next step and what was read; until the task is
// judged it says so. Model-written text renders as plain text only (TR-5).
import { useEffect, useState } from "react";
import { api } from "../api";
import { clock, folderName, jobName, plural, rungLabel, stateLabel, toolName, when } from "../format";
import type { TaskDetail } from "../types";
import { Banners, Chip, Content, Sk, Title, preview, type Ctx } from "../ui";

type State = { kind: "loading" } | { kind: "ok"; d: TaskDetail } | { kind: "gone" } | { kind: "error"; message: string };

export function Task(ctx: Ctx & { taskId: string }) {
  const { snap, go, taskId } = ctx;
  const [st, setSt] = useState<State>({ kind: "loading" });
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let live = true;
    setSt({ kind: "loading" });
    api.task(taskId).then(
      (d) => live && setSt(d ? { kind: "ok", d } : { kind: "gone" }),
      (e) => live && setSt({ kind: "error", message: String(e?.message ?? e) }),
    );
    return () => {
      live = false;
    };
  }, [taskId, attempt]);

  // Esc goes back to Tasks, as in the mockup, unless a field or dialog has it.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (e.key === "Escape" && !t.closest("input,textarea,select,dialog")) go("tasks");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [go]);

  const back = (
    <a
      href="#tasks"
      className="meta backlink"
      aria-keyshortcuts="Escape"
      onClick={(e) => {
        e.preventDefault();
        go("tasks");
      }}
    >
      ← Tasks <kbd aria-hidden="true">Esc</kbd>
    </a>
  );

  if (st.kind === "loading") {
    return (
      <div className="page" aria-busy="true">
        <Banners {...ctx} />
        {back}
        <Title>Task</Title>
        <div className="card">
          <Sk w="40%" />
          <Sk w="90%" />
          <Sk w="70%" />
        </div>
      </div>
    );
  }
  if (st.kind !== "ok") {
    return (
      <div className="page">
        <Banners {...ctx} />
        {back}
        <Title>Task</Title>
        {st.kind === "gone" ? (
          <div className="empty">
            <b>This task isn't listed any more.</b>
            <span className="mute">It may now be older than {snap.home.window_days} days, or its data was deleted.</span>
          </div>
        ) : (
          <div className="banner bad" role="alert">
            <div className="grow">
              <b>AIQ can't read this task right now.</b> {st.message}
            </div>
            <button className="btn" onClick={() => setAttempt((n) => n + 1)}>Try again</button>
          </div>
        )}
      </div>
    );
  }

  const { task: t, prompts } = st.d;
  const j = t.judgment;
  const first = prompts.find((p) => p.text)?.text;
  const minutes = Math.round((t.last_ms - t.first_ms) / 60_000);
  return (
    <div className="page">
      <Banners {...ctx} />
      {back}
      <header className="group">
        <Title>{first ? preview(first) : "Task (prompt text deleted)"}</Title>
        <p className="mute">
          {toolName(t.tool)} · started {when(t.first_ms, snap.now_ms, snap.offset_min)}
          {minutes > 0 && <>, {plural(minutes, "minute")}</>} · {plural(t.prompts, "prompt")}
          {" · "}
          <span title={t.folder ?? undefined}>{t.folder ? folderName(t.folder) : "Folder not recorded"}</span>
          {t.origin === "imported" && <> · <Chip>Imported</Chip></>}
        </p>
      </header>

      {j ? (
        <section className="card group" aria-labelledby="jd">
          <h2 id="jd">How this task was done</h2>
          <div className="row">
            <Chip>Rung: {rungLabel(j.current)}</Chip>
            {stateLabel(j.state) && <Chip>{stateLabel(j.state)}</Chip>}
            <Chip>Job: {jobName(j.job)}</Chip>
          </div>
          {j.state === "uncertain" && <p className="mute">AIQ isn't sure about this one, so it won't coach on it.</p>}
          <ul className="evidence">
            {j.evidence.map((e, i) => (
              <li key={i}>{e}</li>
            ))}
          </ul>
          {j.potential > j.current && j.gap && (
            <p>
              <b>Next step ({rungLabel(j.current)} → {rungLabel(j.potential)}):</b> {j.gap}
            </p>
          )}
          {j.blockers.length > 0 && <p className="mute">Outside your control: {j.blockers.join(" ")}</p>}
          <p className="meta">
            Judged {when(j.judged_ms, snap.now_ms, snap.offset_min)} by your {toolName(j.provider)}
            {j.model && <> ({j.model})</>}. It read {plural(j.read_prompts, "prompt")}, {plural(j.read_chars, "character")}
            {j.read_actions ? " and a summary of the agent's actions" : ""}.
          </p>
        </section>
      ) : (
        <div className="empty">
          <b>Not judged yet.</b>
          <span className="mute">
            {snap.settings.judge === "auto"
              ? "AIQ judges finished tasks in the background, through your own Claude Code or Copilot CLI."
              : "Run aiq judge once to start; it shows what it sends first."}
          </span>
        </div>
      )}

      <section className="group" aria-labelledby="tp">
        <h2 id="tp">Your prompts in this task</h2>
        <ol className="prompts">
          {prompts.map((p) => (
            <li key={p.event_id}>
              <span className="meta num">{p.ts_ms != null ? clock(p.ts_ms, snap.offset_min) : "Time not recorded"}</span>
              <div className={p.text ? "wrap" : "mute"}>{p.text ? <Content text={p.text} /> : p.deleted ? "Text deleted" : "No text captured"}</div>
            </li>
          ))}
        </ol>
        <p className="meta">
          Stored only on this computer. {t.origin === "live" ? "Delete a prompt in Your data." : "Imported from your tool's own history."} Read in {st.d.read_ms} ms.
        </p>
      </section>
    </div>
  );
}
