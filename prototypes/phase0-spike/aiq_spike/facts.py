"""Facts-first judge, after the rung-model spec (PR #3, docs/specs/rung-model.md, decision Q6).

The model answers narrow fact questions about a task. Log facts come from the collector events.
Deterministic per-job rules (the spec's illustrative rubric) turn facts into a rung. A rule that
rests on a fact below the confidence threshold counts as not met, so mixed evidence lands lower.

Fact ids follow the spec's alpha fact set:
  F1 agent made the edits (log)            F2 person's share of the final change (log; unknown in CLI)
  F3 who ran the checks (log)              F4 goal, context and constraints stated (model)
  F5 re-prompts added information (model)  F6 output reviewed or checked before use (log + model)
  F7 written spec / brief given (log + model)
  F8 two or more sub-agents with distinct roles, with an automated check (log)
  F9 saved workflow run, versioned, run by others (log + repo metadata)
  F12 person drove the agent step by step (model)

Repo metadata (F9 reuse counts) cannot come from a session. In the spike it is supplied per corpus
as `repo_meta` (simulated, by construction); in the product it comes from git history and CI.
"""
import re

from aiq_spike.judge import (CHECK_RX, SPEC_RX, TEST_RX, WORKFLOW_RX, excerpt, signals)

PERSON_RAN_RX = re.compile(r"\b(I ran|ran (them|it|the tests) (locally|myself)|I'?ll run)\b", re.I)


def log_facts(turns, repo_meta):
    sig = signals(turns)
    tools = [x for t in turns for x in t["tools"]]
    bash = [x.get("command", "") for x in tools if x["tool"] == "Bash"]
    agent_checks = sum(1 for b in bash if TEST_RX.search(b) or CHECK_RX.search(b) or "rerun" in b)
    person_ran = any(PERSON_RAN_RX.search(t["prompt"]) for t in turns)
    subs = [s for t in turns for s in t["subagents"]]
    read = [x.get("path", "") for x in tools if x["tool"] == "Read"]
    wf = next((p for p in read if WORKFLOW_RX.search(p)), None)
    meta = repo_meta.get(wf) if wf else None
    return {
        "F1_agent_made_edits": sig["files_edited_by_agent"] > 0 or len(subs) > 0,
        "F2_person_share": None,  # CLI-only, no git diff in the spike: unknown, as the spec says
        "F3_checks_run_by": "agent" if agent_checks else ("person" if person_ran else "nobody"),
        "F7_spec_file_given": any(SPEC_RX.search(p) for p in read) or any(SPEC_RX.search(p) for t in turns for p in t["mentioned"]),
        "F8_subagents": len(subs),
        "F9_workflow": {"command": sig["typed_workflow_command"], "definition": wf, "versioned": bool(wf),
                        "authors": (meta or {}).get("authors"), "runs_by_others_30d": (meta or {}).get("runs_by_others_30d")},
        "reprompts": sig["reprompts_without_new_context"],
    }


def _fact(desc, kind="boolean"):
    return {"type": "object", "properties": {"value": {"type": kind}, "confidence": {"type": "number"}},
            "required": ["value", "confidence"], "description": desc}


FACT_SCHEMA = {
    "type": "object",
    "properties": {
        "job": {"type": "string", "enum": ["code_change", "write_doc", "other"]},
        "job_confidence": {"type": "number"},
        "F2_person_share_pct": _fact("Estimated share (0-100) of the final change the person wrote themselves", "integer"),
        "F4_goal_context_constraints": _fact("The request stated a goal, context and constraints"),
        "F5_reprompts_added_information": _fact("Every re-prompt added new information (true if there were no re-prompts)"),
        "F6_output_checked": _fact("The output was reviewed or checked before use (diff read, tests run, sources checked, person says they checked)"),
        "F7_written_brief": _fact("A written spec, brief, template or instructions file was given to the agent"),
        "F12_step_by_step": _fact("The person drove the agent one small instruction per turn, steering each result"),
        "F13_whole_outcome_handed_over": _fact("A whole outcome was handed to the agent with done criteria, rather than one piece or step"),
        "evidence": {"type": "array", "items": {"type": "string"}},
    },
    "required": ["job", "job_confidence", "F2_person_share_pct", "F4_goal_context_constraints",
                 "F5_reprompts_added_information", "F6_output_checked", "F7_written_brief", "F12_step_by_step",
                 "F13_whole_outcome_handed_over", "evidence"],
}

FACT_SYSTEM = """You are AIQ's fact reader. You read ONE task a person did with AI agents and answer narrow questions
about HOW the work was done. Do not judge skill or assign a level. Answer each fact with a value and a
confidence (0-1, your probability the value is right). If the task does not show it, give your best
reading with low confidence.

JOB: code_change (change, fix, refactor, test or migrate code or config; incidental doc edits stay
code_change), write_doc (produce or edit a spec, design note, ADR, README, runbook, release notes, docstrings),
other (questions, research, data analysis, status updates, environment troubleshooting, reviewing others' work).

Facts:
F2 share (0-100) of the final change or document the person wrote themselves rather than the agent.
F4 the request stated a goal, context and constraints (not just a short ad hoc ask).
F5 every re-prompt added new information (true if there were no re-prompts).
F6 the output was reviewed or checked before use.
F7 a written spec, brief, template or instructions file was given to the agent.
F12 the person drove the agent step by step, one small instruction per turn.
F13 a whole outcome was handed to the agent with done criteria (what finished looks like, checks to
    pass), rather than one piece or one step at a time.
evidence: up to 3 short observations behind the facts."""


def model_facts(turns, call):
    from json import dumps
    prompt = "Task signals (extracted on device):\n" + dumps(signals(turns), indent=1) + "\n\nMinimised excerpt:\n" + excerpt(turns)
    return call(FACT_SYSTEM, prompt, FACT_SCHEMA)


def _ok(f, th):
    return bool(f and f["value"] is True and f["confidence"] >= th)


def place(log, mf, th=0.7, f2_source="model", factory_needs_orchestrated=True, min_subagents=2):
    """The spec's illustrative rubric. Returns (rung, uncertain, reasons)."""
    reasons, uncertain = [], False
    directed = _ok(mf["F4_goal_context_constraints"], th) and _ok(mf["F6_output_checked"], th)
    f5 = mf["F5_reprompts_added_information"]
    if log["reprompts"] >= 2 and f5["value"] is False and f5["confidence"] >= th:
        directed = False
        reasons.append("re-prompts added no information")
    share = log["F2_person_share"]
    if share is None and f2_source == "model":
        f = mf["F2_person_share_pct"]
        share = f["value"] if f["confidence"] >= th else None
    if share is None:
        uncertain = True
        reasons.append("person's share of the change unknown")
    step = mf["F12_step_by_step"]
    delegated = (log["F1_agent_made_edits"] and share is not None and share < 30
                 and _ok(mf["F6_output_checked"], th) and not (step["value"] is True and step["confidence"] >= th))
    orchestrated = (delegated and (log["F7_spec_file_given"] or _ok(mf["F7_written_brief"], th))
                    and log["F8_subagents"] >= min_subagents and log["F3_checks_run_by"] == "agent")
    w = log["F9_workflow"]
    reused = bool(w["versioned"] and (w["runs_by_others_30d"] or 0) > 0)
    factory = reused and (orchestrated if factory_needs_orchestrated else delegated)
    rung = 5 if factory else 4 if orchestrated else 3 if delegated else 2 if directed else 1
    return rung, uncertain, reasons


def rules_v2(log, mf, th=0.7):
    """Rung-model spec rubric after the spike's first facts-first run (PR #3 revision):
    Delegated = F1 + F13 + F6, ruled out by F12 or F2 >= 50% (F2 from git; unknown in CLI, so no
    rule-out); Orchestrated = Delegated + F7 + F8 (one sub-agent with an automated gate counts);
    Factory = F9 runs by others + checks run by the agent or automation, no Orchestrated prerequisite.
    Returns (rung, flags) where flags holds the clamps' inputs for the hybrid."""
    f5 = mf["F5_reprompts_added_information"]
    directed = (_ok(mf["F4_goal_context_constraints"], th) and _ok(mf["F6_output_checked"], th)
                and not (log["reprompts"] >= 2 and f5["value"] is False and f5["confidence"] >= th))
    step = mf["F12_step_by_step"]
    share = log["F2_person_share"]
    f13 = _ok(mf["F13_whole_outcome_handed_over"], th)
    delegated = (log["F1_agent_made_edits"] and f13 and _ok(mf["F6_output_checked"], th)
                 and not (step["value"] is True and step["confidence"] >= th)
                 and not (share is not None and share >= 50))
    f8 = log["F8_subagents"] >= 1 and log["F3_checks_run_by"] == "agent"
    orchestrated = delegated and (log["F7_spec_file_given"] or _ok(mf["F7_written_brief"], th)) and f8
    w = log["F9_workflow"]
    f9 = bool(w["versioned"] and (w["runs_by_others_30d"] or 0) > 0)
    factory = f9 and log["F3_checks_run_by"] == "agent"
    rung = 5 if factory else 4 if orchestrated else 3 if delegated else 2 if directed else 1
    return rung, {3: delegated, 4: orchestrated, 5: factory}


def guarded(holistic_rung, holds):
    """Hybrid chosen on PR #3 (Q8): the model places the rung, and guardrails only lower it, to the
    highest rung at or below the model's whose guardrail holds. Rungs 1-2 have no guardrail.
    `holds` is the second value of rules_v2: {3: Delegated holds, 4: Orchestrated holds, 5: Factory holds}."""
    r = holistic_rung
    while r > 2 and not holds[r]:
        r -= 1
    return r


def guarded_with_floor(holistic_rung, holds):
    """Variant: guardrails lower as above, and log-verifiable Orchestrated/Factory facts (F8, F9 with
    runs by others, checks by the agent) also act as a floor, raising an under-called task."""
    r = guarded(holistic_rung, holds)
    for top in (5, 4):
        if holds[top] and r < top:
            return top
    return r
