#!/usr/bin/env python3
"""Second pass over shape.py output: tighter allowlist, AIQ-relevant events only, small files.

usage: trim.py <in dir> <out dir>
"""
import json, os, re, sys

KEEP_TYPES = {"session.start", "session.resume", "session.shutdown", "user.message", "assistant.turn_start",
              "assistant.turn_end", "assistant.message", "tool.execution_start", "tool.execution_complete",
              "subagent.selected", "subagent.deselected", "subagent.started", "subagent.completed", "abort",
              "session.task_complete", "hook.start", "hook.end", "permission.requested", "permission.completed"}
VALUES = {
    "type": KEEP_TYPES | {"function", "string", "object", "array", "boolean", "integer", "number", "null"},
    "delivery": {"idle", "queued", "steering"},
    "role": {"user", "assistant", "system"},
    "status": {"completed", "not-run", "failed"},
    "kind": {"shell", "commands", "path", "create", "edit", "read"},
}
DROP_KEYS = {"encrypted_content", "encryptedContent", "reasoningOpaque", "reasoning_opaque", "requestMessages",
             "assignmentContext", "enabledFeatureFlags"}


def clean(v, key=None):
    if isinstance(v, dict):
        out = {}
        for k, x in v.items():
            if k in DROP_KEYS:
                continue
            if k.startswith("call_"):
                k = "call_id"
            out[k] = clean(x, k)
        return out
    if isinstance(v, list):
        return [clean(x, key) for x in v[:5]]
    if isinstance(v, str):
        if v.startswith("id-") or v.startswith("<") or re.match(r"^\d{4}-\d\d-\d\dT", v):
            return v
        if key == "toolName" and v in {"view", "powershell", "bash", "apply_patch", "edit", "create", "task", "other_tool"}:
            return v
        if key in VALUES and v in VALUES[key]:
            return v
        return "<text>"
    return v


def main():
    src, out = sys.argv[1], sys.argv[2]
    os.makedirs(out, exist_ok=True)
    for name in sorted(os.listdir(src)):
        rows = []
        for line in open(os.path.join(src, name), encoding="utf-8"):
            e = json.loads(line)
            if e.get("type") in KEEP_TYPES:
                rows.append(json.dumps(clean(e), separators=(",", ":")))
        # Keep the head of the session and every typed message with its neighbours, at most 120 rows.
        keep = set(range(min(30, len(rows))))
        for i, r in enumerate(rows):
            if "\"type\":\"user.message\"" in r or ("\"agentId\"" in r and i < 400):
                keep.update(range(max(0, i - 2), min(len(rows), i + 8)))
        picked = [rows[i] for i in sorted(keep)][:120]
        with open(os.path.join(out, name), "w", encoding="utf-8") as fh:
            fh.write("\n".join(picked) + "\n")
        print(name, len(rows), "->", len(picked))


if __name__ == "__main__":
    main()
