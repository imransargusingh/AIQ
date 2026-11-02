// Task detail (spec US-H4; design aiq-home-deferred-pages E8): one task's facts and its prompts in order.
// The rung judge isn't built yet, so the rung, what it read and the next step say "Not judged yet" in the
// places they will fill.
import { useEffect, useState } from "react";
import { api } from "../api";
import { NOT_CLASSIFIED, clock, folderName, plural, rungLabel, toolName, when } from "../format";
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

      <div className="empty">
        <b>No next step yet: AIQ doesn't judge tasks yet.</b>
        <span className="mute">
          When it does, this shows which rung the task is on today and why, the facts the judge read from it, one next step that would let the agent
          do more, and a starter prompt to copy.
        </span>
        <div className="row">
          <Chip>Rung: {t.rung ? rungLabel(t.rung) : "Not judged yet"}</Chip>
          <Chip>Job: {t.job ?? NOT_CLASSIFIED}</Chip>
        </div>
      </div>

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
