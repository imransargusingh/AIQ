---
id: design-day-view
title: "Design: Day view, live rung trend, and goals inside a task"
type: design
status: proposed
owner: "Imran"
phase: "5"
tags: [design, aiq-home, tasks, sessions, live-trend, collectors]
links:
  - { to: spec-aiq-home, rel: implements }
  - { to: design-aiq-home-app, rel: depends-on }
  - { to: design-project-filter, rel: relates-to }
  - { to: design-copilot-vscode-slice, rel: relates-to }
review-by: 2027-05-03
summary: >-
  Imran asked whether AIQ should reason about sessions of intent instead of tasks. Measured on his store,
  the task (one session cut at 30 idle minutes) already separates goals well, so tasks stay the unit and
  three views are added on top. A, a Day view on the Tasks page (one row per session, built in this change).
  C and D, a live rung trend on Home and in the status line, which first need AIQ to record the agent's
  actions live (step one, its own change). Goal changes inside a task are a later step.
---

# Design: Day view, live rung trend, and goals inside a task

Imran, 2026-11-03: *"I wonder if a better unit to reason about is 'session' (... a time-bounded set of tasks
that represent a user's intent)."* Then: *"we could still provide users with a task-oriented view in
near-real time with some indication of what the session is trending towards."* He liked options A, C and D
from the rendered options (https://claude.ai/artifact/7LGhvfgLTQTLkUq8w5AsB8) and asked for row context in A.

## Goal state

| | |
|---|---|
| **Goal** | Show a person's day as sessions and tasks (A), and later where the open task is heading (C, D) |
| **Done when** | A ships on the Tasks page with row and block details on hover and focus; this note records why tasks stay the unit and the order of the later steps |
| **Not in scope** | Building C or D; splitting a task into goals; reading VS Code chat-panel files |
| **Tier** | T1 (UI view over existing data; no store change, no new data leaves the device) |

## Evidence: tasks stay the unit (measured)

Read-only on Imran's laptop store, 2026-11-03: 1,708 prompts, 293 sessions, 810 tasks, 532 judged. These
counts include 7 sessions stored twice (live and imported), which "Stop counting sessions twice" fixes:
805 tasks become 785.

| Question | Measured |
|---|---|
| Sessions holding more than one task | 141 of 293 (48%) |
| Sessions with 2+ judged tasks that mix jobs / kinds | 64 of 98 (65%) / 82 of 98 (84%) |
| Back-to-back tasks in one session with the same job and kind | 57 of 335 (17%); two random tasks in one folder: 13% |
| Next task in another session, same folder, within 4 h, same job and kind | 26 of 137 (19%) |
| Tasks running at the same time as another session | 91 of 810 (11%) |
| Gaps between prompts inside a task of 10 to 30 minutes | 415 of 898 (46%) |
| Hand read of 30 tasks with 4+ prompts: one goal / two / three or more | 21 / 7 / 2 (one reader; 9 of 30 from one session) |

So: the 30-minute cut already separates goals (17% vs 13% chance). Goals rarely span sessions. The gap is a
goal change inside a task, in about 3 of 10 long tasks; long tasks are few, so roughly 3 to 5% of all tasks
(Inferred from the sample). A shorter idle cut would split goals more often than it would catch a change.

## Decisions

| # | Decision | Why |
|---|---|---|
| D1 | Tasks stay the capture and judging unit; sessions and goals are views on top | Numbers above. Replacing tasks would re-judge everything (premium requests) for a few percent of tasks |
| D2 | A: Tasks page gets a List / Day switch. Day shows one row per tool session for one day; each task is a block placed by time, coloured by its judged rung, with a tick per prompt | Answers "where did my day go, and at what rung?" The list stays the default, so nothing is replaced |
| D3 | A row names the tool, project, time span and prompt count. Hover or focus shows the session summary: time span, tasks, prompts, kinds of task, how many are not judged yet, the rung most of its tasks sit at | Imran: "the user would need more context on what each row is referring to" |
| D4 | A block's hover or focus names the kind, time, prompts and rung in words, with "could be" when the judge saw a next rung. Enter or a click opens the existing task page | Rung is never colour alone (U-standard); the task page already holds the detail |
| D5 | The Day view uses the same task rows as the list (same rule, same project and job filters) | The two can never disagree. It inherits the duplicate fix when that lands |
| D6 | The snapshot now carries each task's `session` and its prompt times (`prompt_ms`). Prompt ids and text still never reach the page | Lanes need the session; ticks need the times. Nothing new leaves the device |
| D7 | The day shown first is the newest day with a task; Earlier / Later step through days that have tasks | No empty days to page through |

## Later steps (not built here)

**Step 1: record the agent's actions live (its own thread, "Capture the agent's actions live").** C and D need
to know what the agent did each turn. Today only imported history has that (the import summariser in
`crates/aiq/src/import.rs`: tool counts, commands, files, sub-agents per turn); live stop events carry none
(measured: 0 of the live stops, all 1,629 imported stops). Design agreed with Imran on 2026-11-03:

- After each turn ends, the core (never the hook, rule RC; opening the transcript in the hook cost ~0.4 s on
  the laptop) reads only the new part of the tool's own log, read-only. Claude Code: the `transcript_path`
  already in every event. Copilot CLI and VS Code agent mode on the Copilot target: the Copilot session log
  the core already reads at each turn end (`crates/aiq-core/src/copilot_log.rs`, PR #89).
- Reuse the import summariser so live and imported turns match. One append-only row per turn, counts and
  tool names only (no command text, no file names), behind a switch in Settings > What AIQ records.
- Measure read time per turn from the first build. A failed read is "not recorded" plus a counter, never a
  guess. Fix once, in the shared summariser, that Copilot's `files` stays empty when `apply_patch` runs.
- Per-tool hooks were rejected: Copilot hooks run synchronously (ADR-0005), about 65 to 70 ms per tool call.

**Coverage.** VS Code agent mode on the Copilot target is covered by the Copilot log (6 of the 7 sessions
stored twice were VS Code live sessions with Copilot CLI session ids). Not covered: VS Code agent mode on the
Local target (runs no hooks), ask mode, inline chat. VS Code keeps chat-panel files per workspace
(`chatSessions`, now `.jsonl` in Insiders, a format change since 1 November). Reading them is possible but is
its own reader for an undocumented format; a field-names-only shape check comes first.

**Step 2: C and D.** C, a "Right now" card at the top of Home while a task is open: a step line of the estimated
rung after each turn, the rung the task could reach, and the tip at the turn it applies to. D, the same trend in
the existing status-line tip. Both are an estimate until the task ends; the judge's rung replaces it. Imran gave
the go on 2026-11-03 ("build C and D"); built as designed in `docs/design/live-rung-trend.md`.

**Step 3: goals inside a task.** The judge marks where the goal changes, in the same call it already makes, and
names a kind per part; stored as append-only segment rows, never by rewriting tasks. Before it goes live,
hand-label goal changes in 50 to 80 tasks from many sessions and switch on only what beats the plain time gap.
The Day view then draws a dashed line inside a block where the goal changed.

**Parked: babysitting tip.** Measured: 14 of 810 tasks were mostly status checks, 11 of them from one session of
scheduled prompts, none live (detector precision 19 of 20 prompts, 13 of 14 tasks). Too concentrated for a tip.

## Surface list (A)

store (unchanged) → read model `aiq_home::TaskRow` serialises `session`, `prompt_ms` → `aiq.home.v1`
snapshot → `types.ts` `TaskRow` → `day.ts` (lanes, days, axis) → `pages/DayView.tsx` + Tasks switch → styles.
Fixtures regenerated from the Rust read model.

## Tests

- `apps/home/src/day.test.ts`: lanes per tool session in start order, kinds and usual rung, days follow the
  core's offset, axis in whole hours and past midnight.
- `apps/home/tests/e2e.mjs` "Tasks Day view": a row per session, row summary and rung in words on focus,
  Enter opens the task, no axe violations, no page scroll at 720 px.
- `crates/aiq-home/tests/snapshot.rs`: the snapshot still never carries prompt ids (`members`).

## Risks

| Risk | Handling |
|---|---|
| Snapshot grows by one number per prompt | ~1,700 numbers on Imran's store; the 5,000-prompt performance test still passes |
| Duplicate sessions show twice in the Day view until the dedup fix lands | Same as the list today; fixed for both by "Stop counting sessions twice" |
| Hover-only detail | Every row and block is focusable and shows the same detail on focus; blocks carry a full `aria-label` |
