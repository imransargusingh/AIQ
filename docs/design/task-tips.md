---
id: design-task-tips
title: "Design: the tip for one task, on its page"
type: design
status: accepted
owner: "Imran"
phase: "5"
tags: [design, coaching, tips, home]
links:
  - { to: design-tip-funnel, rel: depends-on }
  - { to: design-aiq-home-deferred-pages, rel: relates-to }
review-by: 2027-05-12
summary: >-
  Imran (2026-11-12) asked to see which tips were presented for a task. The store already linked each tip to its
  task; nothing showed it. The task page gains a "Tip for this task" card and each Tip history row opens its task.
  No schema change.
---

# Design: the tip for one task, on its page

**Ask (Imran, 2026-11-12):** "for a given task, is there a way to display what tips were presented for it?"
He picked the mockup plus a link from Tip history (decision card, 04:48Z).

## What was already stored (checked in code at f511ecc)

| Fact | Where |
|---|---|
| tip to task | `card_decision.signal_id` to `signal.task_id` (the task's first prompt event id, as Home names tasks) |
| shown, where, when | `card_delivery` (at most one per session: unique index) |
| opened | `card_open` |
| answer | `tip_feedback` by `card_id` (helpful, not_helpful, snooze, off; `no_answer` ignored) |
| why no tip | `tip_path`, newest step per task (since schema v11, 2026-11-08) |

## Read

`TaskDetail.tips` (crates/aiq-home/src/detail.rs):

- `shown`: Tip history's own rows whose `task_id` is this task, so both pages always agree. `TipRow` gains
  `task_id` and `answer`.
- `held`: the newest decided card for the task with no delivery; its newest decision's reason in words
  (`funnel::words`) and what helps (`funnel::help`, which gains `daily_cap`).
- `step`, `step_words`, `step_help`: the funnel's newest step for the task.

## Surface

One card between "How this task was done" and the prompts (clear of the Kind chip in the analysis card). One view,
in order: shown tips; else the held tip and why; else why no tip. Imported tasks say tips are for live work. An
open task whose label is still on its way says AIQ is still working out its kind, not that the label was lost.
A task with no recorded step says "AIQ didn't record why", never a guessed reason.

**Team tips.** The "Tool to try" line is read from the team file by `tip_id` when the page opens, as Tip history
does. The store keeps only the entry id and fingerprint; team tip text is never stored.

**Tip history.** Each row offers "Open its task" when the newest showing's task is still in the Tasks list.

## Proof

- Rust: `task_detail_shows_the_tasks_tips` (shown row equals Tip history, newest answer wins over `no_answer`,
  held reason, newest funnel step).
- Unit: `taskTips.test.ts` (view order, open-task label wording, imported, unrecorded, answer words).
- E2E: the three views in light, dark and contrast with axe; Tip history link renders the task page.
