"""Read-only, counts only: does a Copilot CLI session file say how the session was started (typed in the terminal,
or run by a program with `copilot -p`)? AIQ's importer reads `session.start` only for the folder and start time, and
sets `non_interactive` only for Claude Code (entrypoint sdk-cli), so a program's one-prompt Copilot sessions are
counted as typed. This looks for a field that tells them apart.

Groups (from the AIQ store, by Copilot session id):
  app      imported sessions whose first prompt starts with the opening given on the command line (a known
           program's prompt; the opening is never printed)
  typed2+  imported sessions with two or more typed prompts (a person drove them)
  one      the other imported one-prompt sessions
  aside    sessions AIQ already set aside as automated
For every key path under `session.start` data and under the first `user.message` data (except its text), it prints
how often each group has it, and the value when the value is a boolean, a number, or a short word from a safe
character set under a key that is not a path, folder, repository, branch, name or title. Nothing else is printed:
no prompt, folder, id or path.

Usage:
    python3 -I copilot_launch_signs.py <aiq.db> <copilot home, e.g. %USERPROFILE%\\.copilot> "<opening of the app's prompt>"
"""
import collections
import json
import os
import re
import sqlite3
import sys

DB, HOME, OPENING = sys.argv[1], sys.argv[2], sys.argv[3].strip().lower()
con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True, timeout=5)
con.execute("PRAGMA query_only = ON")
SAFE = re.compile(r"^[A-Za-z0-9_.+:-]{1,24}$")
UNSAFE_KEY = re.compile(r"cwd|path|dir|folder|repo|branch|git|name|title|user|host|email|url|content|text|prompt|summary|id$",
                        re.I)

group = {}
for sid, outcome, turns in con.execute(
        "SELECT source_session_id, outcome, human_turns FROM import_session WHERE tool = 'copilot-cli'"):
    if outcome != "imported":
        group[sid.lower()] = "aside"
        continue
    first = con.execute("SELECT text FROM import_event WHERE tool = 'copilot-cli' AND source_session_id = ? "
                        "AND kind = 'prompt' ORDER BY seq LIMIT 1", (sid,)).fetchone()
    if first and first[0] and first[0].strip().lower().startswith(OPENING):
        group[sid.lower()] = "app"
    else:
        group[sid.lower()] = "typed2+" if (turns or 0) >= 2 else "one"
sizes = collections.Counter(group.values())
print(f"sessions in the store by group {dict(sorted(sizes.items()))}")


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
stats = collections.defaultdict(collections.Counter)  # (where, key) -> Counter of (group, value)
event_types = collections.defaultdict(collections.Counter)  # group -> event types in the first 8 events
state = os.path.join(HOME, "session-state")
for sid in os.listdir(state) if os.path.isdir(state) else []:
    g = group.get(sid.lower())
    path = os.path.join(state, sid, "events.jsonl")
    if g is None or not os.path.isfile(path):
        continue
    seen[g] += 1
    got_start = got_msg = False
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            for i, line in enumerate(f):
                if got_start and got_msg and i >= 8:
                    break
                try:
                    e = json.loads(line)
                except ValueError:
                    continue
                t = e.get("type")
                if i < 8:
                    event_types[g][str(t)] += 1
                where = None
                if t == "session.start" and not got_start:
                    where, got_start = "session.start", True
                elif t == "user.message" and not got_msg:
                    where, got_msg = "user.message", True
                if where:
                    for k, v in flatten(e.get("data") or {}):
                        stats[(where, k)][(g, shown(k, v))] += 1
                    for k, v in flatten({x: y for x, y in e.items() if x not in ("data", "id", "parentId", "timestamp")}):
                        stats[(where + " (record)", k)][(g, shown(k, v))] += 1
    except OSError:
        continue
print(f"session files read by group {dict(sorted(seen.items()))}")
print("\n== first 8 event types by group ==")
for g in sorted(event_types):
    print(f"  {g}: {dict(event_types[g].most_common(12))}")
print("\n== fields: key -> group: {value: sessions} (only where groups differ or the value is short) ==")
for (where, key), c in sorted(stats.items()):
    by_g = collections.defaultdict(dict)
    for (g, v), n in c.items():
        by_g[g][v] = n
    line = "; ".join(f"{g} {dict(sorted(v.items(), key=lambda kv: -kv[1])[:6])}" for g, v in sorted(by_g.items()))
    print(f"  {where} {key}: {line}")
