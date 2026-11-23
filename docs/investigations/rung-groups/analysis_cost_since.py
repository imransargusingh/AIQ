"""Read-only, counts only: what the background analysis spent since a moment (for example a re-read of history).

Counts analyses written since the moment by analysis version and why the task was analyzed (imported or live;
new task or analyzed again), analysis calls by provider and outcome (each Copilot call is about one premium
request), and the label and check calls in the same window. Nothing else is printed: no prompt, id or folder.

Usage:
    python3 -I analysis_cost_since.py <path to aiq.db> <since, ISO UTC such as 2026-11-15T22:48:00Z>
"""
import collections
import datetime
import sqlite3
import sys

DB, SINCE = sys.argv[1], sys.argv[2]
since = int(datetime.datetime.fromisoformat(SINCE.replace("Z", "+00:00")).timestamp() * 1000)
con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True, timeout=5)
con.execute("PRAGMA query_only = ON")


def has_table(t):
    return con.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name=?", (t,)).fetchone() is not None


new_or_again = collections.Counter()
by_version = collections.Counter()
for task, judged, version in con.execute(
        "SELECT task_id, judged_utc_ms, judge_version FROM judgment WHERE judged_utc_ms >= ?", (since,)):
    before = con.execute("SELECT 1 FROM judgment WHERE task_id = ? AND judged_utc_ms < ? LIMIT 1", (task, judged)).fetchone()
    origin = "imported" if task.startswith("import:") else "live"
    new_or_again[(origin, "analyzed again" if before else "first analysis")] += 1
    by_version[version] += 1
print(f"analyses since {SINCE}: {sum(by_version.values())}  by version {dict(sorted(by_version.items()))}")
for (origin, why), n in sorted(new_or_again.items()):
    print(f"  {origin}, {why}: {n}")

calls = collections.Counter()
hours = collections.Counter()
for provider, outcome, started in con.execute(
        "SELECT provider, outcome, started_utc_ms FROM judge_call WHERE started_utc_ms >= ?", (since,)):
    calls[(provider, outcome)] += 1
    hours[(started - since) // 3_600_000] += 1
print(f"analysis calls since {SINCE}: {sum(calls.values())}")
for (provider, outcome), n in sorted(calls.items()):
    print(f"  {provider} {outcome}: {n}")
if hours:
    print("  calls per hour after the moment: " + ", ".join(f"h{h} {n}" for h, n in sorted(hours.items())))

if has_table("kind_label"):
    cols = {r[1] for r in con.execute("PRAGMA table_info(kind_label)")}
    at = next((c for c in ("labeled_utc_ms", "created_utc_ms", "utc_ms") if c in cols), None)
    if at:
        n = con.execute(f"SELECT COUNT(*) FROM kind_label WHERE {at} >= ?", (since,)).fetchone()[0]
        print(f"kind labels since {SINCE}: {n}")
n = con.execute("SELECT COUNT(*) FROM judge_check WHERE checked_utc_ms >= ?", (since,)).fetchone()[0]
print(f"check readings since {SINCE}: {n}")
