#!/usr/bin/env python3
"""Phase 2 laptop probe: Copilot CLI status line, and Copilot hooks inside VS Code agent mode.

  python prototypes/phase2-probe/probe.py cli         # A: starts Copilot CLI in a throwaway COPILOT_HOME
                                                    #    (run it in Windows Terminal, not VS Code's terminal)
  python prototypes/phase2-probe/probe.py vscode-on   # B: adds ONE file, ~/.copilot/hooks/aiq-probe.json
  python prototypes/phase2-probe/probe.py vscode-off  #    removes that file again
  python prototypes/phase2-probe/probe.py report      # writes results/laptop-probe.json (scrubbed)

A never touches the person's real Copilot config: it uses a temp COPILOT_HOME (the Phase 0 probe showed
that needs no auth files). B adds one hook file to the real ~/.copilot/hooks and nothing else; it is the
only way to see whether VS Code agent mode runs user-level hooks. While it exists, Copilot CLI sessions
run it too (about 150 ms per event), so turn it off straight after.

Questions answered (design doc docs/design/copilot-vscode-slice.md, spikes P1-P4):
 P1 Does Copilot CLI show a status-line command's output, and when does it refresh?
 P2 What JSON does the status-line command get on stdin (is there a session id)?
 P3 Do user-level hooks fire in VS Code agent mode, and can a hook tell VS Code from the CLI?
 P4 Does the session id in hooks match the one the status line gets?
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from collections import Counter, defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))
LOGGER = os.path.join(HERE, "logger.py")
LOG = os.path.join(tempfile.gettempdir(), "aiq-phase2-probe.jsonl")
TEMP_HOME = os.path.join(tempfile.gettempdir(), "aiq-probe2-copilot-home")
REAL_HOOK = os.path.join(os.environ.get("COPILOT_HOME") or os.path.join(os.path.expanduser("~"), ".copilot"),
                         "hooks", "aiq-probe.json")
EVENTS = ["sessionStart", "userPromptSubmitted", "agentStop", "sessionEnd"]
PY = sys.executable


def hook_file():
    return {"version": 1, "hooks": {e: [{"type": "command", "exec": PY, "args": [LOGGER, "hook", e],
                                         "timeoutSec": 10}] for e in EVENTS}}


def statusline_command(home):
    if os.name == "nt":  # a .cmd wrapper, as the third-party Windows setups use; path has no spaces
        wrapper = os.path.join(home, "aiq-probe-statusline.cmd")
        with open(wrapper, "w", encoding="ascii") as fh:
            fh.write(f'@"{PY}" "{LOGGER}" statusline\r\n')
        return wrapper
    return f'"{PY}" "{LOGGER}" statusline'


def cli():
    shutil.rmtree(TEMP_HOME, ignore_errors=True)
    os.makedirs(os.path.join(TEMP_HOME, "hooks"))
    with open(os.path.join(TEMP_HOME, "hooks", "aiq-probe.json"), "w", encoding="utf-8") as fh:
        json.dump(hook_file(), fh, indent=2)
    settings = {"experimental": True, "feature_flags": {"enabled": ["STATUS_LINE"]},
                "statusLine": {"type": "command", "command": statusline_command(TEMP_HOME)}}
    with open(os.path.join(TEMP_HOME, "settings.json"), "w", encoding="utf-8") as fh:
        json.dump(settings, fh, indent=2)
    exe = shutil.which("copilot")
    if not exe:
        sys.exit("copilot is not on PATH")
    print("Starting Copilot CLI with a throwaway COPILOT_HOME. Your real config is not touched.\n"
          "1. Look at the bottom of the screen for 'AIQ probe: status line works <time>'.\n"
          "   If it is not there, type /statusline and turn it on.\n"
          "2. Send two short prompts (e.g. 'say hi', then 'say bye'). Note whether the time changes.\n"
          "3. Exit Copilot (/exit). The probe prints a short summary.\n")
    env = dict(os.environ, COPILOT_HOME=TEMP_HOME)
    started = time.time()
    subprocess.call([exe], env=env)
    summary(since=started)


def vscode_on():
    if os.path.exists(REAL_HOOK):
        sys.exit(f"{REAL_HOOK} already exists; run vscode-off first")
    os.makedirs(os.path.dirname(REAL_HOOK), exist_ok=True)
    with open(REAL_HOOK, "w", encoding="utf-8") as fh:
        json.dump(hook_file(), fh, indent=2)
    print(f"Added {REAL_HOOK}.\n"
          "Now in VS Code: open Copilot Chat in Agent mode (session target Copilot), send one short prompt,\n"
          "wait for the reply, then run: python prototypes/phase2-probe/probe.py vscode-off")


def vscode_off():
    if os.path.exists(REAL_HOOK):
        os.remove(REAL_HOOK)
        print(f"Removed {REAL_HOOK}.")
    else:
        print("Nothing to remove.")
    summary(since=time.time() - 3600)


def load(since=0.0):
    if not os.path.exists(LOG):
        return []
    with open(LOG, encoding="utf-8") as fh:
        return [r for r in (json.loads(l) for l in fh if l.strip()) if r.get("t", 0) >= since]


ID_KEYS = ("sessionId", "session_id", "id")


def find_session_id(p):
    """Hooks send camelCase sessionId; the status line's key is unknown (P2), so look one level down too."""
    for k in ID_KEYS[:2]:
        if isinstance(p.get(k), str):
            return p[k]
    s = p.get("session")
    return s.get("id") if isinstance(s, dict) and isinstance(s.get("id"), str) else None


def redact(v):
    if isinstance(v, dict):
        return {k: ("<id>" if k in ID_KEYS and isinstance(x, str) else redact(x)) for k, x in v.items()}
    if isinstance(v, list):
        return [redact(x) for x in v]
    return v


def is_vscode(r):
    return any(k.startswith("VSCODE") for k in r.get("env", {})) or r["env"].get("TERM_PROGRAM") == "vscode"


def analyse(rows):
    out = {"runs": len(rows), "by_mode_event": Counter(), "payload_keys": defaultdict(set),
           "env_names": defaultdict(set), "statusline_gaps_s": [], "session_ids": defaultdict(set)}
    last_sl = None
    for r in rows:
        where = "vscode-env" if is_vscode(r) else "no-vscode-env"
        key = f'{r["mode"]}:{r["event"]}:{where}'
        out["by_mode_event"][key] += 1
        out["payload_keys"][key].add(json.dumps(redact(r["payload"]), sort_keys=True))
        out["env_names"][where].update(r["env"])
        p = r["payload"] if isinstance(r["payload"], dict) else {}
        sid = find_session_id(p)
        if sid:
            out["session_ids"][r["mode"]].add(sid)
        if r["mode"] == "statusline":
            if last_sl is not None:
                out["statusline_gaps_s"].append(round(r["t"] - last_sl, 2))
            last_sl = r["t"]
    shared = set.intersection(*out["session_ids"].values()) if len(out["session_ids"]) > 1 else set()
    return {
        "runs": out["runs"],
        "by_mode_event": dict(out["by_mode_event"]),
        "payload_shapes": {k: sorted(v) for k, v in out["payload_keys"].items()},
        "env_names": {k: sorted(v) for k, v in out["env_names"].items()},
        "statusline_gaps_s": out["statusline_gaps_s"],
        "hook_and_statusline_share_a_session_id": bool(shared),
    }


def summary(since):
    a = analyse(load(since))
    print(json.dumps({k: a[k] for k in ("runs", "by_mode_event", "statusline_gaps_s",
                                        "hook_and_statusline_share_a_session_id")}, indent=2))


def report():
    a = analyse(load())
    os.makedirs(os.path.join(HERE, "results"), exist_ok=True)
    path = os.path.join(HERE, "results", "laptop-probe.json")
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(a, fh, indent=2)
    print(f"Wrote {path}. Session ids and paths are not in it; payload values are lengths or short ids.")


if __name__ == "__main__":
    cmds = {"cli": cli, "vscode-on": vscode_on, "vscode-off": vscode_off, "report": report}
    if len(sys.argv) != 2 or sys.argv[1] not in cmds:
        sys.exit(__doc__)
    cmds[sys.argv[1]]()
