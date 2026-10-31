---
id: spec-coaching-loop
title: "Spec: In-the-moment coaching loop"
type: spec
status: draft
owner: "Imran"
phase: specify
tags: [coaching, triggers, coaching-card, phase-1]
links: []
review-by: 2026-11-30
summary: >-
  What AIQ's coaching loop must do for a developer in VS Code, Claude Code and Copilot CLI: notice one habit
  holding them back during a task, offer one actionable step at the right moment, and check whether
  the habit stuck. Private to the person.
---

# Spec: In-the-moment coaching loop

Draft produced with AI-Forward `/specify` (v2026.10.19.4), run interactively with Imran.
Source of truth: AIQ brief and Design > Coaching tab (https://claude.ai/artifact/6PhqpuQvWMcU6xBD7rBbRw).

## Decisions so far

| # | Decision | Source |
|---|---|---|
| D1 | Specify the coaching loop (card + triggers) first | Imran, 2026-10-31 (decision card) |
| D2 | Surfaces: VS Code, Claude Code and GitHub Copilot CLI, all from the start | Imran, 2026-10-31 (decision card) |
| D3 | Frequency: at most one in-the-moment tip per session, a small daily cap, per-trigger cooldowns | Design > Coaching > Guardrails (recorded owner ruling) |
| D4 | Only the person sees their data; AIQ never posts anything others can see | Imran, 2026-10-31 (privacy rule) |

## Part A — Functional specification

### Problem
Developers are expected to let agents do more of their work, but most still use AI one prompt at a
time. They repeat habits that cap what agents can do (re-prompting without new information, pasting
context by hand, rewriting agent output instead of fixing the instruction) and get no feedback while
it matters. Reports a week later don't change habits; feedback during the task does.

### Target users & personas
- **Dev, rung 1–2 (primary):** uses completions and chat; rarely delegates whole tasks. Motivated
  (self-selected), short on time, wary of being watched.
- **Dev, rung 3 (secondary):** delegates tasks to a CLI agent; loses time to rework and missing context.
- PMs are out of this spec (their surfaces are summary-first).

### Core scenario
Priya asks the agent to fix a failing test. After her third rephrase with no new information, AIQ
shows one card at the agent's next pause: "You've rephrased this three times without new
information. Try giving it the failing test output and the expected result." It has a one-click
"insert starter prompt" action and links to the three prompts as evidence. She uses it, the agent
fixes the test, and over the next week AIQ sees her include test output up front in most similar
tasks. It records the habit as adopted, privately.

### In scope / Out of scope (explicit non-goals)
In scope: detect → choose → show → collect feedback → check adoption, for VS Code, Claude Code and
GitHub Copilot CLI;
the coaching card; trigger rules (condition, cooldown, surface, priority); the interruption budget;
per-person tip ranking and suppression from feedback.

Out of scope: rung model scoring (separate spec; this spec consumes "a rung estimate is available or
not"); AIQ home views; weekly summary; browser and closed-app surfaces; PM pack; any sharing; any
org or aggregate reporting; authoring UI for packs.

### Conceptual domain model (bounded context: Coaching)
Ubiquitous language:
- **Task**: one unit of work the person does with AI, grouped from events (prompt, tool calls, edits).
- **Signal**: a fact a detector derives from a Task ("3 re-prompts, no new context").
- **Opportunity**: a habit the person could change, tied to one or more Signals and a target rung.
- **Environment blocker**: a cause outside the person (missing instructions file, no tool access).
  Never counted against them and never coached as a habit.
- **Tip**: pack content that addresses one Opportunity type with one Try-this step.
- **Coaching card**: one Tip shown for one Opportunity, with evidence, at one moment, on one surface.
- **Trigger rule**: when a card may appear (condition, cooldown, surface, priority).
- **Interruption budget**: the person's remaining allowance of in-the-moment cards.
- **Habit**: the target behavior an Opportunity points to; **Adopted** when observed per the
  adoption rule (pending decision).
- **Feedback**: helpful / not helpful / snooze / don't show again, plus optional reason.

Aggregates and the invariant each protects:
- **Coaching session** (root: the person's session on a surface): never more than the budgeted number
  of in-the-moment cards; never two cards at once.
- **Habit track** (root: Habit per person): adoption status changes only from observed behavior,
  never from self-report or a single task.
- **Tip preference** (root: Tip type per person): a "don't show again" is never overridden by ranking.

Value objects: Signal, Evidence link, Feedback, Trigger rule.

### User stories & acceptance criteria
_To be written after the remaining decisions (agent choice, adoption rule)._

### Non-functional requirements (ISO/IEC 25010)
_Draft pending; known constraints from the brief: fast path decides within seconds; no measurable IDE
slowdown (CPU, memory, latency budgets to set); all data stays on device; every card keyboard and
screen-reader accessible._

## Part B — UX specification
_Pending Part A._

## Part C — UI specification
_Pending Part B._

### Surface parity (two CLI agents)
The loop, card content, budget and adoption rule are identical on every surface; only the collector
and renderer differ. Interruption budget and cooldowns are per person across surfaces, not per
surface, so using two agents never doubles the tips. Where a surface cannot show a card mid-task,
the card waits for that surface's next pause; the spec does not lower the bar for that surface.

## Flagged risks & residual unknowns
- Whether each CLI agent's hooks can show a message to the person mid-session (vs only at session
  end or in a status line) is unverified for both Claude Code and Copilot CLI; Phase 0 spike item.
- Hook coverage and latency per surface are unproven until the Phase 0 spike.
- Grouping events into Tasks is a load-bearing assumption with no evidence yet.

## Gate record
_Not yet run._
