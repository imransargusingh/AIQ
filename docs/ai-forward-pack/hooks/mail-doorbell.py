#!/usr/bin/env python3
"""mail-doorbell.py - ring the host's doorbell with a COUNT and a POINTER, never a body.

A doorbell is a hint; the inbox is truth (docs/design/message-layer.md section 8). At a hook
seam the host runs this script; it folds the caller's inbox (`$AGENT_SESSION`, or --session)
and, when unacknowledged mail newer than five minutes exists, emits one line in the host's
response shape saying how many and which newest id - the model then runs `coord mail read`,
which renders the bodies under an untrusted heading. The string is built by `payload_for`,
which has no body parameter: a doorbell that carries a body is refused by construction.

Hosts and shapes (KB data-and-constants.md "Hook surfaces"; execution status per harness lives
in .agents/harness-status.json, written by coord-mail.py dispatch):
  claude  PreToolUse / UserPromptSubmit -> {"hookSpecificOutput":{"hookEventName":E,"additionalContext":T}}
  grok    same (Claude-format hooks)
  agy     PreInvocation                 -> {"injectSteps":[{"ephemeralMessage":T}]}
  copilot preToolUse                    -> {"additionalContext":T}
          agentStop                     -> {"decision":"block","reason":T}  only when count > 0 and
                                           stop_hook_active is not set (the 8-block guard is never approached)
Exits 0 and prints nothing on every other path (fail-open at the tool seam, as reread-guard.py).

Usage: mail-doorbell.py --host claude|grok|agy|copilot [--event <name>] [--session <id>]
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
import sys
from pathlib import Path
from typing import Any, Dict, Optional

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

TEXT = "coord mail: {count} new for {session}; newest {pointer}; run coord mail read"
CLAUDE_FORMAT_HOSTS = ("claude", "grok")


def _load_mail():
    """coord-mail.py sits in ../../scripts (pack source) or ../scripts (deployed bundle)."""
    for candidate in (HERE.parent.parent / "scripts" / "coord-mail.py", HERE.parent / "scripts" / "coord-mail.py"):
        if candidate.is_file():
            spec = importlib.util.spec_from_file_location("coord_mail_for_doorbell", candidate)
            module = importlib.util.module_from_spec(spec)
            assert spec.loader is not None
            spec.loader.exec_module(module)
            return module
    return None


def doorbell_text(session: str, count: int, pointer: Optional[str]) -> str:
    return TEXT.format(count=int(count), session=session, pointer=pointer)


def payload_for(host: str, event: str, session: str, count: int, pointer: Optional[str],
                stop_hook_active: bool = False) -> Optional[Dict[str, Any]]:
    """The host-shaped response, or None when nothing should ring. No body ever enters here."""
    if not count or count <= 0 or not pointer:
        return None
    text = doorbell_text(session, count, pointer)
    if host in CLAUDE_FORMAT_HOSTS:
        return {"hookSpecificOutput": {"hookEventName": event or "PreToolUse", "additionalContext": text}}
    if host == "agy":
        return {"injectSteps": [{"ephemeralMessage": text}]}   # objects, never strings (protojson; docs/hooks)
    if host == "copilot":
        if event in ("agentStop", "subagentStop"):
            if stop_hook_active:
                return None
            return {"decision": "block", "reason": text}
        return {"additionalContext": text}
    return None


def _event_of(explicit: Optional[str], payload: Dict[str, Any]) -> str:
    return str(explicit or payload.get("hook_event_name") or payload.get("hookEventName")
               or payload.get("event") or "")


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
        mail = _load_mail()
        if mail is None:
            return 0
        root, err = mail.coord_core.resolve_root(os.getcwd(), os.environ.get("COORD_ROOT"))
        if err or root is None or not mail.coord_core.coordination_active(root):
            return 0  # coordination is off here: no inbox to fold (B15)
        count, pointer = mail.doorbell_state(root, session)
        stop_active = bool(payload.get("stop_hook_active") or payload.get("stopHookActive"))
        response = payload_for(args.host, _event_of(args.event, payload), session, count, pointer,
                               stop_hook_active=stop_active)
        if response is not None:
            print(json.dumps(response))
    except Exception:  # noqa: BLE001 - a doorbell must never deny a tool call
        return 0
    return 0


if __name__ == "__main__":
    sys.exit(main())
