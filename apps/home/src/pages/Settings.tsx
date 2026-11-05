// Settings (spec US-H7): tips a day and how long a Copilot CLI tip stays, written through the core.
// UI review 2026-11-04, item 8: a section index at the top, where tips show folded under one line, judging says
// when it was switched off (a stored fact) and what "today" counts, Sharing is one privacy line under Judging, and
// the keyboard switch sits with the tools.
import { useState } from "react";
import { api, type SettingKey } from "../api";
import type { Settings as S } from "../types";
import { jobName, plural, when } from "../format";
import { AccuracyPaused, Banners, Title, type Ctx } from "../ui";
import { ver } from "../updateNow";

const STAYS: { v: S["copilot_tip_stays"]; label: string; note?: string }[] = [
  { v: "next-prompt", label: "Until your next prompt", note: "Default" },
  { v: "two-prompts-or-5-min", label: "For 2 prompts or 5 minutes, then “1 tip waiting”" },
  { v: "until-opened", label: "Until you open it" },
];

/** Where tips show (each a switch, default on). Words say what the person sees, not how. */
const WHERE: { k: "copilot_tip_in_chat" | "tip_popup" | "tip_highlight"; label: string; note: string }[] = [
  { k: "copilot_tip_in_chat", label: "Show Copilot CLI tips in the conversation", note: "As well as in the status line at the bottom." },
  { k: "tip_popup", label: "Show a Windows pop-up when a tip appears in Claude Code or Copilot CLI", note: "Helps when you've switched to another window while the agent works. VS Code shows its own." },
  { k: "tip_highlight", label: "Highlight tips in yellow", note: "The \u201cAIQ tip:\u201d label in terminals, and the AIQ item in the VS Code status bar." },
];

const TREND = { label: "Show where an open task is heading", note: "On Home while you work, and in the Copilot CLI status line between tips. An estimate; the judge's rung replaces it when the task ends." };

/** The section index: each entry scrolls to its card and moves focus to the heading. */
const SECTIONS = [
  ["st1", "Tips"],
  ["st7", "What AIQ records"],
  ["st3", "Judging"],
  ["st6", "Updates"],
  ["st5", "Tools"],
] as const;

function jump(id: string) {
  const h = document.getElementById(id);
  if (!h) return;
  // Scroll the page's own scroller only: scrollIntoView would also shift the window under the title bar.
  const main = h.closest("main");
  const nav = document.querySelector(".secnav");
  if (main) {
    const top = main.scrollTop + h.getBoundingClientRect().top - main.getBoundingClientRect().top - (nav?.getBoundingClientRect().height ?? 0) - 16;
    main.scrollTo({ top, behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth" });
  }
  h.focus({ preventScroll: true });
}

const NEXT: { v: S["next_rung_tips"]; label: string; note?: string }[] = [
  { v: "auto", label: "On, unless AIQ's guesses are often wrong", note: "Default" },
  { v: "on", label: "Always on" },
  { v: "off", label: "Off" },
];

const UPDATE: { v: "auto" | "ask" | "off"; label: string; note?: string }[] = [
  { v: "auto", label: "Update automatically, when AIQ home is closed", note: "Default" },
  { v: "ask", label: "Tell me first" },
  { v: "off", label: "Don't check for updates" },
];

/** Daily judge caps offered as buttons; any other 0-500 value is set with `aiq config`. */
const JUDGE_CAPS = [20, 50, 100, 200];

export function Settings(ctx: Ctx & { singleKeys: boolean; setSingleKeys: (on: boolean) => void }) {
  const { snap, announce, refresh, singleKeys, setSingleKeys, update } = ctx;
  const [msg, setMsg] = useState<string | null>(null);
  const down = !snap.status.core;
  const st = snap.settings;
  const up = snap.update;
  const judging = st.judge === "auto";
  const jt = snap.home.judge_today;
  // Switched off today: today's counts came before that (the switch time is a stored setting change).
  const offToday = !judging && st.judge_since_ms != null && when(st.judge_since_ms, snap.now_ms, snap.offset_min).startsWith("Today");
  const goBack = async () => {
    const r = await api.runUpdate("rollback");
    const m = r.ok ? "Going back to the previous version. AIQ home closes and opens again." : r.message;
    setMsg(r.ok ? null : m);
    announce(m);
  };
  const [preview, setPreview] = useState<string | null>(null);
  const [previewing, setPreviewing] = useState(false);
  const showPreview = async () => {
    setPreviewing(true);
    try {
      const r = await api.judgePreview();
      setPreview(r.text);
      announce(r.ok ? "Showing what judging would send. Nothing was sent." : "AIQ couldn't make the preview.");
    } catch (e) {
      setPreview(String((e as Error)?.message ?? e));
    }
    setPreviewing(false);
  };
  const [changedFiles, setChangedFiles] = useState<string[]>([]);
  const TOOL_NAME = { "claude-code": "Claude Code", copilot: "GitHub Copilot" } as const;
  const disconnect = async (tool: "claude-code" | "copilot") => {
    const r = await api.disconnectTool(tool);
    setChangedFiles(r.changed);
    const m = r.ok ? `Disconnected ${TOOL_NAME[tool]}. AIQ no longer sees its prompts.` : (r.message ?? "AIQ couldn't disconnect it.");
    setMsg(r.ok ? null : m);
    announce(m);
    refresh();
  };
  const privacy = async () => {
    const r = await api.openPrivacy();
    if (!r.ok) {
      setMsg(r.message);
      announce(r.message);
    }
  };
  // The habit a tip names, from the kinds of task or habits AIQ knows; else the tip's own id.
  const tipName = (tipId: string) => {
    const move = tipId.replace(/^next_rung\./, "");
    const k = snap.coaching.kinds.find((x) => x.next?.move_id === move);
    return k?.next?.habit ?? snap.coaching.habits.find((h) => h.move_id === move)?.name ?? st.next_tips.find((t) => t.tip_id === tipId)?.habit ?? tipId;
  };
  // Next-step tips by job, pack order kept: each job's own tips, then those for every job.
  const tipsOff = new Set(st.tips_off);
  const nextIds = new Set(st.next_tips.map((t) => t.tip_id));
  const byJob = ["code_change", "write_doc", "ask_research", "run_ship", "any"]
    .map((job) => [job, st.next_tips.filter((t) => t.job === job)] as const)
    .filter(([, ts]) => ts.length > 0);
  const nextOn = st.next_tips.filter((t) => !tipsOff.has(t.tip_id)).length;
  // Other tips turned off (the next-step ones have their own switch above).
  const otherOff = st.tips_off.filter((t) => !nextIds.has(t));
  const tipOn = async (tipId: string, on = true) => {
    const r = await (on ? api.tipOn(tipId) : api.tipOff(tipId));
    if (r.ok) {
      setMsg(null);
      announce(`${tipName(tipId)}: tip turned ${on ? "back on" : "off"}.`);
      refresh();
    } else {
      setMsg(r.message);
      announce(r.message);
    }
  };

  const change = async (key: SettingKey, value: string, done: string) => {
    const r = await api.setSetting(key, value);
    if (r.ok) {
      setMsg(null);
      announce(done);
      refresh();
    } else {
      setMsg(r.message);
      announce(r.message);
    }
  };

  return (
    <div className="page">
      <Banners {...ctx} />
      <header className="group">
        <Title>Settings</Title>
        {down && <p className="mute">AIQ isn't running, so tip settings can't change right now. It starts with your next prompt in Claude Code or Copilot.</p>}
        {msg && !down && <p className="errtext" role="alert">{msg}</p>}
      </header>
      <div className="secnav" role="navigation" aria-label="Settings sections">
        {SECTIONS.map(([id, label]) => (
          <button key={id} className="btnlink" onClick={() => jump(id)}>
            {label}
          </button>
        ))}
      </div>

      <section className="card" aria-labelledby="st1">
        <h2 id="st1" tabIndex={-1}>Tips</h2>
        <div className="group">
          <span id="cap">Tips a day, across all your tools</span>
          <div className="row">
            <div className="seg" role="group" aria-labelledby="cap">
              {[1, 3, 5].map((n) => (
                <button key={n} aria-pressed={snap.settings.daily_cap === n} disabled={down} onClick={() => change("daily_cap", String(n), `Tips a day set to ${n}.`)}>
                  {n}
                </button>
              ))}
            </div>
            {![1, 3, 5].includes(snap.settings.daily_cap) && <span className="meta">Now {snap.settings.daily_cap}, set with aiq config</span>}
          </div>
        </div>
        <div>
          <div className="row" style={{ justifyContent: "space-between" }}>
            <span id="w-live_trend">{TREND.label}</span>
            <button
              className="switch"
              role="switch"
              aria-checked={st.live_trend}
              aria-labelledby="w-live_trend"
              aria-describedby="wn-live_trend"
              disabled={down}
              onClick={() => (st.live_trend ? change("live_trend", "off", `${TREND.label}: off.`) : change("live_trend", "on", `${TREND.label}: on.`))}
            />
          </div>
          <span className="meta" id="wn-live_trend">
            {TREND.note}
          </span>
        </div>
        <fieldset className="group" style={{ border: 0, padding: 0, margin: 0 }} disabled={down}>
          <legend style={{ padding: 0, marginBottom: "var(--s2)" }}>Next-step tips, for the kinds of task you repeat</legend>
          {NEXT.map((n) => (
            <label className="opt" key={n.v}>
              <input
                type="radio"
                name="nextrung"
                checked={st.next_rung_tips === n.v}
                onChange={() => change("next_rung_tips", n.v, `Next-step tips: ${n.label.toLowerCase()}.`)}
              />
              <span>
                {n.label} {n.note && <span className="meta">· {n.note}</span>}
              </span>
            </label>
          ))}
        </fieldset>
        {st.guess_auto_off && st.next_rung_tips === "auto" ? (
          <div className="banner" role="status">
            <div className="grow">
              AIQ guessed the kind of task right {st.guess_right} of {st.guess_checked} times, so it stopped these tips.
            </div>
            <button className="btn" disabled={down} onClick={() => change("next_rung_tips", "on", "Next-step tips are back on.")}>
              Turn back on
            </button>
          </div>
        ) : (
          <span className="meta">
            {st.guess_checked > 0
              ? `AIQ checks its guesses of the kind of task against the judge: right ${st.guess_right} of ${st.guess_checked} times so far. Below 8 in 10, it stops these tips by itself.`
              : "AIQ checks its guesses of the kind of task against the judge. Below 8 in 10 right, it stops these tips by itself."}
          </span>
        )}
        <AccuracyPaused {...ctx} />
        {st.next_rung_tips === "auto" && st.accuracy_paused.length === 0 && (
          <span className="meta">AIQ also pauses the tips for a job or kind of task by itself when an accuracy check finds a second model agrees with its judge less than 8 in 10 times.</span>
        )}
        {st.next_tips.length > 0 && (
          <details className="group">
            <summary>
              Choose which next-step tips AIQ may show <span className="meta">· {nextOn} of {st.next_tips.length} on</span>
            </summary>
            <fieldset className="group" aria-label="Next-step tips AIQ may show" style={{ border: 0, padding: 0, margin: "var(--s2) 0 0" }} disabled={down}>
              {byJob.map(([job, ts]) => (
                <div className="group" role="group" aria-labelledby={`nt-${job}`} key={job}>
                  <span className="meta" id={`nt-${job}`}>
                    {job === "any" ? "Every job" : jobName(job)}
                  </span>
                  {ts.map((t) => (
                    <label className="opt" key={t.tip_id}>
                      <input type="checkbox" checked={!tipsOff.has(t.tip_id)} onChange={(e) => tipOn(t.tip_id, e.target.checked)} />
                      <span>{t.habit}</span>
                    </label>
                  ))}
                </div>
              ))}
            </fieldset>
            <span className="meta">A tip only shows for a kind of task you repeat, at most your tips a day, and never within 30 minutes of another.</span>
          </details>
        )}
        {otherOff.length > 0 && (
          <div className="group">
            <span id="toff">Tips you turned off</span>
            <ul aria-labelledby="toff" style={{ listStyle: "none", margin: 0, padding: 0 }} className="group">
              {otherOff.map((t) => (
                <li className="row" key={t} style={{ justifyContent: "space-between" }}>
                  <span>{tipName(t)}</span>
                  <button className="btn" onClick={() => tipOn(t)}>Turn back on</button>
                </li>
              ))}
            </ul>
          </div>
        )}
        <details className="group fold">
          <summary>
            Where tips show and how long they stay <span className="meta">· {WHERE.filter((w) => st[w.k]).length} of {WHERE.length} on · {STAYS.find((x) => x.v === st.copilot_tip_stays)?.label.toLowerCase()}</span>
          </summary>
          <div className="group" role="group" aria-label="Where tips show">
            {WHERE.map((w) => (
              <div key={w.k}>
                <div className="row" style={{ justifyContent: "space-between" }}>
                  <span id={`w-${w.k}`}>{w.label}</span>
                  <button
                    className="switch"
                    role="switch"
                    aria-checked={st[w.k]}
                    aria-labelledby={`w-${w.k}`}
                    aria-describedby={`wn-${w.k}`}
                    disabled={down}
                    onClick={() =>
                      st[w.k] ? change(w.k, "off", `${w.label}: off.`) : change(w.k, "on", `${w.label}: on.`)
                    }
                  />
                </div>
                <span className="meta" id={`wn-${w.k}`}>
                  {w.note}
                </span>
              </div>
            ))}
          </div>
          <fieldset className="group" style={{ border: 0, padding: 0, margin: 0 }} disabled={down}>
            <legend style={{ padding: 0, marginBottom: "var(--s2)" }}>A Copilot CLI tip stays</legend>
            {STAYS.map((s) => (
              <label className="opt" key={s.v}>
                <input
                  type="radio"
                  name="stays"
                  checked={snap.settings.copilot_tip_stays === s.v}
                  onChange={() => change("copilot_tip_stays", s.v, `A Copilot CLI tip now stays: ${s.label.toLowerCase()}.`)}
                />
                <span>
                  {s.label} {s.note && <span className="meta">· {s.note}</span>}
                </span>
              </label>
            ))}
            <span className="meta">Every tip stays in Habits and tips.</span>
          </fieldset>
        </details>
      </section>

      <section className="card" aria-labelledby="st7">
        <h2 id="st7" tabIndex={-1}>What AIQ records</h2>
        <div className="row" style={{ justifyContent: "space-between" }}>
          <span id="midturn">Copilot messages you send while the agent is still working</span>
          <button
            className="switch"
            role="switch"
            aria-checked={st.copilot_mid_turn === "on"}
            aria-labelledby="midturn"
            disabled={down}
            onClick={() =>
              st.copilot_mid_turn === "on"
                ? change("copilot_mid_turn", "off", "AIQ no longer records Copilot messages sent while the agent works.")
                : change("copilot_mid_turn", "on", "AIQ records Copilot messages sent while the agent works.")
            }
          />
        </div>
        <span className="meta">
          Copilot doesn't tell AIQ about these, so AIQ reads them from Copilot's own history file on this computer when the agent
          stops. Nothing leaves your computer.
        </span>
        <div className="row" style={{ justifyContent: "space-between" }}>
          <span id="actions">What the agent did in each turn</span>
          <button
            className="switch"
            role="switch"
            aria-checked={st.agent_actions === "on"}
            aria-labelledby="actions"
            disabled={down}
            onClick={() =>
              st.agent_actions === "on"
                ? change("agent_actions", "off", "AIQ no longer records what the agent did in each turn.")
                : change("agent_actions", "on", "AIQ records what the agent did in each turn.")
            }
          />
        </div>
        <span className="meta">
          Counts only: tool calls, commands, files and helper agents, read from the tool's own history file when the agent
          stops. No command text or file names. Nothing leaves your computer.
        </span>
      </section>

      <section className="card" aria-labelledby="st3">
        <div className="head">
          <h2 id="st3" tabIndex={-1}>Judging</h2>
          <span className="chip">{judging ? "Judging in the background" : st.judge_since_ms != null ? `Off since ${when(st.judge_since_ms, snap.now_ms, snap.offset_min).replace(/^Today /, "").replace(/^Yesterday/, "yesterday")}` : "Off"}</span>
        </div>
        <p className="mute">
          AIQ judges each finished task through the tool that already saw it, with your own sign-in: Claude Code tasks through your
          Claude Code, Copilot and VS Code tasks through your Copilot CLI. It sends the task's prompts and a summary of what the agent
          did, never your own edits. Judge calls use your plan's usage, and a work account's policies and logging apply.
        </p>
        <div className="row" style={{ justifyContent: "space-between" }}>
          <span id="jon">Judge finished tasks in the background</span>
          <button
            className="switch"
            role="switch"
            aria-checked={st.judge === "auto"}
            aria-labelledby="jon"
            disabled={down}
            onClick={() =>
              st.judge === "auto"
                ? change("judge", "off", "Judging is off. AIQ judges no new tasks.")
                : change("judge", "auto", "Judging is on.")
            }
          />
        </div>
        <div className="group">
          <span id="jcap">Tasks judged a day, two calls each on your own plan</span>
          <div className="row">
            <div className="seg" role="group" aria-labelledby="jcap">
              {JUDGE_CAPS.map((n) => (
                <button
                  key={n}
                  aria-pressed={snap.settings.judge_daily_tasks === n}
                  disabled={down}
                  onClick={() => change("judge_daily_tasks", String(n), `Tasks judged a day set to ${n}.`)}
                >
                  {n}
                </button>
              ))}
            </div>
            {!JUDGE_CAPS.includes(snap.settings.judge_daily_tasks) && <span className="meta">Now {snap.settings.judge_daily_tasks}, set with aiq config</span>}
          </div>
        </div>
        <p data-testid="judge-today">
          {judging ? <>Up to {plural(st.judge_daily_tasks, "task")} a day. Today: </> : <>AIQ judges no new task while this is off. {jt.tasks > 0 && (offToday ? "Earlier today, before it was off: " : "Judged today: ")}</>}
          {(judging || jt.tasks > 0) && (
            <>
              {plural(jt.tasks, "task")}, {plural(jt.calls, "call")}
              {jt.failed_calls > 0 && <>, {jt.failed_calls} failed</>}
              {jt.cost_usd !== null && <>, ${jt.cost_usd.toFixed(2)} reported by Claude Code</>}.
            </>
          )}
        </p>
        <div className="row">
          <button className="btn" onClick={showPreview} disabled={previewing} aria-expanded={preview != null}>
            {previewing ? "Making the preview…" : "See what would be sent"}
          </button>
          {preview != null && (
            <button className="btn" onClick={() => setPreview(null)}>
              Hide
            </button>
          )}
        </div>
        {preview != null && (
          <pre className="preview" tabIndex={0} aria-label="What judging would send for the next task. Nothing was sent.">
            {preview}
          </pre>
        )}
        <p className="meta" data-testid="privacy-line">
          Sharing is off: AIQ shares nothing with anyone. Only judge calls and the update check, which sends nothing about you, leave this computer.{" "}
          <button className="btnlink" onClick={privacy}>Read the privacy page</button> (opens in your browser)
        </p>
      </section>

      <section className="card" aria-labelledby="st6">
        <div className="head">
          <h2 id="st6" tabIndex={-1}>Updates</h2>
          <span className="chip">AIQ {up.running}</span>
        </div>
        {up.supported ? (
          <>
            <fieldset className="group" style={{ border: 0, padding: 0, margin: 0 }} disabled={down}>
              <legend className="sr">When a newer AIQ is out</legend>
              {UPDATE.map((o) => (
                <label className="opt" key={o.v}>
                  <input type="radio" name="update" checked={up.mode === o.v} onChange={() => change("update", o.v, `Updates: ${o.label.toLowerCase()}.`)} />
                  <span>
                    {o.label} {o.note && <span className="meta">· {o.note}</span>}
                  </span>
                </label>
              ))}
            </fieldset>
            {up.mode === "off" ? (
              <span className="meta">
                AIQ doesn't check for updates and makes no network call of its own. You're on AIQ {up.running}.
                {up.checked_ms != null && ` Last checked ${when(up.checked_ms, snap.now_ms, snap.offset_min)}.`}
              </span>
            ) : (
            <span className="meta">
              {up.checked_ms != null ? `Last checked ${when(up.checked_ms, snap.now_ms, snap.offset_min)}.` : "Not checked yet."}
              {up.check_error ? " That check didn't reach GitHub; AIQ tries again within the hour." : ""} AIQ asks GitHub once a day, and every
              hour while AIQ home is open, whether a newer version is out, checks the download's fingerprint, and keeps the version before so you can go back. Nothing about you is sent.
            </span>
            )}
            <div className="row">
              <button className="btn" onClick={update.run} disabled={update.phase !== "idle"}>
                {update.phase === "checking" ? "Checking…" : update.phase === "installing" ? "Installing…" : up.note === "waiting" ? `Update to ${ver(up.available)} now` : "Update now"}
              </button>
              {up.can_rollback && (
                <button className="btn" onClick={goBack}>
                  Go back to the previous version
                </button>
              )}
            </div>
            <span className="meta" data-testid="update-progress">
              {update.msg ?? (up.note === "waiting" ? `AIQ ${ver(up.available)} is downloaded and checked.${up.blocked ? ` Last try waited: ${up.blocked}.` : ""}` : "Checks for a newer AIQ and installs it.")}
            </span>
          </>
        ) : (
          <p className="mute">This copy of AIQ wasn't installed from a release, so it doesn't update itself.</p>
        )}
      </section>

      <section className="card" aria-labelledby="st5">
        <div className="head">
          <h2 id="st5" tabIndex={-1}>Tools and keyboard</h2>
          <div className="row">
            <button className="btn" onClick={ctx.startTour}>Take the tour</button>
            <button className="btn" onClick={() => ctx.go("setup")}>Run setup again</button>
          </div>
        </div>
        <ul className="group" aria-label="Tools AIQ watches" style={{ listStyle: "none", margin: 0, padding: 0 }}>
          {snap.setup.tools.filter((t) => t.present).map((t) => (
            <li className="row" key={t.tool} style={{ justifyContent: "space-between" }}>
              <span>
                <b>{TOOL_NAME[t.tool]}</b> · {t.hooked ? "connected" : "not connected"}
              </span>
              {t.hooked && (
                <button className="btn" onClick={() => disconnect(t.tool)} aria-label={`Disconnect ${TOOL_NAME[t.tool]}`}>
                  Disconnect
                </button>
              )}
            </li>
          ))}
        </ul>
        {changedFiles.length > 0 && (
          <ul className="meta" aria-label="Files AIQ changed">
            {changedFiles.map((p) => (
              <li key={p}>
                <code>{p}</code>
              </li>
            ))}
          </ul>
        )}
        <p className="mute">Disconnect puts the tool's settings back as they were. Run setup again to connect a tool, or go through the privacy and tips steps again.</p>
        <div className="row" style={{ justifyContent: "space-between" }}>
          <span id="sk">Single-key shortcuts: G then a letter to go, P to pause, J and K in tables</span>
          <button className="switch" role="switch" aria-checked={singleKeys} aria-labelledby="sk" onClick={() => setSingleKeys(!singleKeys)} />
        </div>
        <span className="meta">Ctrl+K opens the command palette either way.</span>
      </section>
    </div>
  );
}
