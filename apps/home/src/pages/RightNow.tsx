// Right now card at the top of Home (design live-rung-trend C): while a task is open, where it is heading after
// each turn, the rung tasks of its kind could reach, and the tip at the turn it showed. An estimate from what the
// agent did; the judge's rung replaces it when the task ends. Every rung is also named in text.
import { clock, jobName, plural, rungLabel, toolName } from "../format";
import type { Ctx } from "../ui";
import type { Live } from "../types";

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
  if (l.brief === false) out.push({ ok: false, text: "First prompt was short: say the goal, context and limits up front" });
  if (l.prompts >= 3) out.push({ ok: false, text: `Asked a piece at a time: ${plural(l.prompts, "prompt")}` });
  if (l.subagents > 0) out.push({ ok: true, text: `Split across ${plural(l.subagents, "sub-agent")}` });
  l.unrecorded.forEach((t, i) => {
    const why = WHY[l.unrecorded_why[i] ?? ""];
    out.push({ ok: false, text: `Turn ${t}: not recorded${why ? ` (${why})` : ""}` });
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
  const label = l.steps.map((s) => `turn ${s.turn}: ${s.rung ? rungLabel(s.rung) : "no estimate"}${s.recorded ? "" : " (not recorded)"}`).join("; ");
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

export function RightNow(ctx: Ctx) {
  const l = ctx.snap.live;
  if (!l) return null;
  const mins = Math.max(1, Math.round((ctx.snap.now_ms - l.first_ms) / 60_000));
  const what = l.kind ?? (l.job ? jobName(l.job) : "Open task");
  return (
    <section className="card rn" aria-labelledby="rnhd" data-testid="right-now">
      <div className="head">
        <h2 id="rnhd">
          <span className="mute" style={{ fontWeight: 400 }}>Right now · </span>
          {what}
        </h2>
        <span className="meta num">
          {toolName(l.surface)} · since {clock(l.first_ms, ctx.snap.offset_min)} · {plural(l.prompts, "prompt")} · {mins} min{l.working && " · agent working"}
        </span>
      </div>
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
          <span className="rn-badge">AIQ tip:</span> {l.tip.try_this} <span className="meta">· from turn {l.tip.turn}</span>
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
        An estimate from what the agent did, until the task ends. The judge's rung replaces it then.{" "}
        <button className="btnlink" onClick={() => ctx.openTask(l.task_id)}>
          Open task
        </button>
      </p>
    </section>
  );
}
