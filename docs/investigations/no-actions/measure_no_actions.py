"""Read-only: why were current judgments made with no agent actions? Counts only, no prompt text.
Usage: python measure_no_actions.py [path-to-aiq.db]"""
import json, os, sqlite3, sys, glob, collections
home = os.path.expanduser("~")
db = sys.argv[1] if len(sys.argv) > 1 else os.path.join(home, ".aiq", "aiq.db")
c = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
claude = os.environ.get("CLAUDE_CONFIG_DIR") or os.path.join(home, ".claude")
copilot = os.environ.get("COPILOT_HOME") or os.path.join(home, ".copilot")

def tool_calls_in(path, kind):
    n = 0
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            for line in f:
                if kind == "claude" and '"tool_use"' in line: n += 1
                if kind == "copilot" and '"tool.execution_start"' in line: n += 1
    except OSError:
        return None
    return n

def src_file(tool, sid):
    if "claude" in tool:
        hits = glob.glob(os.path.join(claude, "projects", "*", f"{glob.escape(sid)}.jsonl"))
        return (hits[0], "claude") if hits else (None, "claude")
    p = os.path.join(copilot, "session-state", sid, "events.jsonl")
    return (p if os.path.isfile(p) else None, "copilot")

rows = c.execute("""SELECT j.task_id, j.origin, j.provider, j.job, j.current_rung, j.read, j.facts FROM judgment j
  WHERE j.rowid = (SELECT x.rowid FROM judgment x WHERE x.task_id = j.task_id ORDER BY x.judged_utc_ms DESC, x.rowid DESC LIMIT 1)""").fetchall()
total = len(rows); cats = collections.Counter(); by = collections.Counter(); rungs = collections.Counter(); jobs = collections.Counter()
f3 = collections.Counter(); f1 = collections.Counter()
for task_id, origin, provider, job, rung, read, facts in rows:
    r = json.loads(read or "{}")
    if r.get("actions") is not False:
        continue
    fx = json.loads(facts or "{}")
    f3[str(fx.get("F3", {}).get("value"))] += 1; f1[str(fx.get("F1", {}).get("value"))] += 1
    keys = r.get("keys", [])
    stops_present = stops_missing = 0; sessions = set(); tool = None
    for k in keys:
        if k.startswith("import:"):
            _, tool, rest = k.split(":", 2); sid, seq = rest.rsplit(":", 1)
            sessions.add((tool, sid))
            s = c.execute("SELECT actions IS NULL FROM import_event WHERE tool=? AND source_session_id=? AND seq=? AND kind='stop'", (tool, sid, int(seq) + 1)).fetchone()
            if s is None: stops_missing += 1
            else: stops_present += 1
        else:
            e = c.execute("SELECT surface, surface_session_id FROM event WHERE event_id=?", (k,)).fetchone()
            if e: tool = e[0]; sessions.add(e)
    files = [src_file(t, s) for t, s in sessions]
    exist = [p for p, _ in files if p]
    calls = sum(tool_calls_in(p, kind) or 0 for p, kind in files if p)
    if origin == "imported":
        cat = "imported: stop rows present, none recorded" if stops_missing == 0 else ("imported: all stop rows missing" if stops_present == 0 else "imported: some stop rows missing")
    else:
        cat = "live: tool log not found" if not exist else "live: tool log found, no turn matched/recorded"
    if exist:
        cat += f" | source log has tool calls now: {'yes' if calls else 'no'}"
    else:
        cat += " | source log gone"
    cats[cat] += 1; by[(origin, tool or "?")] += 1; rungs[rung] += 1; jobs[job] += 1
print(f"current judgments: {total}; with no agent actions: {sum(cats.values())}")
for k, v in cats.most_common(): print(f"  {v:4}  {k}")
print("by origin/tool:", dict(by)); print("rung:", dict(sorted(rungs.items()))); print("job:", dict(jobs))
print("stored F1:", dict(f1), " stored F3:", dict(f3))
