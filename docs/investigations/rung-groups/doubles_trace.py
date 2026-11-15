"""Read-only, counts only: why a task holds an analysis from this computer and one from another (the 33 doubled
j7 analyses after AIQ 0.1.65). Only the computer that saw a task should analyze it (design multi-machine-sync).

For each doubled task it asks: is it an imported session, and does the other computer's own change files (in
<aiq home>/sync/<its id>/) hold that same session as its own import, with the same working folder and source file
time? Does this computer still hold the session's source file? Prints counts, the other computer's name, and no
prompt, note, folder or id.

Usage:
    python3 -I doubles_trace.py <aiq home folder, e.g. %USERPROFILE%\\.aiq> [<copilot home, e.g. %USERPROFILE%\\.copilot>]
"""
import collections
import json
import os
import sqlite3
import sys

home = sys.argv[1]
copilot = sys.argv[2] if len(sys.argv) > 2 else None
con = sqlite3.connect(f"file:{os.path.join(home, 'aiq.db')}?mode=ro&immutable=1", uri=True)


def cnt(it):
    return dict(sorted(collections.Counter(it).items(), key=lambda kv: (-kv[1], str(kv[0]))))


rows = con.execute("SELECT task_id, computer, judged_utc_ms FROM judgment WHERE judge_version = 'j7'").fetchall()
by_task = collections.defaultdict(list)
for tid, comp, ms in rows:
    by_task[tid].append((comp, ms))
doubled = {tid: v for tid, v in by_task.items() if any(c is None for c, _ in v) and any(c is not None for c, _ in v)}
others = cnt(c for v in doubled.values() for c, _ in v if c is not None)
print(f"doubled tasks (one j7 analysis from here, one from another computer): {len(doubled)}")

# The other computers' names, from their own folders.
names = {}
sync = os.path.join(home, "sync")
for d in os.listdir(sync) if os.path.isdir(sync) else []:
    try:
        with open(os.path.join(sync, d, "computer.json"), encoding="utf-8") as f:
            c = json.load(f)
        names[c.get("id", d)] = c.get("name", "?")
    except Exception:
        pass
print(f"other computers that wrote them: {{{', '.join(f'{names.get(c, 'unnamed')}: {n}' for c, n in others.items())}}}")

imported = {tid for tid in doubled if tid.startswith("import:")}
print(f"imported sessions: {len(imported)}; live prompts: {len(doubled) - len(imported)}")

# This computer's own copy of each imported session.
key = {}
for tid in imported:
    _, tool, rest = tid.split(":", 2)
    sid = rest.rsplit(":", 1)[0]
    key[tid] = (tool, sid)
mine = {}
for tool, sid, folder, mtime, comp in con.execute(
        "SELECT tool, source_session_id, folder, source_mtime_ms, computer FROM import_session"):
    mine[(tool, sid)] = (folder, mtime, comp)
print(f"  held here as this computer's own import: {sum(1 for k in key.values() if k in mine and mine[k][2] is None)}; "
      f"as another computer's: {sum(1 for k in key.values() if k in mine and mine[k][2] is not None)}; "
      f"not held: {sum(1 for k in key.values() if k not in mine)}")

# The other computers' copies, from their change files (rows they recorded as their own).
theirs = collections.defaultdict(list)
want = set(key.values())
for d in os.listdir(sync) if os.path.isdir(sync) else []:
    folder = os.path.join(sync, d)
    if not os.path.isdir(folder):
        continue
    for fn in sorted(os.listdir(folder)):
        if not fn.endswith(".jsonl"):
            continue
        with open(os.path.join(folder, fn), encoding="utf-8") as f:
            for line in f:
                try:
                    v = json.loads(line)
                except Exception:
                    continue
                if v.get("t") != "import_session":
                    continue
                r = v.get("r") or {}
                k = (r.get("tool"), r.get("source_session_id"))
                if k in want:
                    theirs[k].append((d, r.get("folder"), r.get("source_mtime_ms"), r.get("run_id")))
print(f"  also imported as its own by another computer (in its change files): {sum(1 for k in want if k in theirs)} of {len(want)}")
same = collections.Counter()
for k in want:
    if k not in theirs or k not in mine:
        continue
    f_here, m_here, _ = mine[k]
    for _, f_there, m_there, _ in theirs[k]:
        same[("same folder" if f_there == f_here else "different folder",
              "same file time" if m_there == m_here else ("their file newer" if (m_there or 0) > (m_here or 0) else "their file older"))] += 1
print(f"  their copy against ours: {dict(same)}")

# Does this computer still hold each session's source file?
if copilot:
    st = os.path.join(copilot, "session-state")
    have = sum(1 for (tool, sid) in want if tool == "copilot-cli" and os.path.isfile(os.path.join(st, sid, "events.jsonl")))
    print(f"  Copilot sessions whose source file is here: {have} of {sum(1 for t, _ in want if t == 'copilot-cli')}")
print(f"  by tool {cnt(t for t, _ in want)}")
