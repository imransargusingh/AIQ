#!/usr/bin/env python3
"""dispatch.py - ONE hook process per host event (hook cost, bench decision B15).

Every surface used to register one command per hook, so a host launched three or four
`git -> sh -> python` chains before every tool call. Measured in pack-on bench cells on
Windows: a Grok file read took a median 11 s against ~0 pack-off, and launching one hook cost
~500 ms here (2026-10-14) before its own code ran. Each surface now registers this script once
per event; it runs that event's hooks, in their documented order, inside this one process.

Each hook keeps its own contract. It reads the same stdin payload and decides alone; its
exit code and output keep their meaning:
  - exit 2 from any hook (a blocking guard) makes this process exit 2. Its reason stays on stderr.
  - a hook that raises is fail-open (exit 0), as each hook is when run alone.
  - JSON objects printed by two hooks are merged into ONE object, because a host parses one.
    Equal values are kept, two strings are joined by a newline, two lists are joined, and two
    objects are merged key by key. Output that is not a JSON object is passed through as text.

Byte-code caches go under the user's own cache home (sys.pycache_prefix), never into the working
tree, and never into a cache another user can write (FR-086): then no byte code is written.

Hook time, where it is asked for (bench REC-UNREAD-A, 2026-10-17: Claude Code's record keeps no
start for a hook, so a session's hook time was not recorded). When <git-dir>/ai-forward/
hook-times.jsonl exists, each dispatch appends one line: host, event, hooks, start, ms, exit.
The pack never creates the file, so nothing is written unless a caller (the bench) asked, and
never into the working tree. `ms` is this process's time from main() to its exit code; the host's
launch chain (git -> sh -> python start-up) is outside it. A write that fails is skipped.

Usage: dispatch.py --host claude|grok|copilot|agy --event <host event name>
"""
import argparse
import contextlib
import importlib.util
import io
import json
import os
import stat
import sys
import time
from datetime import datetime, timezone

HERE = os.path.dirname(os.path.abspath(__file__))
HOOK_TIMES = os.path.join("ai-forward", "hook-times.jsonl")  # under the checkout's git dir; see the docstring

# The hooks each surface runs per event, in order. Revision 100 registered these one process each.
# A hook that takes --event gets the event name; the other hooks take only --host.
_EVENT_HOOKS = ("mail-doorbell.py", "heartbeat.py", "owner-review-gate.py")
_PLAN = {
    "claude": {
        "PreToolUse": ("reread-guard.py", "mail-doorbell.py", "git-identity-guard.py"),
        "UserPromptSubmit": ("reread-guard.py", "mail-doorbell.py"),
        "SessionStart": ("session-start.py",),
        "SubagentStart": ("session-start.py",),
        "PostToolUse": ("heartbeat.py",),
        "Stop": ("heartbeat.py", "owner-review-gate.py"),
        "SubagentStop": ("owner-review-gate.py",),
    },
    "grok": {
        "PreToolUse": ("reread-guard.py", "mail-doorbell.py", "heartbeat.py", "git-identity-guard.py"),
        "UserPromptSubmit": ("reread-guard.py", "mail-doorbell.py"),
        "SessionStart": ("session-start.py",),
        "SubagentStart": ("session-start.py",),
        "Stop": ("owner-review-gate.py",),
    },
    "copilot": {
        "sessionStart": ("session-start.py",),
        "subagentStart": ("session-start.py",),
        "preToolUse": ("reread-guard.py", "mail-doorbell.py", "git-identity-guard.py"),
        "postToolUse": ("heartbeat.py",),
        "userPromptSubmitted": ("reread-guard.py",),
        "agentStop": ("mail-doorbell.py", "heartbeat.py", "owner-review-gate.py"),
        "subagentStop": ("mail-doorbell.py", "heartbeat.py", "owner-review-gate.py"),
    },
    "agy": {
        "PreToolUse": ("reread-guard.py",),
        "PreInvocation": ("session-start.py", "mail-doorbell.py"),
        "PostToolUse": ("heartbeat.py",),
        "Stop": ("heartbeat.py", "owner-review-gate.py"),
    },
}
ROUTES = {host: {event: [(script, ["--host", host] + (["--event", event] if script in _EVENT_HOOKS else []))
                         for script in scripts]
                 for event, scripts in events.items()}
          for host, events in _PLAN.items()}


def _merge(into, item):
    for key, value in item.items():
        if key not in into or into[key] == value:
            into[key] = value
        elif isinstance(into[key], dict) and isinstance(value, dict):
            _merge(into[key], value)
        elif isinstance(into[key], list) and isinstance(value, list):
            into[key] = into[key] + value
        elif isinstance(into[key], str) and isinstance(value, str):
            into[key] = into[key] + "\n" + value
        else:
            into[key] = value


def merge_outputs(outputs):
    """The stdout texts of several hooks -> the one text the host reads."""
    texts = [text.strip() for text in outputs if text and text.strip()]
    if not texts:
        return ""
    try:
        objects = [json.loads(text) for text in texts]
    except ValueError:
        objects = None
    if objects is None or not all(isinstance(item, dict) for item in objects):
        return "\n".join(texts)
    merged = {}
    for item in objects:
        _merge(merged, item)
    return json.dumps(merged)


def _run(script, argv, raw):
    """Run one hook's main() on the shared payload. Returns (exit code, stdout text)."""
    out = io.StringIO()
    saved_stdin, saved_argv = sys.stdin, sys.argv
    sys.stdin, sys.argv = io.StringIO(raw), [script] + argv
    try:
        name = "aif_hook_" + script[:-3].replace("-", "_")
        spec = importlib.util.spec_from_file_location(name, os.path.join(HERE, script))
        module = importlib.util.module_from_spec(spec)
        with contextlib.redirect_stdout(out):
            spec.loader.exec_module(module)
            code = module.main(argv)
    except SystemExit as stop:
        code = stop.code if isinstance(stop.code, int) else 0
    except Exception:  # noqa: BLE001 - a hook that breaks must never cost the host's tool call
        code = 0
    finally:
        sys.stdin, sys.argv = saved_stdin, saved_argv
    return (code or 0), out.getvalue()


def _bytecode_cache():
    """The per-user byte-code cache, or None when it is not private (FR-086).

    A fixed name under the shared temp dir let another user of a POSIX host create it first
    and plant `.pyc` files the hooks would import. This one is under the user's own cache
    home and refused unless it and its parent are directories only their owner can write.
    """
    base = os.path.join(os.environ.get("XDG_CACHE_HOME") or os.path.join(os.path.expanduser("~"), ".cache"),
                        "ai-forward")
    path = os.path.join(base, "pycache")
    try:
        os.makedirs(path, mode=0o700, exist_ok=True)
        if os.name != "nt":
            for directory in (base, path):
                info = os.lstat(directory)
                if (not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid()
                        or info.st_mode & 0o022):
                    return None
    except OSError:
        return None
    return path


def _git_dir(path):
    """The git dir of the checkout holding `path` (`.git`, or the directory a linked worktree's
    `.git` file names), or None outside git. Filesystem only (audit-log.py's rule)."""
    here = os.path.abspath(path)
    while True:
        dot = os.path.join(here, ".git")
        if os.path.isdir(dot):
            return dot
        if os.path.isfile(dot):
            with open(dot, encoding="utf-8") as f:
                text = f.read().strip()
            if not text.startswith("gitdir:"):
                return None
            target = text.split(":", 1)[1].strip()
            target = target if os.path.isabs(target) else os.path.join(here, target)
            return target if os.path.isdir(target) else None
        parent = os.path.dirname(here)
        if parent == here:
            return None
        here = parent


def record_time(raw, host, event, scripts, code, start, ms):
    """Append one timing line where <git-dir>/ai-forward/hook-times.jsonl exists (see the docstring)."""
    try:
        cwd = json.loads(raw).get("cwd") if raw.strip().startswith("{") else None
        git_dir = _git_dir(cwd if isinstance(cwd, str) and cwd else os.getcwd())
        path = os.path.join(git_dir, HOOK_TIMES) if git_dir else None
        if path and os.path.isfile(path):
            row = {"host": host, "event": event, "hooks": scripts, "start": start, "ms": round(ms, 1), "exit": code}
            with open(path, "a", encoding="utf-8", newline="\n") as f:
                f.write(json.dumps(row) + "\n")
    except Exception:  # noqa: BLE001 - a measurement that fails must never cost the host's tool call
        pass


def main(argv=None):
    started = time.perf_counter()
    start = datetime.now(timezone.utc).isoformat(timespec="milliseconds")
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--host", required=True)
    parser.add_argument("--event", required=True)
    args = parser.parse_args(argv)
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            try:
                stream.reconfigure(encoding="utf-8", errors="replace")
            except (ValueError, OSError):
                pass
    route = ROUTES.get(args.host, {}).get(args.event)
    if not route:
        return 0  # an event this surface does not wire: nothing to run, never a denial
    try:
        cache = _bytecode_cache()
        if cache:
            sys.pycache_prefix = cache
        else:
            sys.dont_write_bytecode = True
        stdin = io.TextIOWrapper(sys.stdin.buffer, encoding="utf-8", errors="replace") \
            if hasattr(sys.stdin, "buffer") else sys.stdin
        raw = "" if stdin.isatty() else stdin.read()
    except Exception:  # noqa: BLE001 - no readable stdin is an empty payload, never a denial
        raw = ""
    if HERE not in sys.path:
        sys.path.insert(0, HERE)
    codes, outputs = [], []
    for script, hook_argv in route:
        code, text = _run(script, hook_argv, raw)
        codes.append(code)
        outputs.append(text)
    text = merge_outputs(outputs)
    if text:
        sys.stdout.write(text + "\n")
    sys.stdout.flush()
    code = 2 if 2 in codes else 0
    record_time(raw, args.host, args.event, [script for script, _argv in route], code, start,
                (time.perf_counter() - started) * 1000)
    return code


if __name__ == "__main__":
    sys.exit(main())
