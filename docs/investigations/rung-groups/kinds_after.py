"""Read-only, counts only: what the re-cut Kinds list (analysis j7, AIQ 0.1.65) did on this computer's store.

1. The re-analysis: j7 analyses by computer and tool, and the tasks that hold more than one j7 analysis, with
   which computer wrote each one and how far apart, so a task analyzed twice can be traced to its cause.
2. Home's ladder with the shipped rules for a task with no rung (aiq-domain coach::no_rung): a progress check
   first, then a release notice. All time and the last 30 days, on Home's rebuilt task list (kinds_home.py).

Prints no text from the store: only counts and the fixed word lists below. The store is opened
mode=ro&immutable=1 (no lock, nothing written).

Usage (same folder as kinds_home.py):
    python3 -I kinds_after.py <path to aiq.db>
"""
import collections
import contextlib
import io
import json
import re
import runpy
import sqlite3
import sys

buf = io.StringIO()
with contextlib.redirect_stdout(buf):
    home = runpy.run_path("kinds_home.py", run_name="kinds_home")
H, tasks, NOW, DAY = home["H"], home["tasks"], home["NOW"], home["DAY"]
person_words = home["person_words"]

J7 = "j7"
MAX_WORDS = 25  # coach::PROGRESS_CHECK_MAX_WORDS
PROGRESS_KIND = ("ask_research", "progress")  # coach::PROGRESS_KIND under rung::ANSWER_JOB
RELEASE_KIND = ("run_ship", "release")  # coach::RELEASE_KIND
PROGRESS_CUES = [["status"], ["progress"], ["where", "are", "you"], ["why", "did", "you", "stop"]]
RELEASE_WORDS = {"published", "released"}  # coach::RELEASE_NOTICE_WORDS


def cnt(it):
    return dict(sorted(collections.Counter(it).items(), key=lambda kv: (-kv[1], str(kv[0]))))


con = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro&immutable=1", uri=True)
jcols = [r[1] for r in con.execute("PRAGMA table_info(judgment)")]
jcomp = "computer" if "computer" in jcols else "NULL"

# Computers by a stable short name: this one, then others in order of first appearance.
names = {None: "this computer"}


def name(c):
    if c not in names:
        names[c] = f"other computer {len(names)}"
    return names[c]


# ---------------------------------------------------------------- 1. the re-analysis
rows = list(con.execute(
    f"SELECT task_id, provider, judged_utc_ms, {jcomp}, read, job, task_type FROM judgment WHERE judge_version = ? "
    "ORDER BY judged_utc_ms, rowid", (J7,)))
by_task = collections.defaultdict(list)
for r in rows:
    by_task[r[0]].append(r)
task_of = {t["task_id"]: t for t in tasks}
print(f"== j7 analyses: {len(rows)} rows over {len(by_task)} tasks ==")
print(f"  by computer {cnt(name(r[3]) for r in rows)}; by tool {cnt(r[1] for r in rows)}")
if rows:
    print(f"  first {rows[0][2]} last {rows[-1][2]} (ms UTC)")

doubles = {tid: rs for tid, rs in by_task.items() if len(rs) > 1}
print(f"\n== tasks with more than one j7 analysis: {len(doubles)} ==")
print(f"  rows per task {cnt(len(rs) for rs in doubles.values())}")
print(f"  who wrote them (in order) {cnt(' + '.join(name(r[3]) for r in rs) for rs in doubles.values())}")


def keys(read):
    try:
        return set(json.loads(read).get("keys") or [])
    except Exception:
        return set()


shape = collections.Counter()
for tid, rs in doubles.items():
    a, b = rs[0], rs[-1]
    gap = (b[2] - a[2]) / 60000
    gap_b = "under 10 min" if gap < 10 else ("10-60 min" if gap < 60 else "over 60 min")
    ka, kb = keys(a[4]), keys(b[4])
    grew = "later read more prompts" if kb > ka else ("same prompts" if ka == kb else "different prompts")
    t = task_of.get(tid)
    owner = "not on Home" if t is None else name(t["computer"])
    same = "same answer" if (a[5], a[6]) == (b[5], b[6]) else "different job or kind"
    shape[(owner, gap_b, grew, same, a[1])] += 1
print("  (task's computer on Home, time between first and last, prompts read, answer, tool): tasks")
for k, n in shape.most_common():
    print(f"    {k}: {n}")

# Is a doubled task's prompt recorded by more than one computer? Live prompts are events; imported ones are
# import_event rows of a session that both computers may hold.
ev_comp = collections.defaultdict(set)
if "computer" in [r[1] for r in con.execute("PRAGMA table_info(event)")]:
    for eid, c in con.execute("SELECT event_id, computer FROM event"):
        ev_comp[eid].add(c)
print(f"  doubled tasks whose first prompt is a live event: {sum(1 for tid in doubles if tid in ev_comp)}; "
      f"of the others (imported or not found): {sum(1 for tid in doubles if tid not in ev_comp)}")
print(f"  doubled tasks by origin on Home {cnt((task_of[tid]['origin'] if tid in task_of else 'not on Home') for tid in doubles)}")

# ---------------------------------------------------------------- 2. Home's ladder with the shipped rules
f4 = {}
for tid, facts in con.execute(
    """SELECT j.task_id, j.facts FROM judgment j WHERE j.rowid = (SELECT x.rowid FROM judgment x WHERE x.task_id = j.task_id
       ORDER BY x.judged_utc_ms DESC, x.rowid DESC LIMIT 1)"""
):
    try:
        f4[tid] = (json.loads(facts).get("F4") or {}).get("value") is True
    except Exception:
        f4[tid] = False

text_of = {}
for r in home["kept"]:
    text_of[r[0]] = r[5]
for p in home["imp"]:
    text_of[p[0]] = p[5]


def short_words(s):
    if s is None:
        return None
    pw = person_words(s)
    if pw is None:
        return None
    w = [x.lower() for x in re.split(r"[^\w']+", pw[1]) if x]
    return w if 1 <= len(w) <= MAX_WORDS else None


def has(words, cue):
    return any(words[i:i + len(cue)] == cue for i in range(len(words) - len(cue) + 1))


def no_rung(t):
    if t["state"] == "uncertain" or t["rung"] != 1 or f4.get(t["task_id"], False):
        return None
    kind = (t["job"], t["kind"])
    words = short_words(text_of.get(t["task_id"]))
    if kind == PROGRESS_KIND or (words and any(has(words, c) for c in PROGRESS_CUES)):
        return "progress check"
    if kind == RELEASE_KIND and words and RELEASE_WORDS & set(words):
        return "release notice"
    return None


for label, S in (("all time", H), ("last 30 days (first prompt)", [t for t in H if t["first"] >= NOW - 30 * DAY])):
    why = {t["task_id"]: no_rung(t) for t in S}
    sure = [t for t in S if t["state"] != "uncertain"]
    print(f"\n== Home's ladder, {label}: {len(S)} analyzed tasks ==")
    print(f"  analyses by version {cnt(t['jv'] for t in S)}")
    print(f"  sure rungs before the no-rung rules {dict(sorted(collections.Counter(t['rung'] for t in sure).items()))}; unsure {len(S) - len(sure)}")
    print(f"  no rung of its own {cnt(w for w in why.values() if w)}")
    on = [t for t in sure if why[t["task_id"]] is None]
    print(f"  on the ladder {dict(sorted(collections.Counter(t['rung'] for t in on).items()))} (sure Assisted on the ladder: "
          f"{sum(1 for t in on if t['rung'] == 1)})")
    named = sum(1 for t in S if t["job"] != "unclassified" and t["kind"] not in (None, "other"))
    print(f"  named kind {named}; kind other {sum(1 for t in S if t['job'] != 'unclassified' and t['kind'] == 'other')}; "
          f"no job {sum(1 for t in S if t['job'] == 'unclassified')}; no kind recorded "
          f"{sum(1 for t in S if t['job'] != 'unclassified' and t['kind'] is None)}")
    print(f"  progress checks by how found: kind {sum(1 for t in S if why[t['task_id']] == 'progress check' and (t['job'], t['kind']) == PROGRESS_KIND)}, "
          f"words only {sum(1 for t in S if why[t['task_id']] == 'progress check' and (t['job'], t['kind']) != PROGRESS_KIND)}")
