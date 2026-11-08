"""Read-only: replay AIQ's Copilot import reader before (reader 1) and after (reader 2) the turn-end fix over every
Copilot session log, and count per prompt the main-line tool calls and sub-agent starts each keeps.
Prints counts only, never prompt text, tool arguments or file names.
Usage: python -I measure_recovered.py [copilot home]   (default $COPILOT_HOME or ~/.copilot)"""
import collections, json, os, sys

home = sys.argv[1] if len(sys.argv) > 1 else os.environ.get("COPILOT_HOME") or os.path.join(os.path.expanduser("~"), ".copilot")
root = os.path.join(home, "session-state")


def replay(path):
    """[(reader-1 counts, reader-2 counts)] per typed prompt, each (tool calls, sub-agents)."""
    out, cur, open1 = [], None, False
    with open(path, encoding="utf-8", errors="replace") as f:
        for line in f:
            try:
                e = json.loads(line)
            except ValueError:
                continue
            t, main, d = e.get("type"), e.get("agentId") is None, e.get("data") or {}
            if t == "user.message" and main and d.get("delivery") in ("idle", "steering"):
                cur = [[0, 0], [0, 0]]
                out.append(cur)
                open1 = True
            elif cur is None:
                continue
            elif t == "tool.execution_start" and main:
                cur[1][0] += 1
                cur[0][0] += open1
            elif t == "subagent.started":
                cur[1][1] += 1
                cur[0][1] += open1
            elif t == "assistant.turn_end" and main:
                open1 = False
    return out


agg = collections.Counter()
for sid in sorted(os.listdir(root)) if os.path.isdir(root) else []:
    p = os.path.join(root, sid, "events.jsonl")
    if not os.path.isfile(p):
        continue
    try:
        prompts = replay(p)
    except OSError:
        agg["sessions unreadable"] += 1
        continue
    agg["sessions"] += 1
    changed = False
    for old, new in prompts:
        agg["prompts"] += 1
        agg["tool calls, reader 1"] += old[0]
        agg["tool calls, reader 2"] += new[0]
        agg["sub-agents, reader 1"] += old[1]
        agg["sub-agents, reader 2"] += new[1]
        if new != old:
            agg["prompts that gain"] += 1
            changed = True
            if old == [0, 0]:
                agg["prompts that go from no actions to some"] += 1
    agg["sessions that change"] += changed
print(f"Copilot logs under {root}:")
for k in ("sessions", "sessions unreadable", "sessions that change", "prompts", "prompts that gain",
          "prompts that go from no actions to some", "tool calls, reader 1", "tool calls, reader 2",
          "sub-agents, reader 1", "sub-agents, reader 2"):
    print(f"  {agg[k]:7}  {k}")
