"""Read-only: for judgments with no agent actions, replay today's Copilot import reader on the source log and
count, per prompt, the tool calls it keeps, the ones it drops after `assistant.turn_end`, and sub-agent ones.
Prints counts and event-type sequences only, never text. Usage: python measure_copilot_turns.py [aiq.db]"""
import json, os, sqlite3, sys, collections
home = os.path.expanduser("~")
db = sys.argv[1] if len(sys.argv) > 1 else os.path.join(home, ".aiq", "aiq.db")
copilot = os.environ.get("COPILOT_HOME") or os.path.join(home, ".copilot")
c = sqlite3.connect(f"file:{db}?mode=ro", uri=True)

def replay(path):
    """Today's read_copilot, plus what it drops. Returns {prompt seq: counters} and the version seen."""
    events, turns, cur, opened, version = [], {}, None, False, None
    with open(path, encoding="utf-8", errors="replace") as f:
        for line in f:
            try: e = json.loads(line)
            except ValueError: continue
            t, main, d = e.get("type"), e.get("agentId") is None, e.get("data") or {}
            if t == "session.start":
                version = d.get("copilotVersion") or d.get("version") or version
            if t == "user.message" and main and d.get("delivery") in ("idle", "steering"):
                if opened: events.append("stop")
                events.append("prompt"); cur = len(events) - 1
                turns[cur] = collections.Counter(); turns[cur]["types"] = []; opened = True
                continue
            if cur is None: continue
            k = turns[cur]
            if t and (not k["types"] or k["types"][-1] != t) and len(k["types"]) < 14: k["types"].append(t)
            if t == "tool.execution_start":
                if not main: k["sub_tools"] += 1
                elif opened: k["kept"] += 1
                else: k["dropped_after_turn_end"] += 1
            if t == "subagent.started": k["subagents_open" if opened else "subagents_dropped"] += 1
            if t == "assistant.turn_end" and main:
                k["turn_ends"] += 1
                if opened: events.append("stop"); opened = False
    return turns, version

rows = c.execute("""SELECT j.read FROM judgment j WHERE j.rowid = (SELECT x.rowid FROM judgment x
  WHERE x.task_id = j.task_id ORDER BY x.judged_utc_ms DESC, x.rowid DESC LIMIT 1)""").fetchall()
agg, seqs, versions, missing = collections.Counter(), collections.Counter(), collections.Counter(), 0
for (read,) in rows:
    r = json.loads(read or "{}")
    if r.get("actions") is not False: continue
    for k in r.get("keys", []):
        if not k.startswith("import:copilot-cli:"): continue
        sid, seq = k[len("import:copilot-cli:"):].rsplit(":", 1); seq = int(seq)
        p = os.path.join(copilot, "session-state", sid, "events.jsonl")
        if not os.path.isfile(p): missing += 1; continue
        turns, v = replay(p); versions[str(v)] += 1
        t = turns.get(seq)
        agg["prompts"] += 1
        if t is None: agg["prompt seq not found in log today"] += 1; continue
        for name in ("kept", "dropped_after_turn_end", "sub_tools", "subagents_open", "subagents_dropped"):
            if t[name]: agg[f"prompts with {name}"] += 1; agg[f"total {name}"] += t[name]
        agg[f"prompts with {min(t['turn_ends'], 3)}{'+' if t['turn_ends'] >= 3 else ''} turn_ends"] += 1
        seqs[" > ".join(t["types"])] += 1
print(f"Copilot prompts in no-action judgments (source log missing for {missing}):")
for k, v in sorted(agg.items()): print(f"  {v:5}  {k}")
print("Copilot versions (per prompt):", dict(versions))
print("Most common event-type sequences after the prompt:")
for s, n in seqs.most_common(6): print(f"  {n:4}  {s}")
