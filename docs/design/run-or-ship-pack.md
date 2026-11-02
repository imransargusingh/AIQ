---
id: design-run-or-ship-pack
title: "Design: the Run or ship it pack"
type: design
status: draft
owner: "Imran"
phase: "5"
tags: [design, rung-model, judge, coaching, pack]
links:
  - { to: design-no-job-tasks, rel: depends-on }
  - { to: design-rung-judge, rel: depends-on }
  - { to: design-coaching-from-rungs, rel: depends-on }
  - { to: spec-rung-model, rel: implements }
review-by: 2027-05-02
summary: >-
  A fourth job pack for tasks whose result is a running, released or set-up system: restart, release,
  publish, deploy, run a job, connect a tool. The no-job survey found these are over half of the tasks
  no pack covers. Draft for Imran's decision; nothing is built yet.
---

# Design: the Run or ship it pack

Produced with AI-Forward `/design-slice`. Drafted before Imran's decision, under his standing rule: it is
cloud-only and changes nothing anyone sees. If he says no, this stays a draft and nothing is built.

## Goal state

| | |
|---|---|
| **Goal** | Coach the tasks where the person runs, ships or sets up a system, which no pack covers today |
| **Done when** | The judge names the job; a different-model check agrees on it at least 8 of 10; its tips can fire |
| **Not in scope** | A pack for chat or tests, planning, or brainstorming (each 3 of 30 or fewer); changing the other three packs |
| **Tier** | T1 (new job value, judge version bump, pack data; no schema change) |

## Grounding (measured unless marked)

- No job: 101 of 473 judged tasks. Of those, 76 had both runs say `other`. A different model agreed they
  are `other` 18 of 18 (check run 01a19d99). So these tasks are outside every pack, not misread.
- Survey of 30 of the 76 (`aiq judge --survey-other`, 2026-11-02 19:39Z): operate systems 17, set up
  tools 3, chat or test 3, plan or manage 3, other 2, learn or brainstorm 1, review or feedback 1. The
  kinds are Inferred (one model's reading, no person).
- Labels for operate systems: application restart troubleshooting, package application release, publish
  clean project repository, publish project source, refresh data and launch. For set up tools:
  authenticate tool connection, network access configuration.
- Scaled (worked out, not measured): operate systems plus set up tools is 20 of 30, so about 51 of the 76
  tasks (Wilson 90%: 52% to 79%, so 39 to 60), or about 11% of all judged tasks.

## Decisions

| # | Question | Decision | Why |
|---|---|---|---|
| P1 | What is the job? | **`run_ship`, "Run or ship it"**: the result is a system in a new state, not changed files and not an answer. Start, restart or fix a running app; package, release, publish or deploy; run a script, pipeline or data refresh; install, connect or configure a tool or environment | One job for both kinds the survey found. "Set up tools" is the same shape of work (act on a system, then check it took), and 3 tasks alone would never reach the 3-repeat coaching minimum |
| P2 | Where is the line with `code_change`? | If the person's goal is edited code or config in the repo, it is `code_change`, even if they then run it. If the goal is that something runs, ships or connects, it is `run_ship`, even if a file changes on the way | Matches how `code_change` already keeps incidental doc edits. The judge prompt states it in one line |
| P3 | Task types | `restart_fix` (get a stopped or broken thing running), `release` (package, publish or deploy), `run_job` (run a script, pipeline or data refresh), `setup` (install, connect or configure a tool), `other` | Covers every survey label for the two kinds. Few types, so the judge can name them confidently |
| P4 | Which facts? | The shared F4, F5, F6, F7, F12, F13. "The agent did it itself" reads **F14** (the agent read, searched, fetched or ran something), as Ask or research does, not F1 (the agent edited files). Sub-agents read F15 | Running commands is what the agent does in this job, and F14 is already measured from the log. No new fact, so no new log reader |
| P5 | Rungs for this job | 1 Assisted: you ask for one command at a time and run it yourself. 2 Directed: you say what should be running, where, and what must not break, and you check it took. 3 Delegated: you hand over the whole release or run with a done check, and the agent runs the commands itself. 4 Orchestrated: a written runbook, with an automated check between steps. 5 Factory: a versioned script or pipeline that others can trigger | The shared ladder, worded for this job (rung-model R5) |
| P6 | Tips (moves) | See the table below. All step 1 and 2 moves are job-specific. Step 3 and 4 reuse the `any` moves `written_brief` and `split_and_check` | The same pattern the other packs use |
| P7 | Safety | The check tip (F6) comes before the hand-over tips in pack order. The hand-over tip names a limit ("not production" or "stop before the publish") in its starter. No tip tells anyone to let an agent act on production unchecked | Mistakes here can reach live systems, which costs more than a wrong file. Ties go to the earlier move |
| P8 | Going live | Judge version **j5**. Only j4 judgments where both runs said `other` are judged again (76 tasks, 2 calls each, about 152 calls: under Imran's daily cap). Then `aiq judge --check` must agree on `run_ship` at least 8 of 10, with at least 10 checked. Until then the job's judgments are Uncertain and drive no coaching | The same bar as every other pack (rung-model US-7) |

### Tips

| Move | Step | Fact | Habit | Starter |
|---|---|---|---|---|
| `brief_run` | 1 | F4 (sure at 0.8) | Say what should be running, where, and what must not break | Goal: <what should be running or released>. Where: <app, environment, account>. Must not: <what must not break or change>. Then: <the task>. |
| `check_running` | 1 | F6 | Check it really runs before you move on | After <the step>, show me proof it worked: the status, the URL loading, the version number or the log line. |
| `hand_over_run` | 2 | F13 | Hand over the whole run with a done check and a limit | Do <the whole release or run>. Done means <the check that passes>. Stop and ask before <the risky step>. |
| `let_agent_run` | 2 | F14 | Let the agent run the commands | Run the commands yourself to <goal>. Tell me each command and what it printed. |

Step 3 and 4 use the existing `written_brief` (a runbook is a written brief) and `split_and_check`.

## Change surfaces (E7)

| Surface | Change |
|---|---|
| Contracts | `judge::JOBS` and the task-type table gain `run_ship`; `pack.rs` accepts `run_ship` as a move job |
| Domain | `PACK_JOBS`, `job_label`, judge `SYSTEM` (job line, P2 boundary line, task types, rung lines); F14 and F15 routing extends from Ask or research to `run_ship`; judge version j5 and the re-judge rule (P8) |
| Pack data | `bundled.toml`: the four moves above |
| Check | The second reader's job list gains `run_ship` (it shares the judge's text, so one place) |
| Home UI | `format.ts` job name; Settings tip list by job |
| Store | None. The job is a string in the judgment's JSON |

## Questions for Imran

Only one: whether to add this pack (the card in the thread). The name "Run or ship it" is a draft he can change.

## Gate record

`GATE design-slice · 2026-11-02 · Simplifier, Test Architect, Data & Persistence, Security (seats played by Claude; not independent) · pass (draft)`
- Simplifier: no new fact and no new log reader. One job covers two survey kinds.
- Test Architect: build red-first tests for the job list, the F14 routing for the new job, the re-judge rule, and the pack's move validation.
- Data & Persistence: the job is a value in the existing judgment JSON. Old judgments keep their job, and only the 76 are judged again.
- Security: P7. The tips never widen what the agent may do without a check.

## Status and next action

| | |
|---|---|
| **Completed** | Survey; this draft |
| **Remaining** | Imran's decision; then build, re-judge, check |
| **Best next action** | If yes: build P1 to P8 in one PR, then the laptop re-judges and runs the check |
