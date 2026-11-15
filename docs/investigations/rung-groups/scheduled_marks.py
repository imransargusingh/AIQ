"""Read-only, counts only: what marks the Copilot sessions a scheduler starts, so AIQ can set them all aside?

The person says the long mail-and-meetings round-ups in the newest check of one Kind (scope kind:<job>/<kind>,
default ask_research/find) come from an app's scheduled automations, and the second reading called them research.
This compares their Copilot session files with the other tasks of that run (find and progress) and with a sample of
sessions AIQ already set aside:
  - per session: user.message events, how many carry isAutopilotContinuation, and whether AIQ's stored typed-prompt
    count equals the messages minus those (it should since COPILOT_READER 3, AIQ 0.1.66);
  - every key under session.start data and under each user.message (not the text), and its value only when it is a
    boolean, a number or a short safe word under a key that is not a path, folder, name or title.
Nothing else is printed: no prompt, folder, id or path.

Usage:
    python3 -I scheduled_marks.py <aiq.db> <copilot home, e.g. %USERPROFILE%\\.copilot> [<job>/<kind>] [<scheduled Kind>]
"""
import collections
import json
import os
import re
import sqlite3
import sys

DB, HOME = sys.argv[1], sys.argv[2]
SCOPE = sys.argv[3] if len(sys.argv) > 3 else "ask_research/find"
SCHEDULED = sys.argv[4] if len(sys.argv) > 4 else "research"
ASIDE_SAMPLE = 300
con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True, timeout=5)
con.execute("PRAGMA query_only = ON")
SAFE = re.compile(r"^[A-Za-z0-9_.+:-]{1,24}$")
UNSAFE_KEY = re.compile(r"cwd|path|dir|folder|repo|branch|git|name|title|user|host|email|url|content|text|prompt|summary|id$",
                        re.I)

run = con.execute("SELECT run_id FROM judge_check_run WHERE scope = ? ORDER BY started_utc_ms DESC LIMIT 1",
                  (f"kind:{SCOPE}",)).fetchone()
if not run:
    print(f"no check run of {SCOPE} only")
    sys.exit(0)
job, kind = SCOPE.split("/", 1)
group = {}
for ans_text, task_id in con.execute("SELECT answer, task_id FROM judge_check WHERE run_id = ? AND outcome = 'ok'", run):
    if not task_id.startswith("import:copilot-cli:"):
        continue
    sid = task_id[len("import:copilot-cli:"):].rsplit(":", 1)[0].lower()
    try:
        ans = json.loads(ans_text)
    except (TypeError, ValueError):
        ans = {}
    picked = ans.get("task_type") if ans.get("job") == job else "other job"
    g = "scheduled" if picked == SCHEDULED else "rest of the run"
    if group.get(sid) != "scheduled":
        group[sid] = g
turns = {sid.lower(): (outcome, n) for sid, outcome, n in con.execute(
    "SELECT source_session_id, outcome, human_turns FROM import_session WHERE tool = 'copilot-cli'")}
aside = sorted(s for s, (o, _) in turns.items() if o == "automated" and s not in group)
for s in aside[-ASIDE_SAMPLE:]:
    group[s] = "aside"
print(f"sessions by group {dict(sorted(collections.Counter(group.values()).items()))}")


def flatten(d, prefix=""):
    if isinstance(d, dict):
        for k, v in d.items():
            yield from flatten(v, f"{prefix}.{k}" if prefix else k)
    elif isinstance(d, list):
        yield (f"{prefix}[]", f"list of {len(d)}")
    else:
        yield (prefix, d)


def shown(key, v):
    if isinstance(v, bool) or v is None:
        return str(v).lower()
    if isinstance(v, (int, float)):
        return "number"
    if isinstance(v, str):
        leaf = key.rsplit(".", 1)[-1]
        if not UNSAFE_KEY.search(leaf) and SAFE.match(v):
            return v
        return "text"
    return type(v).__name__


seen = collections.Counter()
per_session = collections.defaultdict(collections.Counter)
stats = collections.defaultdict(collections.Counter)  # (where, key) -> Counter of (group, value)
state = os.path.join(HOME, "session-state")
for sid in os.listdir(state) if os.path.isdir(state) else []:
    g = group.get(sid.lower())
    path = os.path.join(state, sid, "events.jsonl")
    if g is None or not os.path.isfile(path):
        continue
    seen[g] += 1
    msgs = cont = 0
    got_start = False
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            for i, line in enumerate(f):
                if i >= 20000:
                    break
                try:
                    e = json.loads(line)
                except ValueError:
                    continue
                t = e.get("type")
                data = e.get("data") or {}
                if t == "session.start" and not got_start:
                    got_start = True
                    for k, v in flatten(data):
                        stats[("session.start", k)][(g, shown(k, v))] += 1
                if t == "user.message":
                    msgs += 1
                    cont += data.get("isAutopilotContinuation") is True
                    for k, v in flatten({x: y for x, y in data.items() if x != "content"}):
                        stats[("user.message", k)][(g, shown(k, v))] += 1
    except OSError:
        continue
    stored = turns.get(sid.lower(), (None, None))[1]
    c = per_session[g]
    c[f"user.messages {min(msgs, 4) if msgs < 4 else '4+'}"] += 1
    c[f"autopilot continuations {min(cont, 3) if cont < 3 else '3+'}"] += 1
    if stored is None:
        c["no stored typed count"] += 1
    else:
        c["stored typed count = messages minus continuations" if stored == msgs - cont
          else ("stored typed count = all messages" if stored == msgs else "stored typed count is something else")] += 1
print(f"session files read by group {dict(sorted(seen.items()))}")
print("\n== per session ==")
for g in sorted(per_session):
    print(f"  {g}: {dict(sorted(per_session[g].items()))}")
print("\n== fields: key -> group: {value: count} (only keys whose share differs between groups by 20 points or more) ==")
for (where, key), c in sorted(stats.items()):
    by_g = collections.defaultdict(dict)
    for (g, v), n in c.items():
        by_g[g][v] = n
    share = {g: sum(by_g[g].values()) / max(seen[g], 1) for g in seen}
    values = {g: max(by_g[g], key=by_g[g].get) if by_g.get(g) else None for g in seen}
    if max(share.values()) - min(share.values()) < 0.2 and len(set(values.values())) == 1:
        continue
    line = "; ".join(f"{g} {dict(sorted(v.items(), key=lambda kv: -kv[1])[:6])}" for g, v in sorted(by_g.items()))
    print(f"  {where} {key}: {line}")
