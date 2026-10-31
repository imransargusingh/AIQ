"""Read Claude Code and Copilot CLI session stores into one list of human turns.

Session discovery and the low-level readers are AI-Forward's session-profile.py, loaded from the
installed pack (docs/ai-forward-pack/scripts/) rather than copied: claude_sessions,
copilot_sessions, _load_events, _text_of, _tool_result_texts, _args_of. Everything here is
read-only and local.

A turn = one human prompt (or typed slash command) plus everything the agent did until the next
human prompt. Turns are the unit the task grouper works on.
"""
import importlib.util
import json
import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
SP_PATH = os.path.join(ROOT, "docs", "ai-forward-pack", "scripts", "session-profile.py")


def _load_sp():
    spec = importlib.util.spec_from_file_location("session_profile", SP_PATH)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


SP = _load_sp()

CLAUDE_TOOL = {"view": "Read", "edit": "Edit", "create": "Write", "bash": "Bash", "grep": "Grep",
               "task": "Task", "powershell": "Bash", "shell": "Bash"}
CMD_RX = re.compile(r"<command-name>(/[^<]+)</command-name>.*?<command-args>(.*?)</command-args>", re.S)
PATH_RX = re.compile(r"(?<![\w/.-])((?:[\w.-]+/)+[\w.-]+\.\w+|[\w-]+\.(?:md|py|ts|js|yaml|yml|json|csv|toml))")


def _new_turn(surface, sid, key, ts, prompt, command=None):
    return {"key": key, "surface": surface, "session": sid, "start": ts, "end": ts,
            "prompt": prompt, "command": command, "tools": [], "files": [], "subagents": [],
            "assistant": []}


def _note_tool(turn, name, args, result, ts):
    name = CLAUDE_TOOL.get(name, name)
    path = str(args.get("file_path") or args.get("path") or "")
    entry = {"tool": name}
    if path:
        entry["path"] = path
        turn["files"].append(path)
    if name == "Bash":
        entry["command"] = str(args.get("command") or "")[:200]
        entry["result"] = str(result or "")[:160]
    if name == "Task":
        turn["subagents"].append({"name": args.get("description") or "", "prompt": str(args.get("prompt") or "")[:200]})
    turn["tools"].append(entry)
    turn["end"] = ts or turn["end"]


def claude_turns(path, sid):
    turns, cur, pending = [], None, {}
    with open(path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            try:
                r = json.loads(line)
            except ValueError:
                continue
            if r.get("isSidechain"):
                continue
            t, msg, ts = r.get("type"), r.get("message") or {}, r.get("timestamp")
            content = msg.get("content")
            if t == "user":
                if isinstance(content, str) and content.strip():
                    m = CMD_RX.search(content)
                    if m:
                        cur = _new_turn("claude", sid, "claude:{0}:{1}".format(sid, r.get("uuid")), ts,
                                        (m.group(1) + " " + m.group(2)).strip(), command=m.group(1))
                    else:
                        cur = _new_turn("claude", sid, "claude:{0}:{1}".format(sid, r.get("uuid")), ts, content)
                    turns.append(cur)
                elif cur is not None:
                    for tid, res in SP._tool_result_texts(content).items():
                        if tid in pending:
                            name, args = pending.pop(tid)
                            _note_tool(cur, name, args, res, ts)
            elif t == "assistant" and cur is not None:
                for b in content or []:
                    if isinstance(b, dict) and b.get("type") == "tool_use":
                        pending[b.get("id")] = (b.get("name"), b.get("input") or {})
                text = SP._text_of(content)
                if text:
                    cur["assistant"].append(text[:300])
                cur["end"] = ts or cur["end"]
    return turns


def copilot_turns(events_path, sid):
    events = SP._load_events(events_path)
    turns, cur, starts = [], None, {}
    for e in events:
        et, d, ts = e.get("type"), e.get("data") or {}, e.get("timestamp")
        if et == "user.message" and not e.get("agentId") and d.get("delivery") in ("idle", "steering"):
            c = d.get("content") or ""
            cmd = c.split()[0] if c.startswith("/") else None
            cur = _new_turn("copilot", sid, "copilot:{0}:{1}".format(sid, e.get("id")), ts, c, command=cmd)
            turns.append(cur)
        elif cur is None or e.get("agentId"):
            continue  # sub-agent internals: counted through the parent's task call
        elif et == "tool.execution_start":
            starts[d.get("toolCallId")] = d
        elif et == "tool.execution_complete":
            s = starts.pop(d.get("toolCallId"), None)
            if s:
                res = d.get("result")
                res = res.get("content") if isinstance(res, dict) else res
                _note_tool(cur, s.get("toolName") or "?", SP._args_of(s), res, ts)
        elif et == "assistant.message":
            if d.get("content"):
                cur["assistant"].append(str(d["content"])[:300])
            cur["end"] = ts or cur["end"]
    return turns


def load_turns(repo, claude_home=None, copilot_home=None):
    ident = SP.repo_identity(repo)
    turns = []
    for s in SP.claude_sessions(ident, None, claude_home or SP.claude_home()):
        turns += claude_turns(s["path"], s["id"])
    for s in SP.copilot_sessions(ident, None, copilot_home or SP.copilot_home()):
        turns += copilot_turns(s["events"], s["id"])
    for t in turns:
        t["mentioned"] = sorted(set(PATH_RX.findall(t["prompt"])))
        t["files"] = sorted(set(t["files"]))
    turns.sort(key=lambda t: t["start"] or "")
    return turns
