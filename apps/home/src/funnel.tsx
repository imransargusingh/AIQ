// The tip funnel (design tip-funnel): where new tasks' tips went this week, and a note on Home after a working day
// with no tip. Tips stopped for four days (2026-11-04 to 11-08) and nothing showed it. Every number and every reason's
// words come from the core's reader (aiq_core::funnel), so Home and `aiq status` agree.
import { useState } from "react";
import { plural, sentence } from "./format";
import type { Snapshot, TipDrought, TipFunnel } from "./types";
import type { Ctx } from "./ui";

const DAY = 86_400_000;

export const shownCount = (f: TipFunnel) => f.shown.reduce((n, s) => n + s.count, 0);

/** One sentence that says what the week's funnel means: how many tasks had a tip, and the main reason the rest didn't. */
export function funnelInsight(f: TipFunnel): string {
  if (f.tasks === 0) return "No new tasks this week yet.";
  const ready = `${f.tip_found} of ${plural(f.tasks, "new task")} had a tip ready this week`;
  const top = f.no_tip[0];
  if (!top || f.tip_found === f.tasks) return `${ready}.`;
  return `${ready}. Main reason the others didn't: ${top.words} (${top.count}).`;
}

/** "yesterday" or "on Fri 6 Nov", for a YYYY-MM-DD local day. */
export function dayWords(iso: string, nowMs: number, offsetMin: number): string {
  const [y, m, d] = iso.split("-").map(Number);
  const dayNo = Math.floor(Date.UTC(y, m - 1, d) / DAY);
  const today = Math.floor((nowMs + offsetMin * 60_000) / DAY);
  if (today - dayNo === 1) return "yesterday";
  const dt = new Date(Date.UTC(y, m - 1, d));
  const days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
  const months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  return `on ${days[dt.getUTCDay()]} ${d} ${months[m - 1]}`;
}

/** The Home note after a dry working day. */
export function droughtLine(d: TipDrought, nowMs: number, offsetMin: number): string {
  const head = `No tips ${dayWords(d.day, nowMs, offsetMin)} across ${plural(d.tasks, "task")}.`;
  return d.main_reason ? `${head} Main reason: ${d.main_reason.words}.` : head;
}

const SEEN = "aiq.droughtSeen";

/** Home: the newest dry day, until the person closes it (once per day; the Tips page keeps the detail). */
export function DroughtNote({ snap, go }: Pick<Ctx, "snap" | "go">) {
  const d = snap.tip_funnel.droughts[0];
  const [closed, setClosed] = useState(() => {
    try {
      return d != null && localStorage.getItem(SEEN) === d.day;
    } catch {
      return false;
    }
  });
  if (!d || closed || snap.status.paused) return null;
  const close = () => {
    try {
      localStorage.setItem(SEEN, d.day);
    } catch {
      /* No storage: it stays closed until Home reloads. */
    }
    setClosed(true);
  };
  return (
    <div className="banner info drought">
      <div className="grow">{droughtLine(d, snap.now_ms, snap.offset_min)}</div>
      <button className="btn" onClick={() => go("tips")}>See why</button>
      <button className="btn small" aria-label="Close this note" onClick={close}>
        ✕
      </button>
    </div>
  );
}

interface Stage {
  label: string;
  n: number;
}

/** The stages as bars; the step that lost the most is marked. Shown can pass Tip found: mid-task tips count too. */
export function stages(f: TipFunnel): { stages: Stage[]; biggestLoss: number | null } {
  const s: Stage[] = [
    { label: "New tasks", n: f.tasks },
    { label: "Kind known", n: f.kind_known },
    { label: "Tip ready", n: f.tip_found },
    { label: "Shown", n: shownCount(f) },
    { label: "Opened", n: f.opened },
  ];
  let biggestLoss: number | null = null;
  let most = 0;
  for (let i = 1; i < 3; i++) {
    const lost = s[i - 1].n - s[i].n;
    if (lost > most) [most, biggestLoss] = [lost, i];
  }
  return { stages: s, biggestLoss };
}

/** Tips page: the week's funnel, why tasks had no tip, and why ready tips were held. */
export function TipFunnelCard({ snap }: { snap: Snapshot }) {
  const f = snap.tip_funnel.week;
  if (f.tasks === 0 && f.tips === 0) return null;
  const { stages: st, biggestLoss } = stages(f);
  const max = Math.max(1, ...st.map((s) => s.n));
  const held = [...f.held];
  if (f.expired > 0) held.push({ code: "expired", words: "ready, but the session ended first", count: f.expired });
  return (
    <section className="group" aria-labelledby="tf">
      <h2 id="tf">Tips this week</h2>
      <div className="card funnel">
        <p className="tf-insight">{funnelInsight(f)}</p>
        <ol className="tf-bars">
          {st.map((s, i) => (
            <li key={s.label} className={i === biggestLoss ? "tf-loss" : undefined}>
              <span className="tf-label">{s.label}</span>
              <span className="tf-track">
                <span className="tf-bar" style={{ width: `${(100 * s.n) / max}%` }} />
              </span>
              <span className="tf-n">{s.n}</span>
              <span className="tf-tag">{i === biggestLoss ? "Most lost here" : ""}</span>
            </li>
          ))}
        </ol>
        {f.no_tip.length > 0 && (
          <div className="tf-why">
            <h3>Why tasks had no tip</h3>
            <ul>
              {f.no_tip.slice(0, 4).map((c) => (
                <li key={c.code}>
                  <span className="tf-n">{c.count}</span> {sentence(c.words)}
                </li>
              ))}
            </ul>
            {f.no_tip.length > 4 && <p className="meta">and {plural(f.no_tip.slice(4).reduce((n, c) => n + c.count, 0), "more task")}</p>}
          </div>
        )}
        {held.length > 0 && (
          <div className="tf-why">
            <h3>Ready, but held back</h3>
            <ul>
              {held.map((c) => (
                <li key={c.code}>
                  <span className="tf-n">{c.count}</span> {sentence(c.words)}
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </section>
  );
}
