"""Stage 1 (local signal extraction) and Stage 2 (model judge with rubric) for one task.

Two input modes, so the spike can measure what privacy costs in accuracy:
  signals  structured signals only: counts, flags, file kinds. No prompt text, no paths, no output.
  excerpt  signals + a minimised excerpt: each prompt cut to 300 chars (max 8), file paths, and
           the commands the agent ran (first 120 chars).
"""
import collections
import re

from aiq_spike.group import words, jaccard

TEST_RX = re.compile(r"\b(pytest|npm (run )?test|go test|cargo test|jest|vitest|unittest)\b")
CHECK_RX = re.compile(r"\b(ruff|mypy|eslint|flake8|markdownlint|lychee|pip-audit|tsc|pyright|contract)\b")
VERIFY_SAY_RX = re.compile(r"\b(reviewed|checked|read it|ran them|diff looks|skimmed|looks right|agree)\b", re.I)
REPROMPT_RX = re.compile(r"^(still|again|more |shorter|try again|doesn'?t work|not working|just make it)", re.I)
STRUCT_RX = re.compile(r"\b(goal|context|constraint|acceptance|must|don'?t|do not|max|audience|cases?:|follow|"
                       r"repro|criteria|gate|stop and ask|report|flag)\b", re.I)
INSTR_RX = re.compile(r"(CLAUDE\.md|AGENTS\.md|copilot-instructions\.md)$")
WORKFLOW_RX = re.compile(r"(\.claude/commands/|\.github/prompts/|/skills/|SKILL\.md)")
SPEC_RX = re.compile(r"(^|/)(specs?|adr|design)/|spec", re.I)
CODE_EXT = {".py", ".ts", ".js", ".go", ".rs", ".java", ".cs", ".sql", ".tsx", ".jsx"}
DOC_EXT = {".md", ".rst", ".txt", ".adoc"}


def _ext(p):
    m = re.search(r"(\.[A-Za-z0-9]+)$", p)
    return m.group(1).lower() if m else ""


def signals(turns):
    prompts = [t["prompt"] for t in turns]
    tools = [x for t in turns for x in t["tools"]]
    c = collections.Counter(x["tool"] for x in tools)
    bash = [x.get("command", "") for x in tools if x["tool"] == "Bash"]
    edited = {x["path"] for x in tools if x["tool"] in ("Edit", "Write") and x.get("path")}
    read = {x["path"] for x in tools if x["tool"] == "Read" and x.get("path")}
    ext = collections.Counter(("code" if _ext(p) in CODE_EXT else "doc" if _ext(p) in DOC_EXT else "data/other")
                              for p in edited)
    reprompts = sum(1 for p in prompts[1:] if REPROMPT_RX.search(p))
    for a, b in zip(prompts, prompts[1:]):
        if jaccard(words(a), words(b)) > 0.6:
            reprompts += 1
    # prompts after the first that each order one small step (agent driven step by step)
    stepwise = sum(1 for t in turns[1:] if len(t["prompt"]) < 120 and t["tools"]
                   and any(x["tool"] in ("Edit", "Write", "Bash", "Read") for x in t["tools"])
                   and not REPROMPT_RX.search(t["prompt"]))
    first = prompts[0] if prompts else ""
    return {
        "prompts": len(prompts),
        "first_prompt_chars": len(first),
        "first_prompt_structure_markers": len(set(m.lower() for m in STRUCT_RX.findall(first))),
        "first_prompt_names_files": len(turns[0]["mentioned"]) if turns else 0,
        "first_prompt_has_pasted_code_or_text": bool(re.search(r"\n\s{4}|```|\n\n", first)),
        "reprompts_without_new_context": reprompts,
        "stepwise_follow_up_instructions": stepwise,
        "typed_workflow_command": next((t["command"] for t in turns if t["command"]), None),
        "workflow_definition_read": any(WORKFLOW_RX.search(p) for p in read),
        "instructions_file_read": any(INSTR_RX.search(p) for p in read),
        "spec_or_design_doc_referenced": any(SPEC_RX.search(p) for t in turns for p in t["mentioned"]) or any(SPEC_RX.search(p) for p in read),
        "agent_tool_calls": dict(c),
        "files_read_by_agent": len(read),
        "files_edited_by_agent": len(edited),
        "edited_file_kinds": dict(ext),
        "subagents": len([s for t in turns for s in t["subagents"]]),
        "tests_run_by_agent": sum(1 for b in bash if TEST_RX.search(b)),
        "checks_run_by_agent": sum(1 for b in bash if CHECK_RX.search(b)),
        "person_says_they_verified": any(VERIFY_SAY_RX.search(p) for p in prompts[1:]),
        "surfaces": sorted({t["surface"] for t in turns}),
        "sessions": len({t["session"] for t in turns}),
        "agent_text_only_replies": sum(1 for t in turns if not t["tools"]),
    }


def excerpt(turns):
    lines = []
    for i, t in enumerate(turns[:8], 1):
        lines.append("Prompt {0}: {1}".format(i, t["prompt"][:300].replace("\n", " / ")))
        if t["files"]:
            lines.append("  files: " + ", ".join(t["files"][:8]))
        for x in t["tools"]:
            if x["tool"] == "Bash":
                lines.append("  ran: " + x.get("command", "")[:120])
        for s in t["subagents"]:
            lines.append("  sub-agent: {0}: {1}".format(s["name"], s["prompt"][:120]))
    return "\n".join(lines)


JUDGE_SCHEMA = {
    "type": "object",
    "properties": {
        "job": {"type": "string", "enum": ["code_change", "write_doc", "other"]},
        "job_confidence": {"type": "number"},
        "current_rung": {"type": "integer", "minimum": 1, "maximum": 5},
        "current_confidence": {"type": "number"},
        "potential_rung": {"type": "integer", "minimum": 1, "maximum": 5},
        "potential_confidence": {"type": "number"},
        "evidence": {"type": "array", "items": {"type": "string"}},
        "gap": {"type": "string"},
        "blockers": {"type": "array", "items": {"type": "string"}},
    },
    "required": ["job", "job_confidence", "current_rung", "current_confidence", "potential_rung",
                 "potential_confidence", "evidence", "gap", "blockers"],
}

JUDGE_SYSTEM = """You are AIQ's rung judge. You read ONE task a person did with AI agents and return a structured judgment.

JOB (what the task is for, judged from the task, never the person's title):
- code_change: "Code a change" - change, fix, refactor, test or migrate code or config. Incidental doc edits
  (changelog, README line) inside a code change are still code_change.
- write_doc: "Write a spec or doc" - produce or edit a spec, design note, ADR, README, runbook, release notes,
  docstrings or other durable written document.
- other: any other job (questions and learning, research or comparisons, data analysis, status updates,
  troubleshooting the environment, reviewing someone else's change).
job_confidence: 0-1, your probability that the job label is right.

RUNG RUBRIC (place the task at the highest rung whose signals are clearly met; when signals are mixed, pick
the lower rung and say why):
1 Assisted: short, ad hoc prompts or pasted snippets; the person does the work and AI fills in pieces.
  Not 1 if: any structured context or delegation.
2 Directed: structured prompt with goal, context and constraints; output checked before use.
  Not 2 if: re-prompting without adding context; output used unchecked (then it is 1).
3 Delegated: a whole task handed to an agent; the person reviews the result rather than doing the steps.
  Not 3 if: the person makes most of the edits, or drives the agent step by step.
4 Orchestrated: several agents or steps coordinated with a written spec, guardrails, and automated checks or evals.
  Not 4 if: manual handoffs between steps; no automated verification.
5 Factory: a reusable, versioned workflow that runs again with little change, and others can trigger it.
  Not 5 if: one-off setups only this person runs.
Judge outcomes and verification, not activity volume.

POTENTIAL RUNG: the NEXT reachable rung (at most one above current), not the theoretical maximum. Consider
fit (well-specified, testable, repeatable work climbs; ambiguous, judgment-heavy or trivial work may not),
readiness (acceptance criteria, tests, context an agent can use), availability (tools visible in the task),
and repetition (rung 5 only for work that recurs for a team). It is legitimate for potential to equal
current when the task is already at its right rung.

evidence: up to 3 short plain-language signals behind the judgment. gap: one sentence on what would reach the
potential rung (empty if potential == current). blockers: things outside the person that cap potential."""


def judge_prompt(turns, mode):
    import json as _json
    sig = signals(turns)
    parts = ["Task signals (extracted on device):", _json.dumps(sig, indent=1)]
    if mode == "excerpt":
        parts += ["", "Minimised excerpt:", excerpt(turns)]
    return "\n".join(parts)


def judge(turns, mode, call):
    return call(JUDGE_SYSTEM, judge_prompt(turns, mode), JUDGE_SCHEMA)
