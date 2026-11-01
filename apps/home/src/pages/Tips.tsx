// Habits and tips (spec US-H5). Tip history is real; habit tracking (coaching-loop D5) is a later phase.
import { toolName, when } from "../format";
import { Banners, Chip, Title, type Ctx } from "../ui";

export function Tips(ctx: Ctx) {
  const { snap } = ctx;
  return (
    <div className="page wide">
      <Banners {...ctx} />
      <header className="group">
        <Title>Habits and tips</Title>
        <p className="mute">A habit counts as adopted when you do it in more than half of the tasks where it fits, over 14 days.</p>
      </header>
      <section className="group" aria-labelledby="hab">
        <h2 id="hab">Habits</h2>
        <div className="empty">
          <b>Habit tracking comes in a later version.</b>
          <span className="mute">Your tips are below. Each one names one habit to try.</span>
        </div>
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
                        <span>{t.noticed}</span>
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
