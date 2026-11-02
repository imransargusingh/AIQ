// First-run setup (spec US-H1; design aiq-home-deferred-pages E5–E7), laid out and worded as the signed-off
// mockup's setup (docs/mockups/aiq-home.html): a step rail, one heading per step, Continue then Back.
// Start is enabled only after "I understand" on the last step.
import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { Snapshot, ToolFound } from "../types";
import { Title, type Ctx } from "../ui";

const STEPS = ["Privacy", "Tools", "Rung model", "Tips", "Your history"];
const TOOL: Record<ToolFound["tool"], { name: string; where: string }> = {
  "claude-code": { name: "Claude Code", where: "Tips show when Claude Code finishes a turn." },
  copilot: { name: "GitHub Copilot", where: "Copilot CLI and VS Code agent mode share these hooks. Tips show in Copilot CLI's status line." },
};

export function Setup(ctx: Ctx & { rerun: boolean; finish: (s: Snapshot) => void; close: () => void }) {
  const { snap, announce, refresh, rerun, finish, close } = ctx;
  const [step, setStep] = useState(0);
  const [understood, setUnderstood] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const down = !snap.status.core;
  // Tools AIQ connected in this setup, so their row can say how to undo it.
  const [connected, setConnected] = useState<string[]>([]);
  const [connecting, setConnecting] = useState(false);
  const tried = useRef(false);
  const waiting = snap.setup.tools.filter((t) => t.present && !t.hooked);

  // Imran, 2026-11-02: setup connects the tools it finds by itself (first run only; "Run setup again"
  // offers a Connect button instead, so a tool the person disconnected isn't silently reconnected).
  const connect = async () => {
    const names = waiting.map((t) => t.tool);
    setConnecting(true);
    try {
      const r = await api.connectTools();
      if (r.ok) {
        setConnected((c) => [...c, ...names]);
        setMsg(null);
        announce(`Connected ${names.map((n) => TOOL[n].name).join(" and ")}.`);
      } else setMsg(r.message);
    } catch (e) {
      setMsg(String((e as Error)?.message ?? e));
    }
    setConnecting(false);
    refresh();
  };
  useEffect(() => {
    if (step === 1 && !rerun && !tried.current && waiting.length > 0) {
      tried.current = true;
      void connect();
    }
  });

  const to = (n: number) => {
    setMsg(null);
    setStep(n);
  };
  const setCap = async (n: number) => {
    const r = await api.setSetting("daily_cap", String(n));
    if (r.ok) {
      setMsg(null);
      announce(`Tips a day set to ${n}.`);
      refresh();
    } else setMsg(r.message);
  };
  const start = async () => {
    setBusy(true);
    try {
      finish(await api.finishSetup());
    } catch (e) {
      setMsg(String((e as Error)?.message ?? e));
      setBusy(false);
    }
  };
  const nav = (primary = <button className="btn pri" onClick={() => to(step + 1)}>Continue</button>) => (
    <div className="row">
      {primary}
      {step > 0 && (
        <button className="btn" onClick={() => to(step - 1)}>
          Back
        </button>
      )}
      {rerun && (
        <button className="btn" onClick={close}>
          Close
        </button>
      )}
    </div>
  );

  return (
    <div className="setup">
      <ol className="steps" aria-label="Setup steps">
        {STEPS.map((s, i) => (
          <li key={s} className={i < step ? "done" : i === step ? "on" : undefined} aria-current={i === step ? "step" : undefined}>
            <b aria-hidden="true">{i < step ? "✓" : i + 1}</b>
            {s}
            {i < step && <span className="sr"> (done)</span>}
          </li>
        ))}
      </ol>
      <div className="page" style={{ gap: "var(--s5)" }} key={step}>
        {step === 0 && (
          <>
            <header className="group">
              <Title>Your coach, on your computer only</Title>
              <p className="mute">
                AIQ watches how you work with AI agents and suggests one small change at a time, so agents finish more of your work with less
                redoing.
              </p>
            </header>
            <div className="group">
              <div className="kv">
                <b>Only you see it</b>
                <span className="mute">No manager or team view exists. Your data stays on this computer.</span>
              </div>
              <div className="kv">
                <b>About the task, not you</b>
                <span className="mute">Tips are about the work, never a score of you.</span>
              </div>
              <div className="kv">
                <b>You control the data</b>
                <span className="mute">Pause any time; delete one prompt, or everything.</span>
              </div>
            </div>
            {nav()}
          </>
        )}

        {step === 1 && (
          <>
            <header className="group">
              <Title>Tools AIQ will watch</Title>
              <p className="mute">AIQ reads your prompts and when each turn ends. If AIQ stops, your tools work as before.</p>
            </header>
            <div className="group">
              {snap.setup.tools.map((t) => (
                <div className="opt" key={t.tool}>
                  <span>
                    <b>{TOOL[t.tool].name}</b> ·{" "}
                    {t.hooked
                      ? connected.includes(t.tool)
                        ? "connected just now"
                        : "connected"
                      : t.present
                        ? connecting
                          ? "connecting…"
                          : "found, not connected yet"
                        : "not found on this computer"}
                    <br />
                    <span className="meta">
                      {t.hooked ? (
                        <>
                          {TOOL[t.tool].where}
                          {connected.includes(t.tool) && (
                            <>
                              {" "}
                              AIQ backed up the file it changed. To undo, run <code>aiq uninstall {t.tool}</code>.
                            </>
                          )}
                        </>
                      ) : t.present ? (
                        connecting ? (
                          "Adding AIQ's hooks. AIQ backs up the file first."
                        ) : (
                          <>
                            AIQ adds its hooks and backs up the file first. Or run <code>{t.install}</code> in a terminal to see the change.
                          </>
                        )
                      ) : (
                        "Install it, then run setup again from Settings."
                      )}
                    </span>
                  </span>
                </div>
              ))}
            </div>
            {waiting.length > 0 && !connecting && (
              <div className="row">
                <button className="btn" onClick={connect}>
                  Connect {waiting.length > 1 ? "them" : TOOL[waiting[0].tool].name}
                </button>
              </div>
            )}
            {msg && (
              <p className="errtext" role="alert">
                {msg}
              </p>
            )}
            {snap.setup.tools.every((t) => !t.hooked) && !connecting && (
              <p className="mute">AIQ isn't connected to a tool yet. You can finish setup now and connect one later.</p>
            )}
            {nav()}
          </>
        )}

        {step === 2 && (
          <>
            <header className="group">
              <Title>How AIQ will judge your tasks</Title>
              <p className="mute">Nothing to connect yet. The rung model comes in a later version of AIQ. Tips work without it.</p>
            </header>
            <p>
              When it comes, AIQ will judge finished tasks through your own signed-in Claude Code or Copilot, using your own plan. AIQ never reads
              or stores your sign-in.
            </p>
            <p className="meta">On a work account, your organisation's policies apply to these calls, including logging and retention.</p>
            {nav()}
          </>
        )}

        {step === 3 && (
          <>
            <header className="group">
              <Title>How many tips a day?</Title>
              <p className="mute">At most one per session, shown when the agent pauses. Change it any time in Settings.</p>
            </header>
            <div className="seg" role="group" aria-label="Tips a day">
              {[1, 3, 5].map((n) => (
                <button key={n} aria-pressed={snap.settings.daily_cap === n} disabled={down} onClick={() => setCap(n)}>
                  {n}
                  {n === 5 ? " (default)" : ""}
                </button>
              ))}
            </div>
            {down && (
              <p className="mute">
                AIQ isn't running yet, so this stays at {snap.settings.daily_cap} for now. It starts with your next prompt in Claude Code or
                Copilot, and you can change it in Settings then.
              </p>
            )}
            {msg && (
              <p className="errtext" role="alert">
                {msg}
              </p>
            )}
            {nav()}
          </>
        )}

        {step === 4 && (
          <>
            <header className="group">
              <Title>One thing to know before you start</Title>
              <p className="mute">Your history lives only on this computer. AIQ keeps no copy and has no backup in this version.</p>
            </header>
            <p>If you lose this computer or reinstall Windows, your AIQ history is gone. Your tools and your work aren't affected.</p>
            <label className="opt" style={{ maxWidth: 300 }}>
              <input type="checkbox" checked={understood} onChange={(e) => setUnderstood(e.target.checked)} /> I understand
            </label>
            {msg && (
              <p className="errtext" role="alert">
                {msg}
              </p>
            )}
            {nav(
              <button className="btn pri" disabled={!understood || busy} onClick={start}>
                Start AIQ
              </button>,
            )}
          </>
        )}
      </div>
    </div>
  );
}
