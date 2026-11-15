"""Read-only, counts only: which checks a paused Kind's "agreed only k of n times" rests on now.

The pause rule (aiq-core coaching::readings + aiq-domain check::verdicts) counts only second readings whose judgment
is still the task's current one, newest check run first, whole runs at a time, until the scope's minimum (5 for a
Kind). A task analyzed again since (a new analysis version) drops out. This prints, for one Kind:
  - its current analyses by analysis version (how many are on the newest wording, how many wait for re-analysis);
  - every check run that read a task of the Kind: when (days ago), its scope, its check version, how many of its
    readings of the Kind still count and how many dropped because the task was analyzed again, and agreement;
  - the window the pause rule takes, and the verdict it gives.
Counts, versions and days only: no prompt, note, folder or id.

Usage:
    python3 -I pause_window.py <path to aiq.db> [<job>/<kind>] [<minimum, default 5>] [<bar, default 0.8>]
"""
import collections
import json
import sqlite3
import sys
import time

DB = sys.argv[1]
SCOPE = sys.argv[2] if len(sys.argv) > 2 else "ask_research/find"
MIN = int(sys.argv[3]) if len(sys.argv) > 3 else 5
BAR = float(sys.argv[4]) if len(sys.argv) > 4 else 0.8
job, kind = SCOPE.split("/", 1)
now = int(time.time() * 1000)
con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True, timeout=5)
con.execute("PRAGMA query_only = ON")

# Current judgment per task, as Store::current_judgments: newest judged_utc_ms, then newest rowid.
current = {}
for jid, task, judged, jjob, jtype, ver in con.execute(
        "SELECT judgment_id, task_id, judged_utc_ms, job, task_type, judge_version FROM judgment ORDER BY task_id, judged_utc_ms, rowid"):
    current[task] = (jid, jjob, jtype, ver)
current_ids = {v[0]: v for v in current.values()}
by_version = collections.Counter(v[3] for v in current.values() if v[1] == job and v[2] == kind)
print(f"current analyses of {SCOPE} by analysis version: {dict(sorted(by_version.items()))}")

judgment = {jid: (jjob, jtype, ver) for jid, jjob, jtype, ver in
            con.execute("SELECT judgment_id, job, task_type, judge_version FROM judgment")}


def agrees(judge_job, judge_type, ans):
    job_ok = ans.get("job") == judge_job
    return job_ok and (ans.get("task_type") == judge_type or judge_type in (ans.get("task_types_also_fit") or []))


rows = []
for run_id, started, scope, version in con.execute(
        "SELECT run_id, started_utc_ms, scope, check_version FROM judge_check_run ORDER BY started_utc_ms DESC, rowid DESC"):
    counted = dropped = agreed = 0
    counted_versions = collections.Counter()
    for jid, answer in con.execute("SELECT judgment_id, answer FROM judge_check WHERE run_id = ?", (run_id,)):
        j = judgment.get(jid)
        if j is None or j[0] != job or j[1] != kind:
            continue
        try:
            ans = json.loads(answer) if answer else None
        except ValueError:
            ans = None
        if not isinstance(ans, dict) or "job" not in ans or "task_type" not in ans:
            continue
        if jid not in current_ids:
            dropped += 1
            continue
        counted += 1
        counted_versions[j[2]] += 1
        agreed += agrees(j[0], j[1], ans)
    if counted or dropped:
        rows.append((run_id, started, scope or "all jobs", version, counted, dropped, agreed, counted_versions))

print(f"\ncheck runs that read a task AIQ analyzed as {SCOPE} (newest first):")
for i, (_, started, scope, version, counted, dropped, agreed, vers) in enumerate(rows):
    print(f"  run {i + 1}: {(now - started) / 86_400_000:.1f} days ago, scope {scope}, check {version}: "
          f"{counted} still count ({agreed} agree; analyses {dict(sorted(vers.items()))}), "
          f"{dropped} dropped (task analyzed again since)")

k = n = 0
used = 0
for _, _, _, _, counted, _, agreed, _ in rows:
    if counted == 0:
        continue
    if n >= MIN:
        break
    k, n, used = k + agreed, n + counted, used + 1
paused = n >= MIN and k < BAR * n
print(f"\nwindow: {used} run(s) with readings that still count, agreed {k} of {n}; "
      f"{'PAUSED' if paused else ('not enough readings' if n < MIN else 'clears the bar')}")
