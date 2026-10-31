#!/usr/bin/env python3
"""owner-review-gate.py - a session may not stop while a decision request it sent is still open (D6).

At the stop seam the host runs this script. It resolves the session to `$AGENT_SESSION` (the
identity decision requests are written under - the harness `session_id` on stdin is a different
namespace and cannot find them), reads P1's request store through the bounded strict
`coord-core.py decision_request_state` projection and, when that session has an unresolved
request with `reason: decision-request` that IT sent, refuses the stop with a reason that names the
count and the ids - never a body.

Hosts and shapes (pack/adapters/hooks/README.md; per-host status in its table):
  claude  Stop / SubagentStop  -> exit 2, one line on stderr (the host feeds it back as the reason)
  grok    same requested refusal; host enforcement requires runtime observation
  codex   Stop -> exit 0 + {"decision":"block","reason":T}; otherwise valid empty JSON
  copilot agentStop / subagentStop -> exit 0 + {"decision":"block","reason":T} on stdout
  agy     Stop -> exit 0 + {"decision":"continue"} on stdout (at most AGY_MAX_REFUSALS times per stop sequence)

FAIL-SAFE (bounded native feedback; final runner acceptance is independently fail-closed). Exit 0
on: no AGENT_SESSION; stdin not JSON; `stop_hook_active` set (the host's loop
guard); coord-core.py not beside the pack; COORD_ROOT outside the repository; no requests store; a
store with a malformed line; requests sent by other sessions or merely addressed to this one; any
exception. Codex prints valid empty JSON on these paths; other hosts print nothing.
Evaluated invocations append sanitized requested-decision receipts when attribution/state
root is available; a receipt is not proof the host honored it. Nothing on stdin is executed.

Usage: owner-review-gate.py --host claude|codex|grok|copilot|agy [--event <name>] [--session <id>]
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
import re
import sys
import time
from pathlib import Path
from typing import Any, Dict, List, Optional

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

AGY_MAX_REFUSALS = 2
COPILOT_STOP_EVENTS = ("agentStop", "subagentStop")
TEXT = ("owner-review: {count} unresolved decision request(s) sent by {session} ({ids}); "
        "rule it (coord decide rule) or let it expire (coord request expire) before stopping")


def _load_core() -> Optional[Any]:
    """coord-core.py sits in ../../scripts (pack source) or ../scripts (deployed bundle)."""
    for candidate in (HERE.parent.parent / "scripts" / "coord-core.py", HERE.parent / "scripts" / "coord-core.py"):
        if candidate.is_file():
            spec = importlib.util.spec_from_file_location("coord_core_for_owner_review", str(candidate))
            if spec is None or spec.loader is None:
                return None
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            return module
    return None


def reason_text(session: str, ids: List[str], count: int) -> str:
    return TEXT.format(count=count, session=session, ids=", ".join(ids))


def _event_of(explicit: Optional[str], payload: Dict[str, Any]) -> str:
    return str(explicit or payload.get("hook_event_name") or payload.get("hookEventName")
               or payload.get("event") or "")


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--host", choices=["claude", "codex", "grok", "copilot", "agy"], required=True)
    parser.add_argument("--event", default=None)
    parser.add_argument("--session", default=None)
    args = parser.parse_args(argv)
    # Codex requires JSON on successful Stop, including the no-op/loop-guard path.
    def allow():
        if args.host == "codex":
            print("{}")
        return 0

    try:
        if args.host == "claude" and os.environ.get("AGENT_HOST") == "copilot":
            return allow()
        env_session = parent_session(os.environ.get("AGENT_SESSION"))
        if os.environ.get("AGENT_SESSION") and not env_session:
            return allow()
        raw = sys.stdin.read() if not sys.stdin.isatty() else ""
        try:
            payload = json.loads(raw or "{}")
        except ValueError:
            return allow()
        if not isinstance(payload, dict):
            return allow()
        native_child = copilot_child_identity_from_payload(payload) if args.host == "copilot" else None
        if args.host == "copilot" and (payload.get("agentId") or payload.get("agent_id")) and not native_child:
            return allow()
        session = args.session or native_child or env_session or ""
        if not re.fullmatch(r"[A-Za-z0-9_.-]{1,160}", session):
            return allow()
        if args.host == "copilot" and _event_of(args.event, payload) not in COPILOT_STOP_EVENTS:
            return 0
        core = _load_core()
        if core is None:
            return allow()
        root, err = core.resolve_root(os.getcwd(), os.environ.get("COORD_ROOT"))
        if err or root is None or not core.coordination_active(root):
            return allow()  # coordination is off here: no request was ever sent (B15)
        state = core.decision_request_state(Path(root), session)
        ids = state["open_ids"]
        loop_guard = bool(payload.get("stop_hook_active") or payload.get("stopHookActive"))
        attempts = payload.get("executionNum") or 1
        agy_limit = args.host == "agy" and isinstance(attempts, int) and attempts > AGY_MAX_REFUSALS
        result = ("not_checked" if not state["checked"] else "loop_guard" if loop_guard or agy_limit
                  else "refused" if ids else "allowed")
        observed_event = _event_of(args.event, payload)
        if observed_event not in ("Stop", "SubagentStop", "agentStop", "subagentStop"):
            observed_event = "unknown"  # Never persist an arbitrary stdin string as telemetry.
        if Path(root).is_dir() and (Path(root) / "log").is_dir():
            try:
                # This records our requested native decision, not proof the host enforced it.
                core.append_event(root, {"kind": "owner-review-stop", "session": session,
                    "agent": (str(payload.get("agentId") or payload.get("agent_id") or "").strip()
                              or os.environ.get("AGENT_HOST") or args.host), "at": time.time(),
                    "hook_host": args.host, "hook_cwd": str(Path.cwd().resolve()),
                    "event": observed_event, "result": result, **state})
            except (OSError, ValueError):
                pass  # Missing telemetry is not a fabricated receipt or a reason to spin.
        if not state["checked"] or loop_guard or not ids:
            return allow()
        text = reason_text(session, ids, state["open_count"])
        if args.host in ("copilot", "codex"):
            print(json.dumps({"decision": "block", "reason": text}))
            return 0
        if args.host == "agy":
            # Antigravity Stop: {"decision": "continue"} re-enters the loop (docs/hooks). executionNum counts the
            # stop attempts in this sequence; after two refusals the stop is allowed and the reason is left on
            # stderr, so an unruled request can never spin the agent forever.
            print(text, file=sys.stderr)
            if agy_limit:
                return 0
            print(json.dumps({"decision": "continue"}))
            return 0
        print(text, file=sys.stderr)
        return 2
    except Exception:  # noqa: BLE001 - a gate at the trust seam never blocks on a path it cannot evaluate
        return allow()


if __name__ == "__main__":
    sys.exit(main())
