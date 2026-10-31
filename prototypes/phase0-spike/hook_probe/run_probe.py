#!/usr/bin/env python3
"""Claude Code hook probe: does a hook observe the turn, and can a Stop hook render a card? How much
does the collector cost?

  python3 prototypes/phase0-spike/hook_probe/run_probe.py --runs 3 --bench 50

1. Bench: runs the collector process N times with a sample payload, timing the whole process
   (interpreter start to exit): the cost a synchronous hook adds to the agent.
2. Live: writes a throwaway project with user-scope-style hooks in its .claude/settings.json (the
   probe can only use project scope inside a temp dir; ADR 0005 installs at user scope, same schema),
   runs `claude -p` with stream-json + hook events, and reports which hooks fired, whether the Stop
   hook's systemMessage reached the output stream, and the hook timings the harness reports.
"""
import argparse
import json
import os
import shutil
import statistics
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
COLLECTOR = os.path.join(HERE, "collector.py")


def pct(v, q):
    v = sorted(v)
    return v[min(len(v) - 1, int(q * len(v)))]


def bench(n, log):
    payload = json.dumps({"hook_event_name": "PostToolUse", "session_id": "bench", "tool_name": "Bash",
                          "tool_input": {"command": "echo hi"}, "tool_response": {"stdout": "hi"}})
    wall = []
    env = dict(os.environ, AIQ_PROBE_LOG=log)
    for _ in range(n):
        t0 = time.perf_counter()
        subprocess.run([sys.executable, COLLECTOR], input=payload, text=True, capture_output=True, env=env)
        wall.append((time.perf_counter() - t0) * 1000)
    return {"runs": n, "process_ms_p50": round(pct(wall, 0.5), 1), "process_ms_p95": round(pct(wall, 0.95), 1),
            "process_ms_max": round(max(wall), 1)}


def settings(log):
    cmd = "AIQ_PROBE_LOG={0} {1} {2}".format(log, sys.executable, COLLECTOR)
    obs = {"type": "command", "command": cmd, "async": True}
    return {"hooks": {
        "UserPromptSubmit": [{"hooks": [obs]}],
        "PostToolUse": [{"matcher": "*", "hooks": [obs]}],
        "Stop": [{"hooks": [{"type": "command", "command": cmd}]}],  # sync: renders the card
    }}


def live(runs, log):
    out = []
    for i in range(runs):
        d = tempfile.mkdtemp(prefix="aiq-hookprobe-")
        os.makedirs(os.path.join(d, ".claude"))
        with open(os.path.join(d, ".claude", "settings.json"), "w") as fh:
            json.dump(settings(log), fh)
        t0 = time.perf_counter()
        p = subprocess.run(["claude", "-p", "--model", "claude-haiku-5-5", "--setting-sources", "project",
                            "--strict-mcp-config", "--no-session-persistence", "--allowedTools", "Bash",
                            "--output-format", "stream-json", "--verbose", "--include-hook-events",
                            "Use the Bash tool to run: echo aiq-probe. Then reply with the single word done."],
                           cwd=d, capture_output=True, text=True, timeout=300)
        wall = (time.perf_counter() - t0) * 1000
        events, card_seen, hook_events = [], False, []
        for line in p.stdout.splitlines():
            try:
                e = json.loads(line)
            except ValueError:
                continue
            events.append(e)
            s = json.dumps(e)
            if "coaching card would render" in s:
                card_seen = True
            if "hook" in str(e.get("subtype", "")) or "hook" in str(e.get("type", "")):
                hook_events.append({k: e.get(k) for k in ("type", "subtype", "hook_event", "hook_name", "exit_code", "outcome") if k in e})
        out.append({"run": i + 1, "exit": p.returncode, "session_wall_ms": round(wall),
                    "stream_events": len(events), "stop_card_in_stream": card_seen,
                    "hook_events": hook_events[:12], "stderr": p.stderr[-300:]})
        shutil.rmtree(d, ignore_errors=True)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--bench", type=int, default=50)
    ap.add_argument("--out", default=os.path.join(tempfile.gettempdir(), "aiq-hookprobe.json"))
    a = ap.parse_args()
    log = os.path.join(tempfile.gettempdir(), "aiq-hookprobe-log.jsonl")
    if os.path.exists(log):
        os.remove(log)
    res = {"bench": bench(a.bench, log)}
    open(log, "w").close()
    res["live"] = live(a.runs, log)
    recs = [json.loads(x) for x in open(log)]
    res["hooks_fired"] = {}
    for r in recs:
        res["hooks_fired"][r["event"]] = res["hooks_fired"].get(r["event"], 0) + 1
    cm = [r["collector_ms"] for r in recs]
    res["collector_in_process_ms_p95"] = round(pct(cm, 0.95), 2) if cm else None
    res["payload_keys"] = {r["event"]: r["keys"] for r in recs}
    with open(a.out, "w") as fh:
        json.dump(res, fh, indent=1)
    print(json.dumps(res, indent=1))


if __name__ == "__main__":
    main()
