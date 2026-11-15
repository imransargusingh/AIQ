"""Read-only: why AIQ did not set aside tasks the person says a scheduled prompt sent.

For the newest check run of one Kind (scope kind:<job>/<kind>, default ask_research/find), it groups the tasks by
the Kind the second reading picked and counts, per group, what AIQ's set-aside rule (aiq import.rs `automated`) saw
in each task's session: how many typed prompts, whether a first-prompt template was recorded and how many sessions
in the same folder share it (rule T needs `template_sessions`, 11 by default), how many sessions the folder holds,
how many of them were set aside, whether the task was the session's first prompt, and whether the prompt starts
with Copilot's scheduled-prompt mark. A group is shown whole, so a long round-up group can be compared with the rest.

It prints counts and Kind ids only (no prompt, note, folder or id). The store is opened mode=ro with query_only,
not immutable, so rows still in the write-ahead log are read too (DC-090).

Usage:
    python3 -I scheduled_shape.py <path to aiq.db> [<job>/<kind>]
"""
import collections
import json
import sqlite3
import sys

DB = sys.argv[1]
SCOPE = sys.argv[2] if len(sys.argv) > 2 else "ask_research/find"
con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True, timeout=5)
con.execute("PRAGMA query_only = ON")

run = con.execute("SELECT run_id FROM judge_check_run WHERE scope = ? ORDER BY started_utc_ms DESC LIMIT 1",
                  (f"kind:{SCOPE}",)).fetchone()
if not run:
    print(f"no check run of {SCOPE} only")
    sys.exit(0)
job, kind = SCOPE.split("/", 1)

# One import record per session (primary key tool, source_session_id).
sessions = {}
for row in con.execute("""SELECT tool, source_session_id, folder, human_turns, template_key, outcome, why
                          FROM import_session"""):
    sessions[(row[0], row[1])] = row
per_folder = collections.Counter()
per_folder_aside = collections.Counter()
per_template = collections.Counter()
for tool, _sid, folder, _turns, template, outcome, _why in sessions.values():
    per_folder[(tool, folder)] += 1
    per_folder_aside[(tool, folder)] += outcome == "automated"
    if template is not None:
        per_template[(tool, folder, template)] += 1


def band(n, cuts):
    for lo, hi, name in cuts:
        if lo <= n < hi:
            return name
    return "?"


groups = collections.defaultdict(collections.Counter)
folders = collections.defaultdict(set)
for ans_text, task_id in con.execute("SELECT answer, task_id FROM judge_check WHERE run_id = ? AND outcome = 'ok'", run):
    try:
        ans = json.loads(ans_text)
    except (TypeError, ValueError):
        ans = {}
    g = kind if ans.get("job") == job and ans.get("task_type") == kind else f"{ans.get('job')}/{ans.get('task_type')}"
    c = groups[g]
    c["tasks"] += 1
    if not task_id.startswith("import:"):
        c["live (not imported)"] += 1
        continue
    tool, rest = task_id[len("import:"):].split(":", 1)
    sid, seq = rest.rsplit(":", 1)
    s = sessions.get((tool, sid))
    if s is None:
        c["session not in the import record"] += 1
        continue
    _, _, folder, turns, template, outcome, why = s
    folders[g].add((tool, folder))
    c[f"tool {tool}"] += 1
    c[f"typed prompts in its session: {band(turns or 0, [(0, 1, '0'), (1, 2, '1'), (2, 4, '2-3'), (4, 1 << 30, '4+')])}"] += 1
    c["task is the session's first prompt" if int(seq) == 0 else "task is a later prompt"] += 1
    if template is None:
        c["no template recorded"] += 1
    else:
        n = per_template[(tool, folder, template)]
        c[f"sessions sharing its template in the folder: {band(n, [(1, 2, '1'), (2, 11, '2-10'), (11, 1 << 30, '11+')])}"] += 1
    n = per_folder[(tool, folder)]
    c[f"sessions in its folder: {band(n, [(1, 11, '1-10'), (11, 101, '11-100'), (101, 1 << 30, '101+')])}"] += 1
    share = per_folder_aside[(tool, folder)] / n if n else 0
    c[f"its folder set aside: {band(int(share * 100), [(0, 1, 'none'), (1, 50, 'under half'), (50, 81, 'half to 80%'), (81, 101, 'over 80%')])}"] += 1
    c[f"session outcome: {outcome}"] += 1
    text = con.execute("SELECT text FROM import_event WHERE tool = ? AND source_session_id = ? AND seq = ?",
                       (tool, sid, int(seq))).fetchone()
    if text and text[0] and text[0].lstrip().startswith("[Scheduled prompt"):
        c["starts with Copilot's scheduled-prompt mark"] += 1

order = sorted(groups, key=lambda g: (g != kind, -groups[g]["tasks"]))
for g in order:
    print(f"\n{g}: {groups[g]['tasks']} tasks, {len(folders[g])} folders")
    for k, v in sorted(groups[g].items()):
        if k != "tasks":
            print(f"  {k}: {v}")
