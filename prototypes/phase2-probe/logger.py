#!/usr/bin/env python3
"""Phase 2 probe logger. Copilot runs it as a hook (`logger.py hook <event>`) or as the status-line
command (`logger.py statusline`). It appends one JSON line per run to the probe log and exits 0.

What it records, and what it never records:
- payload keys, and values only for short id/name fields; prompt text is replaced by its length;
- names of environment variables that hint at the host (VSCODE_*, TERM_PROGRAM, COPILOT_*), with values
  only for TERM_PROGRAM and the VS Code / Copilot version variables;
- the time, so refresh cadence can be measured.
The status-line run prints one line, so the person can see whether Copilot shows it.
"""
import json
import os
import sys
import tempfile
import time

LOG = os.environ.get("AIQ_PROBE2_LOG") or os.path.join(tempfile.gettempdir(), "aiq-phase2-probe.jsonl")
SAFE_VALUE_KEYS = {"sessionId", "session_id", "hook_event_name", "source", "reason", "stopReason",
                   "stop_hook_active", "version", "name", "id", "display_name", "session_name"}
ENV_VALUE_KEYS = {"TERM_PROGRAM", "TERM_PROGRAM_VERSION", "VSCODE_GIT_ASKPASS_EXTRA_ARGS"}


def shape(v, key="", depth=0):
    """Keys all the way down; values only where they are short ids or names; text becomes its length."""
    if isinstance(v, dict):
        return {k: shape(x, k, depth + 1) for k, x in v.items()} if depth < 4 else "<dict>"
    if isinstance(v, list):
        return [shape(v[0], key, depth + 1), f"<{len(v)} items>"] if v else []
    if isinstance(v, str):
        return v[:80] if key in SAFE_VALUE_KEYS else f"<str {len(v)}>"
    return v


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else "?"
    event = sys.argv[2] if len(sys.argv) > 2 else ""
    raw = sys.stdin.read() if not sys.stdin.isatty() else ""
    try:
        payload = json.loads(raw) if raw.strip() else {}
    except ValueError:
        payload = {"<unparsed>": len(raw)}
    env = {k: (os.environ[k] if k in ENV_VALUE_KEYS else "<set>")
           for k in os.environ if k.startswith(("VSCODE", "COPILOT", "TERM_PROGRAM", "GITHUB_COPILOT"))}
    rec = {"t": round(time.time(), 3), "mode": mode, "event": event, "payload": shape(payload),
           "env": env, "ppid": os.getppid()}
    with open(LOG, "a", encoding="utf-8") as fh:
        fh.write(json.dumps(rec) + "\n")
    if mode == "statusline":
        print("AIQ probe: status line works " + time.strftime("%H:%M:%S"))


if __name__ == "__main__":
    try:
        main()
    except Exception:  # noqa: BLE001 - a probe must never break the host
        pass
    sys.exit(0)
