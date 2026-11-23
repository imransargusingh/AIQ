"""Read-only, counts only: recheck the two Assisted habits against the 11-06 baseline.

Baseline (measured 2026-11-05 23:35Z, 836 tasks in 30 days): rung 1 n=626, F6 (result checked) no 452 (72%),
F4 (goal, context and limits) no 410 (65%).
Reuses kinds_home.py (Home's task list with AIQ's typed-turn rules, latest analysis per task).
Prints no prompt or analysis text. Usage (same folder as kinds_home.py): python3 -I habits_recheck.py <aiq.db>
"""
import calendar
import collections
import contextlib
import io
import json
import runpy
import sqlite3
import sys
import time

buf = io.StringIO()
with contextlib.redirect_stdout(buf):
    g = runpy.run_path("kinds_home.py", run_name="kinds_home")
H, NOW, DAY = g["H"], g["NOW"], g["DAY"]
check = [ln for ln in buf.getvalue().splitlines() if "rebuild" in ln or "left out" in ln]
print("== rebuild self-check (from kinds_home.py) ==")
for ln in check[:12]:
    print(ln)

con = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True, timeout=5)  # not immutable: that skips WAL rows
con.execute("PRAGMA query_only = ON")
F = {}
for r in con.execute(
    """SELECT j.task_id, j.facts, j.fact_set_version, j.model_rung FROM judgment j
       WHERE j.rowid = (SELECT x.rowid FROM judgment x WHERE x.task_id = j.task_id
                        ORDER BY x.judged_utc_ms DESC, x.rowid DESC LIMIT 1)"""
):
    try:
        facts = json.loads(r[1])
    except Exception:
        facts = {}
    F[r[0]] = dict(facts=facts, fs=r[2], model=r[3])


def v(t, fid):
    f = F.get(t["task_id"], {}).get("facts", {}).get(fid)
    if not isinstance(f, dict):
        return "absent"
    x = f.get("value")
    return "yes" if x is True else "no" if x is False else "unknown"


def pct(n, d):
    return f"{100.0 * n / d:.0f}%" if d else "-"


def ts(ms):
    return time.strftime("%Y-%m-%d", time.gmtime(ms / 1000))


def block(label, S):
    R1 = [t for t in S if t["rung"] == 1]
    R1s = [t for t in R1 if t["state"] != "uncertain"]
    rungs = collections.Counter(t["rung"] for t in S if t["state"] != "uncertain")
    sure = sum(rungs.values())
    print(f"-- {label}: tasks {len(S)}; rung 1 {len(R1)} (sure {len(R1s)}); "
          f"sure rungs {dict(sorted(rungs.items()))}; Directed or higher {pct(sure - rungs[1], sure)} of sure")
    for name, P in (("rung 1 all", R1), ("rung 1 sure", R1s)):
        n = len(P)
        f6 = collections.Counter(v(t, "F6") for t in P)
        f4 = collections.Counter(v(t, "F4") for t in P)
        both = sum(1 for t in P if v(t, "F6") == "no" and v(t, "F4") == "no")
        print(f"   {name} n={n}: F6 no {f6['no']} ({pct(f6['no'], n)}), unknown {f6['unknown']}, yes {f6['yes']}, absent {f6['absent']} | "
              f"F4 no {f4['no']} ({pct(f4['no'], n)}), unknown {f4['unknown']}, yes {f4['yes']}, absent {f4['absent']} | both no {both}")
    print(f"   fact sets on rung 1: {dict(collections.Counter(F.get(t['task_id'], {}).get('fs') for t in R1))}; "
          f"tools {dict(collections.Counter(t['tool'] for t in S))}")


B0 = calendar.timegm((2026, 11, 6, 6, 0, 0)) * 1000  # 2026-11-06 06:00Z
print(f"\n== snapshot {time.strftime('%Y-%m-%dT%H:%MZ', time.gmtime(NOW / 1000))}; Home tasks with an analysis {len(H)}; "
      f"newest first prompt {time.strftime('%Y-%m-%dT%H:%MZ', time.gmtime(max(t['first'] for t in H) / 1000))} ==")
block("baseline window, 30 days to 11-06 06:00Z (re-read today)", [t for t in H if B0 - 30 * DAY <= t["first"] < B0])
block("since 11-06 06:00Z", [t for t in H if t["first"] >= B0])
block("last 14 days", [t for t in H if t["first"] >= NOW - 14 * DAY])
block("last 30 days", [t for t in H if t["first"] >= NOW - 30 * DAY])
for tool in sorted({t["tool"] for t in H}):
    block(f"{tool}, 30 days to 11-06", [t for t in H if t["tool"] == tool and B0 - 30 * DAY <= t["first"] < B0])
    block(f"{tool}, since 11-06", [t for t in H if t["tool"] == tool and t["first"] >= B0])

print("\n== by week (first prompt), rung 1 all: n, F6 no %, F4 no %, sure Directed+ % ==")
wk = collections.defaultdict(list)
for t in H:
    if t["first"] >= B0 - 42 * DAY:
        wk[time.strftime("%G-W%V", time.gmtime(t["first"] / 1000))].append(t)
for k in sorted(wk):
    S = wk[k]
    R1 = [t for t in S if t["rung"] == 1]
    sure = [t for t in S if t["state"] != "uncertain"]
    print(f"  {k}: tasks {len(S)}, rung 1 {len(R1)}, F6 no {pct(sum(1 for t in R1 if v(t, 'F6') == 'no'), len(R1))}, "
          f"F4 no {pct(sum(1 for t in R1 if v(t, 'F4') == 'no'), len(R1))}, Directed+ {pct(sum(1 for t in sure if t['rung'] >= 2), len(sure))}")

print("\n== tips shown (card_delivery), by tip id and week ==")
tip_of = {}
for cid, cj in con.execute("SELECT card_id, card_json FROM card_decision WHERE outcome = 'show_ready'"):
    try:
        tip_of[cid] = json.loads(cj).get("tip_id")
    except Exception:
        tip_of[cid] = None
dl = list(con.execute("SELECT card_id, surface, delivered_utc_ms FROM card_delivery"))
print(f"  deliveries {len(dl)}; first {ts(min(d[2] for d in dl)) if dl else '-'}; last {ts(max(d[2] for d in dl)) if dl else '-'}")
print(f"  by tip id: {dict(collections.Counter(tip_of.get(d[0]) for d in dl).most_common())}")
print(f"  by surface: {dict(collections.Counter(d[1] for d in dl))}")
print(f"  by week: {dict(sorted(collections.Counter(time.strftime('%G-W%V', time.gmtime(d[2] / 1000)) for d in dl).items()))}")
fb = list(con.execute("SELECT tip_id, answer, utc_ms FROM tip_feedback"))
print(f"  feedback rows {len(fb)}: {dict(collections.Counter((r[0], r[1]) for r in fb).most_common(15))}")
print(f"  card opens: {con.execute('SELECT COUNT(*), MIN(utc_ms), MAX(utc_ms) FROM card_open').fetchone()}")
