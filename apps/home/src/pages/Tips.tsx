// Habits and tips (spec US-H5; UI review 2026-11-04, item 6; Imran 2026-11-05, option B). Habits are replayed from
// judged tasks with coaching-loop D5 (design coaching-from-rungs D7), never stored and never self-reported. They are
// grouped by the rung each one helps you reach (the pack move's step + 1, read from the rung guide), with how many of
// that rung's habits you've adopted; inside a rung what you're trying comes first. Tip history folds a tip shown more
// than once into one row, names the rung of a next-step tip, and says what happened after it: whether its habit
// showed in the first judged tasks after it where it fits. Rungs and states carry words, not colour alone.
import { useState } from "react";
import { api } from "../api";
import { RUNGS, day, jobName, plural, sentence, toolName, when } from "../format";
import type { GuideMove, HabitRow, TipRow } from "../types";
import { Banners, Chip, Title, type Ctx } from "../ui";
import { TipFunnelCard } from "../funnel";

/** A habit's move shows its date for the habit window (14 days). */
const RECENT_MS = 14 * 86_400_000;

/** One habit as shown: rows with the same name and state across jobs are one line, their counts added. */
export interface Habit {
  key: string;
  name: string;
  state: HabitRow["state"];
  jobs: string[];
  seen: number;
  eligible: number;
  moved: HabitRow | null;
}

/** The habits that help you reach one rung; `rung` is null for a move the guide doesn't know (shown last). */
export interface RungGroup {
  rung: number | null;
  habits: Habit[];
  adopted: number;
}

const STATE_ORDER: HabitRow["state"][] = ["tried", "adopted", "not_yet"];
const STATE_WORD: Record<HabitRow["state"], string> = { tried: "Trying", adopted: "Adopted", not_yet: "Not yet" };

/** The rung a move helps you reach: its guide step (the rung it climbs from) + 1, or null when the guide lacks it. */
export const reaches = (guide: GuideMove[], moveId: string): number | null => {
  const m = guide.find((g) => g.move_id === moveId);
  return m ? m.step + 1 : null;
};

/** Habits grouped by the rung they help you reach, lowest rung first; trying, then adopted, then not yet inside. */
export function habitsByRung(rows: HabitRow[], guide: GuideMove[]): RungGroup[] {
  const groups = new Map<number | null, Habit[]>();
  for (const h of rows) {
    const r = reaches(guide, h.move_id);
    const list = groups.get(r) ?? [];
    groups.set(r, list);
    const same = list.find((x) => x.name === h.name && x.state === h.state);
    const moved = h.moved_to && h.moved_ms != null ? h : null;
    if (same) {
      same.jobs.push(h.job);
      same.seen += h.seen;
      same.eligible += h.eligible;
      if (moved && (!same.moved || moved.moved_ms! > same.moved.moved_ms!)) same.moved = moved;
    } else {
      list.push({ key: `${h.job}/${h.move_id}/${h.state}`, name: h.name, state: h.state, jobs: [h.job], seen: h.seen, eligible: h.eligible, moved });
    }
  }
  return [...groups.entries()]
    .sort(([a], [b]) => (a ?? 99) - (b ?? 99))
    .map(([rung, hs]) => {
      const habits = hs.map((h, i) => [h, i] as const).sort(([x, i], [y, j]) => STATE_ORDER.indexOf(x.state) - STATE_ORDER.indexOf(y.state) || i - j).map(([h]) => h);
      return { rung, habits, adopted: habits.filter((h) => h.state === "adopted").length };
    });
}

/** Tips with the same tip and wording, newest first; the newest delivery leads. */
function foldTips(tips: TipRow[]): TipRow[][] {
  const groups = new Map<string, TipRow[]>();
  for (const t of tips) {
    const k = `${t.tip_id}\u0000${t.noticed}\u0000${t.try_this}`;
    groups.set(k, [...(groups.get(k) ?? []), t]);
  }
  return [...groups.values()];
}

/** A rung as its numbered swatch (in the rung's colour) and its name; hovering says what the rung means. */
const RungMark = ({ r }: { r: number }) => (
  <span className="rmark" title={RUNGS[r - 1].means}>
    <span className={`gd-r rg-${r}`} aria-hidden="true">{r}</span>
    {RUNGS[r - 1].name}
  </span>
);

export function Tips(ctx: Ctx) {
  const { snap, announce, refresh } = ctx;
  const c = snap.coaching;
  const groups = habitsByRung(c.habits, snap.guide);
  const off = new Set(snap.settings.tips_off);
  const [msg, setMsg] = useState<string | null>(null);

  const moveText = (h: Habit) => {
    const m = h.moved;
    if (!m || snap.now_ms - m.moved_ms! > RECENT_MS) return null;
    const on = day(m.moved_ms!, snap.offset_min);
    return m.moved_to === "adopted" ? `Adopted ${on}` : m.moved_back ? `Back to trying since ${on}` : `First tried ${on}`;
  };

  const toggle = async (tipId: string, on: boolean) => {
    const r = await (on ? api.tipOn(tipId) : api.tipOff(tipId));
    if (r.ok) {
      setMsg(null);
      announce(on ? "Tips like this are back on." : "AIQ won't show tips like this. Turn them back on here or in Settings.");
      refresh();
    } else {
      setMsg(r.message);
      announce(r.message);
    }
  };

  return (
    <div className="page wide">
      <Banners {...ctx} />
      <header className="group">
        <Title>Habits and tips</Title>
        <p className="mute">Habits by the rung they help you reach. A habit counts as adopted when you do it in more than half of the tasks where it fits, over 14 days.</p>
        {msg && <p className="errtext" role="alert">{msg}</p>}
      </header>
      <TipFunnelCard snap={ctx.snap} />
      {c.habits.length === 0 ? (
        <section className="group" aria-labelledby="hab">
          <div className="head">
            <h2 id="hab">Habits</h2>
          </div>
          <div className="empty">
            <b>No habits to show yet.</b>
            <span className="mute">Habits show once AIQ has analyzed a few of your tasks. Each tip names one habit to try.</span>
          </div>
        </section>
      ) : (
        groups.map((g) => {
          const id = `hab-r${g.rung ?? "x"}`;
          const title = g.rung ? `${RUNGS[g.rung - 1].name} habits` : "Other habits";
          return (
            <section className="group" aria-labelledby={id} key={id}>
              <div className="head">
                <h2 id={id} className="hb-rh">
                  {g.rung && <span className={`gd-r rg-${g.rung}`} aria-hidden="true">{g.rung}</span>}
                  {title} <span className="meta">{g.habits.length}</span>
                </h2>
                <span className="hb-prog">
                  <span className="meter" aria-hidden="true">
                    <i className={g.rung ? `rg-${g.rung}` : undefined} style={{ width: `${(100 * g.adopted) / g.habits.length}%` }} />
                  </span>
                  <span className="meta">
                    {g.adopted} of {g.habits.length} adopted
                  </span>
                </span>
              </div>
              {g.rung && <p className="meta hb-means">{RUNGS[g.rung - 1].means}</p>}
              <ul className="habits" aria-label={title}>
                {g.habits.map((h) => {
                  const moved = moveText(h);
                  return (
                    <li key={h.key} className="hb">
                      <span className="group" style={{ gap: "var(--s1)" }}>
                        <b>{h.name}</b>
                        <span className="meta">
                          {h.jobs.map(jobName).join(", ")} · <span className={`hb-state hb-${h.state}`}>{STATE_WORD[h.state]}</span>
                          {moved && <> · {moved}</>}
                        </span>
                      </span>
                      {h.eligible > 0 ? (
                        <>
                          <span className="meter" aria-hidden="true">
                            <i style={{ width: `${(100 * h.seen) / h.eligible}%` }} />
                          </span>
                          <span className="num">
                            Seen in {h.seen} of {plural(h.eligible, "task")}
                          </span>
                        </>
                      ) : (
                        <span className="meta hb-none">No task where it fits in 14 days</span>
                      )}
                    </li>
                  );
                })}
              </ul>
            </section>
          );
        })
      )}
      {c.habits_imported && c.habits.length > 0 && <p className="meta">Includes tasks AIQ imported from your history.</p>}
      <p className="meta">
        <button className="btnlink" onClick={() => ctx.go("tasks")}>Kinds of task and their next steps</button>
      </p>

      <section className="group" aria-labelledby="th">
        <div className="head">
          <h2 id="th">Tip history</h2>
          <span className="meta">Last 90 days · tips today: {snap.home.tips_today} of {snap.settings.daily_cap}</span>
        </div>
        {snap.tips.length === 0 ? (
          <div className="empty">
            <b>No tips yet.</b>
            <span className="mute">AIQ shows a tip in your tool when it notices something worth changing, at most {snap.settings.daily_cap} a day.</span>
          </div>
        ) : (
          <div className="tablewrap">
            <table aria-label="Tip history">
              <thead>
                <tr>
                  <th>When</th>
                  <th>Rung</th>
                  <th>Tip</th>
                  <th>Where</th>
                  <th>After the tip</th>
                  <th>
                    <span className="sr">Turn off or on</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {foldTips(snap.tips).map((g) => {
                  const t = g[0];
                  const tools = [...new Set(g.map((x) => toolName(x.tool)))];
                  const isOff = t.tip_id !== "" && off.has(t.tip_id);
                  const rung = t.tip_id.startsWith("next_rung.") ? reaches(snap.guide, t.tip_id.slice("next_rung.".length)) : null;
                  return (
                    <tr key={t.card_id}>
                      <td className="num" style={{ whiteSpace: "nowrap" }}>
                        {when(t.delivered_ms, snap.now_ms, snap.offset_min)}
                        {g.length > 1 && (
                          <div className="meta">
                            Shown {g.length} times, first {when(g[g.length - 1].delivered_ms, snap.now_ms, snap.offset_min)}
                          </div>
                        )}
                      </td>
                      <td>{rung ? <RungMark r={rung} /> : <span className="meta" title="This tip is about one task, not a rung's habit.">—</span>}</td>
                      <td>
                        <div className="group" style={{ gap: "var(--s1)" }}>
                          <span>
                            {sentence(t.noticed)} {g.length > 1 && <Chip>×{g.length}</Chip>}
                          </span>
                          <span className="mute">Try this: {t.try_this}</span>
                          <span className="meta">{g.some((x) => x.opened) ? "You opened it" : "Not opened"}</span>
                        </div>
                      </td>
                      <td style={{ whiteSpace: "nowrap" }}>{tools.join(", ")}</td>
                      <td>
                        {!t.after ? (
                          <span className="meta" title="This tip is about one task, not a habit AIQ can follow.">No habit to follow</span>
                        ) : t.after.checked === 0 ? (
                          <span className="meta">No task where it fits yet</span>
                        ) : (
                          <Chip tone={t.after.seen * 2 > t.after.checked ? "ok" : undefined}>
                            Did it in {t.after.seen} of the next {plural(t.after.checked, "task")}
                          </Chip>
                        )}
                      </td>
                      <td style={{ whiteSpace: "nowrap" }}>
                        {t.tip_id !== "" && (
                          <button className="btnlink" disabled={!snap.status.core} onClick={() => toggle(t.tip_id, isOff)}>
                            {isOff ? "Turn back on" : "Stop tips like this"}
                          </button>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
        <p className="meta">
          "After the tip" reads the first 3 analyzed tasks after a next-step tip where its habit fits. It is AIQ's analysis of those tasks.
        </p>
      </section>
    </div>
  );
}
