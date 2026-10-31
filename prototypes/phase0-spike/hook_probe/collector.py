#!/usr/bin/env python3
"""Minimal AIQ collector for the Claude Code hook probe.

Reads the hook payload on stdin, appends one line to a local log (what a collector would hand to
the core), and exits. On Stop it also prints a coaching card as {"systemMessage": ...}, which the
architecture draft (ADR 0005) names as Claude Code's render path. Fails open: any error exits 0
with no output, so the agent is never blocked.

Each line records the hook's own wall time from interpreter start, so the probe measures the
collector cost directly rather than inferring it.
"""
import json
import os
import sys
import time

T0 = time.perf_counter()
START_NS = time.time_ns()


def main():
    log = os.environ.get("AIQ_PROBE_LOG", os.path.join(os.path.dirname(os.path.abspath(__file__)), "probe.jsonl"))
    try:
        payload = json.load(sys.stdin)
    except ValueError:
        payload = {}
    event = payload.get("hook_event_name") or (sys.argv[1] if len(sys.argv) > 1 else "?")
    out = None
    if event == "Stop":
        out = {"systemMessage": "AIQ (probe): this is where a coaching card would render."}
    rec = {"event": event, "start_ns": START_NS, "tool": payload.get("tool_name"),
           "session": payload.get("session_id"), "keys": sorted(payload),
           "collector_ms": round((time.perf_counter() - T0) * 1000, 2)}
    with open(log, "a", encoding="utf-8") as fh:
        fh.write(json.dumps(rec) + "\n")
    if out:
        print(json.dumps(out))


if __name__ == "__main__":
    try:
        main()
    except Exception:  # noqa: BLE001 - fail open by design
        pass
    sys.exit(0)
