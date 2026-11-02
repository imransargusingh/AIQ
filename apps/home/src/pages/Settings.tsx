// Settings (spec US-H7): tips a day and how long a Copilot CLI tip stays, written through the core.
// Quiet hours, tip types, preset, judge and sharing wait for their phases; they say so.
import { useState } from "react";
import { api, type SettingKey } from "../api";
import type { Settings as S } from "../types";
import { plural } from "../format";
import { Banners, Title, type Ctx } from "../ui";

const STAYS: { v: S["copilot_tip_stays"]; label: string; note?: string }[] = [
  { v: "next-prompt", label: "Until your next prompt", note: "Default" },
  { v: "two-prompts-or-5-min", label: "For 2 prompts or 5 minutes, then “1 tip waiting”" },
  { v: "until-opened", label: "Until you open it" },
];

const NEXT: { v: S["next_rung_tips"]; label: string; note?: string }[] = [
  { v: "auto", label: "On, unless AIQ's guesses are often wrong", note: "Default" },
  { v: "on", label: "Always on" },
  { v: "off", label: "Off" },
];

export function Settings(ctx: Ctx & { singleKeys: boolean; setSingleKeys: (on: boolean) => void }) {
  const { snap, announce, refresh, singleKeys, setSingleKeys } = ctx;
  const [msg, setMsg] = useState<string | null>(null);
  const down = !snap.status.core;
  const st = snap.settings;
  // The habit a tip names, from the kinds of task or habits AIQ knows; else the tip's own id.
  const tipName = (tipId: string) => {
    const move = tipId.replace(/^next_rung\./, "");
    const k = snap.coaching.kinds.find((x) => x.next?.move_id === move);
    return k?.next?.habit ?? snap.coaching.habits.find((h) => h.move_id === move)?.name ?? tipId;
  };
  const tipOn = async (tipId: string) => {
    const r = await api.tipOn(tipId);
    if (r.ok) {
      setMsg(null);
      announce(`${tipName(tipId)}: tip turned back on.`);
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

      <section className="card" aria-labelledby="st1">
        <h2 id="st1">Tips</h2>
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
        {st.tips_off.length > 0 && (
          <div className="group">
            <span id="toff">Tips you turned off</span>
            <ul aria-labelledby="toff" style={{ listStyle: "none", margin: 0, padding: 0 }} className="group">
              {st.tips_off.map((t) => (
                <li className="row" key={t} style={{ justifyContent: "space-between" }}>
                  <span>{tipName(t)}</span>
                  <button className="btn" onClick={() => tipOn(t)}>Turn back on</button>
                </li>
              ))}
            </ul>
          </div>
        )}
      </section>

      <section className="card" aria-labelledby="st2">
        <h2 id="st2">Keyboard</h2>
        <div className="row" style={{ justifyContent: "space-between" }}>
          <span id="sk">Single-key shortcuts: G then a letter to go, P to pause, J and K in tables</span>
          <button className="switch" role="switch" aria-checked={singleKeys} aria-labelledby="sk" onClick={() => setSingleKeys(!singleKeys)} />
        </div>
        <span className="meta">Ctrl+K opens the command palette either way.</span>
      </section>

      <section className="card" aria-labelledby="st3">
        <div className="head">
          <h2 id="st3">Rung model</h2>
          <span className="chip">{snap.settings.judge === "auto" ? "Judging in the background" : "Off"}</span>
        </div>
        <p className="mute">
          AIQ judges each finished task through the tool that already saw it, with your own sign-in: Claude Code tasks through your
          Claude Code, Copilot and VS Code tasks through your Copilot CLI. It sends the task's prompts and a summary of what the agent
          did, never your own edits. Judge calls use your plan's usage, and a work account's policies and logging apply.
        </p>
        <p>
          Up to {plural(snap.settings.judge_daily_tasks, "task")} a day, two calls each. Today: {plural(snap.home.judge_today.tasks, "task")},{" "}
          {plural(snap.home.judge_today.calls, "call")}
          {snap.home.judge_today.failed_calls > 0 && <>, {snap.home.judge_today.failed_calls} failed</>}
          {snap.home.judge_today.cost_usd !== null && <>, ${snap.home.judge_today.cost_usd.toFixed(2)} reported by Claude Code</>}.
        </p>
        <p className="meta">
          Change it in a terminal: <code>aiq config set judge {snap.settings.judge === "auto" ? "off" : "auto"}</code> ·{" "}
          <code>aiq config set judge-daily-tasks 20</code> · see what would be sent: <code>aiq judge --dry-run</code>
        </p>
      </section>

      <section className="card" aria-labelledby="st4">
        <div className="head">
          <h2 id="st4">Sharing</h2>
          <span className="chip">Off · nothing leaves this computer</span>
        </div>
        <p className="mute">AIQ home makes no network calls. Nothing is shared.</p>
      </section>

      <section className="card" aria-labelledby="st5">
        <div className="head">
          <h2 id="st5">Setup</h2>
          <button className="btn" onClick={() => ctx.go("setup")}>Run setup again</button>
        </div>
        <p className="mute">See which tools AIQ is connected to, and go through the privacy and tips steps again.</p>
      </section>
    </div>
  );
}
