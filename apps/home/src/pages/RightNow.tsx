// Right now card at the top of Home (design live-rung-trend C): while a task is open, where it is heading after
// each turn, the rung tasks of its kind could reach, and the tip at the turn it showed. An estimate from what the
// agent did; the judge's rung replaces it when the task ends. Every rung is also named in text. Several open tasks
// (Imran 2026-11-07, option A): one chip per task names it and shows the rung it is heading for; the chosen task's
// chart shows below, and it stays chosen while the others move.
import { useState } from "react";
import { clock, jobName, plural, rungLabel, sentence, toolName, when } from "../format";
import { projectName } from "../project";
import type { Ctx } from "../ui";
import type { Live, Snapshot } from "../types";

const RN = ["Assisted", "Directed", "Delegated", "Orchestrated"];
/** Why a turn's actions were not recorded (`turn_action.why`), in plain words. */
const WHY: Record<string, string> = {
  no_log: "AIQ found no log for it",
  read_error: "the log couldn't be read",
  too_large: "too much log to read at once",
  unparsed: "a log line AIQ didn't understand",
  no_start: "AIQ restarted during the turn",
  not_read: "AIQ didn't read its log",
};
const turns = (ts: number[]) => (ts.length === 1 ? `turn ${ts[0]}` : `turns ${ts.join(", ")}`);

/** The reasons behind the estimate, in plain words (no model text). */
export function reasons(l: Live): { ok: boolean; text: string }[] {
  const out: { ok: boolean; text: string }[] = [];
  const work = l.did_is_lookup ? "looked things up itself" : "made the edits itself";
  if (l.did_turns.length > 0) out.push({ ok: true, text: `Agent ${work} (${turns(l.did_turns)})` });
  else if (l.steps.some((s) => s.rung !== null)) out.push({ ok: false, text: l.did_is_lookup ? "Agent hasn't looked anything up yet" : "Agent hasn't edited anything yet" });
  if (l.brief === false) out.push({ ok: false, text: "Short first prompt: the goal, context and limits help the agent" });
  if (l.prompts >= 3) out.push({ ok: false, text: `Step by step: ${plural(l.prompts, "prompt")}` });
  if (l.subagents > 0) out.push({ ok: true, text: `Split across ${plural(l.subagents, "sub-agent")}` });
  l.unrecorded.forEach((t, i) => {
    const why = WHY[l.unrecorded_why[i] ?? ""];
    out.push({ ok: false, text: `Turn ${t}: no details${why ? ` (${why})` : ""}` });
  });
  return out;
}

function Trace({ l }: { l: Live }) {
  const n = Math.max(l.steps.length, 1);
  const L = 104, W = Math.max(160, n * 44), top = 8, RH = 26, H = top + 4 * RH;
  const Y = (r: number) => top + (4 - r) * RH + RH / 2;
  const X = (i: number) => L + (n === 1 ? W / 2 : (i * W) / (n - 1));
  let d = "";
  let started = false;
  l.steps.forEach((s, i) => {
    if (s.rung === null) return;
    d += started ? ` H${X(i)} V${Y(s.rung)}` : `M${X(i)},${Y(s.rung)}`;
    started = true;
  });
  const label = l.steps.map((s) => `turn ${s.turn}: ${s.rung ? rungLabel(s.rung) : "no estimate"}${s.recorded ? "" : " (no details)"}`).join("; ");
  return (
    <svg className="rn-trace" width={L + W + 24} height={H + 20} viewBox={`0 0 ${L + W + 24} ${H + 20}`} role="img" aria-label={`Estimate after each turn: ${label}`}>
      {[1, 2, 3, 4].map((r) => (
        <g key={r}>
          <line x1={L} x2={L + W} y1={Y(r)} y2={Y(r)} className="rn-grid" />
          <text x={L - 8} y={Y(r) + 4} textAnchor="end" className="rn-ax">
            {RN[r - 1]} ({r})
          </text>
        </g>
      ))}
      {l.could !== null && l.could <= 4 && (
        <>
          <line x1={L} x2={L + W} y1={Y(l.could)} y2={Y(l.could)} className="rn-could" />
          <text x={L + 4} y={Y(l.could) - 6} className="rn-could-t">
            could reach
          </text>
        </>
      )}
      <path d={d} className="rn-line" />
      {l.steps.map((s, i) => (
        <g key={s.turn}>
          {s.rung === null ? (
            <rect x={X(i) - 5} y={Y(2) - 5} width={10} height={10} className="rn-none" />
          ) : (
            <circle cx={X(i)} cy={Y(s.rung)} r={4.5} className={`rn-pt${i === l.steps.length - 1 ? " rn-last" : ""}${s.recorded ? "" : " rn-unrec"}`} />
          )}
          <text x={X(i)} y={H + 14} textAnchor="middle" className="rn-ax">
            {s.turn}
          </text>
        </g>
      ))}
      {l.tip && l.tip.turn >= 1 && l.tip.turn <= n && (
        <g>
          <line x1={X(l.tip.turn - 1)} x2={X(l.tip.turn - 1)} y1={top} y2={H} className="rn-tipline" />
          <rect x={X(l.tip.turn - 1) - 10} y={top - 2} width={20} height={14} rx={3} className="rn-tipbox" />
          <text x={X(l.tip.turn - 1)} y={top + 9} textAnchor="middle" className="rn-tiptext">
            tip
          </text>
        </g>
      )}
    </svg>
  );
}

const what = (l: Live) => l.kind ?? (l.job ? jobName(l.job) : "Open task");

/** The chosen task: the one picked while it stays open, else the newest activity. */
export function chosen(all: Live[], picked: string | null): Live | undefined {
  return all.find((l) => l.task_id === picked) ?? all[0];
}

/** Chips keep their place as tasks move: the task that started first comes first. */
export const chipOrder = (all: Live[]) =>
  [...all].sort((a, b) => a.first_ms - b.first_ms || a.task_id.localeCompare(b.task_id));

export function RightNow(ctx: Ctx) {
  const [picked, setPicked] = useState<string | null>(null);
  const all = ctx.snap.live;
  const l = chosen(all, picked);
  if (!l) return ctx.snap.settings.live_trend ? <Quiet {...ctx} /> : null;
  const many = all.length > 1;
  const mins = Math.max(1, Math.round((ctx.snap.now_ms - l.first_ms) / 60_000));
  const at = (x: Live) => clock(x.first_ms, ctx.snap.offset_min);
  const project = (x: Live) => projectName(ctx.snap, x.folder ?? "");
  // Two chips that would read the same say when each started instead of the prompt count.
  const twin = (x: Live) => all.some((y) => y !== x && what(y) === what(x) && y.folder === x.folder && y.surface === x.surface);
  const meta = (
    <>
      {many && `${project(l)} · `}
      {toolName(l.surface)} · since {at(l)} · {plural(l.prompts, "prompt")} · {mins} min
      {l.working ? " · agent working" : many ? " · waiting for you" : ""}
    </>
  );
  return (
    <section className="card rn" aria-labelledby="rnhd" data-testid="right-now">
      <div className="head">
        <h2 id="rnhd">
          <span className="mute" style={{ fontWeight: 400 }}>Right now · </span>
          {many ? `${all.length} open tasks` : what(l)}
        </h2>
        {!many && <span className="meta num">{meta}</span>}
      </div>
      {many && (
        <div className="rn-chips" role="tablist" aria-label="Open tasks">
          {chipOrder(all).map((x) => (
            <button
              key={x.task_id}
              type="button"
              role="tab"
              id={`rn-tab-${x.task_id}`}
              aria-selected={x === l}
              aria-controls="rn-panel"
              className={`rn-chip${x === l ? " on" : ""}`}
              tabIndex={x === l ? 0 : -1}
              onClick={() => setPicked(x.task_id)}
              onKeyDown={(e) => {
                const order = chipOrder(all);
                const i = order.indexOf(x);
                const j = { ArrowRight: i + 1, ArrowLeft: i - 1, Home: 0, End: order.length - 1 }[e.key];
                if (j === undefined) return;
                e.preventDefault();
                const next = order[(j + order.length) % order.length];
                setPicked(next.task_id);
                document.getElementById(`rn-tab-${next.task_id}`)?.focus();
              }}
            >
              {x.rung === null ? (
                <span className="rn-sw rn-sw-none" aria-hidden="true">?</span>
              ) : (
                <span className={`rn-sw rg-${x.rung}`} aria-hidden="true">{x.rung}</span>
              )}
              <span className="rn-cn">
                <b>{what(x)}</b>
                <span className="meta num">
                  {project(x)} · {toolName(x.surface)} · {twin(x) ? `since ${at(x)}` : plural(x.prompts, "prompt")}
                </span>
              </span>
              <span className={`rn-dot${x.working ? " busy" : ""}`} aria-hidden="true" />
              <span className="sr">
                , {x.rung === null ? "no estimate yet" : `heading for ${rungLabel(x.rung)}`}, {x.working ? "agent working" : "waiting for you"}
              </span>
            </button>
          ))}
        </div>
      )}
      <div id="rn-panel" role={many ? "tabpanel" : undefined} aria-labelledby={many ? `rn-tab-${l.task_id}` : undefined}>
        {many && <p className="meta num rn-meta">{meta}</p>}
        <p className="rn-head">
          {l.rung !== null ? (
            <>
              Heading for <b>{rungLabel(l.rung)}</b>
              {l.could !== null && (
                <>
                  {" "}· could reach <b>{rungLabel(l.could)}</b>
                </>
              )}
            </>
          ) : (
            <span className="mute">No estimate yet: {l.working ? "the first turn hasn't ended" : "the turns so far weren't recorded"}</span>
          )}
        </p>
        {l.steps.some((s) => s.rung !== null) && (
          <div className="rn-scroll">
            <Trace l={l} />
          </div>
        )}
        {l.tip && (
          <p className="rn-tip">
            <span className="rn-badge">AIQ tip:</span> {sentence(l.tip.try_this)} <span className="meta">· from turn {l.tip.turn}</span>
          </p>
        )}
        <ul className="rn-why">
          {reasons(l).map((r) => (
            <li key={r.text}>
              <span aria-hidden="true">{r.ok ? "✓" : "–"}</span> {r.text}
            </li>
          ))}
        </ul>
        <p className="meta">
          An estimate from what the agent did, until the task ends. The analyzed rung replaces it then.{" "}
          <button className="btnlink" onClick={() => ctx.openTask(l.task_id)}>
            Open task
          </button>
        </p>
      </div>
      <LeftOut snap={ctx.snap} />
    </section>
  );
}

/** A connected tool (Settings > Tools) and the surface its events carry. */
const SURFACE: Record<string, [string, string]> = {
  "claude-code": ["claude-code", "Claude Code"],
  copilot: ["copilot-cli", "Copilot"],
  vscode: ["vscode", "VS Code"],
};
/** A connected tool silent this long gets the "is it reaching AIQ?" hint. simplify: one fixed span; per-person
 * rhythm (how often each tool is usually used) if the hint shows for tools people rarely open. */
export const QUIET_HINT_MS = 3 * 86_400_000;

/** "just now", "12 min ago", else "Today 09:14" / "Yesterday 18:40" / "Sat 31 Oct 18:40". */
export function ago(ms: number, nowMs: number, offsetMin: number): string {
  const m = Math.round((nowMs - ms) / 60_000);
  if (m < 1) return "just now";
  if (m < 60) return `${m} min ago`;
  return when(ms, nowMs, offsetMin);
}

/** What the card says when no task shows (Imran 2026-11-08): the card stays, so Home keeps its shape and the person
 * can tell "AIQ sees nothing" from "AIQ is broken". The first match wins. */
export function quiet(snap: Snapshot) {
  const tools = snap.setup.tools
    .filter((t) => t.hooked && t.present && SURFACE[t.tool])
    .map((t) => {
      const [surface, name] = SURFACE[t.tool];
      const last = snap.seen.find((x) => x.surface === surface)?.last_ms ?? null;
      // Activity left out as automated still is activity (Imran 2026-11-08): the card names it instead of calling the
      // tool idle. It sits beside the person's own last turn, never in its place (Imran 2026-11-11: "Yesterday 21:28 ·
      // automated" alone read as if nothing had happened today).
      const a = snap.aside.find((x) => x.surface === surface && (last === null || x.last_ms > last));
      const own = a ? a.last_ms : null;
      return { name, last, own, aside: a?.reason ?? null, latest: own ?? last };
    });
  const silent = tools.filter((t) => t.latest === null || snap.now_ms - t.latest > QUIET_HINT_MS);
  const aside = tools.filter((t) => t.aside !== null);
  const state = snap.status.paused
    ? "paused"
    : snap.status.core_state === "failed"
      ? "stopped"
      : snap.starting.length > 0
        ? "starting"
        : tools.length === 0
          ? "none"
          : "idle";
  return { state, tools, silent, aside } as const;
}

/** Why a tool's newest activity isn't counted, in plain words (rule names from the core's set-aside rules). */
export function asideText(tools: { name: string; aside: string | null }[]): string {
  const names = tools.map((t) => t.name).join(" and ");
  const all = (r: string) => tools.every((t) => t.aside === r);
  return all("projects")
    ? "Claude's own work in a claude.ai project isn't counted. Turns you type yourself count."
    : all("not-typed")
      ? `Work in ${names} that an app scheduled or started isn't counted. Turns you type yourself count.`
      : `Work in ${names} that an app or script runs isn't counted. Turns you type yourself count.`;
}

/** What the card calls a tool's newest activity left out as automated, by rule. */
export function ownWorkLabel(reason: string): string {
  return reason === "projects" ? "Claude's own work" : reason === "not-typed" ? "started by an app" : "automated";
}

/** Same bar as crates/aiq-contracts `AsideNow::mostly_left_out` (10 prompts, 90%; Imran 2026-11-09). */
export const LEFT_OUT_MIN = 10;
export const LEFT_OUT_SHARE = 0.9;
const NAME: Record<string, string> = { "claude-code": "Claude Code", "copilot-cli": "Copilot", vscode: "VS Code" };
const LEFT_WHY: Record<string, string> = {
  projects: "Claude's own work in a claude.ai project",
  "not-typed": "scheduled or started by an app, not typed",
};

/** Tools most of whose prompts today were left out as automated, in plain words. */
export function leftOut(snap: Snapshot): { surface: string; text: string }[] {
  return snap.aside
    .filter((a) => a.left_out_today >= LEFT_OUT_MIN && a.left_out_today >= LEFT_OUT_SHARE * (a.left_out_today + a.counted_today))
    .map((a) => ({
      surface: a.surface,
      text: `Today AIQ left out ${a.left_out_today} of ${a.left_out_today + a.counted_today} ${NAME[a.surface] ?? toolName(a.surface)} prompts as automated, not counted: ${LEFT_WHY[a.reason] ?? "an app or script seems to run them"}.`,
    }));
}

/** Imran 2026-11-09: 269 Claude Code prompts were left out over five days and nothing said so. Shown whatever the card
 * shows, so an open task from another tool never hides it. */
function LeftOut({ snap }: { snap: Snapshot }) {
  const rows = leftOut(snap);
  if (rows.length === 0) return null;
  return (
    <div className="rn-left" data-testid="right-now-left-out">
      {rows.map((r) => (
        <p key={r.surface} className="meta">
          {r.text}
        </p>
      ))}
      <p className="meta">
        <button className="btnlink" onClick={() => document.getElementById("hb")?.scrollIntoView({ behavior: "smooth" })}>
          See what's not counted
        </button>
      </p>
    </div>
  );
}

function Quiet(ctx: Ctx) {
  const { snap } = ctx;
  const { state, tools, silent, aside } = quiet(snap);
  const s = snap.starting[0];
  const title = {
    paused: "Paused",
    stopped: "Waiting for AIQ to start",
    starting: snap.starting.length > 1 ? `${snap.starting.length} tasks starting` : "Task starting",
    none: "No tool connected",
    idle: "Nothing in progress",
  }[state];
  const meta =
    state === "starting" ? (
      <>
        {toolName(s.surface)} · since {clock(s.first_ms, snap.offset_min)} · {plural(s.prompts, "prompt")} · {s.working ? "agent working" : "waiting for you"}
      </>
    ) : null;
  return (
    <section className="card rn rn-quiet" aria-labelledby="rnhd" data-testid="right-now" data-state={state}>
      <div className="head">
        <h2 id="rnhd">
          <span className="mute" style={{ fontWeight: 400 }}>Right now · </span>
          {title}
        </h2>
        {meta && <span className="meta num">{meta}</span>}
      </div>
      {state === "paused" && (
        <p className="rn-head">
          AIQ records nothing new while paused, so tasks don't show here.{" "}
          <button className="btnlink" onClick={ctx.togglePause}>
            Resume
          </button>
        </p>
      )}
      {state === "stopped" && <p className="rn-head">Nothing new is recorded until then. The note at the top says what happened.</p>}
      {state === "starting" && (
        <p className="rn-head mute">
          Where {snap.starting.length > 1 ? "each task" : "it"} is heading shows after the agent's first reply.
        </p>
      )}
      {state === "none" && (
        <p className="rn-head">
          Connect Claude Code, Copilot or VS Code, and your next task shows here.{" "}
          <button className="btnlink" onClick={() => ctx.go("settings")}>
            Open Settings
          </button>
        </p>
      )}
      {(state === "idle" || state === "starting") && (
        <>
          <ul className="rn-tools" aria-label="Connected tools">
            {tools.map((t) => {
              const recent = t.latest !== null && snap.now_ms - t.latest < 60 * 60_000;
              const when = (ms: number) => ago(ms, snap.now_ms, snap.offset_min);
              return (
                <li key={t.name}>
                  <span className={`rn-dot${recent ? " busy" : ""}${t.latest === null ? " none" : ""}`} aria-hidden="true" />
                  <b>{t.name}</b>
                  <span className="meta num">
                    {t.last !== null ? `your last turn ${when(t.last)}` : t.own !== null ? "no turn of yours yet" : "nothing yet"}
                    {t.own !== null && t.aside && ` · ${ownWorkLabel(t.aside)} ${when(t.own)}`}
                  </span>
                </li>
              );
            })}
          </ul>
          {state === "idle" && <p className="meta">A task shows here once the agent replies to your first prompt.</p>}
          {state === "idle" && aside.length > 0 && leftOut(snap).length === 0 && (
            <p className="meta rn-hint" data-testid="right-now-aside">
              {asideText(aside)}{" "}
              <button className="btnlink" onClick={() => document.getElementById("hb")?.scrollIntoView({ behavior: "smooth" })}>
                See what's not counted
              </button>
            </p>
          )}
          {state === "idle" && silent.length > 0 && (
            <p className="meta rn-hint" data-testid="right-now-hint">
              Using {silent.map((t) => t.name).join(" or ")} with an agent right now? AIQ has had nothing from {silent.length > 1 ? "them" : "it"}{" "}
              {silent.length === 1 && silent[0].latest !== null ? `since ${when(silent[0].latest, snap.now_ms, snap.offset_min)}` : "lately"}.{" "}
              <button className="btnlink" onClick={() => ctx.go("settings")}>
                Check it in Settings
              </button>
            </p>
          )}
        </>
      )}
      <LeftOut snap={snap} />
    </section>
  );
}
