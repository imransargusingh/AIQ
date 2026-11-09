// Tips a day: one picker for Settings and first-run setup, so the two can never offer different limits
// (they did: setup kept 1 / 3 / 5 after Settings moved to 5 / 15 / 30 / Custom, reported 2026-11-09).
import { useState, type FormEvent } from "react";

/** Tips a day offered as buttons; Custom takes any whole number up to the core's limit (`MAX_DAILY_CAP`), which
 * `aiq config set daily-cap` accepts too. Imran 2026-11-07: 5, 15, 30 and a way to type any limit. */
export const CAPS = [5, 15, 30];
export const MAX_CAP = 50;
/** The core's default (`default_daily_cap` in the bundled pack). */
export const DEFAULT_CAP = 5;

/** The buttons plus Custom. `set` writes the value and says whether it took. */
export function TipsCap({ value, down, labelledBy, set }: { value: number; down: boolean; labelledBy: string; set: (n: number) => Promise<boolean> }) {
  // Custom: null until Custom is pressed; open while the saved value isn't one of the buttons.
  const [custom, setCustom] = useState<string | null>(null);
  const open = custom !== null || !CAPS.includes(value);
  const text = (custom ?? String(value)).trim();
  const bad = !/^\d+$/.test(text) || Number(text) > MAX_CAP;
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (bad) return;
    const n = Number(text);
    if (await set(n)) setCustom(CAPS.includes(n) ? null : String(n));
  };
  return (
    <div className="row">
      <div className="seg" role="group" aria-labelledby={labelledBy}>
        {CAPS.map((n) => (
          <button
            key={n}
            aria-pressed={!open && value === n}
            disabled={down}
            onClick={() => {
              setCustom(null);
              void set(n);
            }}
          >
            {n}
          </button>
        ))}
        <button aria-pressed={open} disabled={down} onClick={() => setCustom(String(value))}>
          Custom
        </button>
      </div>
      {open && (
        <form className="row" onSubmit={submit}>
          <input
            className={`input${bad ? " err" : ""}`}
            style={{ width: "6ch" }}
            inputMode="numeric"
            aria-label={`Tips a day, 0 to ${MAX_CAP}`}
            aria-invalid={bad}
            aria-describedby={`${labelledBy}-n`}
            disabled={down}
            value={custom ?? String(value)}
            onChange={(e) => setCustom(e.target.value)}
          />
          <button className="btn" disabled={down || bad || (custom ?? "") === String(value)}>
            Set
          </button>
          <span id={`${labelledBy}-n`} className={bad ? "errtext" : "meta"}>
            0 to {MAX_CAP}
          </span>
        </form>
      )}
    </div>
  );
}

/** When a tip may show: the core's budget (`may_deliver`: one per session, quiet period, daily cap) and the
 * same-tip cooldown (72 h, funnel reason `cooldown`), in the words both Settings and setup use. */
export const TIP_RULES =
  "A tip shows when the agent finishes a turn, for a kind of task you repeat: at most one a session, never within 30 minutes of another, and the same tip not again for 3 days.";
