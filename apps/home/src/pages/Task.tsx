// Task detail (spec US-H4; design aiq-home-deferred-pages E8): one task's facts and its prompts in order.
// The judgment (design rung-judge) shows the rung, why, one next step and what was read; until the task is
// judged it says so. Model-written text renders as plain text only (TR-5).
import { useEffect, useState } from "react";
import { api } from "../api";
import { clock, folderName, jobName, plural, stateLabel, toolName, when } from "../format";
import type { TaskAnalysis, TaskDetail } from "../types";
import { Banners, Chip, Content, Rung, Sk, Title, preview, type Ctx } from "../ui";

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
          <span title={t.folder ?? undefined}>{t.folder ? folderName(t.folder) : "Folder unknown"}</span>
          {t.origin === "imported" && <> · <Chip>Imported</Chip></>}
        </p>
      </header>

      {j ? (
        <section className="card group" aria-labelledby="jd">
          <h2 id="jd">How this task was done</h2>
          <div className="row">
            <Chip>
              <span>
                Rung: <Rung r={j.current} />
              </span>
            </Chip>
            {stateLabel(j.state) && <Chip>{stateLabel(j.state)}</Chip>}
            <Chip>Job: {jobName(j.job)}</Chip>
            {j.task_type_label && <Chip>Kind: {j.task_type_label}</Chip>}
          </div>
          {j.state === "uncertain" && <p className="mute">AIQ isn't sure about this one, so it won't coach on it.</p>}
          <ul className="evidence">
            {j.evidence.map((e, i) => (
              <li key={i}>{e}</li>
            ))}
          </ul>
          {j.potential > j.current && j.gap && (
            <p>
              <b>Next step (<Rung r={j.current} /> → <Rung r={j.potential} />):</b> {j.gap}
            </p>
          )}
          {j.blockers.length > 0 && <p className="mute">Outside your control: {j.blockers.join(" ")}</p>}
          <p className="meta">
            Analyzed {when(j.judged_ms, snap.now_ms, snap.offset_min)} by your {toolName(j.provider)}
            {j.model && <> ({j.model})</>}. It read {plural(j.read_prompts, "prompt")}, {plural(j.read_chars, "character")}
            {j.read_actions ? " and a summary of the agent's actions" : ""}.
          </p>
        </section>
      ) : (
        <AnalysisState a={st.d.analysis} tool={t.tool} taskId={t.task_id} ctx={ctx} reload={() => setAttempt((n) => n + 1)} />
      )}

      <section className="group" aria-labelledby="tp">
        <h2 id="tp">Your prompts in this task</h2>
        <ol className="prompts">
          {prompts.map((p) => (
            <li key={p.event_id}>
              <span className="meta num">{p.ts_ms != null ? clock(p.ts_ms, snap.offset_min) : "Time unknown"}</span>
              <div className={p.text ? "wrap" : "mute"}>{p.text ? <Content text={p.text} /> : p.deleted ? "Text deleted" : "No text saved"}</div>
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

/** Tries before AIQ stops trying a task (Rust `aiq_home::TRIES`). */
const TRIES = 3;

/** Why a task has no analysis yet, in the words of its state (DC-055): waiting, trying again at a time, stopped
 * trying, or blocked by the tool. Before, one "Not analyzed yet" covered all of them, and a machine whose every
 * call failed looked the same as one that was busy. */
function AnalysisState({ a, tool, taskId, ctx, reload }: { a: TaskAnalysis; tool: string; taskId: string; ctx: Ctx; reload: () => void }) {
  const { snap, go, announce } = ctx;
  const [trying, setTrying] = useState(false);
  // Mid-sentence: "after today 09:00", not "after Today 09:00".
  const at = (ms: number) => when(ms, snap.now_ms, snap.offset_min).replace(/^(Today|Yesterday|Tomorrow)/, (w) => w.toLowerCase());
  const via = a.blocked ? toolName(a.blocked.tool) : toolName(tool === "vscode" ? "copilot-cli" : tool);
  const tryNow = async (label: string) => {
    setTrying(true);
    const r = await api.analyzeTask(taskId);
    setTrying(false);
    announce(r.ok ? `${label}: done. The page shows the result.` : r.message);
    if (r.ok) reload();
  };
  const tryButton = (label: string) => (
    <button className="btn pri" onClick={() => tryNow(label)} disabled={trying} aria-busy={trying}>
      {trying ? "Trying…" : label}
    </button>
  );
  const reply = a.reply && (
    <>
      <span className="mute">{via} answered{a.state === "gave_up" ? " on the last try" : ""}:</span>
      <code className="an-reply">{a.reply}</code>
    </>
  );
  const tries = (
    <span className="an-tries meta" aria-label={`${a.tries} of ${TRIES} tries`}>
      {Array.from({ length: TRIES }, (_, i) => (
        <i key={i} className={i < a.tries ? (a.state === "gave_up" ? "bad" : "warn") : undefined} aria-hidden="true" />
      ))}
      {a.state === "gave_up" ? `${plural(a.tries, "try", "tries")}` : `Try ${a.tries} of ${TRIES}`}
      {a.last_try_ms != null && <> · last {at(a.last_try_ms)}</>}
    </span>
  );
  switch (a.state) {
    case "off":
      return (
        <div className="empty" data-testid="analysis-state">
          <b>Not analyzed.</b>
          <span className="mute">Analysis is off. Turn it on in Settings, under Analysis.</span>
        </div>
      );
    case "paused":
      return (
        <div className="empty" data-testid="analysis-state">
          <b>Not analyzed yet.</b>
          <span className="mute">AIQ is paused, so it isn't analyzing. Resume to carry on.</span>
        </div>
      );
    case "no_tool":
      return (
        <div className="empty" data-testid="analysis-state">
          <b>Not analyzed.</b>
          <span className="mute">AIQ analyzes tasks through Claude Code or Copilot CLI, and this task came from neither.</span>
        </div>
      );
    case "blocked": {
      const b = a.blocked!;
      const act = async () => {
        const r = b.why === "missing" ? await api.openHelp(b.tool) : await api.openSignIn(b.tool);
        announce(r.ok ? (b.why === "missing" ? "Opened the install page." : "Opened a terminal. Type /login there; AIQ tries again by itself.") : r.message);
      };
      return (
        <div className="empty an-bad" data-testid="analysis-state">
          <b>{b.why === "missing" ? `AIQ can't find ${via} on this computer.` : `${via} isn't signed in for AIQ.`}</b>
          <span className="mute">This task waits until it can be analyzed. Nothing is lost, and AIQ tries again by itself.</span>
          <p className="an-act">
            <button className="btn pri" onClick={act}>{b.why === "missing" ? `Get ${via}` : `Sign in to ${via}`}</button>
          </p>
        </div>
      );
    }
    case "open":
      return (
        <div className="empty" data-testid="analysis-state">
          <b>Not analyzed yet.</b>
          <span className="mute">
            AIQ analyzes a task once it has been quiet for 30 minutes{a.closes_ms != null && <>, so this one after {at(a.closes_ms)}</>}.
          </span>
        </div>
      );
    case "retrying":
      return (
        <div className="empty an-warn" data-testid="analysis-state">
          <b>Analysis didn't work yet. AIQ tries again {a.next_try_ms != null ? at(a.next_try_ms) : "soon"}.</b>
          {tries}
          {reply}
          <p className="an-act">{tryButton("Try now")}</p>
        </div>
      );
    case "gave_up":
      return (
        <div className="empty an-bad" data-testid="analysis-state">
          <b>AIQ couldn't analyze this task, so it stopped trying.</b>
          {tries}
          {reply}
          <p className="an-act">
            {tryButton("Try again")}
            <button className="btn" onClick={() => go("data")}>Open Your data</button>
            <span className="meta">A diagnostics file there shows every try.</span>
          </p>
        </div>
      );
    case "today_done":
      return (
        <div className="empty" data-testid="analysis-state">
          <b>Waiting for tomorrow.</b>
          <span className="mute">AIQ analyzed today's {plural(snap.settings.judge_daily_tasks, "task")}. This one waits until after midnight.</span>
          <p className="an-act">
            <button className="btn" onClick={() => go("settings")}>Change the limit</button>
          </p>
        </div>
      );
    default:
      return (
        <div className="empty" data-testid="analysis-state">
          <b>Waiting to be analyzed.</b>
          <span className="mute">
            {a.ahead > 0 ? `${plural(a.ahead, "newer task")} go${a.ahead === 1 ? "es" : ""} first. ` : "It's next. "}
            AIQ analyzes in the background, through your own {via}.
          </span>
        </div>
      );
  }
}
