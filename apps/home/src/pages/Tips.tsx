// Habits and tips (spec US-H5). Habits are replayed from judged tasks with coaching-loop D5 (design
// coaching-from-rungs D7), never stored and never self-reported. States carry words, not colour alone.
import { jobName, plural, sentence, toolName, when } from "../format";
import type { HabitRow } from "../types";
import { Banners, Chip, Title, type Ctx } from "../ui";

const HABIT_STATE: Record<HabitRow["state"], string> = { not_yet: "Not yet", tried: "Tried", adopted: "Adopted" };
/** Banners for habit moves in the last 14 days (the habit window). */
const RECENT_MS = 14 * 86_400_000;
/** At most this many, newest first, so the page doesn't open on a wall of banners. */
const MAX_BANNERS = 3;

export function Tips(ctx: Ctx) {
  const { snap } = ctx;
  const c = snap.coaching;
  const byJob: [string, HabitRow[]][] = [...new Set(c.habits.map((h) => h.job))].map((j) => [j, c.habits.filter((h) => h.job === j)]);
  const moves = c.habits.filter((h) => h.moved_to && h.moved_ms != null && snap.now_ms - h.moved_ms <= RECENT_MS)
    .sort((a, b) => b.moved_ms! - a.moved_ms!)
    .slice(0, MAX_BANNERS);
  return (
    <div className="page wide">
      <Banners {...ctx} />
      <header className="group">
        <Title>Habits and tips</Title>
        <p className="mute">A habit counts as adopted when you do it in more than half of the tasks where it fits, over 14 days.</p>
      </header>
      <section className="group" aria-labelledby="hab">
        <div className="head">
          <h2 id="hab">Habits</h2>
          <button className="btnlink" onClick={() => ctx.go("tasks")}>Kinds of task and their next steps</button>
        </div>
        {moves.map((h) => (
          <div className="banner info" key={`m-${h.job}-${h.move_id}`}>
            <div className="grow">
              {h.moved_to === "adopted" ? (
                <>
                  <b>You adopted a habit:</b> {h.name} ({jobName(h.job)}).
                </>
              ) : h.moved_back ? (
                <>
                  <b>Back to Tried:</b> {h.name} ({jobName(h.job)}). You did it in {h.seen} of {h.eligible} recent tasks where it fits.
                </>
              ) : (
                <>
                  <b>You tried a habit:</b> {h.name} ({jobName(h.job)}).
                </>
              )}
            </div>
          </div>
        ))}
        {c.habits.length === 0 ? (
          <div className="empty">
            <b>No habits to show yet.</b>
            <span className="mute">Habits show once AIQ has judged a few of your tasks. Each tip names one habit to try.</span>
          </div>
        ) : (
          byJob.map(([job, rows]) => (
            <div className="group" key={job}>
              <h3>{jobName(job)}</h3>
              <ul className="list" aria-label={`Habits for ${jobName(job)}`} style={{ listStyle: "none", margin: 0, padding: 0 }}>
                {rows.map((h) => (
                  <li className="opp" key={h.move_id}>
                    <span className="name">{h.name}</span>
                    <span>
                      <Chip tone={h.state === "adopted" ? "ok" : h.state === "tried" ? "tried" : undefined}>{HABIT_STATE[h.state]}</Chip>
                    </span>
                    <span className="next">
                      {h.eligible > 0 ? `Seen in ${h.seen} of ${plural(h.eligible, "task")} in 14 days` : "No task where it fits in the last 14 days"}
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          ))
        )}
        {c.habits_imported && c.habits.length > 0 && <p className="meta">Includes tasks AIQ imported from your history.</p>}
      </section>
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
                  <th>Tip</th>
                  <th>Where</th>
                  <th>Opened</th>
                </tr>
              </thead>
              <tbody>
                {snap.tips.map((t) => (
                  <tr key={t.card_id}>
                    <td className="num" style={{ whiteSpace: "nowrap" }}>{when(t.delivered_ms, snap.now_ms, snap.offset_min)}</td>
                    <td>
                      <div className="group" style={{ gap: "var(--s1)" }}>
                        <span>{sentence(t.noticed)}</span>
                        <span className="mute">Try this: {t.try_this}</span>
                      </div>
                    </td>
                    <td>{toolName(t.tool)}</td>
                    <td>{t.opened ? <Chip tone="tried">Opened</Chip> : <span className="mute">Not opened</span>}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </div>
  );
}
