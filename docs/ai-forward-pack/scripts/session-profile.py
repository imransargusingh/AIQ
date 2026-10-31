#!/usr/bin/env python3
"""session-profile.py — measure how agent sessions actually ran, across harnesses and models.

Instrumentation over inference (IO1) pointed at the agent's own work: instead of reasoning
about why a session felt slow, expensive or drifty, READ the telemetry every harness already
writes to disk and turn it into a findings table (what happened, with evidence) and a fixes
table (which pack surface owns the control). This is the "asleep half" of continuous
improvement (`/dream`) specialised to performance, efficiency, task adherence, fan-out and
cross-harness coordination — the /session-profiler skill drives it.

Sources (all local, all read-only):

  GitHub Copilot CLI   ~/.copilot/session-store.db        sessions, turns, assistant_usage_events
                       ~/.copilot/session-state/<id>/events.jsonl   the full event stream
                       ~/.copilot/settings.json           model / contextTier / effortLevel
  Claude Code          ~/.claude/projects/<slug>/<session>.jsonl    the transcript
                       ~/.claude/projects/<slug>/<session>/subagents/agent-*.jsonl (+ .meta.json)
                                                          every sub-agent: model, requests, tokens,
                                                          tool calls and waits, context, span, resumes

Cost is reported in TOKENS and REQUESTS (the units a subscription-bound operator can act on),
`_meta.quota` where the harness carries it, and dollars ONLY as an "if API-billed" estimate at
first-party list rates that are printed beside every figure (LIST_RATES, with the date they
were cached) - never as a bill.

A repo is selected by path (`--repo <path>`, repeatable). Copilot sessions match on cwd or the
`owner/name` remote; Claude Code sessions match on the project slug of the repo path and of each
of its git worktrees. Every number is either read from the store or labelled as an estimate;
a measurement path that does not exist reports "not recorded", never a plausible number (IO8).

Subcommands
  discover   list the sessions found for the repo(s) in the window
  profile    per-turn metrics + findings + fixes for the selected sessions; writes
             <profiles>/<sp-id>/{profile.json,profile.md} for the FIRST --repo (or --out-root):
             docs/profiles/ in the pack source repo or with --publish, else <git-dir>/ai-forward/profiles/
  compare    aggregate the same metrics by model family x harness (the tuning view)
  fixes      print the fix catalog (finding id -> pack surface -> control)

Python 3.8+, stdlib only. Windows-safe (utf-8 stdout, read-only SQLite URI).
"""
import argparse
import collections
import datetime as _dt
import glob
import hashlib
import importlib.util
import json
import os
import re
import sqlite3
import statistics
import subprocess
import sys

for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass

NOT_RECORDED = "not recorded"
HERE = os.path.dirname(os.path.abspath(__file__))

# Calibration used ONLY for character->token estimates of static text (system prompts, files).
# Two measured points disagree (4.83 chars/token over knowledge docs on claude-opus-5;
# 3.54 over a full Copilot system prompt on claude-opus-4.8), so the figure is a parameter,
# printed with every estimate, never presented as a measurement (NG6).
CHARS_PER_TOKEN = 3.54

# First-party list rates, USD per MTok: (input, cache read, cache write 5m, output). Read from
# the claude-api reference (cached 2026-06-24), not from memory; cache read is 0.1x input and
# cache write 1.25x except on Fable 5.1 (0.025x). Used ONLY for the "if API-billed" column,
# printed with the cache date every time, and a model with no row reads "not recorded" rather
# than being priced as its neighbour (IO8).
LIST_RATES_CACHED = "2026-06-24"
LIST_RATES = collections.OrderedDict([
    ("claude-fable-5-1", (10.0, 0.25, 12.50, 50.0)),
    ("claude-opus-5", (5.0, 0.50, 6.25, 25.0)),
    ("claude-sonnet-5", (2.0, 0.20, 2.50, 10.0)),
])


def est_usd(model, uncached_in, cache_read, cache_write, output):
    """USD at list rates for one usage row, or None when the model has no rate row."""
    rate = LIST_RATES.get(str(model or ""))
    if not rate:
        return None
    return (uncached_in * rate[0] + cache_read * rate[1] + cache_write * rate[2] + output * rate[3]) / 1e6


def est_usd_rows(rows):
    """(usd, unpriced_requests) over usage rows that carry model/in/cr/cw/out."""
    usd, unpriced = 0.0, 0
    for r in rows:
        v = est_usd(r.get("model"), max((r.get("in") or 0) - (r.get("cr") or 0) - (r.get("cw") or 0), 0),
                    r.get("cr") or 0, r.get("cw") or 0, r.get("out") or 0)
        if v is None:
            unpriced += 1
        else:
            usd += v
    return round(usd, 2), unpriced

# --------------------------------------------------------------------------- catalogs
# The fix catalog: every finding maps to a pack surface that owns its control. Keep the ids
# stable - profiles reference them and /dream mines them.
FIXES = collections.OrderedDict([
    ("F-01", {"title": "CLAUDE.md is an `@AGENTS.md` import, not a copy",
              "where": "adapters/managed-blocks/CLAUDE.block.md; INSTALL.md 1.1; pack-doctor `claude-md import`",
              "control": "pack-doctor FAILs a repo whose CLAUDE.md carries the managed block beside an AGENTS.md that carries it too"}),
    ("F-02", {"title": "Measure the real static prefix, not the knowledge docs alone",
              "where": "scripts/context-budget.py prefix; context-budget.json",
              "control": "`context-budget.py prefix --gate` ratchets the whole prefix (blocks + always-on + tool/host allowance)"}),
    ("F-03", {"title": "Declare tier and fan-out cap in the goal state; record them in the audit entry",
              "where": "knowledge/communication-and-task-discipline.md CT19; scripts/audit-log.py --tier/--fan-out; /dream PACK-O miner",
              "control": "audit selfcheck + /dream flag a substantive turn with no tier, or a fan-out above the tier cap with no named hard gate"}),
    ("F-04", {"title": "Every delegation carries a tool-call budget and a convergence condition",
              "where": "knowledge/execution-graph-optimization.md GO7; agent cards; audit `agent_runs`",
              "control": "a sub-agent past its budget stops and reports; the audit entry records calls vs budget"}),
    ("F-05", {"title": "Persona cards are self-sufficient; no orientation reads",
              "where": "adapters/*/agents/*.md (inline operating standard + do-not-read list)",
              "control": "eval: a persona transcript contains no view of AGENTS.md / persona-* / agent-body-of-knowledge"}),
    ("F-06", {"title": "Progressive-disclosure skills; never re-invoke an active skill",
              "where": "commands/*/SKILL.md + reference/; context-budget.py skills (ratchet)",
              "control": "`context-budget.py skills --gate` fails unacknowledged SKILL.md growth; /dream flags a skill invoked twice in one turn"}),
    ("F-07", {"title": "Re-read guard hook",
              "where": "adapters/hooks/reread-guard.py (+ .github/hooks/ai-forward.json, .claude/settings.json)",
              "control": "the hook warns on the third identical view in a turn and on a paged tool output viewed whole"}),
    ("F-08", {"title": "UI craft docs load on demand with a rule index; screenshots stay out of the main context",
              "where": "knowledge/ui-*.md (load: skill + rule index); commands/ui-design",
              "control": "Tier B/C totals in context-budget; /ui-design Stage 3 reads the craft JSON"}),
    ("F-09", {"title": "Session hygiene: a new task starts a new session; tier and effort are per phase",
              "where": "knowledge/session-worktree-discipline.md WT1a; INSTALL.md (Copilot); pack-doctor `copilot settings`",
              "control": "pack-doctor WARNs on long_context + high effort as global defaults; this profiler flags context accretion"}),
    ("F-10", {"title": "Tune guidance per model family from measured drift, not priors",
              "where": "docs/profiles/ (this tool's compare view); knowledge/execution-graph-optimization.md GO19",
              "control": "`session-profile.py compare` - a family with 2x the drift indicators of another is a tuning finding"}),
    ("F-11", {"title": "Register the class, not the instance",
              "where": "docs/lessons/defect-classes.md (CTX-*); knowledge/continuous-improvement.md 6",
              "control": "each class row links to the control above; /dream re-surfaces an uncontrolled recurrence"}),
    ("F-12", {"title": "Ask each host for its richest reasoning summary, and treat summary-derived judgements as Inferred",
              "where": "INSTALL.md 1.6; adapters/hooks/claude-code.settings.hooks.json (showThinkingSummaries); pack-doctor `claude settings`",
              "control": "SP-17 reports visible-reasoning share per family; a family under 10% marks every text-derived drift finding Inferred"}),
    ("F-17", {"title": "A node declares whether it needs reasoning, or is deterministic mechanics",
              "where": "knowledge/execution-graph-optimization.md GO19 (per-node Capability); "
                       "commands/optimize-graph (the node table)",
              "control": "SP-22 flags a mechanical-close turn that ran at high effort and real "
                         "cost; the node table has to carry a Capability before a plan is admitted"}),
    ("F-16", {"title": "Resolve the model from usage events, never from the recorded setting",
              "where": "scripts/session-profile.py (effective_model / model_attribution); any pack "
                       "guidance keyed to a model",
              "control": "SP-21 flags a session whose recorded setting is not its effective model; "
                         "a test pins that family attribution is built from the per-request model"}),
    ("F-15", {"title": "`/also` establishes a bound rather than inheriting one that is absent",
              "where": "commands/also/SKILL.md + adapters/copilot/prompts/also.prompt.md",
              "control": "SP-20 flags an `/also` turn with no goal state, or a fan-out on one "
                         "that declared no tier; the `also` eval asserts both rules are written"}),
    ("F-14", {"title": "A budget on the MAIN line, not only on the delegates",
              "where": "knowledge/communication-and-task-discipline.md CT19 (`Main-line budget:`); "
                       "scripts/audit-log.py --main-budget; selfcheck",
              "control": "selfcheck reports a substantive turn with no main-line budget as a gap "
                         "and an over-run as a finding; SP-19 measures the real split from the store"}),
    ("F-13", {"title": "Externalize reasoning by construction: a one-line intent on every shell call",
              "where": "knowledge/communication-and-task-discipline.md CT26; the managed block; agent cards",
              "control": "SP-18 intent-trace coverage per family; below 90% is a finding; the hook and the profiler read the same field"}),
    ("F-18", {"title": "Read the sub-agent store: every node is a row, and SP-01/SP-07 run per node",
              "where": "scripts/session-profile.py profile_claude (subagents/agent-*.jsonl + .meta.json)",
              "control": "a fixture session with a subagents/ dir: the profile's sub_requests equals the store's; "
                         "SP-07 fires on a 60-tool-call node"}),
    ("F-19", {"title": "A task notification or local-command record is a continuation, not a human turn",
              "where": "scripts/session-profile.py (_human_turn); the family table's no_goal",
              "control": "a fixture notification record opens no turn and SP-09 stays silent on it"}),
    ("F-20", {"title": "A gate's exit status is never behind a pipe",
              "where": "adapters/managed-blocks/AGENTS.block.md (the shell rule beside CT26); "
                       "scripts/run-verify-gates.py; scripts/conductor-join.py",
              "control": "SP-24 counts gate runs piped into tail/head/grep with no pipefail, and the subset "
                         "that commit/merge/push on the same line"}),
    ("F-21", {"title": "A multi-line program is a file, then a run - never a heredoc",
              "where": "adapters/managed-blocks/AGENTS.block.md; commands/execute-with-coordination (the brief)",
              "control": "SP-25 counts failed heredoc runs (unexpected EOF, unterminated string, invalid escape)"}),
    ("F-24", {"title": "A resume message carries the `start` line",
              "where": "commands/execute-with-coordination (the resume template); scripts/audit-log.py start --skill",
              "control": "SP-26 flags a resumed node whose store span exceeds its first run by > 30 min"}),
    ("F-25", {"title": "A node never calls a deferred host tool that the session configuration refuses",
              "where": "commands/execute-with-coordination (the brief: absolute paths, no EnterWorktree/ExitWorktree)",
              "control": "SP-23 flags a sub-agent tool wait > 600 s on a non-shell tool"}),
    ("F-26", {"title": "A prose-input skill consumes the compiled prompt (the CO-S0 citation)",
              "where": "knowledge/agent-coordination.md CO-S0; commands/*/SKILL.md Grounding (the one CO-S0 sentence); "
                       "commands/compile; scripts/verify-skill-contracts.py",
              "control": "SP-27 flags a substantive turn that closed with `compiled: false` above T0; "
                         "verify-skill-contracts.py refuses a prose-input skill with no CO-S0 citation"}),
    ("F-27", {"title": "Revise the harness template version from the edit-distance evidence",
              "where": "adapters/prompt-templates/<harness>.md (versioned); scripts/prompt-compile.py distance; "
                       "the compile section of this profile",
              "control": "SP-28 flags a template version whose compiled prompts are edited past a 0.2 median "
                         "before the run; a new template version resets the distribution"}),
])

# Finding catalog: id -> (title, default severity, fix ids). Severity uses the pack scale.
FINDINGS = collections.OrderedDict([
    ("SP-01", ("Context accretion: the main conversation grew past the point where every step re-reads a book", "Major", ["F-09", "F-01"])),
    ("SP-02", ("Instruction double-load: two near-identical custom-instruction blocks in the static prefix", "Major", ["F-01"])),
    ("SP-03", ("Static prefix larger than the budget models", "Major", ["F-02"])),
    ("SP-04", ("Re-reads: the same file viewed three or more times in one turn, or a paged tool output viewed whole", "Minor", ["F-07"])),
    ("SP-05", ("Skill re-injection: the same skill invoked more than once in a session", "Minor", ["F-06"])),
    ("SP-06", ("Council above tier: a fan-out on a turn that declared no tier", "Major", ["F-03"])),
    ("SP-07", ("Sub-agent runaway: a delegation past a sane tool-call/token budget, or one the parent had to tell to converge", "Major", ["F-04"])),
    ("SP-08", ("Persona orientation reads: a sub-agent reading the roster docs or AGENTS.md to find out what it is", "Minor", ["F-05"])),
    ("SP-09", ("No goal state: a substantive turn whose first reply carries no Goal / Done when", "Major", ["F-03"])),
    ("SP-10", ("Tail latency: main-agent time-to-first-token p90 above 20 s", "Minor", ["F-09", "F-02"])),
    ("SP-11", ("Cap firings: harness completion nudges or user aborts inside a turn", "Minor", ["F-03"])),
    ("SP-12", ("Images and failed requests in the main context", "Minor", ["F-08"])),
    ("SP-13", ("Hook overhead above 5% of wall clock", "Nit", [])),
    ("SP-14", ("Model-family gap: one family carries 2x the cost or drift indicators of another on comparable turns", "Major", ["F-10"])),
    ("SP-15", ("Concurrent sessions in one checkout: overlapping sessions with the same cwd", "Major", ["F-09"])),
    ("SP-16", ("Knowledge at hand re-fetched: the main agent viewed an instruction file that is already in its prefix", "Minor", ["F-08", "F-02"])),
    ("SP-17", ("Reasoning visibility: the share of billed reasoning that came back as readable text", "Nit", ["F-12"])),
    ("SP-18", ("Intent-trace coverage: shell calls that carry a one-line description (the reasoning trace a profiler can read)", "Minor", ["F-13"])),
    ("SP-19", ("Main-line dominance: the turn's own loop, not its delegates, is where the cost is", "Major", ["F-14"])),
    ("SP-20", ("Late addition on an unbounded turn: an `/also` that inherited no goal state or fanned out above no tier", "Major", ["F-15"])),
    ("SP-21", ("Model attribution: the recorded setting is not the model that ran", "Major", ["F-16"])),
    ("SP-22", ("Mechanical work at reasoning prices: a closing turn billed as though it needed novelty", "Major", ["F-17"])),
    ("SP-23", ("Deferred-tool block-then-refuse: a sub-agent waited over 600 s on a non-shell tool", "Major", ["F-25"])),
    ("SP-24", ("Gate status behind a pipe: a verify/test run piped into tail/head/grep with no pipefail", "Major", ["F-20"])),
    ("SP-25", ("Failed heredoc runs: a multi-line program passed through the shell and burned the request", "Minor", ["F-21"])),
    ("SP-26", ("Unmarked resume: a node resumed after a long gap, so its second run set no start marker", "Minor", ["F-24"])),
    ("SP-27", ("Substantive turns started without a compiled prompt: a skill run above T0 closed with `compiled: false`", "Major", ["F-26"])),
    ("SP-28", ("Compiled prompts heavily edited before the run: edit_distance median above 0.2 for a template version", "Major", ["F-27"])),
])

INTENT_TOOLS = {"copilot": {"powershell", "bash", "shell"}, "claude": {"Bash", "PowerShell"}}

# SP-24: a gate run (verify-*, a test runner, the gate runner) whose output is piped into a
# formatter, with no pipefail on the line. Measured: 168 main-line lines in one programme,
# 102 of them also committing, merging or pushing on the same line; the register recorded 4.
# The gate must be INVOKED in the segment just before the formatter pipe - a `grep` whose
# argument merely names verify-x.py is not a gate run (that reading over-counted 7x).
GATE_INVOKE_RX = re.compile(
    r"^(?:\S+=\S*\s+)*"                                        # env assignments
    r"(?:(?:\S*python[\w.]*|py)\s+(?:-[XOu]\s*\S*\s+)*)?"     # an interpreter, optionally
    r"(?:\S*(?:verify-[\w-]+\.py|run-verify-gates\.py)\b"
    r"|(?:-m\s+)?pytest\b|dotnet\s+test\b|npm\s+(?:run\s+)?test\b|node\s+--test\b)")
FORMATTER_RX = re.compile(r"^\s*(tail|head|grep|Select-Object|Select-String|findstr|sort|uniq|wc|cut|awk|sed|tee)\b")
PIPE_SPLIT_RX = re.compile(r"(?<!\|)\|(?!\|)")
PIPEFAIL_RX = re.compile(r"pipefail|\$LASTEXITCODE|PIPESTATUS")
COMMIT_RX = re.compile(r"\bgit\s+(commit|merge|push)\b")
# SP-25: the shapes a broken heredoc leaves in the tool result.
HEREDOC_RX = re.compile(r"<<-?\s*['\"]?\w+['\"]?")
HEREDOC_FAIL_RX = re.compile(r"unexpected EOF|SyntaxError|unterminated (string|triple)|invalid escape|"
                             r"here-document|EOF while looking|IndentationError", re.I)
# SP-23: waits on these are shell work, expected to be long; anything else past the bar is a
# deferred host tool blocking until the main line surfaces it (measured: 8,143 s, refused).
SHELL_TOOLS = {"Bash", "PowerShell", "powershell", "bash", "shell", "Agent", "Task"}
DEFERRED_WAIT_S = 600
RESUME_GAP_S = 1800
# F-19: these user records are continuations of the turn in flight, never a human turn.
CONTINUATION_PREFIXES = ("<task-notification>", "<local-command-", "<command-name>", "<command-message>",
                         "This session is being continued from a previous conversation")
CONTINUATION_ORIGINS = {"task-notification", "local-command", "compact", "system"}


def _human_turn(record, content, text):
    """Is this user record a HUMAN turn? Verified from the record's own `origin.kind` where the
    harness writes one; otherwise the text shape. A wake-up carries the harness's words, not
    the operator's, and counting it as a turn inflated no_goal by 50 of 89 in one programme."""
    origin = (record.get("origin") or {}).get("kind")
    if origin == "human":
        return True
    if origin in CONTINUATION_ORIGINS or record.get("isMeta"):
        return False
    if not isinstance(content, str):
        return False
    head = text.lstrip()
    return not head.startswith(CONTINUATION_PREFIXES)


def gate_behind_pipe(cmd):
    """True when a gate INVOCATION is piped straight into a formatter with no pipefail on the
    line. The invocation is the last command of the segment before the pipe (after any
    `&&`, `;` or `(`)."""
    if PIPEFAIL_RX.search(cmd):
        return False
    segments = PIPE_SPLIT_RX.split(cmd)
    for seg, nxt in zip(segments, segments[1:]):
        last = re.split(r"&&|;|\(|\n", seg)[-1].strip()
        if GATE_INVOKE_RX.match(last) and FORMATTER_RX.match(nxt):
            return True
    return False


def shell_shape_counts(commands, results_by_id):
    """SP-24 / SP-25 over [(tool_use_id, command)] and {tool_use_id: result_text}. Returns
    (piped_gate_lines, also_commit, failed_heredocs, evidence_lines)."""
    piped, also, heredocs, ev = 0, 0, 0, []
    for tid, command in commands:
        cmd = str(command or "")
        if gate_behind_pipe(cmd):
            piped += 1
            if COMMIT_RX.search(cmd):
                also += 1
                ev.append("gate piped AND committed on one line: " + cmd.strip().replace("\n", " ")[:90])
            elif len(ev) < 12:
                ev.append("gate piped: " + cmd.strip().replace("\n", " ")[:90])
        if HEREDOC_RX.search(cmd) and HEREDOC_FAIL_RX.search(str(results_by_id.get(tid) or "")):
            heredocs += 1
            ev.append("heredoc failed: " + str(results_by_id.get(tid)).strip().replace("\n", " ")[:90])
    return piped, also, heredocs, ev

ORIENTATION_DOCS = ("agents.md", "claude.md", "agent-persona-catalog", "persona-cards", "persona-audit",
                    "agent-body-of-knowledge")
CONVERGE_RX = re.compile(r"\b(converge now|stop (further )?investigat|stop investigating|wrap up now)\b", re.I)
# CTX-J: the detector must see every spelling the standard prescribes. This required a colon
# for two revisions while CT19 and every worked example write `**Goal** —` or `**Goal** ·`,
# so it was blind to its own mandated form -- and it degraded AS COMPLIANCE IMPROVED, because
# every newly conformant turn was written in the shape it could not see. Measured 2026-09-06:
# 10 of 346 substantive Claude-Code turns detected, 15 present.
#
# The delimiter requirement stays: it is the guard that keeps prose ("the goal of this change
# is ... we are done when ...") from being credited as a declared goal state. Pinned by
# GoalStateSpellingTests, which is the control -- the pattern is only the fix.
GOAL_RX = re.compile(
    r"(?:\*\*\s*Goal\s*\*\*\s*[:\uff1a\u2014\u2013\u00b7\-]"      # **Goal** — / · / - / :
    r"|^\s*#{1,6}\s*Goal\b\s*[:\uff1a\u2014\u2013\u00b7\-]"        # ## Goal —
    r"|\bGoal\s*[:\uff1a])",                    # Goal: / **Goal:**
    re.I | re.M)
DONE_RX = re.compile(r"\bDone[\s-]*when\b", re.I)
TIER_RX = re.compile(r"\bTier\s*[:\uff1a]?\s*\**\s*T[0-3]\b", re.I)
NUDGE_RX = re.compile(r"not yet marked the task as complete|you have not finished|haven't finished|Keep working autonomously", re.I)
PAGED_OUTPUT_RX = re.compile(r"copilot-tool-output-[0-9a-f-]+\.txt$", re.I)
IMAGE_RX = re.compile(r"\.(png|jpe?g|gif|webp|bmp)$", re.I)
INSTRUCTION_RX = re.compile(r"[\\/](\.github[\\/]instructions[\\/][^\\/]+\.instructions\.md|AGENTS\.md|CLAUDE\.md)$", re.I)
SEVERITY_RANK = {"Blocker": 0, "Major": 1, "Minor": 2, "Nit": 3}


# --------------------------------------------------------------------------- helpers



ALSO_RX = re.compile(r"(^|\s)/also\b|<command-(?:message|name)>\s*also\s*</", re.I)


def late_addition_findings(turns):
    """`/also` turns that acquired no bound (F-15, class CTX-N).

    `/also` guards DIRECTION - "an extension is absorbed; a reversal is raised" - and had no
    guard on SIZE. Measured in sp-0004: both `/also` turns were the only substantive turns on
    that model carrying neither a goal state nor a tier, and one became 81 main requests, 6
    sub-agents, 83 minutes and 13,411 AIU - the most expensive turn in the session.

    Deliberately narrower than SP-09, which already owns the generic missing-goal-state case.
    This one is about the *late addition* specifically, because the mechanism is different: an
    addition to an unbounded turn INHERITS unboundedness rather than acquiring a bound, and
    the skill's own flow assumed a goal state was there to re-read.
    """
    rows = []
    for t in turns or []:
        if not ALSO_RX.search(t.get("prompt") or ""):
            continue
        subs = len(t.get("sub_agents") or [])
        if not t.get("goal_state"):
            rows.append({"turn": t.get("turn"),
                         "reason": "no goal state to inherit, so the addition acquired no bound"})
        elif subs and not t.get("tier"):
            rows.append({"turn": t.get("turn"),
                         "reason": "{0} sub-agent(s) on a turn that declared no tier".format(subs)})
    return rows





# The SHAPE of a mechanical close. Deliberately narrow and deliberately dumb about intent:
# it matches how the work was asked for, never whether it was right to ask.
MECHANICAL_RX = re.compile(
    r"^\s*(?:/updatepack|/document\b)"
    r"|\b(?:commit and push|push and merge|commit,\s*push|merge (?:the )?pr\b|"
    r"rebase(?:d)? then commit|regenerate the (?:index|docs)|"
    r"commit and (?:push/)?merge|push/merge)\b", re.I)

MECHANICAL_MIN_AIU = 500.0


def mechanical_cost_findings(turns, min_aiu=MECHANICAL_MIN_AIU):
    """Closing work billed as though it needed novelty (F-17).

    Measured: `/updatepack` ran twice in one session - 1,179 AIU on claude-opus-4.8 and
    8,890 on gpt-6-astra. Same skill, same repo, 7.5x. "yes commit and push/merge all" cost
    5,173 AIU across 21 requests. None of that is novel work; all of it is a script with a
    reviewer, and GO19's per-phase tier does not reach it because the phase boundary is
    inside the turn.

    The finding is the PRICE, not the mechanics - closing work is legitimate and has to
    happen. A cheap mechanical turn is exactly right and is not reported. A turn with no
    recorded cost is not guessed at (IO8).
    """
    rows = []
    for t in turns or []:
        if not MECHANICAL_RX.search(t.get("prompt") or ""):
            continue
        cost = t.get("cost_aiu")
        if cost is None or cost < min_aiu:
            continue
        if (t.get("effort") or "").lower() not in ("high", "medium"):
            continue
        rows.append({"turn": t.get("turn"), "cost_aiu": cost,
                     "effort": t.get("effort"), "requests": t.get("main_requests")})
    return rows


def _settings_note(facts):
    """The settings line, with the EFFECTIVE model beside it when they disagree (SP-21).

    A reader scanning the header takes `settings {'model': ...}` as what ran. On the measured
    session it was not, and the difference decided 95% of the cost.
    """
    if not facts.get("settings"):
        return ""
    note = " \u00b7 settings {0}".format(facts["settings"])
    ma = facts.get("model_attribution") or {}
    if ma.get("mismatch"):
        note += " \u00b7 EFFECTIVE model {0} ({1}% of main-line cost, {2} distinct)".format(
            ma["effective"], ma.get("share"), ma.get("distinct"))
    return note


def effective_model(models):
    """The model a session actually WAS, by cost. `models` is {model: {requests, cost}}.

    By cost rather than request count on purpose: in the measured session `gpt-6-astra` and
    the delegate models had comparable request counts and wildly different prices, and it is
    the expensive one that determines what the session cost and how it behaved.

    Returns None for a corpus it cannot read - an unknown model is not a guess (IO8).
    """
    if not models:
        return {"model": None, "share": None, "distinct": 0, "cost": 0.0}
    total = sum(v.get("cost", 0.0) for v in models.values())
    top = max(models.items(), key=lambda kv: kv[1].get("cost", 0.0))
    return {"model": top[0], "cost": round(top[1].get("cost", 0.0), 1),
            "distinct": len(models),
            "share": round(100.0 * top[1].get("cost", 0.0) / total, 1) if total else None}


def model_attribution(settings, models):
    """Reconcile the RECORDED model against the EFFECTIVE one (class CTX-O).

    The setting is a true statement about what was configured and is simply not a statement
    about what executed: measured, one session recorded `claude-opus-4.8` while `gpt-6-astra`
    ran 1,022 requests for 95% of the spend, across eleven model/effort combinations. Both
    values are plausible, which is why the error is invisible.

    Note what counts as a mismatch: the recorded model having RUN is not enough. In that
    session it ran - on 5% of the requests. Presence is not attribution.

    An absent setting is not a mismatch. Claude Code records no model setting, and absent
    must not read as wrong.
    """
    recorded = (settings or {}).get("model")
    eff = effective_model(models)
    return {"recorded": recorded, "effective": eff["model"], "share": eff["share"],
            "distinct": eff["distinct"],
            "mismatch": bool(recorded and eff["model"] and recorded != eff["model"])}


def main_line_share(buckets):
    """Split a session's requests and cost between the main line and its delegates.

    `buckets` is {initiator: {"requests": n, "cost": aiu}}. The main line is `agent`, `user`
    and `compaction` - a compaction request and the request that opens a user turn are both
    paid on the main conversation, and both were substantial: in sp-0003 the 24 bare
    user-initiated requests alone cost 12,853 AIU, MORE THAN THE ENTIRE DELEGATE FLEET.

    This is the measured half of CT19's `Main-line budget:`, which is only a declaration - an
    agent cannot count its own model requests, and this can. Reconciled, never conflated.

    Returns None for a share or a ratio it cannot establish: a percentage over an empty
    corpus is not a measurement (R4), and no delegates means there is no ratio to report
    rather than a ratio of infinity.
    """
    MAIN = ("agent", "user", "compaction")
    m_req = sum(v.get("requests", 0) for k, v in (buckets or {}).items() if k in MAIN)
    m_cost = sum(v.get("cost", 0.0) for k, v in (buckets or {}).items() if k in MAIN)
    s_req = sum(v.get("requests", 0) for k, v in (buckets or {}).items() if k not in MAIN)
    s_cost = sum(v.get("cost", 0.0) for k, v in (buckets or {}).items() if k not in MAIN)
    total = m_cost + s_cost
    per_main = (m_cost / m_req) if m_req else None
    per_sub = (s_cost / s_req) if s_req else None
    return {"main_requests": m_req, "sub_requests": s_req,
            "main_cost": round(m_cost, 1), "sub_cost": round(s_cost, 1),
            "main_pct": round(100.0 * m_cost / total, 1) if total else None,
            "cost_per_main_request": round(per_main, 1) if per_main is not None else None,
            "cost_per_sub_request": round(per_sub, 1) if per_sub is not None else None,
            "cost_ratio": round(per_main / per_sub, 1)
                          if (per_main is not None and per_sub) else None}


def _basename(path):
    """Last path segment, splitting on BOTH separators regardless of this platform.

    `os.path.basename` is per-platform, and the paths here are not: they come out of a
    harness store that was recorded on whatever machine ran the session. Profiling a
    Windows-captured Copilot store from Linux or WSL - which `--copilot-home` exists to
    allow - made `os.path.basename("C:\\repo\\AGENTS.md")` return the whole string, so
    every evidence line printed a full path and the orientation-read detector compared
    against a basename that was never going to match.

    Observed red in CI run 34061643244 (ubuntu-latest); green on Windows, which is exactly
    why it survived. Class PACK-C's sibling: a platform assumption that is invisible on the
    author's platform.
    """
    if not path:
        return path
    return re.split(r"[\\/]", str(path))[-1]


def parse_ts(s):
    if not s:
        return None
    try:
        s = s.replace("Z", "+00:00")
        d = _dt.datetime.fromisoformat(s)
        if d.tzinfo is None:
            d = d.replace(tzinfo=_dt.timezone.utc)
        return d
    except (ValueError, AttributeError):
        return None


def iso(d):
    return d.astimezone(_dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ") if d else None


def pct(values, q):
    if not values:
        return None
    vals = sorted(values)
    idx = min(len(vals) - 1, max(0, int(round(q * (len(vals) - 1)))))
    return vals[idx]


def est_tokens(chars):
    return int(round(chars / CHARS_PER_TOKEN))


def model_family(model):
    m = (model or "").lower()
    if m.startswith("claude"):
        return "anthropic"
    if m.startswith(("gpt", "o1", "o3", "o4")):
        return "openai"
    if m.startswith("gemini"):
        return "google"
    return "other" if m else "unknown"


def norm_path(p):
    return os.path.normcase(os.path.normpath(os.path.abspath(p))) if p else ""


def git(args, cwd):
    try:
        # text=True alone decodes with locale.getpreferredencoding() — cp1252 on a Windows
        # console — so a non-ASCII branch name or author line raises UnicodeDecodeError.
        # FR-105: git runs inside the profiled repository; its config may not start a program.
        out = subprocess.run(["git", "-c", "core.fsmonitor=false"] + args, cwd=cwd, capture_output=True, text=True,
                             encoding="utf-8", errors="replace", timeout=20)
        return out.stdout if out.returncode == 0 else ""
    except (OSError, subprocess.SubprocessError):
        return ""


def repo_identity(path):
    """Everything a session could have recorded to say 'I ran in this repo'."""
    path = os.path.abspath(path)
    roots = {norm_path(path)}
    top = git(["rev-parse", "--show-toplevel"], path).strip()
    if top:
        roots.add(norm_path(top))
    for line in git(["worktree", "list", "--porcelain"], path).splitlines():
        if line.startswith("worktree "):
            roots.add(norm_path(line[len("worktree "):].strip()))
    remote = git(["remote", "get-url", "origin"], path).strip()
    slug = ""
    m = re.search(r"[:/]([^/:]+)/([^/]+?)(?:\.git)?/?$", remote)
    if m:
        slug = "{0}/{1}".format(m.group(1), m.group(2)).lower()
    return {"path": path, "roots": roots, "slug": slug}


def repo_label(path):
    """A canonical, worktree-independent name for a repo (class PACK-P: a generated artifact
    must never stamp the worktree folder name).

    Delegates to `repo_identity.canonical_project` -- this was the third correct copy of one
    resolution ladder, and copies only diverge later (DM7/ONE-A).
    """
    _here = os.path.dirname(os.path.abspath(__file__))
    if _here not in sys.path:
        sys.path.insert(0, _here)
    from repo_identity import canonical_project
    return canonical_project(path)


def claude_slug(path):
    """Claude Code names a project directory by replacing every non-alphanumeric character in
    the absolute path with '-'. Observed: C:\\projects\\ai-forward -> C--projects-ai-forward."""
    return re.sub(r"[^A-Za-z0-9]", "-", os.path.abspath(path))


def in_repo(identity, cwd, repository=None):
    if repository and identity["slug"] and repository.lower() == identity["slug"]:
        return True
    if not cwd:
        return False
    c = norm_path(cwd)
    for r in identity["roots"]:
        if c == r or c.startswith(r + os.sep):
            return True
    return False


# --------------------------------------------------------------------------- Copilot CLI
def copilot_home():
    return os.environ.get("COPILOT_HOME") or os.path.join(os.path.expanduser("~"), ".copilot")


def copilot_settings(home):
    try:
        with open(os.path.join(home, "settings.json"), encoding="utf-8") as fh:
            return json.load(fh)
    except (OSError, ValueError):
        return {}


def _ro(db):
    return sqlite3.connect("file:{0}?mode=ro".format(db.replace("\\", "/")), uri=True)


def copilot_sessions(identity, since, home):
    db = os.path.join(home, "session-store.db")
    if not os.path.isfile(db):
        return []
    try:
        con = _ro(db)
        rows = con.execute("select id, cwd, repository, summary, created_at, updated_at from sessions").fetchall()
    except sqlite3.Error as exc:
        print("session-profile: cannot read {0}: {1}".format(db, exc), file=sys.stderr)
        return []
    out = []
    for sid, cwd, repository, summary, created, updated in rows:
        if not in_repo(identity, cwd, repository):
            continue
        c = parse_ts(created)
        u = parse_ts(updated)
        if since and (u or c) and (u or c) < since:
            continue
        out.append({"harness": "copilot", "id": sid, "cwd": cwd, "repository": repository,
                    "title": summary or "", "started": iso(c), "updated": iso(u),
                    "events": os.path.join(home, "session-state", sid, "events.jsonl"), "db": db})
    con.close()
    return out


def _load_events(path):
    events = []
    if not os.path.isfile(path):
        return events
    with open(path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            try:
                events.append(json.loads(line))
            except ValueError:
                continue
    return events


def _args_of(x):
    a = x.get("arguments")
    if isinstance(a, str):
        try:
            a = json.loads(a)
        except ValueError:
            a = {"_raw": a}
    return a or {}


def _new_turn():
    return {"prompt": "", "started": None, "delivery": "", "tools": collections.Counter(), "view_bytes": 0,
            "views": collections.Counter(), "paged_full_views": 0, "image_views": 0, "instruction_views": [],
            "skills": collections.Counter(), "asst_msgs": 0, "asst_text_chars": 0, "goal_state": False, "tier": False,
            "first_asst_seen": False, "nudges": 0, "aborts": 0, "errors": 0, "hook_s": 0.0, "sub": {},
            "converge_nudges": 0, "sub_orientation_reads": [], "sub_tools": 0,
            "reasoning_chars": 0, "intent_eligible": 0, "intent_with": 0}


def profile_copilot(sess, settings):
    """One Copilot session -> normalized turns + session-level facts."""
    con = _ro(sess["db"])
    usage = con.execute(
        "select turn_index, agent_id, model, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, "
        "reasoning_tokens, total_nano_aiu, duration_ms, time_to_first_token_ms, initiator, finish_reason, created_at, "
        "reasoning_effort from assistant_usage_events where session_id=? order by id", (sess["id"],)).fetchall()
    con.close()
    events = _load_events(sess["events"])
    byid = {e.get("id"): e for e in events}

    # Turn boundaries = main-conversation user messages. The log is linear, so the rule is
    # positional (verified on the captured stream): the first user.message after a
    # subagent.started is that sub-agent's prompt; a 'queued' message is a parent->sub-agent
    # follow-up (write_agent); a message with no delivery value is never a human turn.
    # Attribution (verified on the captured stream): every event that belongs to a sub-agent
    # carries a top-level `agentId` (its user.message, system.message, assistant.*, tool.*),
    # main-conversation events carry none. A human turn is a main user.message delivered
    # 'idle' or 'steering'; anything else on the main line ('queued', no delivery) is a
    # parent->sub-agent message.
    sub_names = {}
    for e in events:
        if e.get("type") == "subagent.started":
            d = e.get("data") or {}
            sub_names[e.get("agentId")] = d.get("agentDisplayName") or d.get("agentName") or "sub-agent"
    mains = [e for e in events if e.get("type") == "user.message" and not e.get("agentId")
             and (e.get("data") or {}).get("delivery") in ("idle", "steering")]
    main_iids = {(e.get("data") or {}).get("interactionId") for e in mains}
    bounds = [parse_ts(e.get("timestamp")) for e in mains]

    def window(t):
        idx = None
        for i, b in enumerate(bounds):
            if b and t and t >= b:
                idx = i
        return idx

    # system prompt facts (the first big system.message is the main agent's prefix)
    prefix = {"chars": None, "first_chars": None, "blocks": [], "note": NOT_RECORDED}
    for e in events:
        if e.get("type") != "system.message" or e.get("agentId"):
            continue
        c = (e.get("data") or {}).get("content") or ""
        if len(c) < 100000:
            continue
        if prefix["first_chars"] is None:
            prefix["first_chars"] = len(c)
        prefix["chars"] = len(c)  # the most recent main-conversation prefix wins: it is what the next request pays
        sizes = []
        for m in re.finditer(r"<custom_instruction>", c):
            end = c.find("</custom_instruction>", m.start())
            sizes.append((end - m.start()) if end > 0 else 0)
        prefix["blocks"] = sizes
        prefix["note"] = "measured chars of the latest main prefix; tokens are an estimate at {0} chars/token".format(CHARS_PER_TOKEN)

    T = collections.defaultdict(_new_turn)
    for i, e in enumerate(mains):
        d = e.get("data") or {}
        c = d.get("content") or ""
        t = T[i]
        t["prompt"] = c[:160].replace("\n", " ")
        t["started"] = iso(parse_ts(e.get("timestamp")))
        t["delivery"] = d.get("delivery") or ""
        tc = d.get("transformedContent")
        tc = tc if isinstance(tc, str) else ""
        if c == "" and NUDGE_RX.search(tc):
            t["nudges"] += 1
            t["prompt"] = "(harness completion nudge)"
    starts, hooks, sub_names = {}, {}, {}
    for e in events:
        et = e.get("type")
        d = e.get("data") or {}
        ts = parse_ts(e.get("timestamp"))
        if et == "subagent.started":
            sub_names[e.get("agentId")] = d.get("agentDisplayName") or d.get("agentName") or "sub-agent"
        w = window(ts)
        if w is None:
            continue
        t = T[w]
        if et == "tool.execution_start":
            starts[d.get("toolCallId")] = (ts, d, e)
        elif et == "tool.execution_complete":
            s = starts.get(d.get("toolCallId"))
            if not s:
                continue
            sd, se = s[1], s[2]
            a = _args_of(sd)
            tn = sd.get("toolName") or "?"
            res = d.get("result")
            rs = len(json.dumps(res)) if res is not None else 0
            path = str(a.get("path") or a.get("file_path") or "")
            if not e.get("agentId") and d.get("interactionId") in main_iids:
                t["tools"][tn] += 1
                if tn == "view":
                    t["view_bytes"] += rs
                    if path:
                        t["views"][path] += 1
                        if PAGED_OUTPUT_RX.search(path) and not a.get("view_range"):
                            t["paged_full_views"] += 1
                        if IMAGE_RX.search(path):
                            t["image_views"] += 1
                        if INSTRUCTION_RX.search(path):
                            t["instruction_views"].append(path)
                if tn == "skill":
                    t["skills"][str(a.get("skill") or a.get("name") or "?")] += 1
                if tn == "write_agent":
                    msg = str(a.get("message") or a.get("prompt") or a.get("content") or "")
                    if CONVERGE_RX.search(msg):
                        t["converge_nudges"] += 1
                if tn in INTENT_TOOLS["copilot"]:
                    t["intent_eligible"] += 1
                    if str(a.get("description") or "").strip():
                        t["intent_with"] += 1
            else:
                t["sub_tools"] += 1
                name = sub_names.get(e.get("agentId") or se.get("agentId"), "sub-agent")
                if tn == "view" and path and any(k in path.lower() for k in ORIENTATION_DOCS):
                    t["sub_orientation_reads"].append("{0}: {1}".format(name or "sub-agent", _basename(path)))
        elif et == "assistant.message" and not e.get("agentId") and d.get("interactionId") in main_iids:
            t["asst_msgs"] += 1
            c = d.get("content") or ""
            t["asst_text_chars"] += len(c)
            t["reasoning_chars"] += len(d.get("reasoningText") or "")
            if not t["first_asst_seen"] and c.strip():
                t["first_asst_seen"] = True
                t["goal_state"] = bool(GOAL_RX.search(c) and DONE_RX.search(c))
                t["tier"] = bool(TIER_RX.search(c))
        elif et == "subagent.completed":
            t["sub"][e.get("agentId")] = {
                "name": d.get("agentDisplayName") or d.get("agentName"), "model": d.get("model"),
                "tokens": d.get("totalTokens"), "tool_calls": d.get("totalToolCalls"),
                "duration_s": round((d.get("durationMs") or 0) / 1000.0)}
        elif et == "abort":
            t["aborts"] += 1
        elif et == "session.error":
            t["errors"] += 1
        elif et == "hook.start":
            hooks[d.get("hookInvocationId")] = ts
        elif et == "hook.end":
            s = hooks.get(d.get("hookInvocationId"))
            if s and ts:
                t["hook_s"] += (ts - s).total_seconds()

    # token metrics per turn from the store: align usage rows to event windows by time
    U = collections.defaultdict(list)
    for row in usage:
        (turn_index, agent_id, model, inp, out, cr, cw, rsn, aiu, dur, ttft, initiator, finish, created, effort) = row
        w = window(parse_ts(created))
        if w is None:
            continue
        U[w].append({"main": agent_id is None, "initiator": initiator, "model": model,
                     "in": inp or 0, "out": out or 0,
                     "cr": cr or 0, "cw": cw or 0, "rsn": rsn or 0, "aiu": (aiu or 0) / 1e9,
                     "dur": (dur or 0) / 1000.0, "ttft": (ttft / 1000.0) if ttft else None,
                     "created": parse_ts(created), "finish": finish, "effort": effort})
    # F-14 / SP-19: the main-line vs delegate split, by initiator. Claude Code's transcript
    # carries no initiator, so for that reader this stays absent rather than fabricated (IO8).
    initiators = collections.defaultdict(lambda: {"requests": 0, "cost": 0.0})
    # F-16 / SP-21: per-model totals on the MAIN line, so the session's effective model is
    # resolved from what executed rather than from what was configured (class CTX-O).
    main_models = collections.defaultdict(lambda: {"requests": 0, "cost": 0.0})
    for w_rows in U.values():
        for r in w_rows:
            b = initiators[r.get("initiator") or ("agent" if r["main"] else "sub-agent")]
            b["requests"] += 1
            b["cost"] += r["aiu"]
            if r["main"] and r.get("model"):
                m = main_models[r["model"]]
                m["requests"] += 1
                m["cost"] += r["aiu"]

    turns = []
    for i in sorted(T):
        t = T[i]
        rows = U.get(i, [])
        main = [r for r in rows if r["main"]]
        subs = [r for r in rows if not r["main"]]
        ttfts = [r["ttft"] for r in main if r["ttft"]]
        models = sorted({r["model"] for r in rows if r["model"]})
        wall = None
        cs = [r["created"] for r in rows if r["created"]]
        if cs:
            wall = round((max(cs) - min(cs)).total_seconds() + rows[-1]["dur"])
        rereads = {p: n for p, n in t["views"].items() if n >= 3}
        turns.append({
            "turn": i, "prompt": t["prompt"], "started": t["started"], "delivery": t["delivery"],
            "models": models, "families": sorted({model_family(m) for m in models}),
            "wall_s": wall, "main_requests": len(main), "sub_requests": len(subs),
            "ctx_start": main[0]["in"] if main else None, "ctx_end": main[-1]["in"] if main else None,
            "ctx_max": max([r["in"] for r in main]) if main else None,
            "cache_read": sum(r["cr"] for r in rows), "uncached_in": sum(max(r["in"] - r["cr"] - r["cw"], 0) for r in rows),
            "cache_write": sum(r["cw"] for r in rows), "output": sum(r["out"] for r in rows),
            "reasoning": sum(r["rsn"] for r in rows), "cost_aiu": round(sum(r["aiu"] for r in rows), 1),
            "main_api_s": round(sum(r["dur"] for r in main)), "all_api_s": round(sum(r["dur"] for r in rows)),
            "ttft_p50": round(pct(ttfts, 0.5), 1) if ttfts else None,
            "ttft_p90": round(pct(ttfts, 0.9), 1) if ttfts else None,
            "ttft_max": round(max(ttfts), 1) if ttfts else None,
            "tools": dict(t["tools"]), "view_bytes": t["view_bytes"], "rereads": rereads,
            "paged_full_views": t["paged_full_views"], "image_views": t["image_views"],
            "instruction_views": t["instruction_views"], "skills": {k: v for k, v in t["skills"].items() if v},
            "asst_msgs": t["asst_msgs"], "asst_text_chars": t["asst_text_chars"],
            "goal_state": t["goal_state"], "tier": t["tier"], "nudges": t["nudges"], "aborts": t["aborts"],
            "errors": t["errors"], "hook_s": round(t["hook_s"]),
            "sub_agents": list(t["sub"].values()), "sub_tool_calls": t["sub_tools"],
            "converge_nudges": t["converge_nudges"], "sub_orientation_reads": t["sub_orientation_reads"],
            "reasoning_main": sum(r["rsn"] for r in main), "reasoning_chars": t["reasoning_chars"],
            "intent_eligible": t["intent_eligible"], "intent_with": t["intent_with"],
            "effort": collections.Counter(r["effort"] for r in main if r["effort"]).most_common(1)[0][0] if any(r["effort"] for r in main) else None,
        })
    facts = {"harness": "copilot", "id": sess["id"], "title": sess["title"], "cwd": sess["cwd"],
             "started": sess["started"], "updated": sess["updated"],
             "settings": {k: settings.get(k) for k in ("model", "contextTier", "effortLevel")},
             "prefix_chars": prefix["chars"], "prefix_tokens_est": est_tokens(prefix["chars"]) if prefix["chars"] else None,
             "prefix_first_chars": prefix["first_chars"], "prefix_blocks": prefix["blocks"], "prefix_note": prefix["note"],
             "main_line": main_line_share(dict(initiators)),
             "main_models": {k: {"requests": v["requests"], "cost": round(v["cost"], 1)}
                             for k, v in main_models.items()},
             "model_attribution": model_attribution(
                 {k: settings.get(k) for k in ("model",)}, dict(main_models)),
             "compactions": sum(1 for e in events if str(e.get("type", "")).startswith("session.compact")),
             "events": len(events), "usage_rows": len(usage)}
    return facts, turns


# --------------------------------------------------------------------------- Claude Code
def claude_home():
    return os.environ.get("CLAUDE_CONFIG_DIR") or os.path.join(os.path.expanduser("~"), ".claude")


def claude_sessions(identity, since, home):
    projects = os.path.join(home, "projects")
    if not os.path.isdir(projects):
        return []
    wanted = {claude_slug(r).lower() for r in identity["roots"]} | {claude_slug(identity["path"]).lower()}
    out = []
    for name in sorted(os.listdir(projects)):
        if name.lower() not in wanted:
            continue
        for path in sorted(glob.glob(os.path.join(projects, name, "*.jsonl"))):
            sid = os.path.splitext(_basename(path))[0]
            mtime = _dt.datetime.fromtimestamp(os.path.getmtime(path), _dt.timezone.utc)
            if since and mtime < since:
                continue
            title, cwd = "", None
            try:
                with open(path, encoding="utf-8", errors="replace") as fh:
                    for k, line in enumerate(fh):
                        if k > 400:
                            break
                        if '"ai-title"' in line or ('"cwd"' in line and not cwd):
                            try:
                                r = json.loads(line)
                            except ValueError:
                                continue
                            title = r.get("aiTitle") or title
                            cwd = cwd or r.get("cwd")
            except OSError:
                pass
            out.append({"harness": "claude", "id": sid, "path": path, "project": name,
                        "updated": iso(mtime), "started": None, "title": title, "cwd": cwd})
    return out


def _text_of(content):
    if isinstance(content, str):
        return content
    parts = []
    for b in content or []:
        if isinstance(b, dict) and b.get("type") == "text":
            parts.append(b.get("text") or "")
    return "\n".join(parts)


def _tool_result_texts(content):
    out = {}
    if isinstance(content, list):
        for b in content:
            if isinstance(b, dict) and b.get("type") == "tool_result":
                c = b.get("content")
                if isinstance(c, list):
                    c = "\n".join(str(x.get("text") or "") for x in c if isinstance(x, dict))
                out[b.get("tool_use_id")] = str(c or "")
    return out


def read_subagents(session_dir):
    """Every `subagents/agent-*.jsonl` (+ `.meta.json`) under the session's directory, one
    node row each. This is where 86% of a measured programme's tokens lived while the main
    transcript's `isSidechain` records held none of them (F-18). Returns (nodes, store_note)."""
    store = os.path.join(session_dir, "subagents")
    if not os.path.isdir(store):
        return [], NOT_RECORDED
    nodes = []
    for path in sorted(glob.glob(os.path.join(store, "agent-*.jsonl"))):
        aid = _basename(path)[len("agent-"):-len(".jsonl")]
        meta = {}
        try:
            with open(path[:-len(".jsonl")] + ".meta.json", encoding="utf-8") as fh:
                meta = json.load(fh) or {}
        except (OSError, ValueError):
            pass
        recs = _load_events(path)
        seen, rows, models = set(), [], collections.Counter()
        seen_tools = set()        # a message's blocks are streamed as separate records; a tool_use id counts once
        tool_calls, pending, waits = 0, {}, []
        prompts, first, last, commands, results = [], None, None, [], {}
        reviews = 0
        for r in recs:
            ts = parse_ts(r.get("timestamp"))
            if ts:
                first = first or ts
                last = ts
            rt = r.get("type")
            msg = r.get("message") or {}
            if rt == "user":
                content = msg.get("content")
                if isinstance(content, str):
                    prompts.append(ts)
                for tid, text in _tool_result_texts(content).items():
                    results[tid] = text
                    started, name = pending.pop(tid, (None, None))
                    if started and ts:
                        waits.append((name, round((ts - started).total_seconds()), iso(started)))
                continue
            if rt != "assistant":
                continue
            mid, usage, model = msg.get("id"), msg.get("usage") or {}, msg.get("model")
            if mid and mid not in seen and usage:
                seen.add(mid)
                if model:
                    models[model] += 1
                rows.append({"model": model,
                             "in": (usage.get("input_tokens") or 0) + (usage.get("cache_read_input_tokens") or 0)
                                   + (usage.get("cache_creation_input_tokens") or 0),
                             "cr": usage.get("cache_read_input_tokens") or 0,
                             "cw": usage.get("cache_creation_input_tokens") or 0,
                             "out": usage.get("output_tokens") or 0})
            for b in msg.get("content") or []:
                if isinstance(b, dict) and b.get("type") == "tool_use":
                    if b.get("id") in seen_tools:
                        continue
                    if b.get("id"):
                        seen_tools.add(b["id"])
                    tool_calls += 1
                    name = b.get("name") or "?"
                    if name in ("Agent", "Task"):
                        reviews += 1
                    if b.get("id") and ts:
                        pending[b["id"]] = (ts, name)
                    if name in INTENT_TOOLS["claude"]:
                        commands.append((b.get("id"), (b.get("input") or {}).get("command")))
        gaps = [round((b - a).total_seconds()) for a, b in zip(prompts, prompts[1:]) if a and b]
        long_waits = sorted([w for w in waits if w[0] not in SHELL_TOOLS and w[1] > DEFERRED_WAIT_S],
                            key=lambda w: -w[1])
        nodes.append({
            "id": aid, "name": meta.get("description") or meta.get("agentType") or aid,
            "type": meta.get("agentType"), "depth": int(meta.get("spawnDepth") or 1),
            "parent": meta.get("parentAgentId"), "tool_use_id": meta.get("toolUseId"),
            "model": (models.most_common(1)[0][0] if models else None), "requests": len(rows),
            "tool_calls": tool_calls, "reviews": reviews,
            "cache_read": sum(r["cr"] for r in rows), "cache_write": sum(r["cw"] for r in rows),
            "uncached_in": sum(max(r["in"] - r["cr"] - r["cw"], 0) for r in rows),
            "output": sum(r["out"] for r in rows), "tokens": sum(r["in"] + r["out"] for r in rows),
            "ctx_max": max([r["in"] for r in rows]) if rows else None,
            "started": iso(first) if first else None, "ended": iso(last) if last else None,
            "span_s": round((last - first).total_seconds()) if (first and last) else None,
            "resumes": max(len(prompts) - 1, 0), "resume_gaps_s": gaps,
            "long_waits": long_waits[:4], "shell_shapes": shell_shape_counts(commands, results),
            "est_usd": est_usd_rows(rows)[0] if rows else None,
            "_rows": rows,
        })
    return nodes, "{0} agent file(s)".format(len(nodes))


def profile_claude(sess):
    recs = _load_events(sess["path"])
    quota = NOT_RECORDED
    for r in recs:
        m = (r.get("message") or {}).get("_meta") or r.get("_meta") or {}
        if isinstance(m, dict) and m.get("quota") is not None:
            quota = m.get("quota")
    title, cwd, cost_state = "", None, None
    for r in recs:
        if r.get("type") == "ai-title":
            title = r.get("aiTitle") or title
        if r.get("type") == "cost-state":
            cost_state = r
        if not cwd and r.get("cwd"):
            cwd = r.get("cwd")
    turn_idx, T = -1, []
    seen_msg = set()
    agent_calls = {}          # tool_use id of an Agent/Task call -> turn index (F-18 attribution)
    for r in recs:
        rt = r.get("type")
        if rt not in ("user", "assistant"):
            continue
        msg = r.get("message") or {}
        side = bool(r.get("isSidechain"))
        ts = parse_ts(r.get("timestamp"))
        if rt == "user" and not side:
            content = msg.get("content")
            has_tool_result = isinstance(content, list) and any(
                isinstance(b, dict) and b.get("type") == "tool_result" for b in content)
            text = _text_of(content)
            if has_tool_result and turn_idx >= 0:
                T[turn_idx]["results"].update(_tool_result_texts(content))
            is_human = _human_turn(r, content, text)
            if is_human and not has_tool_result:
                turn_idx += 1
                T.append({"prompt": text[:160].replace("\n", " "), "started": iso(ts), "requests": [],
                          "sub_requests": [], "tools": collections.Counter(), "views": collections.Counter(),
                          "image_views": 0, "instruction_views": [], "skills": collections.Counter(),
                          "asst_text_chars": 0, "asst_msgs": 0, "goal_state": False, "tier": False,
                          "first_asst_seen": False, "nudges": 0, "sub_agents": collections.OrderedDict(),
                          "sub_tools": 0, "sub_orientation_reads": [], "converge_nudges": 0, "models": set(),
                          "ended": None, "reasoning_chars": 0, "intent_eligible": 0, "intent_with": 0,
                          "commands": [], "results": {}})
                m = re.search(r"<command-name>/([\w-]+)</command-name>", text)
                if m:
                    T[-1]["skills"][m.group(1)] += 1
            elif turn_idx >= 0 and NUDGE_RX.search(text):
                T[turn_idx]["nudges"] += 1
            continue
        if turn_idx < 0:
            continue
        t = T[turn_idx]
        if rt == "assistant":
            mid = msg.get("id")
            usage = msg.get("usage") or {}
            model = msg.get("model")
            if model:
                t["models"].add(model)
            if mid and mid not in seen_msg and usage:
                seen_msg.add(mid)
                row = {"main": not side, "model": model,
                       "in": (usage.get("input_tokens") or 0) + (usage.get("cache_read_input_tokens") or 0)
                             + (usage.get("cache_creation_input_tokens") or 0),
                       "cr": usage.get("cache_read_input_tokens") or 0,
                       "cw": usage.get("cache_creation_input_tokens") or 0,
                       "out": usage.get("output_tokens") or 0,
                       "rsn": ((usage.get("output_tokens_details") or {}).get("thinking_tokens") or 0),
                       "created": ts}
                (t["requests"] if not side else t["sub_requests"]).append(row)
                t["ended"] = ts
                if side:
                    aid = r.get("agentId") or r.get("sessionId")
                    t["sub_agents"].setdefault(aid, {"name": "sub-agent", "tool_calls": 0, "tokens": 0})
                    t["sub_agents"][aid]["tokens"] += row["in"] + row["out"]
            for b in msg.get("content") or []:
                if not isinstance(b, dict):
                    continue
                if b.get("type") == "thinking" and not side:
                    t["reasoning_chars"] += len(b.get("thinking") or "")
                if b.get("type") == "text" and not side:
                    c = b.get("text") or ""
                    t["asst_text_chars"] += len(c)
                    t["asst_msgs"] += 1
                    if not t["first_asst_seen"] and c.strip():
                        t["first_asst_seen"] = True
                        t["goal_state"] = bool(GOAL_RX.search(c) and DONE_RX.search(c))
                        t["tier"] = bool(TIER_RX.search(c))
                elif b.get("type") == "tool_use":
                    name = b.get("name") or "?"
                    inp = b.get("input") or {}
                    path = str(inp.get("file_path") or inp.get("path") or "")
                    if side:
                        t["sub_tools"] += 1
                        aid = r.get("agentId") or r.get("sessionId")
                        t["sub_agents"].setdefault(aid, {"name": "sub-agent", "tool_calls": 0, "tokens": 0})
                        t["sub_agents"][aid]["tool_calls"] += 1
                        if name == "Read" and path and any(k in path.lower() for k in ORIENTATION_DOCS):
                            t["sub_orientation_reads"].append(_basename(path))
                        continue
                    t["tools"][name] += 1
                    if name in INTENT_TOOLS["claude"]:
                        t["intent_eligible"] += 1
                        t["commands"].append((b.get("id"), inp.get("command")))
                        if str(inp.get("description") or "").strip():
                            t["intent_with"] += 1
                    if name in ("Agent", "Task"):
                        desc = str(inp.get("description") or inp.get("subagent_type") or "agent")
                        key = b.get("id") or "task-" + str(len(t["sub_agents"]))
                        agent_calls[key] = turn_idx
                        t["sub_agents"].setdefault(key, {"name": desc, "tool_calls": 0, "tokens": 0})
                    if name == "Read" and path:
                        t["views"][path] += 1
                        if IMAGE_RX.search(path):
                            t["image_views"] += 1
                        if INSTRUCTION_RX.search(path):
                            t["instruction_views"].append(path)
                    if name == "Skill":
                        t["skills"][str(inp.get("skill") or "?")] += 1
                    if name == "SendMessage" and CONVERGE_RX.search(str(inp.get("message") or "")):
                        t["converge_nudges"] += 1
    # F-18: the subagents store. Each depth-1 node attaches to the turn that launched it (its
    # meta.toolUseId is the Agent call's id); a depth-2 review attaches to its parent node.
    nodes, store_note = read_subagents(os.path.splitext(sess["path"])[0])
    by_id = {n["id"]: n for n in nodes}
    for n in nodes:
        n["reviews"] = sum(1 for m in nodes if m.get("parent") == n["id"])
    for n in nodes:
        top = n
        hops = 0
        while top.get("parent") in by_id and hops < 8:
            top = by_id[top["parent"]]
            hops += 1
        ti = agent_calls.get(top.get("tool_use_id"))
        if ti is None or ti >= len(T):
            continue
        t = T[ti]
        for row in n["_rows"]:
            t["sub_requests"].append(dict(row, main=False, rsn=0, created=None))
        t["sub_tools"] += n["tool_calls"]
        entry = t["sub_agents"].setdefault(top.get("tool_use_id"), {"name": top["name"], "tool_calls": 0, "tokens": 0})
        if n is top:
            entry.update({"name": n["name"], "duration_s": n["span_s"], "model": n["model"], "requests": n["requests"],
                          "ctx_max": n["ctx_max"], "resumes": n["resumes"], "long_waits": n["long_waits"]})
        entry["tool_calls"] += n["tool_calls"]
        entry["tokens"] += n["tokens"]
    turns = []
    for i, t in enumerate(T):
        main = t["requests"]
        rows = main + t["sub_requests"]
        started = parse_ts(t["started"])
        wall = round((t["ended"] - started).total_seconds()) if (t["ended"] and started) else None
        rereads = {p: n for p, n in t["views"].items() if n >= 3}
        turns.append({
            "turn": i, "prompt": t["prompt"], "started": t["started"], "delivery": "idle",
            "models": sorted(t["models"]), "families": sorted({model_family(m) for m in t["models"]}),
            "wall_s": wall, "main_requests": len(main), "sub_requests": len(t["sub_requests"]),
            "ctx_start": main[0]["in"] if main else None, "ctx_end": main[-1]["in"] if main else None,
            "ctx_max": max([r["in"] for r in main]) if main else None,
            "cache_read": sum(r["cr"] for r in rows), "uncached_in": sum(max(r["in"] - r["cr"] - r["cw"], 0) for r in rows),
            "cache_write": sum(r["cw"] for r in rows), "output": sum(r["out"] for r in rows),
            "reasoning": sum(r["rsn"] for r in rows), "cost_aiu": None,
            "main_api_s": None, "all_api_s": None, "ttft_p50": None, "ttft_p90": None, "ttft_max": None,
            "tools": dict(t["tools"]), "view_bytes": None, "rereads": rereads, "paged_full_views": 0,
            "image_views": t["image_views"], "instruction_views": t["instruction_views"],
            "skills": dict(t["skills"]), "asst_msgs": t["asst_msgs"], "asst_text_chars": t["asst_text_chars"],
            "goal_state": t["goal_state"], "tier": t["tier"], "nudges": t["nudges"], "aborts": 0, "errors": 0,
            "hook_s": None, "sub_agents": list(t["sub_agents"].values()), "sub_tool_calls": t["sub_tools"],
            "converge_nudges": t["converge_nudges"], "sub_orientation_reads": t["sub_orientation_reads"],
            "reasoning_main": sum(r["rsn"] for r in main), "reasoning_chars": t["reasoning_chars"],
            "intent_eligible": t["intent_eligible"], "intent_with": t["intent_with"], "effort": None,
            "shell_shapes": shell_shape_counts(t["commands"], t["results"]),
            "est_usd": est_usd_rows(rows)[0] if rows else None,
        })
    main_rows = [r for t in T for r in t["requests"]]
    node_rows = [r for n in nodes for r in n["_rows"]]
    usd_main, unpriced_main = est_usd_rows(main_rows)
    usd_nodes, unpriced_nodes = est_usd_rows(node_rows)
    for n in nodes:
        n.pop("_rows", None)
    facts = {"harness": "claude", "id": sess["id"], "title": title, "cwd": cwd,
             "requests": {"main": len(main_rows), "subagents": len(node_rows)},
             "tokens": {"cache_read": sum(r["cr"] for r in main_rows + node_rows),
                        "cache_write": sum(r["cw"] for r in main_rows + node_rows),
                        "uncached_in": sum(max(r["in"] - r["cr"] - r["cw"], 0) for r in main_rows + node_rows),
                        "output": sum(r["out"] for r in main_rows + node_rows)},
             "quota": quota,
             "est_usd_if_api_billed": (round(usd_main + usd_nodes, 2) if (main_rows or node_rows) else None),
             "est_usd_main": usd_main, "est_usd_subagents": usd_nodes,
             "est_usd_note": "list rates cached {0}; {1} request(s) had no rate row and are unpriced".format(
                 LIST_RATES_CACHED, unpriced_main + unpriced_nodes),
             "subagents": {"store": store_note, "agents": len(nodes), "requests": len(node_rows),
                           "depth1": sum(1 for n in nodes if n["depth"] == 1),
                           "tool_calls": sum(n["tool_calls"] for n in nodes),
                           "cache_read": sum(n["cache_read"] for n in nodes), "output": sum(n["output"] for n in nodes)},
             "nodes": nodes,
             "started": T[0]["started"] if T else None, "updated": sess["updated"],
             "settings": {}, "prefix_chars": None, "prefix_tokens_est": None, "prefix_blocks": [],
             "prefix_note": NOT_RECORDED + " (Claude Code transcripts do not store the system prompt; use `context-budget.py prefix`)",
             "compactions": sum(1 for r in recs if r.get("type") == "system" and "compact" in str(r.get("subtype", ""))),
             "events": len(recs), "usage_rows": len(seen_msg),
             "cost_usd": round(cost_state.get("totalCostUSD"), 2) if cost_state and cost_state.get("totalCostUSD") is not None else None}
    return facts, turns


# --------------------------------------------------------------------------- findings
def _ev(turn, note):
    return {"turn": turn, "note": note}


def detect(session):
    """Rule-based detectors over one profiled session. Returns finding dicts with evidence.
    Every rule is deterministic; the 'confidence' is Verified for measured facts and Inferred
    where a heuristic (regex over text) stands in for a field the harness does not record."""
    facts, turns = session["facts"], session["turns"]
    out = []

    def add(fid, evidence, metric=None, confidence="Verified", severity=None):
        title, sev, fixes = FINDINGS[fid]
        out.append({"id": fid, "title": title, "severity": severity or sev, "confidence": confidence,
                    "session": facts["id"], "harness": facts["harness"], "evidence": evidence,
                    "metric": metric, "fixes": fixes})

    ev = [_ev(t["turn"], "context {0:,} -> {1:,} tokens over {2} main requests".format(t["ctx_start"], t["ctx_end"], t["main_requests"]))
          for t in turns if t["ctx_end"] and (t["ctx_end"] > 300000 or (t["ctx_start"] and t["ctx_end"] - t["ctx_start"] > 150000))]
    if ev:
        add("SP-01", ev[:8], {"max_ctx": max(t["ctx_max"] or 0 for t in turns), "compactions": facts["compactions"]})
    blocks = facts.get("prefix_blocks") or []
    big = sorted([b for b in blocks if b > 20000], reverse=True)
    if len(big) >= 2 and abs(big[0] - big[1]) < 0.15 * big[0]:
        add("SP-02", [_ev(None, "custom-instruction blocks of {0:,} and {1:,} chars in the static prefix".format(big[0], big[1]))],
            {"blocks": big[:2], "duplicate_tokens_est": est_tokens(big[1])})
    if facts.get("prefix_tokens_est") and facts["prefix_tokens_est"] > 60000:
        add("SP-03", [_ev(None, "static prefix ~{0:,} est. tokens ({1:,} chars; {2})".format(
            facts["prefix_tokens_est"], facts["prefix_chars"], facts["prefix_note"]))],
            {"prefix_tokens_est": facts["prefix_tokens_est"]}, confidence="Inferred")
    ev = []
    for t in turns:
        for p, n in sorted(t["rereads"].items(), key=lambda kv: -kv[1])[:4]:
            ev.append(_ev(t["turn"], "{0} viewed {1}x".format(_basename(p), n)))
        if t["paged_full_views"]:
            ev.append(_ev(t["turn"], "{0} paged tool output(s) viewed whole".format(t["paged_full_views"])))
    if ev:
        add("SP-04", ev[:10], {"turns_with_rereads": sum(1 for t in turns if t["rereads"] or t["paged_full_views"])})
    tot = collections.Counter()
    for t in turns:
        tot.update(t["skills"])
    rep = {k: v for k, v in tot.items() if v >= 2}
    if rep:
        add("SP-05", [_ev(None, "{0} invoked {1}x".format(k, v)) for k, v in sorted(rep.items(), key=lambda kv: -kv[1])],
            {"repeat_invocations": sum(v - 1 for v in rep.values())})
    ev = [_ev(t["turn"], "{0} sub-agent(s), no tier declared: {1}".format(len(t["sub_agents"]), ", ".join(str(s.get("name")) for s in t["sub_agents"][:6])))
          for t in turns if len(t["sub_agents"]) >= 3 and not t["tier"]]
    if ev:
        add("SP-06", ev[:6], {"turns": len(ev)}, confidence="Inferred")
    ev = []
    for t in turns:
        for s in sorted(t["sub_agents"], key=lambda x: -(x.get("tool_calls") or 0)):
            if (s.get("tool_calls") or 0) > 40 or (s.get("tokens") or 0) > 1000000:
                ev.append(_ev(t["turn"], "{0}: {1} tool calls, {2:,} tokens, {3}s".format(
                    s.get("name"), s.get("tool_calls"), s.get("tokens") or 0, s.get("duration_s", "?"))))
        if t["converge_nudges"]:
            ev.append(_ev(t["turn"], "parent sent {0} converge/stop message(s)".format(t["converge_nudges"])))
    if ev:
        add("SP-07", ev[:8], {"count": len(ev)})
    # SP-23 / SP-26: per node, from the subagents store (Claude Code).
    nodes = facts.get("nodes") or []
    ev = [_ev(None, "{0}: {1} waited {2:,} s at {3}".format(n["name"], w[0], w[1], w[2]))
          for n in nodes for w in n.get("long_waits") or []]
    if ev:
        add("SP-23", ev[:8], {"count": len(ev), "worst_s": max(w[1] for n in nodes for w in n.get("long_waits") or [])})
    ev = [_ev(None, "{0}: resumed after {1:,} s idle; span {2:,} s (the second run set no marker unless the resume carried `start`)".format(
        n["name"], max(n["resume_gaps_s"]), n["span_s"] or 0))
          for n in nodes if n.get("resume_gaps_s") and max(n["resume_gaps_s"]) > RESUME_GAP_S]
    if ev:
        add("SP-26", ev[:8], {"count": len(ev)})
    # SP-24 / SP-25: the shell shapes, main line and nodes together.
    piped = also = heredocs = 0
    ev24, ev25 = [], []
    for t in turns:
        p, a, h, lines = t.get("shell_shapes") or (0, 0, 0, [])
        piped, also, heredocs = piped + p, also + a, heredocs + h
        for line in lines:
            (ev25 if line.startswith("heredoc") else ev24).append(_ev(t["turn"], line))
    for n in nodes:
        p, a, h, lines = n.get("shell_shapes") or (0, 0, 0, [])
        piped, also, heredocs = piped + p, also + a, heredocs + h
        for line in lines:
            (ev25 if line.startswith("heredoc") else ev24).append(_ev(None, n["name"][:24] + ": " + line))
    if piped:
        add("SP-24", ev24[:8], {"piped_gate_lines": piped, "also_commit_merge_push": also})
    if heredocs:
        add("SP-25", ev25[:8], {"failed_heredocs": heredocs})
    ev = [_ev(t["turn"], r) for t in turns for r in t["sub_orientation_reads"]]
    if ev:
        add("SP-08", ev[:10], {"reads": len(ev)})
    subst = [t for t in turns if t["main_requests"] >= 3 and t["delivery"] != "steering" and t["nudges"] == 0]
    ev = [_ev(t["turn"], "'{0}' - first reply has no Goal / Done when".format(t["prompt"][:60])) for t in subst if not t["goal_state"]]
    if ev:
        add("SP-09", ev[:8], {"missing": len(ev), "substantive": len(subst)}, confidence="Inferred")
    ev = [_ev(t["turn"], "ttft p50 {0}s / p90 {1}s / max {2}s over {3} main requests".format(t["ttft_p50"], t["ttft_p90"], t["ttft_max"], t["main_requests"]))
          for t in turns if t["ttft_p90"] and t["ttft_p90"] > 20]
    if ev:
        add("SP-10", ev[:6], {"worst_p90": max(t["ttft_p90"] for t in turns if t["ttft_p90"])})
    n = sum(t["nudges"] + t["aborts"] for t in turns)
    if n:
        add("SP-11", [_ev(t["turn"], "{0} nudge(s), {1} abort(s)".format(t["nudges"], t["aborts"])) for t in turns if t["nudges"] or t["aborts"]][:8], {"count": n})
    n_img = sum(t["image_views"] for t in turns)
    n_err = sum(t["errors"] for t in turns)
    if n_img or n_err:
        add("SP-12", [_ev(t["turn"], "{0} image view(s), {1} failed request(s)".format(t["image_views"], t["errors"])) for t in turns if t["image_views"] or t["errors"]][:8],
            {"image_views": n_img, "errors": n_err})
    hw = [(t["hook_s"], t["wall_s"]) for t in turns if t["hook_s"] and t["wall_s"]]
    if hw:
        share = sum(h for h, _ in hw) / max(1, sum(w for _, w in hw))
        if share > 0.05:
            add("SP-13", [_ev(None, "hooks {0:.0f}s of {1:.0f}s wall ({2:.0f}%)".format(sum(h for h, _ in hw), sum(w for _, w in hw), 100 * share))], {"share": round(share, 3)})
    ev = [_ev(t["turn"], _basename(p)) for t in turns for p in t["instruction_views"]]
    if ev:
        add("SP-16", ev[:8], {"reads": len(ev)})
    # SP-19: where the money actually is. Fires when the main line both dominates the spend and
    # costs materially more per request than the delegates it convened - the shape every budget
    # the pack carries was pointed away from (class CTX-M).
    mech = mechanical_cost_findings(turns)
    if mech:
        add("SP-22", [_ev(r["turn"], "mechanical close at effort={0}: {1:,.0f} AIU over {2} main request(s)".format(
            r["effort"], r["cost_aiu"], r["requests"])) for r in mech][:6],
            {"count": len(mech), "aiu": round(sum(r["cost_aiu"] for r in mech), 1)})
    la = late_addition_findings(turns)
    if la:
        add("SP-20", [_ev(r["turn"], r["reason"]) for r in la][:6], {"count": len(la)})
    ma = facts.get("model_attribution") or {}
    if ma.get("mismatch"):
        add("SP-21", [_ev(None, "recorded setting {0!r}; effective model {1!r} at {2}% of main-line cost across {3} distinct model(s)".format(
            ma["recorded"], ma["effective"], ma.get("share"), ma.get("distinct")))],
            {k: ma.get(k) for k in ("recorded", "effective", "share", "distinct")})
    ml = facts.get("main_line") or {}
    if ml.get("main_pct") is not None and ml.get("cost_ratio") is not None             and ml["main_pct"] >= 80 and ml["cost_ratio"] >= 3:
        add("SP-19", [_ev(None, "main line {0:,} requests / {1:,.0f} AIU ({2}% of the session) vs delegates {3:,} / {4:,.0f}; {5}x the cost per request".format(
            ml["main_requests"], ml["main_cost"], ml["main_pct"], ml["sub_requests"], ml["sub_cost"], ml["cost_ratio"]))],
            {k: ml[k] for k in ("main_pct", "cost_ratio", "main_requests", "sub_requests")})
    # SP-17: how much of the billed reasoning came back as text. Informational: it decides how much
    # weight any text-derived judgement can carry (below 10% visible, drift read from text is Inferred).
    rsn = sum(t["reasoning_main"] for t in turns)
    chars = sum(t["reasoning_chars"] for t in turns)
    if rsn:
        share = min(1.0, est_tokens(chars) / float(rsn))
        add("SP-17", [_ev(None, "{0:,} reasoning tokens billed on the main line; {1:,} chars of reasoning text on disk (~{2:.0f}% visible at {3} chars/token)".format(
            rsn, chars, 100 * share, CHARS_PER_TOKEN))], {"visible_share": round(share, 3), "reasoning_tokens": rsn, "reasoning_chars": chars},
            confidence="Verified", severity="Nit")
    # SP-18: intent-trace coverage - the one reasoning trace every host records (the description on a shell call).
    elig = sum(t["intent_eligible"] for t in turns)
    with_ = sum(t["intent_with"] for t in turns)
    if elig:
        cov = with_ / float(elig)
        if cov < 0.9:
            add("SP-18", [_ev(t["turn"], "{0}/{1} shell calls carried an intent".format(t["intent_with"], t["intent_eligible"]))
                          for t in turns if t["intent_eligible"] and t["intent_with"] < t["intent_eligible"]][:8],
                {"coverage": round(cov, 3), "eligible": elig})
    out.sort(key=lambda f: SEVERITY_RANK.get(f["severity"], 9))
    return out


# --------------------------------------------------------------------------- compile stage (audit log)
# The compile stage (P7) writes its telemetry into the audit log: one `kind: compilation` entry
# per gate-passing compile, and on the workflow entry that started from it `compiled_from` +
# `edit_distance` (else `compiled: false`). These readers derive the rows at run time (derive,
# don't store - DM7) and every empty cell reads NOT_RECORDED (IO8). Pattern: pure reader over an
# append-only log (spec-compile-readers US-1/US-2). simplify: the JSONL read is a local function
# rather than a module shared with dream.py; ceiling: a third reader lifts it into one.
COMPILE_SUBSTANTIVE = {"skill"}
SP28_MEDIAN_THRESHOLD = 0.20  # Flagged in spec-compile-readers: revisit after ten compiles per template


def _read_audit_jsonl(path):
    entries, skipped = [], 0
    with open(path, "r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                entries.append(json.loads(line))
            except ValueError:
                skipped += 1
    return entries, skipped


def _is_number(v):
    return isinstance(v, (int, float)) and not isinstance(v, bool)


def _template_label(comp):
    c = comp.get("compiled") or {}
    return "{0} v{1}".format(c.get("harness") or "?", c.get("template_version") if c.get("template_version") is not None else "?")


def compile_measurements(root, days):
    """Per-session and per-template-version compile measurements from <root>/docs/audit/audit-log.jsonl.
    Missing log, empty log or no compile-bearing entry -> source NOT_RECORDED and empty maps."""
    path = os.path.join(root, "docs", "audit", "audit-log.jsonl")
    empty = {"source": NOT_RECORDED, "window_days": days, "by_session": {}, "by_template": {}, "gaps": [],
             "runs": 0, "compilations": 0, "skipped_lines": 0}
    if not os.path.isfile(path):
        return empty
    try:
        entries, skipped = _read_audit_jsonl(path)
    except (OSError, ValueError) as exc:  # a broken log degrades to `not recorded`, it never takes the profile down
        print("session-profile: compile fields not read - {0}: {1}".format(type(exc).__name__, exc), file=sys.stderr)
        return empty
    since = (_dt.datetime.now(_dt.timezone.utc) - _dt.timedelta(days=days)) if days else None

    def in_window(e):
        if since is None:
            return True
        ts = parse_ts(e.get("datetime") or "")
        return ts is None or ts >= since

    comps, runs, unrecorded = collections.OrderedDict(), [], []
    for e in entries:
        if not isinstance(e, dict) or not in_window(e):
            continue
        kind = e.get("kind")
        if kind == "compilation" and isinstance(e.get("compiled"), dict):
            comps[e.get("id")] = e
        elif kind in COMPILE_SUBSTANTIVE:
            (runs if ("compiled" in e or e.get("compiled_from")) else unrecorded).append(e)
    if not comps and not runs:
        return dict(empty, skipped_lines=skipped)

    by_session, by_template, gaps = collections.OrderedDict(), collections.OrderedDict(), []

    def srow(sid):
        return by_session.setdefault(sid, {"substantive_recorded": 0, "compiled_false": 0, "compiled_false_share": NOT_RECORDED,
                                           "unrecorded": 0, "compilations": 0, "edit_distances": []})

    def trow(label):
        return by_template.setdefault(label, {"compilations": 0, "samples": 0, "samples_unrecorded": 0,
                                              "edit_distance_median": NOT_RECORDED, "edit_distance_p90": NOT_RECORDED,
                                              "decision_requests_per_compilation": NOT_RECORDED, "refusals": 0, "retries": 0,
                                              "engine_seconds_median": NOT_RECORDED, "_dist": [], "_eng": [], "_dr": 0})

    for c in comps.values():
        srow(c.get("session") or "?")["compilations"] += 1
        t = trow(_template_label(c))
        t["compilations"] += 1
        prov = c["compiled"].get("provenance") or {}
        t["refusals"] += len(prov.get("refusals") or [])
        t["retries"] += int(prov.get("retries") or 0)
        if _is_number(prov.get("engine_seconds")):
            t["_eng"].append(float(prov["engine_seconds"]))
        t["_dr"] += len(c["compiled"].get("decision_requests") or [])
    for e in unrecorded:
        srow(e.get("session") or "?")["unrecorded"] += 1
    for e in runs:
        s = srow(e.get("session") or "?")
        s["substantive_recorded"] += 1
        if e.get("compiled_from"):
            c = comps.get(e["compiled_from"])
            t = trow(_template_label(c) if c else "unknown")
            d = e.get("edit_distance")
            if _is_number(d):
                t["_dist"].append(float(d))
                t["samples"] += 1
                s["edit_distances"].append(float(d))
            else:
                t["samples_unrecorded"] += 1
        elif e.get("compiled") is False:
            s["compiled_false"] += 1
            gaps.append({"id": e.get("id"), "shortname": e.get("shortname") or "?", "tier": e.get("tier"),
                         "session": e.get("session") or "?", "skill": e.get("skill")})
    for s in by_session.values():
        if s["substantive_recorded"]:
            s["compiled_false_share"] = round(s["compiled_false"] / float(s["substantive_recorded"]), 4)
    for t in by_template.values():
        if t["_dist"]:
            t["edit_distance_median"] = round(statistics.median(t["_dist"]), 4)
            t["edit_distance_p90"] = round(pct(t["_dist"], 0.9), 4)
        if t["_eng"]:
            t["engine_seconds_median"] = round(statistics.median(t["_eng"]), 4)
        if t["compilations"]:
            t["decision_requests_per_compilation"] = round(t["_dr"] / float(t["compilations"]), 2)
        for k in ("_dist", "_eng", "_dr"):
            t.pop(k, None)
    return {"source": path, "window_days": days, "by_session": by_session, "by_template": by_template, "gaps": gaps,
            "runs": len(runs), "compilations": len(comps), "skipped_lines": skipped}


def compile_findings(measure):
    """SP-27 (Inferred: a T0 closed question needs no compile, so only gaps above T0 count) and
    SP-28 (Verified: the template's median edit distance is a measured number)."""
    out = []
    by_sid = collections.OrderedDict()
    for g in measure.get("gaps") or []:
        if (g.get("tier") or "").upper() == "T0":
            continue
        by_sid.setdefault(g["session"], []).append(g)
    for sid, gs in by_sid.items():
        title, sev, fixes = FINDINGS["SP-27"]
        row = measure["by_session"].get(sid) or {}
        ev = [_ev(None, "{0} ({1}) started with compiled: false at tier {2}".format(g["shortname"], g.get("id"), g.get("tier") or "unset"))
              for g in gs[:7]]
        ev.append(_ev(None, "confirm: a T0 closed question needs no compile; each gap above is a turn to check (Inferred)"))
        out.append({"id": "SP-27", "title": title, "severity": sev, "confidence": "Inferred", "session": sid, "harness": "audit",
                    "evidence": ev[:8], "metric": {"count": len(gs), "share": row.get("compiled_false_share", NOT_RECORDED)},
                    "fixes": fixes})
    for label, t in (measure.get("by_template") or {}).items():
        med = t.get("edit_distance_median")
        if _is_number(med) and med > SP28_MEDIAN_THRESHOLD:
            title, sev, fixes = FINDINGS["SP-28"]
            out.append({"id": "SP-28", "title": title, "severity": sev, "confidence": "Verified", "session": "*", "harness": "audit",
                        "evidence": [_ev(None, "{0}: edit_distance median {1} (p90 {2}) over samples: {3} - above {4}".format(
                            label, med, t.get("edit_distance_p90"), t.get("samples"), SP28_MEDIAN_THRESHOLD))],
                        "metric": {"template": label, "median": med, "samples": t.get("samples")}, "fixes": fixes})
    return out


def render_compile_section(measure, findings=None):
    lines = ["## Compile stage (audit log)", ""]
    if not measure or measure.get("source") == NOT_RECORDED:
        lines += ["*compile fields: {0} - no `kind: compilation` entry and no `compiled` / `compiled_from` field in the window "
                  "(the compile stage writes them; `/compile`).*".format(NOT_RECORDED), ""]
        return lines
    lines += ["source `{0}` · window {1} · compilations {2} · recorded runs {3} · skipped lines {4}".format(
        measure["source"], ("last {0} days".format(measure["window_days"]) if measure.get("window_days") else "all"),
        measure["compilations"], measure["runs"], measure.get("skipped_lines", 0)), ""]
    lines.append(_md_table(["session", "substantive recorded", "compiled: false share", "unrecorded", "compilations"],
                           [[str(sid)[:12], s["substantive_recorded"], s["compiled_false_share"], s["unrecorded"], s["compilations"]]
                            for sid, s in measure["by_session"].items()]))
    lines.append("")
    lines.append(_md_table(["template", "compilations", "samples", "edit dist p50", "p90", "DR/compile", "refusals", "retries", "engine s p50"],
                           [[lbl, t["compilations"], "{0} (+{1} unrecorded)".format(t["samples"], t["samples_unrecorded"]) if t["samples_unrecorded"] else t["samples"],
                             t["edit_distance_median"], t["edit_distance_p90"], t["decision_requests_per_compilation"],
                             t["refusals"], t["retries"], t["engine_seconds_median"]] for lbl, t in measure["by_template"].items()]))
    lines.append("")
    if findings is not None:
        rows = [[f["id"], f["severity"], f["confidence"], f["title"], "; ".join(e["note"] for e in f["evidence"][:2]).replace("|", "/"), ", ".join(f["fixes"])]
                for f in findings]
        lines.append(_md_table(["id", "severity", "confidence", "finding", "evidence", "fix"], rows) if rows else "*no compile findings*")
        lines.append("")
    return lines


def cross_session_findings(sessions):
    """SP-15 concurrent sessions in one checkout (same cwd, overlapping windows)."""
    spans = []
    for s in sessions:
        f = s["facts"]
        a, b = parse_ts(f.get("started")), parse_ts(f.get("updated"))
        if a and b and f.get("cwd"):
            spans.append((norm_path(f["cwd"]), a, b, f["harness"], f["id"]))
    ev = []
    for i in range(len(spans)):
        for j in range(i + 1, len(spans)):
            p1, a1, b1, h1, i1 = spans[i]
            p2, a2, b2, h2, i2 = spans[j]
            if p1 == p2 and a1 <= b2 and a2 <= b1:
                ev.append(_ev(None, "{0}:{1} and {2}:{3} overlapped in {4}".format(h1, i1[:8], h2, i2[:8], p1)))
    if not ev:
        return []
    title, sev, fixes = FINDINGS["SP-15"]
    return [{"id": "SP-15", "title": title, "severity": sev, "confidence": "Verified", "session": "*",
             "harness": "*", "evidence": ev[:8], "metric": {"pairs": len(ev)}, "fixes": fixes}]


def family_comparison(sessions, compile=None):
    """Aggregate per (family, harness): the tuning view. Drift indicators are counts per turn.
    `compile` is the compile_measurements() dict; its per-session rows join on the session id and
    give the group its compiled share and pooled edit-distance median (never a median of medians)."""
    agg = collections.defaultdict(lambda: {"turns": 0, "main_requests": 0, "cache_read": 0, "output": 0, "reasoning": 0,
                                           "cost_aiu": 0.0, "ttft_p90": [], "ctx_end": [], "sub_agents": 0,
                                           "rereads": 0, "skill_repeats": 0, "no_goal": 0, "no_tier_with_fanout": 0,
                                           "converge_nudges": 0, "nudges": 0, "wall_s": 0,
                                           "reasoning_main": 0, "reasoning_chars": 0, "intent_eligible": 0, "intent_with": 0,
                                           "effort": collections.Counter(), "sids": set()})
    for s in sessions:
        for t in s["turns"]:
            if not t["models"]:
                continue
            key = ("+".join(t["families"]), s["facts"]["harness"])
            a = agg[key]
            a["sids"].add(s["facts"]["id"])
            a["turns"] += 1
            a["main_requests"] += t["main_requests"]
            a["cache_read"] += t["cache_read"]
            a["output"] += t["output"]
            a["reasoning"] += t["reasoning"]
            a["cost_aiu"] += t["cost_aiu"] or 0
            if t["ttft_p90"]:
                a["ttft_p90"].append(t["ttft_p90"])
            if t["ctx_end"]:
                a["ctx_end"].append(t["ctx_end"])
            a["sub_agents"] += len(t["sub_agents"])
            a["rereads"] += sum(t["rereads"].values())
            a["skill_repeats"] += sum(max(v - 1, 0) for v in t["skills"].values())
            a["no_goal"] += 0 if (t["goal_state"] or t["main_requests"] < 3) else 1
            a["no_tier_with_fanout"] += 1 if (len(t["sub_agents"]) >= 3 and not t["tier"]) else 0
            a["converge_nudges"] += t["converge_nudges"]
            a["nudges"] += t["nudges"] + t["aborts"]
            a["wall_s"] += t["wall_s"] or 0
            a["reasoning_main"] += t.get("reasoning_main") or 0
            a["reasoning_chars"] += t.get("reasoning_chars") or 0
            a["intent_eligible"] += t.get("intent_eligible") or 0
            a["intent_with"] += t.get("intent_with") or 0
            if t.get("effort"):
                a["effort"][t["effort"]] += 1
    rows = []
    by_session = (compile or {}).get("by_session") or {}
    for (fam, harness), a in sorted(agg.items()):
        n = max(a["turns"], 1)
        drift = a["sub_agents"] + a["rereads"] + a["skill_repeats"] + a["no_goal"] + a["no_tier_with_fanout"] + a["converge_nudges"] + a["nudges"]
        crows = [by_session[sid] for sid in a["sids"] if sid in by_session]
        recorded = sum(r["substantive_recorded"] for r in crows)
        dists = [d for r in crows for d in r.get("edit_distances") or []]
        rows.append({"family": fam, "harness": harness, "turns": a["turns"],
                     "compiled_pct": (round(100.0 * (recorded - sum(r["compiled_false"] for r in crows)) / recorded, 1) if recorded else None),
                     "edit_dist_p50": (round(statistics.median(dists), 4) if dists else None),
                     "requests_per_turn": round(a["main_requests"] / n, 1),
                     "cache_read_per_turn": int(a["cache_read"] / n), "output_per_turn": int(a["output"] / n),
                     "reasoning_per_turn": int(a["reasoning"] / n),
                     "cost_aiu_per_turn": round(a["cost_aiu"] / n, 1) if a["cost_aiu"] else None,
                     "ttft_p90_median": round(statistics.median(a["ttft_p90"]), 1) if a["ttft_p90"] else None,
                     "ctx_end_median": int(statistics.median(a["ctx_end"])) if a["ctx_end"] else None,
                     "wall_s_per_turn": int(a["wall_s"] / n),
                     "reasoning_share_of_output": round(a["reasoning_main"] / float(a["output"]), 2) if a["output"] else None,
                     "visible_reasoning_pct": (round(100.0 * min(1.0, est_tokens(a["reasoning_chars"]) / float(a["reasoning_main"])), 1) if a["reasoning_main"] else None),
                     "intent_trace_pct": (round(100.0 * a["intent_with"] / a["intent_eligible"], 1) if a["intent_eligible"] else None),
                     "effort": (a["effort"].most_common(1)[0][0] if a["effort"] else NOT_RECORDED),
                     "drift_per_turn": round(drift / n, 2),
                     "drift_breakdown": {k: a[k] for k in ("sub_agents", "rereads", "skill_repeats", "no_goal", "no_tier_with_fanout", "converge_nudges", "nudges")}})
    findings = []
    comparable = [r for r in rows if r["turns"] >= 3]
    if len(comparable) >= 2:
        by_drift = sorted(comparable, key=lambda r: -r["drift_per_turn"])
        hi, lo = by_drift[0], by_drift[-1]
        if lo["drift_per_turn"] > 0 and hi["drift_per_turn"] >= 2 * lo["drift_per_turn"]:
            title, sev, fixes = FINDINGS["SP-14"]
            findings.append({"id": "SP-14", "title": title, "severity": sev, "confidence": "Verified", "session": "*",
                             "harness": "*", "evidence": [_ev(None, "{0}/{1}: {2} drift indicators per turn vs {3}/{4}: {5}".format(
                                 hi["family"], hi["harness"], hi["drift_per_turn"], lo["family"], lo["harness"], lo["drift_per_turn"])),
                                                         _ev(None, "caveat: the turn mix differs ({0} vs {1} turns); confirm on like-for-like tasks before tuning".format(hi["turns"], lo["turns"]))],
                             "metric": {"hi": hi["drift_per_turn"], "lo": lo["drift_per_turn"]}, "fixes": fixes})
    return rows, findings


# --------------------------------------------------------------------------- rendering
def _md_table(headers, rows):
    out = ["| " + " | ".join(headers) + " |", "|" + "---|" * len(headers)]
    for r in rows:
        out.append("| " + " | ".join(str(c) if c is not None else "\u2014" for c in r) + " |")
    return "\n".join(out)


def _fmt(n):
    if n is None:
        return NOT_RECORDED
    if isinstance(n, float):
        return "{0:,.1f}".format(n)
    if isinstance(n, int):
        return "{0:,}".format(n)
    return str(n)


def _frontmatter(profile):
    repos = ", ".join(profile.get("repo_labels") or [repo_label(r) for r in profile["repos"]])
    top = ", ".join(f["id"] for f in profile["findings"][:3]) or "none"
    return "\n".join([
        "---",
        "id: profile-{0}".format(profile["id"]),
        'title: "Session profile {0} - {1}"'.format(profile["id"], repos),
        "type: doc",
        "status: accepted",
        'owner: "@timianmalloo"',
        "tags: [profile, session-profiler, efficiency, adherence]",
        "links:",
        "  - { to: design-session-profiler, rel: relates-to }",
        'review-by: "{0}"'.format((_dt.date.today() + _dt.timedelta(days=90)).isoformat()),
        "summary: >-",
        "  Measured pass over {0} session(s) in {1} ({2}); {3} finding(s), top: {4}.".format(
            len(profile["sessions"]), repos, profile["window"], len(profile["findings"]), top),
        "---",
        "",
    ])


def render_markdown(profile):
    lines = [_frontmatter(profile) + "# Session profile {0}".format(profile["id"]), "",
             "*Generated {0} by `session-profile.py`. Every number is read from the harness's own store unless marked est. (chars/token = {1}). "
             "A missing measurement reads `{2}`, never a guess (IO8).*".format(profile["generated"], CHARS_PER_TOKEN, NOT_RECORDED), "",
             "**Repos:** " + ", ".join(profile.get("repo_labels") or [repo_label(r) for r in profile["repos"]]) + "  ", "**Window:** " + profile["window"] + "  ",
             "**Sessions:** {0} ({1})".format(len(profile["sessions"]), ", ".join(sorted({s["facts"]["harness"] for s in profile["sessions"]}))), ""]
    lines += ["## Findings", ""]
    if profile["findings"]:
        rows = []
        for f in profile["findings"]:
            ev = "; ".join(("t{0}: ".format(e["turn"]) if e.get("turn") is not None else "") + e["note"] for e in f["evidence"][:3])
            rows.append([f["id"], f["severity"], f["confidence"], f["title"], "{0}:{1}".format(f["harness"], str(f["session"])[:8]), ev.replace("|", "/"), ", ".join(f["fixes"])])
        lines.append(_md_table(["id", "severity", "confidence", "finding", "session", "evidence", "fix"], rows))
    else:
        lines.append("*No findings - either the sessions are clean or the window is empty. Check the session table below.*")
    lines += ["", "## Fixes (the pack surfaces that own the controls)", ""]
    used = collections.OrderedDict()
    for f in profile["findings"]:
        for fx in f["fixes"]:
            used.setdefault(fx, []).append(f["id"])
    rows = [[fx, FIXES[fx]["title"], FIXES[fx]["where"], FIXES[fx]["control"], ", ".join(sorted(set(ids)))] for fx, ids in used.items()]
    lines.append(_md_table(["fix", "what", "where in the pack", "control that fails on recurrence", "findings"], rows) if rows else "*none*")
    lines += ["", "## Model family x harness (the tuning view)", ""]
    lines.append(_md_table(["family", "harness", "turns", "req/turn", "cache-read/turn", "out/turn", "reasoning/turn", "reasoning visible", "effort", "intent trace", "cost/turn (AIU)", "ttft p90 (median)", "ctx end (median)", "wall s/turn", "drift/turn", "compiled", "edit dist p50"],
                           [[r["family"], r["harness"], r["turns"], r["requests_per_turn"], _fmt(r["cache_read_per_turn"]), _fmt(r["output_per_turn"]), _fmt(r["reasoning_per_turn"]),
                             (str(r["visible_reasoning_pct"]) + "%") if r["visible_reasoning_pct"] is not None else NOT_RECORDED, r["effort"],
                             (str(r["intent_trace_pct"]) + "%") if r["intent_trace_pct"] is not None else NOT_RECORDED,
                             _fmt(r["cost_aiu_per_turn"]), _fmt(r["ttft_p90_median"]), _fmt(r["ctx_end_median"]), r["wall_s_per_turn"], r["drift_per_turn"],
                             (str(r["compiled_pct"]) + "%") if r.get("compiled_pct") is not None else NOT_RECORDED,
                             r["edit_dist_p50"] if r.get("edit_dist_p50") is not None else NOT_RECORDED] for r in profile["comparison"]]))
    lines += ["", "*drift/turn = sub-agents + re-reads + skill repeats + missing goal state + fan-out without tier + converge nudges + cap firings, per turn. "
              "reasoning visible = reasoning text on disk as a share of billed reasoning tokens (est.); below 10% every text-derived drift judgement is Inferred. "
              "effort = the host's recorded reasoning effort (Copilot) or not recorded (Claude Code). intent trace = shell calls carrying a one-line description. "
              "compiled = share of the group's substantive audit entries that started from a compiled prompt; edit dist p50 = pooled median of what the human changed before the run (audit log).*", ""]
    if "compile" in profile:
        lines += render_compile_section(profile.get("compile"))
    for s in profile["sessions"]:
        f = s["facts"]
        lines += ["## {0} session `{1}` \u2014 {2}".format(f["harness"], f["id"][:8], f.get("title") or ""), "",
                  "started {0} \u00b7 updated {1} \u00b7 cwd `{2}` \u00b7 prefix {3} \u00b7 compactions {4}{5}".format(
                      f.get("started"), f.get("updated"), f.get("cwd"),
                      ("~{0:,} est. tokens / {1:,} chars".format(f["prefix_tokens_est"], f["prefix_chars"]) if f.get("prefix_chars") else NOT_RECORDED),
                      f.get("compactions"), _settings_note(f)), ""]
        if f.get("requests") is not None:
            tk = f.get("tokens") or {}
            lines += ["**Cost, in the units that are measured.** requests: main {0:,} \u00b7 sub-agents {1:,}; "
                      "tokens: cache-read {2:,} \u00b7 cache-write {3:,} \u00b7 uncached in {4:,} \u00b7 output {5:,}; "
                      "quota: {6}; if API-billed: {7} ({8}); harness cost record: {9}.".format(
                          f["requests"]["main"], f["requests"]["subagents"], tk.get("cache_read", 0), tk.get("cache_write", 0),
                          tk.get("uncached_in", 0), tk.get("output", 0),
                          json.dumps(f.get("quota")) if f.get("quota") != NOT_RECORDED else NOT_RECORDED,
                          ("est. ${0:,.2f}".format(f["est_usd_if_api_billed"]) if f.get("est_usd_if_api_billed") is not None else NOT_RECORDED),
                          f.get("est_usd_note", ""), ("${0:,.2f}".format(f["cost_usd"]) if f.get("cost_usd") is not None else NOT_RECORDED)), ""]
        rows = []
        for t in s["turns"]:
            rows.append([t["turn"], t["prompt"][:48].replace("|", "/"), "+".join(t["families"]) or "\u2014", t["main_requests"], _fmt(t["ctx_start"]), _fmt(t["ctx_end"]),
                         _fmt(t["cache_read"]), _fmt(t["output"]), _fmt(t["cost_aiu"]), _fmt(t["ttft_p90"]), t["wall_s"], len(t["sub_agents"]),
                         sum(t["rereads"].values()), "yes" if t["goal_state"] else "no", "yes" if t["tier"] else "no"])
        lines.append(_md_table(["turn", "prompt", "family", "main req", "ctx start", "ctx end", "cache read", "output", "cost AIU", "ttft p90", "wall s", "subs", "re-reads", "goal", "tier"], rows))
        lines.append("")
        nodes = f.get("nodes") or []
        if nodes:
            lines += ["### Nodes (the sub-agent store: {0}; {1} agent(s))".format(f["subagents"]["store"], len(nodes)), "",
                      _md_table(["node", "type", "depth", "model", "span s", "req", "tools", "reviews", "cache read", "output",
                                 "ctx max", "resumes", "longest wait", "if API-billed"],
                                [[n["name"][:48].replace("|", "/"), n.get("type") or "\u2014", n["depth"], n.get("model") or NOT_RECORDED,
                                  _fmt(n["span_s"]), n["requests"], n["tool_calls"], n["reviews"], _fmt(n["cache_read"]), _fmt(n["output"]),
                                  _fmt(n["ctx_max"]), n["resumes"],
                                  ("{0} {1:,} s".format(n["long_waits"][0][0], n["long_waits"][0][1]) if n["long_waits"] else "\u2014"),
                                  ("est. ${0:,.2f}".format(n["est_usd"]) if n.get("est_usd") is not None else NOT_RECORDED)]
                                 for n in sorted(nodes, key=lambda x: (x["depth"], x.get("started") or ""))]),
                      "", "*if API-billed = tokens x first-party list rates cached {0}; the operator on a subscription pays quota, not this.*".format(LIST_RATES_CACHED), ""]
    return "\n".join(lines) + "\n"


# --------------------------------------------------------------------------- commands
def _select(args):
    since = None
    if args.days:
        since = _dt.datetime.now(_dt.timezone.utc) - _dt.timedelta(days=args.days)
    found = []
    for repo in args.repo:
        if not os.path.isdir(repo):
            print("session-profile: not a directory: {0}".format(repo), file=sys.stderr)
            continue
        ident = repo_identity(repo)
        if args.harness in ("all", "copilot"):
            found += copilot_sessions(ident, since, args.copilot_home or copilot_home())
        if args.harness in ("all", "claude"):
            found += claude_sessions(ident, since, args.claude_home or claude_home())
    if getattr(args, "session", None):
        want = set(args.session)
        found = [s for s in found if s["id"] in want or any(s["id"].startswith(w) for w in want)]
    found.sort(key=lambda s: s.get("updated") or "", reverse=True)
    if getattr(args, "limit", None):
        found = found[:args.limit]
    return found, since


def cmd_discover(args):
    found, _ = _select(args)
    if not found:
        print("no sessions found for {0} (harness={1}, days={2})".format(", ".join(args.repo), args.harness, args.days))
        return 1
    print("{0:8} {1:9} {2:20} {3:20} {4}".format("harness", "id", "started", "updated", "title / cwd"))
    for s in found:
        print("{0:8} {1:9} {2:20} {3:20} {4}".format(s["harness"], s["id"][:8], (s.get("started") or "-")[:19], (s.get("updated") or "-")[:19],
                                                     (s.get("title") or "") + ("  " + s["cwd"] if s.get("cwd") else "")))
    print("\n{0} session(s)".format(len(found)))
    return 0


def _profile_all(found, args, compile=None):
    settings = copilot_settings(args.copilot_home or copilot_home())
    sessions = []
    for s in found:
        try:
            facts, turns = profile_copilot(s, settings) if s["harness"] == "copilot" else profile_claude(s)
        except Exception as exc:  # noqa: BLE001 - a broken store must not abort the profile; it is REPORTED
            print("session-profile: skipping {0}:{1} - {2}: {3}".format(s["harness"], s["id"][:8], type(exc).__name__, exc), file=sys.stderr)
            continue
        sessions.append({"facts": facts, "turns": turns})
    findings = []
    for s in sessions:
        findings += detect(s)
    comparison, fam_findings = family_comparison(sessions, compile)
    findings += fam_findings + cross_session_findings(sessions)
    if compile is not None:
        findings += compile_findings(compile)
    findings.sort(key=lambda f: SEVERITY_RANK.get(f["severity"], 9))
    return sessions, findings, comparison


def _compile_for(args):
    return compile_measurements(os.path.abspath(args.repo[0]), args.days) if args.repo else None


def profiles_dir(root, publish):
    """Where this run's profile goes, and whether that place is committed (FR-104, owner ruling
    2026-10-19). Only the pack's own source repository commits profiles by default; any other
    repository gets the checkout's untracked git dir (or the user's state dir outside git)
    unless the operator passes --publish."""
    if publish or os.path.isfile(os.path.join(root, "pack", "adapters", "INSTALL.md")):
        return os.path.join(root, "docs", "profiles"), True
    common = git(["rev-parse", "--path-format=absolute", "--git-common-dir"], root).strip()
    if common:
        return os.path.join(common, "ai-forward", "profiles"), False
    state = os.environ.get("XDG_STATE_HOME") or os.path.join(os.path.expanduser("~"), ".local", "state")
    key = hashlib.sha256(os.path.abspath(root).encode("utf-8")).hexdigest()[:16]
    return os.path.join(state, "ai-forward", "profiles", key), False


def profile_id(d):
    n = 0
    if os.path.isdir(d):
        for name in os.listdir(d):
            m = re.match(r"sp-(\d+)$", name)
            if m:
                n = max(n, int(m.group(1)))
    return "sp-{0:04d}".format(n + 1)


def _scrubber():
    """dream.py's `scrub`, the pack's one secret/PII pattern set (FR-103). None if it is missing."""
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "dream.py")
    try:
        spec = importlib.util.spec_from_file_location("_ai_forward_dream_scrub", path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module.scrub
    except (OSError, ImportError, AttributeError, SyntaxError):
        return None


def scrubbed(value, scrub):
    """Every string in a profile, scrubbed. FR-103: prompts, titles, paths and shell evidence were
    copied into committed profiles as typed, so a token in a gated command line was published."""
    if isinstance(value, str):
        return scrub(value)[0]
    if isinstance(value, dict):
        return {k: scrubbed(v, scrub) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [scrubbed(v, scrub) for v in value]
    return value


def cmd_profile(args):
    scrub = _scrubber()
    if scrub is None:
        print("COORD-NOT-CHECKED: dream.py (the secret scrub) is missing beside this script; "
              "nothing was written, because an unscrubbed profile can publish a secret", file=sys.stderr)
        return 2
    found, since = _select(args)
    if not found:
        print("no sessions found for {0} (harness={1}, days={2})".format(", ".join(args.repo), args.harness, args.days))
        return 1
    compile = _compile_for(args)
    sessions, findings, comparison = _profile_all(found, args, compile)
    root = os.path.abspath(args.out_root or args.repo[0])
    base, published = profiles_dir(root, getattr(args, "publish", False))
    pid = profile_id(base)
    profile = {"id": pid, "generated": iso(_dt.datetime.now(_dt.timezone.utc)), "repos": [os.path.abspath(r) for r in args.repo],
               "repo_labels": [repo_label(r) for r in args.repo],
             "window": "last {0} days".format(args.days) if args.days else "all sessions",
               "chars_per_token": CHARS_PER_TOKEN, "sessions": sessions, "findings": findings, "comparison": comparison,
               "compile": compile}
    profile = scrubbed(profile, scrub)
    profile["fixes"] = FIXES   # the pack's own static catalog: nothing from a session
    out_dir = os.path.join(base, pid)
    if args.json_only:
        print(json.dumps(profile, ensure_ascii=False, indent=2, default=str))
        return 0
    os.makedirs(out_dir, exist_ok=True)
    with open(os.path.join(out_dir, "profile.json"), "w", encoding="utf-8", newline="\n") as fh:
        json.dump(profile, fh, ensure_ascii=False, indent=2, default=str)
    md = render_markdown(profile)
    with open(os.path.join(out_dir, "profile.md"), "w", encoding="utf-8", newline="\n") as fh:
        fh.write(md)
    _index(base, profile)
    _audit(root, pid, len(sessions), len(findings), args.session_id,
           "docs/profiles/{0}/profile.md".format(pid) if published else None)
    print("profile {0}: {1} session(s), {2} finding(s)".format(pid, len(sessions), len(findings)))
    print("  report: {0}".format(os.path.join(out_dir, "profile.md")))
    if published:
        print("  NOTICE: docs/profiles/ is tracked: a commit publishes this profile. It carries scrubbed prompt "
              "previews, session titles, paths and command fragments.")
    else:
        print("  NOTICE: written to this checkout's git dir, not committed (FR-104). Pass --publish to write "
              "docs/profiles/, which is tracked and may be published.")
    if args.print_markdown:
        print()
        print(md)
    return 0


def _index(d, profile):
    path = os.path.join(d, "PROFILES.md")
    header = ("---\nid: session-profiles\ntitle: \"Session profiles\"\ntype: doc\nstatus: accepted\nowner: \"@timianmalloo\"\n"
              "tags: [profile, session-profiler, index]\nlinks:\n  - { to: design-session-profiler, rel: relates-to }\n"
              "review-by: \"2027-03-05\"\nsummary: >-\n  Index of /session-profiler runs - each row is one measured pass over the harness telemetry, "
              "mined by /dream as findings.\n---\n\n"
              "# Session profiles\n\n*Each row is one measured pass over the harness telemetry (`session-profile.py`). "
              "Mined by `/dream` as findings.*\n\n| id | generated | repos | sessions | findings | top |\n|---|---|---|---|---|---|\n")
    if not os.path.isfile(path):
        with open(path, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(header)
    top = ", ".join(f["id"] for f in profile["findings"][:3]) or "\u2014"
    with open(path, "a", encoding="utf-8", newline="\n") as fh:
        fh.write("| [{0}]({0}/profile.md) | {1} | {2} | {3} | {4} | {5} |\n".format(
            profile["id"], profile["generated"], ", ".join(profile.get("repo_labels") or [repo_label(r) for r in profile["repos"]]),
            len(profile["sessions"]), len(profile["findings"]), top))


_RUN_STARTED = iso(_dt.datetime.now(_dt.timezone.utc))


def _audit(root, pid, n_sessions, n_findings, session, artifact=None):
    """The script's own entry. It passes its OWN start (--started), so it never consumes the
    /session-profiler skill's marker - measured twice: the skill's closing entry read 16 s of a
    3-minute run because this append had eaten the marker (pack finding #2, Addenda C/D)."""
    # FR-105: the audit-log.py beside this script, isolated (-I). The one found in the target
    # repository was run before, so profiling a repository executed that repository's code.
    script = os.path.join(os.path.dirname(os.path.abspath(__file__)), "audit-log.py")
    if not os.path.isfile(script):
        return
    try:
        subprocess.run([sys.executable, "-I", script, "append", "--shortname", "session-profile-" + pid, "--kind", "script",
                        "--skill", "session-profiler", "--session", session or "session-profile-job",
                        "--started", _RUN_STARTED,
                        "--prompt", "session-profile.py profile", "--summary",
                        "Profile {0}: {1} session(s), {2} finding(s)".format(pid, n_sessions, n_findings)]
                       + (["--artifact", artifact] if artifact else []),
                       cwd=root, capture_output=True, timeout=30)
    except (OSError, subprocess.SubprocessError):
        pass


def cmd_compare(args):
    found, _ = _select(args)
    if not found:
        print("no sessions found")
        return 1
    sessions, findings, comparison = _profile_all(found, args)
    if args.json_only:
        print(json.dumps({"comparison": comparison, "findings": [f for f in findings if f["id"] == "SP-14"]}, indent=2, default=str))
        return 0
    print(_md_table(["family", "harness", "turns", "req/turn", "cache-read/turn", "out/turn", "reasoning/turn", "reasoning visible", "effort", "intent trace", "cost/turn", "ttft p90 med", "ctx end med", "wall s/turn", "drift/turn"],
                    [[r["family"], r["harness"], r["turns"], r["requests_per_turn"], _fmt(r["cache_read_per_turn"]), _fmt(r["output_per_turn"]), _fmt(r["reasoning_per_turn"]),
                      (str(r["visible_reasoning_pct"]) + "%") if r["visible_reasoning_pct"] is not None else NOT_RECORDED, r["effort"],
                      (str(r["intent_trace_pct"]) + "%") if r["intent_trace_pct"] is not None else NOT_RECORDED,
                      _fmt(r["cost_aiu_per_turn"]), _fmt(r["ttft_p90_median"]), _fmt(r["ctx_end_median"]), r["wall_s_per_turn"], r["drift_per_turn"]] for r in comparison]))
    for r in comparison:
        print("  {0}/{1} drift breakdown: {2}".format(r["family"], r["harness"], r["drift_breakdown"]))
    for f in findings:
        if f["id"] == "SP-14":
            print("\n{0} [{1}] {2}: {3}".format(f["id"], f["severity"], f["title"], f["evidence"][0]["note"]))
    return 0


def cmd_compile(args):
    """The compile section alone, from the audit log - needs no harness store, so it answers
    'is the compile stage used' even on a machine with no session telemetry."""
    measure = _compile_for(args)
    findings = compile_findings(measure)
    if args.json_only:
        print(json.dumps({"compile": measure, "findings": findings}, ensure_ascii=False, indent=2, default=str))
        return 0
    print("\n".join(render_compile_section(measure, findings)))
    return 0


def cmd_fixes(args):
    print(_md_table(["fix", "what", "where in the pack", "control"], [[k, v["title"], v["where"], v["control"]] for k, v in FIXES.items()]))
    print()
    print(_md_table(["finding", "severity", "title", "fixes"], [[k, v[1], v[0], ", ".join(v[2])] for k, v in FINDINGS.items()]))
    return 0


def main(argv=None):
    global CHARS_PER_TOKEN
    ap = argparse.ArgumentParser(prog="session-profile.py", description=__doc__.split("\n\n")[0])
    ap.add_argument("--repo", action="append", default=[], help="repo path (repeatable); the first one receives docs/profiles/")
    ap.add_argument("--days", type=int, default=30, help="window in days (0 = all)")
    ap.add_argument("--harness", choices=["all", "copilot", "claude"], default="all")
    ap.add_argument("--session", action="append", help="restrict to session id(s) or prefixes")
    ap.add_argument("--limit", type=int, default=None, help="newest N sessions")
    ap.add_argument("--copilot-home", help="override ~/.copilot")
    ap.add_argument("--claude-home", help="override ~/.claude")
    ap.add_argument("--chars-per-token", type=float, default=None, help="override the estimate ratio ({0})".format(CHARS_PER_TOKEN))
    sub = ap.add_subparsers(dest="cmd")
    p = sub.add_parser("discover", help="list matching sessions")
    p.set_defaults(func=cmd_discover)
    p = sub.add_parser("profile", help="profile sessions and write docs/profiles/<sp-id>/")
    p.add_argument("--out-root", help="repo root that receives docs/profiles/ (default: first --repo)")
    p.add_argument("--json-only", action="store_true", help="print JSON, write nothing")
    p.add_argument("--publish", action="store_true",
                   help="write docs/profiles/ (tracked) in a repo that is not the pack source (FR-104)")
    p.add_argument("--print-markdown", action="store_true")
    p.add_argument("--session-id", help="audit session id to record")
    p.set_defaults(func=cmd_profile)
    p = sub.add_parser("compare", help="aggregate by model family x harness")
    p.add_argument("--json-only", action="store_true")
    p.set_defaults(func=cmd_compare)
    p = sub.add_parser("compile", help="the compile-stage measurements from docs/audit/audit-log.jsonl (no harness store needed)")
    p.add_argument("--json-only", action="store_true")
    p.set_defaults(func=cmd_compile)
    p = sub.add_parser("fixes", help="print the fix and finding catalogs")
    p.set_defaults(func=cmd_fixes)
    args = ap.parse_args(argv)
    if args.chars_per_token:
        CHARS_PER_TOKEN = args.chars_per_token
    if not getattr(args, "func", None):
        ap.print_help()
        return 0
    if args.cmd != "fixes" and not args.repo:
        print("session-profile: --repo <path> is required", file=sys.stderr)
        return 2
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
