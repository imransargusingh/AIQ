// Settings (spec US-H7): tips a day and how long a Copilot CLI tip stays, written through the core.
// Quiet hours, tip types, preset, judge and sharing wait for their phases; they say so.
import { useState } from "react";
import { api } from "../api";
import type { Settings as S } from "../types";
import { Banners, Title, type Ctx } from "../ui";

const STAYS: { v: S["copilot_tip_stays"]; label: string; note?: string }[] = [
  { v: "next-prompt", label: "Until your next prompt", note: "Default" },
  { v: "two-prompts-or-5-min", label: "For 2 prompts or 5 minutes, then “1 tip waiting”" },
  { v: "until-opened", label: "Until you open it" },
];

export function Settings(ctx: Ctx & { singleKeys: boolean; setSingleKeys: (on: boolean) => void }) {
  const { snap, announce, refresh, singleKeys, setSingleKeys } = ctx;
  const [msg, setMsg] = useState<string | null>(null);
  const down = !snap.status.core;

  const change = async (key: "daily_cap" | "copilot_tip_stays", value: string, done: string) => {
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
        <h2 id="st3">Rung model</h2>
        <p className="mute">Not available yet. Judging tasks through your own signed-in Claude Code comes in a later version. Tips work without it.</p>
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
