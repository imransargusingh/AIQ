#!/usr/bin/env python3
"""Shape-only copies of Claude Code transcripts (~/.claude/projects/*/*.jsonl) for AIQ test fixtures.

Picks the smallest transcript of each kind AIQ must tell apart (a claude.ai project thread run in the desktop
app, a session typed in the desktop app, a terminal session, a headless `claude -p` run, one with relayed
briefs or peer messages) and keeps: key names, JSON types, numbers, booleans, timestamps (moved to
2026-01-01, gaps kept) and string values from a short allowlist (record types, turnOrigin, entrypoint,
origin kind/subkind, roles, content block types, built-in tool names). Every other string becomes "<text>";
ids become stable fake ids. No prompt, reply, command, path, file content or other tool name is kept.

usage: claude_shape.py <projects dir> <out dir>      (reads only; writes only to <out dir>)
"""
import datetime as dt
import hashlib
import json
import os
import re
import sys

SALT = os.urandom(8)
VALUES = {
    "type": {"user", "assistant", "system", "summary", "queue-operation", "attachment", "text", "tool_use",
             "tool_result", "thinking", "redacted_thinking", "image", "file-history-snapshot", "progress"},
    "turnOrigin": {"human", "system", "peer", "relay", "projects"},
    "entrypoint": {"cli", "claude-desktop", "sdk-cli", "sdk-ts", "sdk-py", "claude-vscode", "remote"},
    "kind": {"peer", "human", "system", "projects", "relay"},
    "subkind": {"projects-relay", "peer", "brief"},
    "role": {"user", "assistant"},
    "operation": {"enqueue", "dequeue", "remove", "popAll"},
    "userType": {"external", "internal"},
    "name": {"Bash", "Edit", "Write", "Read", "MultiEdit", "NotebookEdit", "Task", "Agent", "Grep", "Glob",
             "TodoWrite", "WebFetch", "WebSearch"},
}
TS = re.compile(r"^\d{4}-\d\d-\d\dT[\d:.]+Z$")
ID_KEY = re.compile(r"(Id|_id|ID|^id|uuid|Uuid)$")
EXIT = re.compile(r"exit code (-?\d+)")
KEY_OK = re.compile(r"^[A-Za-z_][A-Za-z0-9_]{0,40}$")
ID_AS_KEY = re.compile(r"^(toolu|call)_")  # a map keyed by real tool-call ids
EPOCH_KEY = re.compile(r"Time$")  # epoch-millisecond numbers, moved with the timestamps
OPAQUE = {"input", "content", "toolUseResult", "result"}


def fake(s):
    return "id-" + hashlib.sha256(SALT + s.encode()).hexdigest()[:12]


def scrub(v, key=None, depth=0):
    if isinstance(v, dict):
        if key in OPAQUE and depth > 2:
            keep = {"command", "file_path", "path", "type", "tool_use_id", "is_error", "content", "name", "id", "input"}
            return {k: scrub(x, k, depth + 1) for k, x in v.items() if k in keep}
        return {(fake(k) if ID_AS_KEY.match(k) else k): scrub(x, k, depth + 1) for k, x in v.items() if KEY_OK.match(k)}
    if isinstance(v, list):
        return [scrub(x, key, depth + 1) for x in v[:8]]
    if isinstance(v, str):
        if TS.match(v):
            return v
        if key in VALUES and v in VALUES[key]:
            return v
        if key and ID_KEY.search(key):
            return fake(v)
        if key == "command":
            return "<command>"
        m = EXIT.findall(v)
        return f"<text> exit code {m[-1]}" if m else "<text>"
    return v


def kind_of(rows):
    users = [r for r in rows if r.get("type") == "user"]
    ep = {r.get("entrypoint") for r in rows if isinstance(r.get("entrypoint"), str)}
    if any(r.get("projectsUserTurn") is True for r in users):
        return "desktop-project-thread"
    if any((r.get("origin") or {}).get("subkind") == "projects-relay" or r.get("turnOrigin") == "peer"
           or (r.get("origin") or {}).get("kind") == "peer" for r in users):
        return "relay-or-peer"
    if "sdk-cli" in ep:
        return "headless"
    if "claude-desktop" in ep:
        return "desktop-sdk"
    if "cli" in ep:
        return "terminal"
    return None


def shift(v, delta, key=None):
    if isinstance(v, dict):
        return {k: shift(x, delta, k) for k, x in v.items()}
    if isinstance(v, int) and not isinstance(v, bool) and key and EPOCH_KEY.search(key) and v > 10**12:
        return v - int(delta.total_seconds() * 1000)
    if isinstance(v, list):
        return [shift(x, delta) for x in v]
    if isinstance(v, str) and TS.match(v):
        t = dt.datetime.fromisoformat(v.replace("Z", "+00:00")) - delta
        return t.isoformat(timespec="milliseconds").replace("+00:00", "Z")
    return v


def main():
    src, out = sys.argv[1], sys.argv[2]
    best = {}
    seen = 0
    for d in os.listdir(src):
        p = os.path.join(src, d)
        if not os.path.isdir(p):
            continue
        for f in os.listdir(p):
            if not f.endswith(".jsonl"):
                continue
            path = os.path.join(p, f)
            size = os.path.getsize(path)
            if size > 2_000_000:
                continue
            seen += 1
            try:
                rows = [json.loads(l) for l in open(path, encoding="utf-8", errors="replace") if l.strip()]
            except ValueError:
                continue
            k = kind_of(rows)
            if k and sum(1 for r in rows if r.get("type") == "user") >= 2 and (k not in best or size < best[k][0]):
                best[k] = (size, rows)
    os.makedirs(out, exist_ok=True)
    base = dt.datetime(2026, 1, 1, tzinfo=dt.timezone.utc)
    print("transcripts read", seen)
    for k, (size, rows) in sorted(best.items()):
        keep = set(range(min(40, len(rows))))
        for i, r in enumerate(rows):
            if r.get("type") == "user":
                keep.update(range(max(0, i - 1), min(len(rows), i + 4)))
        picked = [scrub(rows[i]) for i in sorted(keep)][:120]
        stamps = [dt.datetime.fromisoformat(r["timestamp"].replace("Z", "+00:00"))
                  for r in picked if isinstance(r.get("timestamp"), str) and TS.match(r["timestamp"])]
        delta = (min(stamps) - base) if stamps else dt.timedelta(0)
        with open(os.path.join(out, f"claude-{k}.jsonl"), "w", encoding="utf-8") as fh:
            fh.write("\n".join(json.dumps(shift(r, delta), separators=(",", ":")) for r in picked) + "\n")
        print(k, len(rows), "->", len(picked))
    print("kinds missing:", sorted({"desktop-project-thread", "desktop-sdk", "terminal", "headless",
                                    "relay-or-peer"} - set(best)))


if __name__ == "__main__":
    main()
