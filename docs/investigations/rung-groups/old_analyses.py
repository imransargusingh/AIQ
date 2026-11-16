"""Read-only, counts only: why some current analyses of one Kind stay on an older analysis version.

The background pass redoes an outdated analysis only for a task that is still the person's own on this computer
(own_tasks: not set aside, not synced from another computer). For the current analyses of one Kind
(default ask_research/find) not at the newest version, this counts: imported or live; for an imported task, AIQ's
import outcome for its session today (imported or set aside as automated) and the rule that set it aside; and how
many analysis calls the task had since a moment, by outcome.
Nothing else is printed: no prompt, id or folder.

Usage:
    python3 -I old_analyses.py <path to aiq.db> [<job>/<kind>] [<newest version, default j9>] [<since ISO UTC>]
"""
import collections
import datetime
import sqlite3
import sys

DB = sys.argv[1]
SCOPE = sys.argv[2] if len(sys.argv) > 2 else "ask_research/find"
NEWEST = sys.argv[3] if len(sys.argv) > 3 else "j9"
SINCE = sys.argv[4] if len(sys.argv) > 4 else "2026-11-15T22:48:00Z"
since = int(datetime.datetime.fromisoformat(SINCE.replace("Z", "+00:00")).timestamp() * 1000)
job, kind = SCOPE.split("/", 1)
con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True, timeout=5)
con.execute("PRAGMA query_only = ON")

current = {}
for task, jjob, jtype, ver in con.execute(
        "SELECT task_id, job, task_type, judge_version FROM judgment ORDER BY task_id, judged_utc_ms, rowid"):
    current[task] = (jjob, jtype, ver)
sessions = {(t, s.lower()): (o, w) for t, s, o, w in
            con.execute("SELECT tool, source_session_id, outcome, why FROM import_session")}
c = collections.Counter()
for task, (jjob, jtype, ver) in current.items():
    if jjob != job or jtype != kind or ver == NEWEST:
        continue
    c[f"version {ver}"] += 1
    if task.startswith("import:"):
        tool, rest = task[len("import:"):].split(":", 1)
        sid = rest.rsplit(":", 1)[0].lower()
        o, why = sessions.get((tool, sid), ("session not in the import record", ""))
        c[f"imported, session now {o}{' (' + why + ')' if why else ''}"] += 1
    else:
        c["live"] += 1
    calls = collections.Counter(r[0] for r in con.execute(
        "SELECT outcome FROM judge_call WHERE task_id = ? AND started_utc_ms >= ?", (task, since)))
    c["no analysis call since" if not calls else "calls since: " + ", ".join(f"{k} {v}" for k, v in sorted(calls.items()))] += 1
print(f"current analyses of {SCOPE} not at {NEWEST}:")
for k, v in sorted(c.items()):
    print(f"  {k}: {v}")
