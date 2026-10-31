#!/usr/bin/env python3
"""heartbeat.py - the progress heartbeat, sampled by the HOST at its tool seam (P3, D7).

Liveness is read from the world, never volunteered: the host runs this on every tool call
(`PostToolUse` on Claude Code and Antigravity, Claude-format `PreToolUse` on Grok Build,
`preToolUse` on Copilot CLI) and at its stop-class event (`Stop` / `agentStop`). Each run adds
one call and any file a write-class tool touched to a machine-local accumulator; at most once
per HEARTBEAT_SAMPLE (100 s, D13's renew cadence) - or on a stop-class event - the accumulated
deltas are written as ONE `kind: heartbeat` row in the caller's ledger (`$AGENT_SESSION`). A
zero-delta row is written on Stop on purpose: `coord track` renders it `stalled`, never `live`.

The row carries counts, never the paths (a path from stdin is data: relativised, counted, not
stored, not opened). Tokens are `not recorded` - no host exposes usage at this seam.

Prints nothing and exits 0 on EVERY path, including its own failures: Copilot denies a tool
call on any non-zero exit but 2, and a heartbeat must never cost the session a call.

Usage: heartbeat.py --host claude|grok|agy|copilot [--event <name>] [--session <id>]
Payload keys accepted (KB data-and-constants.md "Hook surfaces"; every key optional):
  Claude/Grok  hook_event_name, session_id, cwd, tool_name, tool_input{file_path|path|...}
  Copilot      sessionId, toolName, toolArgs
  Antigravity  conversationId, workspacePaths, tool_name, tool_input
Per-harness execution status lives with the doorbell's: `verified` only when a live session
on this machine showed the event fire (CO12); Claude Code is executed here against the
documented contract, the other three are observed-only.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
import sys
import time
from pathlib import Path
from typing import Any, Dict, List

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

from coord_identity import copilot_child_identity_from_payload, parent_session

for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass

STOP_EVENTS = frozenset({"Stop", "SubagentStop", "TaskCompleted", "agentStop", "subagentStop",
                         "PostInvocation"})


def _load_core():
    """coord-core.py sits in ../../scripts (pack source) or ../scripts (deployed bundle)."""
    for candidate in (HERE.parent.parent / "scripts" / "coord-core.py", HERE.parent / "scripts" / "coord-core.py"):
        if candidate.is_file():
            spec = importlib.util.spec_from_file_location("coord_core_for_heartbeat", candidate)
            module = importlib.util.module_from_spec(spec)
            assert spec.loader is not None
            spec.loader.exec_module(module)
            return module
    return None


def touched_files(core, payload: Dict[str, Any], host: str) -> List[str]:
    """Paths a WRITE-class tool named, as strings; the caller counts them and never opens them."""
    cwd = payload.get("cwd") or payload.get("workspaceRoot") or os.getcwd()
    try:
        calls = core.parse_hook_request(payload, core.repo_root(cwd), host=host, cwd=cwd)
    except (OSError, ValueError):
        tool = str(payload.get("tool_name") or payload.get("toolName") or "").lower()
        return ["."] if tool in core._WRITE_TOOLS else []
    return [path for tool, path in calls if path and str(tool).lower() in core._WRITE_TOOLS]


def main(argv=None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", choices=["claude", "grok", "agy", "copilot"], required=True)
    parser.add_argument("--event", default=None)
    parser.add_argument("--session", default=None)
    args = parser.parse_args(argv)
    try:
        if args.host == "claude" and os.environ.get("AGENT_HOST") == "copilot":
            return 0
        env_session = parent_session(os.environ.get("AGENT_SESSION"))
        if os.environ.get("AGENT_SESSION") and not env_session:
            return 0
        raw = sys.stdin.read() if not sys.stdin.isatty() else ""
        try:
            payload = json.loads(raw or "{}")
        except ValueError:
            payload = {}
        if not isinstance(payload, dict):
            payload = {}
        native_child = copilot_child_identity_from_payload(payload) if args.host == "copilot" else None
        if args.host == "copilot" and (payload.get("agentId") or payload.get("agent_id")) and not native_child:
            return 0
        session = args.session or native_child or env_session or ""
        if not session:
            return 0
        event = str(args.event or payload.get("hook_event_name") or payload.get("hookEventName")
                    or payload.get("event") or "")
        workspaces = payload.get("workspacePaths") or []
        cwd = str(workspaces[0] if workspaces else (payload.get("cwd") or payload.get("workspaceRoot")
                                                     or os.getcwd()))
        core = _load_core()
        if core is None:
            return 0
        root, err = core.resolve_root(cwd, os.environ.get("COORD_ROOT"))
        if err or root is None or not core.coordination_active(root):
            return 0  # coordination is off here: no ledger, no accumulator (B15)
        repo = core.repo_root(cwd)
        stop = event in STOP_EVENTS
        agent = (str(payload.get("agentId") or payload.get("agent_id") or "").strip()
                 or os.environ.get("AGENT_NAME") or session)
        core.heartbeat_tick(root, repo, session, agent, time.time(),
                            files=touched_files(core, payload, args.host), calls=0 if stop else 1,
                            host=args.host, event=event, cwd=cwd, flush=stop)
    except Exception:  # noqa: BLE001 - a heartbeat must never cost the session a tool call
        return 0
    return 0


if __name__ == "__main__":
    sys.exit(main())
