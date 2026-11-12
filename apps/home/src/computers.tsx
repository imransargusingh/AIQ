// Your computers (design multi-machine-sync): the offer a Devbox shows when it can see the laptop's AIQ through
// Remote Desktop, the Settings card with the one switch, and the tag naming the computer a task ran on.
// Words: "computer", "in sync", "paused"; never "device", "monitor" or "watch".
import { useState } from "react";
import { api } from "./api";
import type { Snapshot, Sync, SyncPeer, TaskRow } from "./types";
import { ago } from "./pages/RightNow";

/** The name of the computer a task ran on, or null when it is this one (or sync never ran). */
export function computerOf(snap: Snapshot, t: TaskRow): string | null {
  if (!t.computer) return null;
  return snap.sync?.peers.find((p) => p.id === t.computer)?.name || "Other computer";
}

/** Small tag beside a task from another computer. Nothing for this computer's own tasks. */
export function ComputerTag({ snap, task }: { snap: Snapshot; task: TaskRow }) {
  const name = computerOf(snap, task);
  if (!name) return null;
  return (
    <span className="chip" title={`Ran on ${name}`}>
      {name}
    </span>
  );
}

/** Tasks' Computer filter: All computers, this one, then each other computer. No options until another computer's
 *  task is here, so a person with one computer never sees it. */
export function computerFilter(snap: Snapshot): { options: { value: string; label: string }[]; keep: (v: string, t: TaskRow) => boolean } {
  const keep = (v: string, t: TaskRow) => v === "all" || (v === "here" ? !t.computer : t.computer === v);
  if (!snap.tasks.some((t) => t.computer)) return { options: [], keep };
  const others = [...new Set(snap.tasks.map((t) => t.computer).filter((c): c is string => !!c))];
  return {
    options: [
      { value: "all", label: "All computers" },
      { value: "here", label: `${snap.sync?.name || "This computer"} (this one)` },
      ...others.map((id) => ({ value: id, label: snap.sync?.peers.find((p) => p.id === id)?.name || "Other computer" })),
    ],
    keep,
  };
}

function peerLine(p: SyncPeer, now: number, off: number): { text: string; tone: "ok" | "wait" | "mute" } {
  const last = p.last_ms != null ? ago(p.last_ms, now, off) : null;
  switch (p.state) {
    case "ok":
      return last ? { text: `In sync · ${last}`, tone: "ok" } : { text: "Joining…", tone: "wait" };
    case "paused":
      return { text: `Sync is off on ${p.name || "that computer"} · each keeps what it has`, tone: "mute" };
    case "update":
      return { text: `${p.name} runs a newer AIQ · its newest tasks follow when this one updates`, tone: "wait" };
    case "waiting":
    case "error":
      return { text: `${last ? `Last in sync ${last}` : "Not in sync yet"} · tries again in a minute`, tone: "wait" };
    default:
      return { text: `${last ? `Last in sync ${last}` : "Not in sync yet"} · catches up next time you connect`, tone: "wait" };
  }
}

/** Home: one button, shown on the computer that can see another computer's AIQ, until the person chooses. */
export function SyncOffer({ snap, refresh, announce }: { snap: Snapshot; refresh: () => void; announce: (m: string) => void }) {
  const [busy, setBusy] = useState(false);
  const s = snap.sync;
  if (!s?.offer || s.setting !== "unset") return null;
  const other = s.offer.name || "your other computer";
  const choose = async (v: "on" | "off") => {
    setBusy(true);
    const r = await api.setSetting("sync", v);
    setBusy(false);
    announce(r.ok ? (v === "on" ? `Keeping ${s.name} and ${other} in sync.` : "Not now. Turn it on any time in Settings.") : r.message);
    refresh();
  };
  return (
    <section className="card" aria-labelledby="sync-offer" data-testid="sync-offer">
      <h2 id="sync-offer">AIQ on {other} is here too</h2>
      <p className="mute" style={{ margin: 0 }}>
        See your work from both computers in one place. It goes through your Remote Desktop connection and stays on your two computers.
      </p>
      <div className="row">
        <button className="btn pri" disabled={busy} onClick={() => choose("on")}>
          Keep in sync
        </button>
        <button className="btn" disabled={busy} onClick={() => choose("off")}>
          Not now
        </button>
      </div>
    </section>
  );
}

/** Settings: this computer, the others, and the one switch. */
export function YourComputers({ snap, down, change }: { snap: Snapshot; down: boolean; change: (k: "sync", v: string, done: string) => Promise<boolean> }) {
  const s: Sync | undefined = snap.sync;
  const on = s?.setting === "on";
  const peers = s?.peers ?? [];
  return (
    <section className="card" aria-labelledby="st10">
      <div className="head">
        <h2 id="st10" tabIndex={-1}>Your computers</h2>
        {s && (peers.length > 0 || s.via || on) && (
          <div className="row">
            <span id="sync-switch" className="meta">Keep in sync</span>
            <button
              className="switch"
              role="switch"
              aria-checked={on}
              aria-labelledby="sync-switch"
              disabled={down}
              onClick={() =>
                on
                  ? change("sync", "off", "Sync is off. Each computer keeps what it has.")
                  : change("sync", "on", "Your computers are kept in sync.")
              }
            />
          </div>
        )}
      </div>
      <ul className="pcs" aria-label="Your computers">
        <li>
          <b>{s?.name ?? "This computer"}</b> <span className="meta">· this computer</span>
        </li>
        {peers.map((p) => {
          const l = peerLine(p, snap.now_ms, snap.offset_min);
          return (
            <li key={p.id}>
              <b>{p.name || "Other computer"}</b> {p.version && <span className="meta">· AIQ {p.version}</span>}
              <div className={`meta sync-${l.tone}`}>{on || p.state === "paused" ? l.text : "Sync is off here · what came from it stays"}</div>
            </li>
          );
        })}
      </ul>
      {peers.length === 0 && (
        <span className="meta">
          {s?.via
            ? `AIQ on ${s.via} is reachable through Remote Desktop. Turn on Keep in sync to see both in one place.`
            : "Use AIQ on another computer you reach with Remote Desktop, like a Devbox? Open AIQ there and choose Keep in sync. Your work goes straight between your computers and is stored nowhere else."}
        </span>
      )}
      {on && peers.length > 0 && <span className="meta">Tips a day and quiet times count across your computers. Each computer analyzes its own tasks.</span>}
    </section>
  );
}
