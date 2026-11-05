// Getting started (UI review 2026-11-04, item 7): until Home has its first numbers, a short list that ticks
// itself from the snapshot, so "is it working?" has an answer without asking the person to do anything. Every
// line is measured: tools connected now, prompts captured, judging on or off, judged tasks against the minimum.
import { plural, toolName, when } from "../format";
import { Title, type Ctx } from "../ui";

const TOOL = { "claude-code": "Claude Code", copilot: "GitHub Copilot" } as const;

interface Step {
  done: boolean;
  title: string;
  detail: string;
  action?: { label: string; run: () => void };
}

function steps({ snap, go }: Pick<Ctx, "snap" | "go">): Step[] {
  const tools = snap.setup.tools.filter((t) => t.present);
  const on = tools.filter((t) => t.hooked).map((t) => TOOL[t.tool]);
  const off = tools.filter((t) => !t.hooked).map((t) => TOOL[t.tool]);
  const h = snap.home;
  const c = snap.coaching;
  const seen = snap.data.prompts > 0;
  const tools_seen = h.by_tool.map(([t]) => toolName(t));
  const judging = snap.settings.judge === "auto";
  return [
    {
      done: on.length > 0,
      title: on.length ? `${on.join(" and ")} connected` : "No tool connected yet",
      detail: off.length ? `${off.join(" and ")} ${off.length === 1 ? "is" : "are"} on this computer but not connected.` : on.length ? "Every tool AIQ found is connected." : "AIQ didn't find Claude Code or GitHub Copilot on this computer.",
      action: off.length ? { label: "Connect", run: () => go("setup") } : undefined,
    },
    {
      done: seen,
      title: seen ? "Prompts captured" : "No prompt captured yet",
      detail: seen
        ? `${plural(snap.data.prompts, "prompt")} and ${plural(h.tasks_in_window, "task")} so far${tools_seen.length ? ` in ${tools_seen.join(", ")}` : ""}${snap.status.last_event_ms ? `, the last ${when(snap.status.last_event_ms, snap.now_ms, snap.offset_min)}` : ""}.`
        : "Your next prompt in a connected tool shows here.",
    },
    {
      done: judging,
      title: judging ? "Analysis is on" : "Analysis is off",
      detail: judging
        ? `Up to ${snap.settings.judge_daily_tasks} finished tasks a day, through your own Claude Code or Copilot CLI.`
        : "Without it AIQ can't place your tasks on a rung. Tips still work.",
      action: judging ? undefined : { label: "Turn on in Settings", run: () => go("settings") },
    },
    {
      done: h.judged > 0,
      title: h.judged > 0 ? `${plural(h.judged, "task")} analyzed` : "No task analyzed yet",
      detail: h.judged > 0 ? `${plural(h.tasks_in_window - h.judged, "task")} waiting.` : "A task is analyzed once it ends: 30 minutes with no new prompt.",
    },
    {
      done: c.split.tasks >= c.split_min,
      title: "Your first numbers",
      detail: `${c.split.tasks} of ${c.split_min} analyzed tasks. Home shows how much agents finish from ${c.split_min}, so a few tasks can't swing it.`,
    },
  ];
}

function List({ items }: { items: Step[] }) {
  return (
    <ul className="cklist" aria-label="Getting started">
      {items.map((s) => (
        <li key={s.title} className="ck">
          <span className={`mark${s.done ? " on" : ""}`} aria-hidden="true">
            {s.done ? "✓" : ""}
          </span>
          <span className="group" style={{ gap: "var(--s1)" }}>
            <span>
              <span className="sr">{s.done ? "Done: " : "Not yet: "}</span>
              <b>{s.title}</b>
            </span>
            <span className="meta">
              {s.detail}
              {s.action && (
                <>
                  {" "}
                  <button className="btnlink" onClick={s.action.run}>{s.action.label}</button>
                </>
              )}
            </span>
          </span>
        </li>
      ))}
    </ul>
  );
}

/** The whole of Home before anything is captured. */
export function GettingStartedPage(ctx: Pick<Ctx, "snap" | "go" | "startTour">) {
  return (
    <>
      <header className="group">
        <Title>AIQ is getting to know your work</Title>
        <p className="mute">Keep working as usual. Nothing here needs you: this list ticks itself.</p>
      </header>
      <section className="card">
        <List items={steps(ctx)} />
      </section>
      <p className="meta">
        <button className="btnlink" onClick={() => ctx.go("data")}>See what AIQ has captured</button>
        {" · "}
        <button className="btnlink" onClick={ctx.startTour}>Take the tour</button>
      </p>
    </>
  );
}

/** A card at the top of Home until the first numbers show. */
export function GettingStartedCard(ctx: Pick<Ctx, "snap" | "go">) {
  const items = steps(ctx);
  const done = items.filter((s) => s.done).length;
  return (
    <section className="card" aria-labelledby="gs">
      <div className="head">
        <h2 id="gs">Getting started</h2>
        <span className="meta">
          {done} of {items.length} done · this goes once Home has its first numbers
        </span>
      </div>
      <List items={items} />
    </section>
  );
}
