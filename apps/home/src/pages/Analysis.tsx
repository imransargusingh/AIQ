// First-run analysis state (design first-run-analysis): what AIQ is doing with the person's finished tasks, how
// far it got, and that there is nothing to do. A reason and one action show only when analysis really can't run.
// Every number is measured from the snapshot; only the finish day is a forecast, from the daily share. The
// tasks counted are Home's 30-day window, so "of N" matches the weekly chart.
import { api } from "../api";
import { RUNGS, day, plural } from "../format";
import type { Analysis, Snapshot } from "../types";
import type { Ctx } from "../ui";

const TOOL = { "copilot-cli": "Copilot CLI", "claude-code": "Claude Code" } as const;
const SIGN_IN = { "copilot-cli": "Copilot", "claude-code": "Claude Code" } as const;

/** "Mon 9 Nov", or "tomorrow" when that is the day. */
function by(ms: number, snap: Snapshot): string {
  const d = (t: number) => Math.floor((t + snap.offset_min * 60_000) / 86_400_000);
  const n = d(ms) - d(snap.now_ms);
  return n <= 0 ? "today" : n === 1 ? "tomorrow" : day(ms, snap.offset_min);
}

/** One line on where analysis stands, for the rung header and the status line. Null when there's nothing to say. */
export function analysisLine(a: Analysis): string | null {
  const total = a.analyzed + a.waiting;
  switch (a.state) {
    case "working":
      return a.analyzed === 0 ? `Analyzing ${plural(a.waiting, "task")}, newest first` : `${a.analyzed} of ${total} tasks analyzed`;
    case "today_done":
      return `${a.analyzed} of ${total} tasks analyzed · more tomorrow`;
    case "blocked":
      return a.blocked ? `Analysis can't run: ${TOOL[a.blocked.tool]} ${a.blocked.why === "missing" ? "not found" : "not signed in"}` : null;
    case "paused":
      return a.waiting > 0 ? `Analysis waits while AIQ is paused (${plural(a.waiting, "task")})` : null;
    default:
      return null;
  }
}

/** Early results: the rung mix of what is analyzed so far, before Home has enough tasks for shares. */
function SoFar({ rungs }: { rungs: number[] }) {
  const n = rungs.reduce((a, b) => a + b, 0);
  if (n === 0) return null;
  const top = rungs.indexOf(Math.max(...rungs));
  return (
    <div className="an-sofar" data-testid="analysis-sofar">
      <span className="meta">
        So far, from {plural(n, "task")}: mostly <b>{RUNGS[top].name}</b>
      </span>
      <div className="rmix sm" role="img" aria-label={rungs.map((v, i) => `${RUNGS[i].name} ${v}`).join(", ")}>
        {rungs.map((v, i) =>
          v > 0 ? (
            <span key={i} className={`r${i + 1}`} style={{ flex: v, background: `var(--rung-${i + 1})` }} title={`${RUNGS[i].name}: ${plural(v, "task")}`}>
              {i + 1}
            </span>
          ) : null,
        )}
      </div>
    </div>
  );
}

/** The body of "Tasks agents finished" until Home has its first numbers. */
export function AnalysisCard({ snap, go, announce, project }: Pick<Ctx, "snap" | "go" | "announce" | "project">) {
  const a = snap.home.analysis;
  const total = a.analyzed + a.waiting;
  const sp = snap.coaching.split;
  const need = snap.coaching.split_min;
  if (a.state === "off")
    return (
      <>
        <div className="hl"><span style={{ font: "var(--t-title)" }}>Analysis is off</span></div>
        <p className="mute">Without it AIQ can't place your tasks on a rung. Tips still work.</p>
        <p><button className="btn" onClick={() => go("settings")}>Turn on in Settings</button></p>
      </>
    );
  if (a.state === "blocked" && a.blocked) {
    const b = a.blocked;
    const act = async () => {
      const r = b.why === "missing" ? await api.openHelp(b.tool) : await api.openSignIn(b.tool);
      announce(r.ok ? (b.why === "missing" ? "Opened the install page." : `Opened ${SIGN_IN[b.tool]}. Type /login there; AIQ tries again by itself.`) : r.message);
    };
    return (
      <>
        <div className="hl"><span style={{ font: "var(--t-title)" }}>Analysis can't run</span></div>
        <p data-testid="analysis-why">
          {b.why === "missing"
            ? `AIQ can't find ${TOOL[b.tool]} on this computer, and it analyzes your tasks through it.`
            : `${TOOL[b.tool]} isn't signed in, and AIQ analyzes your tasks through it.`}{" "}
          {plural(a.waiting, "task")} wait.
        </p>
        <p className="an-act">
          <button className="btn pri" onClick={act}>{b.why === "missing" ? `Get ${TOOL[b.tool]}` : `Sign in to ${SIGN_IN[b.tool]}`}</button>
          <span className="meta">AIQ tries again by itself every 10 minutes.</span>
        </p>
      </>
    );
  }
  const heads: Record<Analysis["state"], string> = {
    off: "Analysis is off",
    blocked: "Analysis can't run",
    paused: "Paused",
    today_done: "Today's analysis is done",
    done: "Not enough analyzed tasks yet",
    working: "Analyzing your tasks",
  };
  const head = heads[a.state];
  const first = sp.tasks < need;
  // One project: the progress and the "nothing to do" line cover every project, so they stay on the full Home.
  if (project !== null)
    return (
      <>
        <div className="hl"><span style={{ font: "var(--t-title)" }}>Not enough analyzed tasks yet</span></div>
        <p className="meta" data-testid="analysis-need">
          How much agents finish shows here from {need} analyzed tasks ({sp.tasks} so far in this project).
        </p>
      </>
    );
  return (
    <>
      <div className="hl"><span style={{ font: "var(--t-title)" }}>{head}</span></div>
      {total > 0 && (
        <div className="an-prog">
          <progress max={total} value={a.analyzed} aria-label={`${a.analyzed} of ${total} tasks analyzed`} />
          <span className="meta">
            <b>{a.analyzed}</b> of {total} analyzed
            {a.state !== "paused" && a.waiting > 0 && (
              <>
                {" "}· about {a.daily} a day{a.done_by_ms != null && <>, all by {by(a.done_by_ms, snap)}</>}
              </>
            )}
          </span>
        </div>
      )}
      <p className="an-ok" data-testid="analysis-ok">
        {a.state === "paused" ? (
          <>Analysis waits while AIQ is paused.</>
        ) : (
          <>
            <span aria-hidden="true">✓</span> Nothing to do.{" "}
            {a.state === "today_done"
              ? `AIQ does the next ${a.daily} tomorrow.`
              : a.state === "done"
                ? "AIQ analyzes each new task when you finish it."
                : "AIQ works in the background, newest first."}
          </>
        )}
      </p>
      <SoFar rungs={snap.home.rungs} />
      {first && sp.tasks > 0 && (
        <p className="meta" data-testid="analysis-need">
          How much agents finish shows here from {need} analyzed tasks ({sp.tasks} so far
          {sp.uncertain > 0 && `; ${plural(sp.uncertain, "task")} AIQ isn't sure about ${sp.uncertain === 1 ? "doesn't" : "don't"} count`}).
        </p>
      )}
    </>
  );
}
