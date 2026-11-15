"""Read-only, counts only: why plan follow-ons and progress checks read as Assisted.
Reuses kinds_cross.py (Home's task list rebuild, keyword groups, position in session). Prints no text from the store.
Usage (same folder as the other scripts): python3 -I rung_groups.py <path to aiq.db>
"""
import collections
import contextlib
import io
import json
import runpy
import sqlite3
import sys

buf = io.StringIO()
with contextlib.redirect_stdout(buf):
    g = runpy.run_path("kinds_cross.py", run_name="kinds_cross")
H, NOW, DAY = g["H"], g["NOW"], g["DAY"]
HOUR = DAY // 24
con = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True, timeout=5)  # not immutable: that skips rows still in the WAL
con.execute("PRAGMA query_only = ON")

PLAN = "carrying out a plan or continuing a build"
PROG = "checking on progress or status"
FACTS = ["F1", "F4", "F5", "F6", "F7", "F12", "F13", "F14"]

row = {}
for r in con.execute(
    """SELECT j.task_id, j.model_rung, j.current_rung, j.facts, j.fact_set_version, j.placement
       FROM judgment j WHERE j.rowid = (SELECT x.rowid FROM judgment x WHERE x.task_id = j.task_id
                                        ORDER BY x.judged_utc_ms DESC, x.rowid DESC LIMIT 1)"""
):
    try:
        facts = json.loads(r[3])
    except Exception:
        facts = {}
    try:
        n_place = len(json.loads(r[5]))
    except Exception:
        n_place = None
    row[r[0]] = dict(model=r[1], cur=r[2], facts=facts, fs=r[4], n_place=n_place)


def val(f):
    if not isinstance(f, dict):
        return "absent"
    v = f.get("value")
    src = f.get("source") or "?"
    return f"{'yes' if v is True else 'no' if v is False else 'unknown' if v is None else 'other'}/{src}"


def cnt(it):
    return dict(sorted(collections.Counter(it).items(), key=lambda kv: (-kv[1], str(kv[0]))))


def quiet(t):
    p = t.get("prev")
    if not p:
        return "first in session"
    d = t["first"] - p["last"]
    return "under 10 min" if d < HOUR / 6 else "10-60 min" if d < HOUR else "1-4 h" if d < 4 * HOUR else "over 4 h"


for label, bucket, lo in ((PLAN, PLAN, NOW - 30 * DAY), (PROG, PROG, 0)):
    S = [t for t in H if t["b"] == bucket and t["first"] >= lo and t["task_id"] in row]
    A = [t for t in S if t["state"] != "uncertain" and t["rung"] == 1]
    print(f"== {label} ({'last 30 days' if lo else 'all time'}): {len(S)} tasks with an analysis, {len(A)} sure Assisted ==")
    print(f"  model's own rung on the Assisted: {cnt(row[t['task_id']]['model'] for t in A)}")
    print(f"  fact set: {cnt(row[t['task_id']]['fs'] for t in A)}")
    for fid in FACTS:
        print(f"  {fid} on the Assisted: {cnt(val(row[t['task_id']]['facts'].get(fid)) for t in A)}")
    print(f"  Directed needs F4 and F6 yes; Assisted with F4 not yes: {sum(1 for t in A if val(row[t['task_id']]['facts'].get('F4')).split('/')[0] != 'yes')}, "
          f"F6 not yes: {sum(1 for t in A if val(row[t['task_id']]['facts'].get('F6')).split('/')[0] != 'yes')}, "
          f"both: {sum(1 for t in A if all(val(row[t['task_id']]['facts'].get(x)).split('/')[0] != 'yes' for x in ('F4', 'F6')))}")
    print(f"  plan named (F7 yes from the log) on the Assisted: {sum(1 for t in A if val(row[t['task_id']]['facts'].get('F7')) == 'yes/log')}")
    print(f"  position: {cnt(quiet(t) for t in S)}")
    prev = [t["prev"] for t in S if t.get("prev")]
    print(f"  previous task in the same session: rung {cnt(p['rung'] for p in prev)}, bucket {cnt(p['b'] == bucket for p in prev)} (True = same group)")
    print(f"  prompts per task: {cnt(g['plen'](t) for t in S)}; by tool {cnt(t['tool'] for t in S)}")
