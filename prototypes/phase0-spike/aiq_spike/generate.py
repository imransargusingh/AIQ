"""Render the scenario library into on-disk session stores, in the formats AIQ will read.

Writes, under <out>/:
  claude_home/projects/<slug>/<session>.jsonl        Claude Code transcripts
  copilot_home/session-store.db                      Copilot CLI session index (sessions table)
  copilot_home/session-state/<session>/events.jsonl  Copilot CLI event stream
  labels.json                                        ground truth: turn -> task, task -> job/rung/potential

Formats follow what AI-Forward's session-profile.py reads (fields it parses, and their meaning as
documented there). assume: fields session-profile.py does not read (e.g. Claude Code's slash-command
wrapper `<command-name>`, Copilot tool argument names) are modelled from memory of the real formats;
if they differ, only the normaliser's command/tool mapping changes. Confirm on one real transcript
per surface before trusting the reader on real data (named in the findings report).

The session plan is hand-designed to exercise the grouping cases the rung model names: several
tasks per session with short gaps, a task resumed in a later session, a task that moves from
Claude Code to Copilot CLI, a long idle gap inside one task, and two tasks on the same feature
with different jobs.
"""
import datetime as dt
import json
import os
import random
import re
import sqlite3
import sys
import uuid

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from aiq_spike.scenarios import SCENARIOS  # noqa: E402

REPO = "/work/acme-shop"
REPO_SLUG = "acme/acme-shop"
COPILOT_TOOL = {"Read": "view", "Edit": "edit", "Write": "create", "Bash": "bash", "Grep": "grep"}

# Each session: (surface, start time, [items]); an item is (scenario id, part, minutes before it).
# part is "all", 0 (before SPLIT) or 1 (after SPLIT).
D = dt.datetime(2026, 10, 19, tzinfo=dt.timezone.utc)
PLAN = [
    ("claude", D.replace(hour=9, minute=2), [("c03", "all", 0), ("d07", "all", 3), ("c09", "all", 6)]),
    ("claude", D.replace(hour=10, minute=40), [("c05", 0, 0)]),
    ("copilot", D.replace(hour=13, minute=15), [("c05", 1, 0), ("o01", "all", 4)]),
    ("claude", D.replace(hour=15, minute=0), [("d02", "all", 0), ("c10", "all", 2), ("d01", "all", 2)]),
    ("copilot", D.replace(day=20, hour=9, minute=30), [("c02", "all", 0), ("o05", "all", 9)]),
    ("claude", D.replace(day=20, hour=11, minute=0), [("x01", "all", 0), ("x02", "all", 3), ("x03", "all", 25)]),
    ("claude", D.replace(day=20, hour=14, minute=10), [("d04", "all", 0), ("o04", "all", 5)]),
    ("copilot", D.replace(day=21, hour=8, minute=55), [("c08", "all", 0), ("d06", "all", 1)]),
    ("claude", D.replace(day=21, hour=10, minute=5), [("c07", "all", 0)]),
    ("copilot", D.replace(day=21, hour=13, minute=30), [("d03", "all", 0), ("o02", "all", 7), ("c04", "all", 4)]),
    ("claude", D.replace(day=22, hour=9, minute=0), [("d05", "all", 0), ("o06", "all", 12)]),
    ("copilot", D.replace(day=22, hour=11, minute=20), [("c11", "all", 0), ("d08", "all", 2), ("o03", "all", 30)]),
    ("claude", D.replace(day=22, hour=15, minute=40), [("c06", "all", 0), ("d09", "all", 6), ("c12", "all", 75)]),
    ("claude", D.replace(day=23, hour=9, minute=10), [("c01", "all", 0)]),
]
# c06 above has a long idle gap inside the task (lunch) injected below, to test time-only grouping.
INNER_GAPS = {"c06": (2, 70)}  # after step index 2, 70 minutes idle


def steps_for(sc, part):
    steps = [tuple(x) for x in sc["steps"]]
    if "c06" == sc["id"] and sc is SCENARIOS.get("c06"):
        i, m = INNER_GAPS["c06"]
        steps.insert(i + 1, ("gap", m))
    if part == "all":
        return [s for s in steps if s[0] != "split"]
    k = next(i for i, s in enumerate(steps) if s[0] == "split")
    return steps[:k] if part == 0 else steps[k + 1:]


class Clock:
    def __init__(self, t, rng):
        self.t, self.rng = t, rng

    def tick(self, lo=2, hi=40):
        self.t += dt.timedelta(seconds=self.rng.randint(lo, hi))
        return self.t.isoformat().replace("+00:00", "Z")

    def wait(self, minutes):
        self.t += dt.timedelta(minutes=minutes)


def claude_slug(path):
    return re.sub(r"[^A-Za-z0-9]", "-", os.path.abspath(path))


# --------------------------------------------------------------------------- Claude Code
def render_claude(sid, start, items, rng, labels):
    clk, recs, parent = Clock(start, rng), [], None
    base = {"sessionId": sid, "cwd": REPO, "gitBranch": "main", "version": "2.3.0",
            "userType": "external", "isSidechain": False}

    def add(rec):
        nonlocal parent
        rec.update(base)
        rec["uuid"] = str(uuid.UUID(int=rng.getrandbits(128)))
        rec["parentUuid"] = parent
        parent = rec["uuid"]
        recs.append(rec)
        return rec["uuid"]

    def assistant(content):
        return {"type": "assistant", "timestamp": clk.tick(3, 30),
                "message": {"role": "assistant", "model": "claude-sonnet-5", "content": content,
                            "usage": {"input_tokens": rng.randint(20, 400), "output_tokens": rng.randint(40, 900)}}}

    def tool(name, args, result, sidechain=False):
        tid = "toolu_" + uuid.UUID(int=rng.getrandbits(128)).hex[:20]
        r = assistant([{"type": "tool_use", "id": tid, "name": name, "input": args}])
        r["isSidechain"] = sidechain
        add(r)
        add({"type": "user", "timestamp": clk.tick(1, 20),
             "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": tid, "content": result}]}})

    for scid, part, before in items:
        clk.wait(before)
        for st in steps_for(SCENARIOS[scid], part):
            kind = st[0]
            if kind == "gap":
                clk.wait(st[1])
            elif kind == "user":
                u = add({"type": "user", "timestamp": clk.tick(20, 120), "message": {"role": "user", "content": st[1]}})
                labels["turns"]["claude:{0}:{1}".format(sid, u)] = scid
            elif kind == "command":
                # assume: the slash-command wrapper Claude Code writes for a typed /command
                body = "<command-name>{0}</command-name>\n<command-message>{1}</command-message>\n<command-args>{2}</command-args>".format(
                    st[1], st[1].lstrip("/"), st[2])
                u = add({"type": "user", "timestamp": clk.tick(20, 60), "message": {"role": "user", "content": body}})
                labels["turns"]["claude:{0}:{1}".format(sid, u)] = scid
            elif kind == "assistant":
                add(assistant([{"type": "text", "text": st[1]}]))
            elif kind == "tool":
                tool(st[1], st[2], st[3])
            elif kind == "subagent":
                tool("Task", {"description": st[1], "prompt": st[2], "subagent_type": "general-purpose"},
                     "sub-agent {0} finished: {1} tool calls".format(st[1], len(st[3])))
                # simplify: real Claude Code writes sub-agent work to <session>/subagents/agent-*.jsonl;
                # the spike reads only the Task call (count, name, prompt). Upgrade when the judge needs
                # sub-agent tool mixes.
                clk.wait(len(st[3]))
    return recs


# --------------------------------------------------------------------------- Copilot CLI
def render_copilot(sid, start, items, rng, labels):
    clk, ev = Clock(start, rng), []

    def add(etype, data, agent=None):
        e = {"type": etype, "id": str(uuid.UUID(int=rng.getrandbits(128))), "timestamp": clk.tick(2, 30), "data": data}
        if agent:
            e["agentId"] = agent
        ev.append(e)
        return e

    iid = None

    def tool(name, args, result, agent=None):
        tcid = "call_" + uuid.UUID(int=rng.getrandbits(128)).hex[:16]
        tn = COPILOT_TOOL.get(name, name.lower())
        a = dict(args)
        if "file_path" in a:  # session-profile.py reads `path` (or file_path) for view
            a["path"] = a.pop("file_path")
        add("tool.execution_start", {"toolCallId": tcid, "toolName": tn, "arguments": json.dumps(a), "interactionId": iid}, agent)
        add("tool.execution_complete", {"toolCallId": tcid, "result": {"content": result}, "interactionId": iid}, agent)

    for scid, part, before in items:
        clk.wait(before)
        for st in steps_for(SCENARIOS[scid], part):
            kind = st[0]
            if kind == "gap":
                clk.wait(st[1])
            elif kind in ("user", "command"):
                iid = str(uuid.UUID(int=rng.getrandbits(128)))
                text = st[1] if kind == "user" else (st[1] + (" " + st[2] if st[2] else ""))
                clk.tick(20, 100)
                e = add("user.message", {"content": text, "delivery": "idle", "interactionId": iid})
                labels["turns"]["copilot:{0}:{1}".format(sid, e["id"])] = scid
            elif kind == "assistant":
                add("assistant.message", {"content": st[1], "interactionId": iid})
            elif kind == "tool":
                tool(st[1], st[2], st[3])
            elif kind == "subagent":
                aid = "agent-" + uuid.UUID(int=rng.getrandbits(128)).hex[:8]
                tool("task", {"agent_type": "general-purpose", "description": st[1], "prompt": st[2]}, "started")
                add("subagent.started", {"agentDisplayName": st[1], "agentName": st[1]}, aid)
                add("user.message", {"content": st[2]}, aid)  # no delivery value: never a human turn
                for sub in st[3]:
                    tool(sub[1], sub[2], sub[3], agent=aid)
    return ev


def main(out, seed=7, scenarios=None, plan=None):
    """Render a corpus. `scenarios`/`plan` default to the dev library; the default is restored after."""
    global SCENARIOS
    default = SCENARIOS
    if scenarios is not None:
        SCENARIOS = scenarios
    try:
        _render(out, seed, plan or PLAN)
    finally:
        SCENARIOS = default


def _render(out, seed, plan):
    rng = random.Random(seed)
    claude_dir = os.path.join(out, "claude_home", "projects", claude_slug(REPO))
    cop = os.path.join(out, "copilot_home")
    os.makedirs(claude_dir, exist_ok=True)
    os.makedirs(os.path.join(cop, "session-state"), exist_ok=True)
    labels = {"turns": {}, "tasks": {}}
    dbp = os.path.join(cop, "session-store.db")
    if os.path.exists(dbp):
        os.remove(dbp)
    con = sqlite3.connect(dbp)
    con.execute("create table sessions (id text primary key, cwd text, repository text, summary text, created_at text, updated_at text)")
    for surface, start, items in plan:
        sid = str(uuid.UUID(int=rng.getrandbits(128)))
        if surface == "claude":
            recs = render_claude(sid, start, items, rng, labels)
            with open(os.path.join(claude_dir, sid + ".jsonl"), "w", encoding="utf-8") as fh:
                for r in recs:
                    fh.write(json.dumps(r) + "\n")
        else:
            ev = render_copilot(sid, start, items, rng, labels)
            d = os.path.join(cop, "session-state", sid)
            os.makedirs(d, exist_ok=True)
            with open(os.path.join(d, "events.jsonl"), "w", encoding="utf-8") as fh:
                for e in ev:
                    fh.write(json.dumps(e) + "\n")
            con.execute("insert into sessions values (?,?,?,?,?,?)",
                        (sid, REPO, REPO_SLUG, "", ev[0]["timestamp"], ev[-1]["timestamp"]))
    con.commit()
    con.close()
    used = {i[0] for _, _, items in plan for i in items}
    missing = set(SCENARIOS) - used
    if missing:
        raise SystemExit("scenarios not placed in any session: {0}".format(sorted(missing)))
    for scid in sorted(used):
        sc = SCENARIOS[scid]
        labels["tasks"][scid] = {k: sc[k] for k in ("job", "rung", "potential", "ambiguous", "note")}
    with open(os.path.join(out, "labels.json"), "w", encoding="utf-8") as fh:
        json.dump(labels, fh, indent=1)
    print("wrote {0} sessions, {1} tasks, {2} human turns to {3}".format(
        len(plan), len(labels["tasks"]), len(labels["turns"]), out))


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "corpus")
