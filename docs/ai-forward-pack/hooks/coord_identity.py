#!/usr/bin/env python3
"""Shared hook identity validation and Copilot child-id construction.

Hooks must never invent or truncate an identity into a collision. Parent identities come
from AGENT_SESSION; Copilot child identities are derived from the native session/agent
pair only when both sides are present, valid, and bounded.
"""
from __future__ import annotations

import re
from typing import Any, Optional

_PARENT_RX = re.compile(r"[A-Za-z0-9_.-]{1,160}\Z")
_CHILD_PART_RX = re.compile(r"[A-Za-z0-9_.-]{1,64}\Z")
_EMPTY = ("", ".", "..")


def valid_parent_session(value: Any) -> bool:
    return isinstance(value, str) and _PARENT_RX.fullmatch(value) is not None and value not in _EMPTY


def parent_session(value: Any) -> Optional[str]:
    if value in (None, ""):
        return None
    return value if valid_parent_session(value) else None


def _valid_child_part(value: Any) -> bool:
    return isinstance(value, str) and _CHILD_PART_RX.fullmatch(value) is not None and value not in _EMPTY


def copilot_child_identity(session_id: Any, agent_id: Any) -> Optional[str]:
    if session_id in (None, "") and agent_id in (None, ""):
        return None
    if not (_valid_child_part(session_id) and _valid_child_part(agent_id)):
        return None
    session = str(session_id)
    agent = str(agent_id)
    identity = f"copilot-child.{len(session)}.{session}.{len(agent)}.{agent}"
    return identity if valid_parent_session(identity) else None


def copilot_child_identity_from_payload(payload: Any) -> Optional[str]:
    if not isinstance(payload, dict):
        return None
    return copilot_child_identity(payload.get("sessionId") or payload.get("session_id"),
                                  payload.get("agentId") or payload.get("agent_id"))
