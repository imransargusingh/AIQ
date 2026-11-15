"""Read-only, counts only: does timing tell a scheduler's Copilot sessions from the person's own?

The fields of a scheduler's sessions match the person's (scheduled_marks.py), and three scheduled round-ups in the
newest check of one Kind still count as typed: one-off prompts, in folders with nothing set aside. A scheduler may
still leave a shape in time. Per group of sessions this counts:
  - when the session starts: seconds past the minute (a timer fires near :00), and minutes from a quarter hour;
  - how fast the first prompt follows the start, and how fast each later prompt follows the agent's last turn end
    (a person reads and types; a script sends at once);
  - whether another session in the same folder started at the same time of day (within 3 minutes) on another day,
    or at the same time of week on another week (a daily or weekly schedule);
  - which event types the session file holds;
  - optionally, how many of the folders contain a word given on the command line (the word is not printed).
Groups: the newest check run of one Kind (scope kind:<job>/<kind>, default ask_research/find) split by the Kind the
second reading picked (the scheduled one, default research) and by AIQ's outcome; then up to 300 of the newest other
imported Copilot sessions (the person's own) and 300 of the newest set-aside ones.
Nothing else is printed: no prompt, folder, id, path or time.

Usage:
    python3 -I scheduled_timing.py <aiq.db> <copilot home> [<job>/<kind>] [<scheduled Kind>] [--folder-word <word>]
"""
import collections
import datetime
import json
import os
import re
import sqlite3
import statistics
import sys

args = sys.argv[1:]
WORD = None
if "--folder-word" in args:
    i = args.index("--folder-word")
    WORD = args[i + 1].lower()
    del args[i:i + 2]
DB, HOME = args[0], args[1]
SCOPE = args[2] if len(args) > 2 else "ask_research/find"
SCHEDULED = args[3] if len(args) > 3 else "research"
SAMPLE = 300
DAY, WEEK, NEAR = 86_400_000, 7 * 86_400_000, 3 * 60_000
SAFE = re.compile(r"^[A-Za-z0-9_.]{1,40}$")
con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True, timeout=5)
con.execute("PRAGMA query_only = ON")

run = con.execute("SELECT run_id FROM judge_check_run WHERE scope = ? ORDER BY started_utc_ms DESC LIMIT 1",
                  (f"kind:{SCOPE}",)).fetchone()
if not run:
    print(f"no check run of {SCOPE} only")
    sys.exit(0)
job, kind = SCOPE.split("/", 1)
rows = {sid.lower(): (folder, started, outcome) for sid, folder, started, outcome in con.execute(
    "SELECT source_session_id, folder, started_utc_ms, outcome FROM import_session WHERE tool = 'copilot-cli'")}
group = {}
for ans_text, task_id in con.execute("SELECT answer, task_id FROM judge_check WHERE run_id = ? AND outcome = 'ok'", run):
    if not task_id.startswith("import:copilot-cli:"):
        continue
    sid = task_id[len("import:copilot-cli:"):].rsplit(":", 1)[0].lower()
    try:
        ans = json.loads(ans_text)
    except (TypeError, ValueError):
        ans = {}
    outcome = rows.get(sid, (None, None, "not in the import record"))[2]
    picked = ans.get("task_type") if ans.get("job") == job else None
    g = f"scheduled, now {outcome}" if picked == SCHEDULED else f"rest of the run, now {outcome}"
    if not group.get(sid, "").startswith("scheduled"):
        group[sid] = g
for outcome, name in (("imported", "other imported (yours)"), ("automated", "other set aside")):
    rest = sorted((started or 0, sid) for sid, (_, started, o) in rows.items() if o == outcome and sid not in group)
    for _, sid in rest[-SAMPLE:]:
        group[sid] = name
print(f"sessions by group {dict(sorted(collections.Counter(group.values()).items()))}")

by_folder = collections.defaultdict(list)
for sid, (folder, started, _) in rows.items():
    if started:
        by_folder[folder].append((started, sid))


def recurs(sid, period):
    folder, started, _ = rows[sid]
    if not started:
        return None
    for other_start, other in by_folder[folder]:
        if other == sid or abs(other_start - started) < period - NEAR:
            continue
        off = (other_start - started) % period
        if min(off, period - off) <= NEAR:
            return True
    return False


def ms(t):
    if isinstance(t, (int, float)):
        return int(t)
    if isinstance(t, str):
        try:
            return int(datetime.datetime.fromisoformat(t.replace("Z", "+00:00")).timestamp() * 1000)
        except ValueError:
            return None
    return None


def band(x, cuts):
    for hi, name in cuts:
        if x < hi:
            return name
    return cuts[-1][1]


GAP = [(1_000, "under 1 s"), (5_000, "1-5 s"), (30_000, "5-30 s"), (300_000, "30 s-5 min"), (1 << 62, "over 5 min")]
counts = collections.defaultdict(collections.Counter)
types = collections.defaultdict(collections.Counter)
seen = collections.Counter()
state = os.path.join(HOME, "session-state")
for sid in os.listdir(state) if os.path.isdir(state) else []:
    g = group.get(sid.lower())
    path = os.path.join(state, sid, "events.jsonl")
    if g is None or not os.path.isfile(path):
        continue
    start = first_msg = last_end = None
    later_gaps, kinds = [], set()
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            for i, line in enumerate(f):
                if i >= 50000:
                    break
                try:
                    e = json.loads(line)
                except ValueError:
                    continue
                if e.get("agentId"):
                    continue  # a sub-agent's events are not the person's conversation
                t, ts = e.get("type"), ms(e.get("timestamp"))
                if isinstance(t, str) and SAFE.match(t):
                    kinds.add(t)
                if t == "session.start" and start is None:
                    start = ms((e.get("data") or {}).get("startTime")) or ts
                elif t == "user.message" and ts is not None:
                    if (e.get("data") or {}).get("isAutopilotContinuation") is True:
                        continue
                    if first_msg is None:
                        first_msg = ts
                    elif last_end is not None:
                        later_gaps.append(ts - last_end)
                elif t == "assistant.turn_end" and ts is not None:
                    last_end = ts
    except OSError:
        continue
    seen[g] += 1
    c = counts[g]
    for k in kinds:
        types[g][k] += 1
    if start is None:
        c["no start time"] += 1
    else:
        sec = (start // 1000) % 60
        c[f"start second past the minute: {band(sec, [(5, '0-4'), (15, '5-14'), (60, '15-59')])}"] += 1
        minute = (start // 60_000) % 15
        c[f"start minutes from a quarter hour: {band(min(minute, 15 - minute), [(1, '0'), (2, '1'), (8, '2-7')])}"] += 1
    if start is not None and first_msg is not None:
        c[f"first prompt after start: {band(first_msg - start, GAP)}"] += 1
    if later_gaps:
        c[f"later prompts: fastest after the turn end {band(min(later_gaps), GAP)}"] += 1
        c[f"later prompts: median after the turn end {band(statistics.median(later_gaps), GAP)}"] += 1
    else:
        c["no later prompt after a turn end"] += 1
    for name, period in (("day", DAY), ("week", WEEK)):
        r = recurs(sid.lower(), period) if sid.lower() in rows else None
        c[f"same folder, same time of {name} on another {name}: {'no start' if r is None else ('yes' if r else 'no')}"] += 1
    if WORD is not None and sid.lower() in rows:
        c["folder holds the given word" if WORD in rows[sid.lower()][0].lower() else "folder lacks the given word"] += 1

print(f"session files read by group {dict(sorted(seen.items()))}")
for g in sorted(counts):
    print(f"\n{g}: {seen[g]} sessions")
    for k, v in sorted(counts[g].items()):
        print(f"  {k}: {v}")
print("\n== event types whose share of sessions differs between groups by 20 points or more ==")
names = sorted({k for t in types.values() for k in t})
for k in names:
    share = {g: types[g][k] / max(seen[g], 1) for g in seen}
    if max(share.values()) - min(share.values()) >= 0.2:
        print(f"  {k}: " + "; ".join(f"{g} {types[g][k]}/{seen[g]}" for g in sorted(seen)))
