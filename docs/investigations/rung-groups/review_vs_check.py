"""Read-only: does the accuracy check c2 agree with the person's own marks from the paused-Kinds review page?

The review page (paused_review.py) saved aiq-review-answers.json: per task, the analysis id it showed, the paused
Kind, whether the first check (c1) agreed, and the person's mark. This script follows each analysis id to its task,
finds the newest c2 reading of that task (check c2 lets the second reading list other Kinds that also fit, aiq-domain
check::kind_agrees), and counts how often c2 now agrees where the person said AIQ's Kind fits, and disagrees where
they said it does not.

Marks: right / aiq / both mean AIQ's Kind fits; wrong / second / neither mean it does not; agent (an agent started
the task) is not the person's to count and is left out, as AIQ leaves such tasks out.

It prints counts only (no prompt, note, folder or id). The store is opened mode=ro with query_only, not immutable,
so the newest rows in the write-ahead log are read too (DC-090).

Usage:
    python3 -I review_vs_check.py <path to aiq.db> <path to aiq-review-answers.json>
"""
import collections
import json
import sqlite3
import sys

DB, ANSWERS = sys.argv[1], sys.argv[2]
con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True, timeout=5)
con.execute("PRAGMA query_only = ON")

FITS = {"right": True, "aiq": True, "both": True, "wrong": False, "second": False, "neither": False}

with open(ANSWERS, encoding="utf-8") as f:
    answers = json.load(f)["answers"]

jcols = {r[1] for r in con.execute("PRAGMA table_info(judgment)")}
kind_col = "task_type" if "task_type" in jcols else "NULL"


def judgment(jid):
    return con.execute(f"SELECT task_id, job, {kind_col} FROM judgment WHERE judgment_id = ?", (jid,)).fetchone()


def current(task_id):
    return con.execute(
        f"SELECT judgment_id, job, {kind_col}, judge_version FROM judgment WHERE task_id = ?"
        " ORDER BY judged_utc_ms DESC, rowid DESC LIMIT 1", (task_id,)).fetchone()


def newest_reading(task_id, version):
    return con.execute(
        """SELECT c.judgment_id, c.answer FROM judge_check c JOIN judge_check_run r ON r.run_id = c.run_id
           WHERE c.task_id = ? AND c.outcome = 'ok' AND r.check_version = ?
           ORDER BY c.checked_utc_ms DESC LIMIT 1""", (task_id, version)).fetchone()


out = collections.Counter()
by_scope = collections.defaultdict(collections.Counter)
now_kinds = collections.Counter()
for a in answers:
    scope, mark = a.get("scope") or "?", a.get("mark")
    s = by_scope[scope]
    s["answers"] += 1
    if mark is None:
        s["no mark"] += 1
        continue
    if mark == "agent":
        s["agent started (left out)"] += 1
        continue
    if mark not in FITS:
        s[f"mark not known: {mark}"] += 1
        continue
    j = judgment(a.get("task"))
    if j is None:
        s["analysis id not in this store"] += 1
        continue
    task_id = j[0]
    cur = current(task_id)
    if cur is None:
        s["task no longer analyzed"] += 1
        continue
    now_kinds[f"{cur[1]}/{cur[2]}"] += 1
    if (cur[1], cur[2]) != (j[1], j[2]):
        s["analysis kind changed since the review"] += 1
    r = newest_reading(task_id, "c2")
    if r is None:
        s["no c2 reading yet"] += 1
        continue
    read_j = con.execute(f"SELECT job, {kind_col} FROM judgment WHERE judgment_id = ?", (r[0],)).fetchone()
    try:
        ans = json.loads(r[1])
    except (TypeError, ValueError):
        s["c2 answer unreadable"] += 1
        continue
    job_ok = ans.get("job") == read_j[0]
    strict = job_ok and ans.get("task_type") == read_j[1]
    agrees = strict or (job_ok and read_j[1] in (ans.get("task_types_also_fit") or []))
    fits = FITS[mark]
    s["c2 read"] += 1
    key = ("you: fits" if fits else "you: does not fit") + (" / c2 agrees" if agrees else " / c2 disagrees")
    s[key] += 1
    if agrees and not strict:
        s["c2 agrees only through also-fits"] += 1
    s["c2 matches you" if agrees == fits else "c2 differs from you"] += 1
    s["first check (c1) matched you" if a.get("agreed") == fits else "first check (c1) differed from you"] += 1
    out["c2 matches you" if agrees == fits else "c2 differs from you"] += 1
    out["c1 matched you" if a.get("agreed") == fits else "c1 differed from you"] += 1

print(f"answers: {len(answers)}")
for scope in sorted(by_scope):
    print(f"\n{scope}")
    for k, v in sorted(by_scope[scope].items()):
        print(f"  {k}: {v}")
print("\nwhat AIQ's analysis says now for the marked tasks (job/kind: tasks)")
for k, v in sorted(now_kinds.items()):
    print(f"  {k}: {v}")
print("\nwhere c2 has read the task:")
for k, v in sorted(out.items()):
    print(f"  {k}: {v}")
