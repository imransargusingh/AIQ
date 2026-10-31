"""Held-out scenarios, written by a model that never saw the dev scenarios, the grouper or the judge.

Why: the dev scenarios, the grouping heuristic and the judge prompt were all written by the same
author in one session, and the heuristic was tuned on the dev corpus. A held-out set with
independent wording is the check on that overfit. Its labels come from the authoring model
applying the same rubric, so it shares the judge's model family: it measures robustness to
wording, not agreement with human experts (that still needs hand-labelled real sessions).

  python3 -m aiq_spike.heldout author --endpoint claude-cli:claude-sonnet-5-5 --out data/heldout_scenarios.json
The authored file is committed so every later run is reproducible without a model call.
"""
import argparse
import datetime as dt
import json
import random

from aiq_spike import llm

STEP = {"type": "object", "properties": {
    "kind": {"type": "string", "enum": ["user", "assistant", "tool", "gap", "command"]},
    "text": {"type": "string"},
    "tool": {"type": "string", "enum": ["Read", "Edit", "Write", "Bash", "Grep", "Task", ""]},
    "path": {"type": "string"}, "command": {"type": "string"}, "result": {"type": "string"},
    "minutes": {"type": "integer"}}, "required": ["kind"]}
SCHEMA = {"type": "object", "properties": {"scenarios": {"type": "array", "items": {"type": "object", "properties": {
    "id": {"type": "string"}, "job": {"type": "string", "enum": ["code_change", "write_doc", "other"]},
    "rung": {"type": "integer"}, "potential": {"type": "integer"}, "ambiguous": {"type": "string"},
    "note": {"type": "string"}, "steps": {"type": "array", "items": STEP}},
    "required": ["id", "job", "rung", "potential", "steps"]}}}, "required": ["scenarios"]}

BRIEF = """Write {n} realistic, varied synthetic tasks that one software person did with CLI coding agents
(Claude Code or GitHub Copilot CLI) in a fictional repo of your choice (NOT a web shop). Use a different
domain, tone and phrasing than you would by default: terse prompts, typos, long briefs, PM-style asks.

Each task is ONE unit of intent, 2-12 steps. Steps: kind=user (the person's prompt, in "text"),
kind=command (a typed slash command: "text" is e.g. "/gen-changelog weekly"), kind=assistant (agent reply),
kind=tool (tool: Read|Edit|Write|Bash|Grep|Task; path for file tools, command for Bash, text for Task's
sub-agent brief; result short), kind=gap (minutes idle). Follow-up prompts in the same task are common
("still broken", "shorter", "lgtm").

Label each with job: code_change (change/fix/refactor/test code), write_doc (spec, design doc, ADR, README,
runbook, release notes, docstrings), or other (Q&A, research, data analysis, status update, env
troubleshooting, reviewing someone else's PR). Label rung with this rubric:
1 Assisted: short ad hoc prompts; person does the work, AI fills pieces.
2 Directed: structured prompt (goal, context, constraints); output checked. Re-prompting without new context is 1.
3 Delegated: whole task handed to an agent; person reviews the result. Driving step by step is 2.
4 Orchestrated: several agents/steps with a written spec, guardrails and automated checks.
5 Factory: a reusable, versioned workflow others can trigger (e.g. a team's checked-in command), run with little change.
potential = the next reachable rung (current+1) or equal to current when the task already fits its rung.
Mix: about 40% code_change, 35% write_doc, 25% other; rungs mostly 1-3 with a few 4 and 5.
Put a one-line reason in "ambiguous" when a careful labeller could disagree, else "".
Ids: h01, h02, ..."""


def author(endpoint, n, out):
    res, meta = llm.endpoint(endpoint)("You write synthetic test data for an evaluation. Output only the JSON.",
                                       BRIEF.format(n=n), SCHEMA)
    if not res:
        raise SystemExit("authoring failed: {0}".format(meta.get("error")))
    with open(out, "w", encoding="utf-8") as fh:
        json.dump(res["scenarios"], fh, indent=1)
    print("wrote {0} scenarios to {1} ({2} ms)".format(len(res["scenarios"]), out, meta["latency_ms"]))


def to_library(path):
    """Authored JSON -> the scenario dicts generate.py renders."""
    lib = {}
    for s in json.load(open(path, encoding="utf-8")):
        steps = []
        for st in s["steps"]:
            k = st["kind"]
            if k == "user":
                steps.append(("user", st.get("text", "")))
            elif k == "command":
                name, _, args = st.get("text", "/run").partition(" ")
                steps.append(("command", name, args))
            elif k == "assistant":
                steps.append(("assistant", st.get("text", "")))
            elif k == "gap":
                steps.append(("gap", int(st.get("minutes") or 5)))
            elif k == "tool":
                tool = st.get("tool") or "Bash"
                if tool == "Task":
                    steps.append(("subagent", "sub-agent", st.get("text", ""), []))
                    continue
                args = {"command": st.get("command", "")} if tool == "Bash" else {"file_path": st.get("path", "")}
                if tool == "Grep":
                    args = {"pattern": st.get("text", ""), "path": st.get("path", ".")}
                steps.append(("tool", tool, args, st.get("result", "ok")))
        if not steps or steps[0][0] not in ("user", "command"):
            steps.insert(0, ("user", "continue"))  # a task must start with the person
        lib[s["id"]] = {"id": s["id"], "job": s["job"], "rung": s["rung"], "potential": s["potential"],
                        "ambiguous": s.get("ambiguous") or None, "note": s.get("note", ""), "steps": steps}
    return lib


def plan_for(lib, seed=11):
    """Pack tasks 1-3 per session with 1-20 minute gaps, alternating surfaces."""
    rng = random.Random(seed)
    ids = sorted(lib)
    rng.shuffle(ids)
    plan, day, i = [], dt.datetime(2026, 10, 26, 9, 0, tzinfo=dt.timezone.utc), 0
    while i < len(ids):
        k = rng.randint(1, 3)
        items = [(sid, "all", 0 if j == 0 else rng.randint(1, 20)) for j, sid in enumerate(ids[i:i + k])]
        plan.append(("claude" if len(plan) % 2 == 0 else "copilot", day, items))
        day += dt.timedelta(hours=rng.randint(2, 5))
        i += k
    return plan


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["author"])
    ap.add_argument("--endpoint", default="claude-cli:claude-sonnet-5-5")
    ap.add_argument("--n", type=int, default=20)
    ap.add_argument("--out", default="data/heldout_scenarios.json")
    a = ap.parse_args()
    author(a.endpoint, a.n, a.out)
