#!/usr/bin/env python3
"""Shape-only copy of Copilot session logs (events.jsonl) for AIQ test fixtures.

Keeps: key names, JSON types, numbers, booleans, nulls, timestamps, and string values only from a short
allowlist (event types, delivery, built-in tool names AIQ classifies). Every other string becomes a
placeholder; ids become stable fake ids so links (toolCallId -> result) survive. Nothing typed by the person,
no command, path, tool argument, tool output or MCP/internal tool name is kept.

usage: shape.py <session-state dir> <out dir> [--survey]
"""
import hashlib, json, os, re, sys, collections

SALT = os.urandom(8)
SAFE_VALUE = re.compile(r"^[a-z][a-z0-9_.\-]{0,40}$")
ENUM_KEYS = {"type", "delivery", "role", "stopReason", "status", "source", "kind", "reason", "level"}
TOOLS_KEPT = {"bash", "powershell", "shell", "task", "view", "edit", "create", "write", "str_replace",
              "apply_patch", "multiedit", "grep", "glob", "read", "report_intent", "think"}
TS = re.compile(r"^\d{4}-\d\d-\d\dT[\d:.]+Z?$")
ID_KEYS = re.compile(r"(Id|_id|ID|id)$")
EXIT = re.compile(r"exit code (-?\d+)")
OPAQUE = {"arguments", "input", "result", "content", "output", "detailedContent", "message", "text"}


def fake_id(s):
    return "id-" + hashlib.sha256(SALT + s.encode()).hexdigest()[:12]


def scrub_str(key, v):
    if TS.match(v):
        return v
    if key == "toolName":
        return v if v.lower() in TOOLS_KEPT else "other_tool"
    if key in ENUM_KEYS and SAFE_VALUE.match(v) and "mcp" not in v:
        return v
    if ID_KEYS.search(key or ""):
        return fake_id(v)
    m = EXIT.findall(v)
    if m:
        return f"<text> exit code {m[-1]}"
    return "<text>"


def scrub(v, key=None, depth=0):
    if isinstance(v, dict):
        if key in OPAQUE and depth > 1:
            # Tool arguments/results: keep which known keys exist, never their names otherwise.
            known = {"command", "file_path", "path", "content", "exitCode", "success", "is_error"}
            return {k: scrub(x, k, depth + 1) for k, x in v.items() if k in known}
        return {k: scrub(x, k, depth + 1) for k, x in v.items() if re.match(r"^[A-Za-z_][A-Za-z0-9_]{0,40}$", k)}
    if isinstance(v, list):
        return [scrub(x, key, depth + 1) for x in v[:20]]
    if isinstance(v, str):
        if key == "command":
            return "<command>"
        return scrub_str(key, v)
    return v


def main():
    src, out = sys.argv[1], sys.argv[2]
    survey = "--survey" in sys.argv
    sessions = []
    for d in sorted(os.listdir(src)):
        p = os.path.join(src, d, "events.jsonl")
        if os.path.isfile(p):
            sessions.append(p)
    types = collections.Counter()
    feats = {}
    for p in sessions:
        f = collections.Counter()
        with open(p, encoding="utf-8", errors="replace") as fh:
            for line in fh:
                try:
                    e = json.loads(line)
                except ValueError:
                    f["bad_line"] += 1
                    continue
                t = e.get("type") if isinstance(e.get("type"), str) else "?"
                t = t if SAFE_VALUE.match(t) and "mcp" not in t else "other"
                types[t] += 1
                f[t] += 1
                d = e.get("data") if isinstance(e.get("data"), dict) else {}
                if t == "user.message":
                    f["delivery:" + str(d.get("delivery"))] += 1
                if e.get("agentId"):
                    f["subagent"] += 1
        feats[p] = f
    print("sessions", len(sessions))
    print("types", dict(types.most_common()))
    if survey:
        return
    # Pick up to 6 sessions covering steering, sub-agents, and the most event types; smallest first.
    picked, covered = [], set()
    wanted = ["delivery:steering", "subagent", "delivery:queued", "delivery:idle"]
    for w in wanted:
        cands = sorted((p for p in sessions if feats[p][w] and p not in picked), key=lambda p: sum(feats[p].values()))
        if cands:
            picked.append(cands[0])
    rest = sorted(sessions, key=lambda p: -len(set(feats[p]) - covered))
    for p in rest:
        if len(picked) >= 6:
            break
        if p not in picked and set(feats[p]) - set().union(*(set(feats[q]) for q in picked)):
            picked.append(p)
    os.makedirs(out, exist_ok=True)
    for i, p in enumerate(picked):
        lines = []
        with open(p, encoding="utf-8", errors="replace") as fh:
            for line in fh:
                try:
                    lines.append(json.dumps(scrub(json.loads(line)), separators=(",", ":")))
                except ValueError:
                    pass
        name = f"copilot-{i + 1}.jsonl"
        with open(os.path.join(out, name), "w", encoding="utf-8") as fh:
            fh.write("\n".join(lines) + "\n")
        print(name, len(lines), dict(feats[p]))


if __name__ == "__main__":
    main()
