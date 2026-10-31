---
name: session-profiler
description: Profile one or more pack-consuming repos' Claude Code and Copilot CLI sessions from their local telemetry and produce a findings table and a fixes table for performance, efficiency, task adherence, parallelism and cross-harness coordination — the continuous-improvement loop for how the pack performs on every model and harness. Use weekly, after any session that felt slow or drifty, and before/after a model or harness change.
runs_as: Coordinator
---

# Skill: /session-profiler

Measure how the pack's sessions **actually ran** — across harnesses (Claude Code, GitHub Copilot CLI) and model families — and turn the measurement into a **findings table** (what happened, with evidence) and a **fixes table** (which pack surface owns the control). This is `/dream` pointed at *performance, efficiency and adherence* instead of defects: the deterministic part reads the telemetry every harness already writes to disk; the reasoning part classifies drift, tangent and ceremony with evidence; the human decides what to tune. Nothing here reasons about how a session *probably* behaved — that is E15 pointed at runtime, and it is exactly the failure this skill replaces (`instrumentation-over-inference.md` IO1/IO5).

**What it is for.** (1) Performance and efficiency — cost per turn, context growth, cache share, tail latency. (2) Repeat mistakes and guesses when the knowledge was at hand — re-reads, skill re-injection, instruction files fetched into a context that already carried them. (3) Parallelism — sub-agent fan-out against the declared tier, runaway delegations, converge nudges, and multi-session / multi-harness coordination (overlapping sessions in one checkout, worktree use). (4) Task adherence — goal-state presence, ceremony above tier, cap firings. (5) Cross-model, cross-harness parity — the same metrics by model family × harness, so guidance is tuned from measured drift rather than a feeling that one family "drifts more". (6) Reasoning-model drift — the compare view is what lets the pack keep the power of a newer model while its controls and gates absorb the extra latitude.

**Spine:** the Rigor Protocol on the telemetry corpus, weighted to **Stage 3 EVIDENCE** (measured, cited, per turn) and **Stage 4 DISCONFIRM** (the Simplifier strikes findings that are not load-bearing; every proposed fix names its control). **Authority:** `instrumentation-over-inference.md` (IO1–IO13), `communication-and-task-discipline.md` (CT19–CT25 — the goal-state, tier and fan-out fields the profiler reads), `execution-graph-optimization.md` (GO7 delegation contract, GO19 tier per phase), `session-worktree-discipline.md` (WT1a), `continuous-improvement.md` (CI1–CI12 — a finding lands as a class with a control). **Mode:** deterministic harness in Peer Mode; the human is the gate. **Lead:** the **AI Systems Engineer** (model tiering, inference cost, prompt-as-contract) composing the **SRE** (cost axes, latency, telemetry), the **Simplifier** (strike noise, soft veto) and the **Test Architect** (a fix without a control is prose — CI6).

## Grounding (first action)
CO-S0 applies first — the sentence is `reference/co-s0.md`. `python3 docs/ai-forward-pack/scripts/audit-log.py start --session <id>` (IO1 — the closing entry records the duration). Then read: the defect-class register (`docs/lessons/defect-classes.md`, the **CTX-\*** classes), the most recent `docs/profiles/PROFILES.md` rows (re-measure known findings), and the fix catalog (`python3 docs/ai-forward-pack/scripts/session-profile.py fixes`). Traverse the graph one hop from `design-session-profiler` (V15) and cite it.

## Input
One or more repo paths that have the pack installed (`--repo <path>`, repeatable — the first receives the profile), an optional window (`--days N`, default 30), an optional harness filter, optional session ids, and an optional focus (*"the Copilot sessions that felt slow this week"*, *"compare opus vs gpt on implementation turns"*). No input at all means: every session of the current repo in the last 30 days.

## Cast
- **Peers:** AI Systems Engineer (lead), SRE & Diagnostician, Domain Researcher (a store schema is read, never recalled — RIG-D).
- **Adversaries:** **The Simplifier** (soft veto: a finding that changes no decision is struck), **Test Architect** (hard veto: a proposed fix with no named control and no red-first observation does not enter the fixes table), **Tech Lead** (a fix is smallest-correct; ceremony added to remove ceremony is rejected).

## Flow (Rigor Protocol, specialized to telemetry)

**Stage 0 — Interdict the rush.** Do not tune guidance, controls or gates from a feeling. Run `session-profile.py --repo … discover` first and confirm the sessions you think you are talking about are the ones in the window. A session store that is missing or unreadable is **reported** (`not recorded`), never estimated around.

**Stage 1 — OPEN.** Name the operator questions this pass answers (IO2): cost per turn and its cache share · context at the start and end of each turn, and whether it was ever compacted · time-to-first-token p50/p90 for the main agent · sub-agents convened per turn against the declared tier and fan-out cap · tool calls per delegation and converge nudges · re-reads and paged-output views · skill invocations per session · goal-state and tier presence on substantive turns · overlapping sessions in one checkout · the model-family × harness comparison. Write the goal state (CT19) with **Tier: T0 · Fan-out cap: 0** — this skill convenes no sub-agents of its own.

**Stage 2 — INTERROGATE (measure).** `python3 docs/ai-forward-pack/scripts/session-profile.py --repo <a> [--repo <b>] --days N profile --session-id <id>` writes `<profiles>/<sp-id>/profile.json` + `profile.md` and the index row. `<profiles>` is the tracked `docs/profiles/` only in the pack source repo or with `--publish` (in a consuming repo, only if the user asked), else the untracked `<git-dir>/ai-forward/profiles/`; strings are scrubbed first (FR-103, FR-104). Read the findings table. Every finding carries an id (**SP-01 … SP-28**), a severity on the pack scale, a confidence, per-turn evidence and the fix ids it maps to. **Verified** findings are read from the store (tokens, requests, sub-agent counts, prefix blocks). **Inferred** findings are heuristics over text (goal-state presence, "council above tier", the family comparison) — for each one, **open the turn** in the transcript and confirm or strike it. This is the REM step: the model classifies *drift · tangent · ceremony · legitimate* per flagged turn, with the prompt and the first reply quoted as evidence; the script never judges intent.

**Stage 3 — EVIDENCE (compare).** `session-profile.py … compare` gives the model-family × harness view: requests, cache-read, output and reasoning tokens per turn, cost where the harness records it, TTFT p90, context at turn end, wall clock, and **drift indicators per turn** (sub-agents + re-reads + skill repeats + missing goal state + fan-out without tier + converge nudges + cap firings). **A family with 2× the drift of another is a tuning finding (SP-14) only on like-for-like turns** — state the turn mix, and never tune on fewer than three comparable turns per family. Check coordination: SP-15 (overlapping sessions with the same cwd) against `coord-core.py worktree list`; a session that wrote in the primary checkout beside another is a WT1/WT4 finding. `session-profile.py compile` reads the compile stage (CO-S0): edit distance p50/p90 per template and the `compiled: false` share (SP-27/SP-28); message, doorbell, board and refused-dispatch counts read `not recorded` until a reader exists (IO8).

**Stage 4 — DISCONFIRM (the gate).** Adversary Mode. The Simplifier strikes every finding that would change no guidance, control or gate. The Test Architect requires each row of the fixes table to name **the pack surface, the control that fails on recurrence, and how it was observed failing** (the profiler re-flagging the finding id is the recurrence signal; a control that cannot be seen red is prose). The Tech Lead rejects a fix that adds ceremony to remove ceremony. Findings survive only with evidence attached; a struck finding is listed as struck, with the reason.

**Stage 5 — CONVERGE.** Produce, in the response and in the profile: **(a)** the findings table (id · severity · confidence · finding · session · evidence · fix); **(b)** the fixes table (fix · what · where in the pack · control that fails on recurrence · findings it closes), starting from the catalog and extended with repo-specific rows; **(c)** the model-family × harness table with the per-family tuning notes — which guidance, control or gate to adjust for that family and why, from the numbers; **(d)** the coordination notes (overlaps, worktrees, harness mix). Register each new shape as a **class** in `docs/lessons/defect-classes.md` (CI1) — the CTX-\* family is the home for context, ceremony and coordination shapes. Then stop: the profile is data for `/dream`, and the fixes are proposals for the human.

**Close with the status table (mandatory).** Completed / Remaining / Best next action.

## Output artifact
- `<profiles>/<sp-id>/profile.json` (canonical) and `profile.md` (the tables) — written by the script; plus one row in `<profiles>/PROFILES.md`.
- The response: the three tables, the struck findings with reasons, and the per-family tuning notes.
- New or updated **CTX-\*** classes in the register; an audit entry (the script appends one; the skill's closing `audit-log.py append … --tier T0 --fan-out 0` records the goal state).

## Definition of done (exit gate)
- [ ] `discover` was run and the sessions in the window were named before anything was measured.
- [ ] `profile` ran; every number in the tables is read from a store or labelled *est.*; every missing measurement reads `not recorded` (IO8).
- [ ] Every **Inferred** finding was confirmed or struck against the transcript; struck findings are listed with reasons.
- [ ] `compare` ran; any SP-14 claim states the turn mix and rests on ≥3 comparable turns per family.
- [ ] Every fixes-table row names the pack surface **and** the control that fails on recurrence; the Test Architect's veto is cleared by the reviewer, not the author.
- [ ] Coordination checked: SP-15 overlaps reconciled against the worktree list and `coord session list`.
- [ ] New shapes registered as classes; the audit entry carries `goal`, `done_when`, `tier` and `fan_out`.
- [ ] Status table emitted.

## Documentation & discoverability (last action)
Each `profile.md` carries V2 frontmatter and links `relates-to` the design node, so a profile is discoverable in the Explorer; `docs/profiles/PROFILES.md` is their index. The script writes both; you run `python3 docs/ai-forward-pack/scripts/docs-graph.py derive` after it. If this run settles a durable decision about profiling itself, capture it as a decision note (V17) and run `python3 docs/ai-forward-pack/scripts/docs-graph.py derive`. The design node is `docs/design/session-profiler.md`.

**Audit (last action).** `python3 docs/ai-forward-pack/scripts/audit-log.py append --shortname "session-profiler-<sp-id>" --session "<id>" --skill session-profiler --kind skill --prompt "<verbatim>" --summary "<findings count, top fixes>" --artifact docs/profiles/<sp-id>/profile.md --goal "<goal>" --done-when "<done when>" --tier T0 --fan-out 0`.

**Handoff:** → `/dream` (mines `docs/profiles/` into control-upgrade proposals) · → `/apply-learnings` (push approved fixes to other pack-consuming repos) · → `/extendaibundle` (a fix that changes the pack itself) · → `/investigate` (a finding that is a defect in running software, not a tuning matter).
