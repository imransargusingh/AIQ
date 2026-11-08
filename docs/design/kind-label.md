---
id: design-kind-label
title: "Design: label the kind of task with a small model"
type: design
status: accepted
owner: "Imran"
phase: "5"
tags: [design, coaching, kind-of-task, judge, copilot]
links:
  - { to: design-coaching-from-rungs, rel: depends-on }
  - { to: design-rung-judge, rel: depends-on }
  - { to: design-kind-of-task, rel: relates-to }
  - { to: design-judge-accuracy-check, rel: relates-to }
  - { to: privacy-and-terms, rel: relates-to }
review-by: 2027-05-02
summary: >-
  Next-step tips almost never fired because the word-match guess could not name a new task's kind
  (8 of 9 new tasks on 2026-11-02). When the word guess finds nothing, AIQ now asks a small model, through
  the tool that already saw the task and sealed like the judge, to name the job and kind from the first
  prompt, in the background. A confident label goes through the same next-step decision as a word guess,
  with its own daily budget and its own 8-in-10 self-check.
---

# Design: label the kind of task with a small model

Approved by Imran on 2026-11-02 ("let's go with your recommendation", thread "Find out why no tips have
fired").

## Problem (measured)

From the tip decision log and store on the laptop, 2026-11-02:

- Since the tips went live: 29 prompts, 9 new tasks. 8 of 9 stopped at the kind guess (`no_guess`).
- Best word overlap (Jaccard) of a real first prompt with any of 122 judged tasks: 0.03 to 0.15. The bar
  (`[coach] match_threshold`) is 0.45; only 2 to 3 word prompts reach it.
- Leave-one-out over the 122 judged tasks: the live rule 9 of 13 right; TF-IDF at most 15 of 32. No word
  rule reaches the 8-in-10 self-check bar.

## Decision

1. **Word guess first.** `coach::guess_type` is unchanged and free. When it names a kind, nothing is sent.
2. **Otherwise ask for a label, off every clock that matters.** The core (`Service::next_rung`) calls a
   labeller the `aiq` binary wires in; it starts `aiq label --task … --surface … --session …` detached, on
   its own thread. The hook already returned (ingest has no reply), and the core never waits on a model
   (C9). Hook time is unchanged: nothing on the hook path changed.
3. **One sealed call, same provider, the person's own sign-in.** `aiq label` reuses the judge's
   `ClaudeCli` / `CopilotCli`: Claude Code `claude -p` in an empty folder, no tools, hooks off, no saved
   session; Copilot CLI with its own empty `COPILOT_HOME` (in `label-work`, not `judge-work`, which the
   judge sweeps under its lock), no tools, no MCP servers, its session id recorded as AIQ's own (US-11).
   Models: Claude `haiku` (`kind-label-claude-model`); Copilot the label model if set
   (`kind-label-copilot-model`), else the judge's Copilot model, else Copilot's default.
4. **What is sent:** only the first prompt, secrets removed again, cut to 4,000 bytes, inside a per-call
   fence. The system prompt is the judge's own JOB and TASK TYPE text, cut from `rung::SYSTEM` at run time,
   so the label and the judgment it is checked against share one vocabulary.
5. **What is stored:** one append-only `kind_label` row per call (schema v8): ids, outcome, job, kind, both
   confidences, model, request time, call time, tokens, cost. Never text.
6. **Decision.** `aiq label` tells the core (`Request::KindLabel`, no reply). `Service::on_kind_label`
   drops the label when the session has moved to another task, the task already has a guess, or
   `label::accept` fails (needs a pack job, a named kind, and both confidences at least 0.8, the coaching
   bar). Otherwise it writes a `type_guess` row with `method = label` and runs the same decision as a word
   guess: the kind's coached move, the accuracy pause (K6b, PR #59), tip answers, cooldown, habit, then the
   trigger engine's budget and quiet period. The card waits for the turn's end (Claude Code Stop, Copilot
   CLI status line) as before.
7. **Own self-check.** D10 (`check_guesses` against the later judgment) now runs per method. Labels turn
   off by themselves below 8 in 10 right with at least 10 checked, without touching word guesses, and the
   other way round. `next-rung-tips on` overrides both, as before.
8. **Switches and budget.** Labels run only while background judging is on (PR #75) and
   `kind-label` is not `off`. Their own daily budget, `kind-label-daily`, default 20, 0 to 500, counted
   from `kind_label` rows since local midnight. It is separate from the judge's budget because a label is
   one call and a judgment two, and the person can see and cap each. Set-aside and automated sessions
   never reach the decision, so they are never labelled.

## Measured vs guessed

| Claim | Status |
| --- | --- |
| Word guess misses 8 of 9 new tasks | Measured on the laptop, one day |
| Hook time unchanged | Inferred from the code: no hook-path change; the hook budget block in `aiq status` will show it |
| A label returns before most first turns end | Inferred: a sealed Copilot judge call took ~11 s (measured); turns ran 18 s to 42 min that day. Each `kind_label` row records `wait_ms` (request to answer) in `label.log`, so this becomes measured after install |
| Label accuracy against the judge | Measured 2026-11-08 (see Accuracy): live 6 of 10; with troubleshoot labels not coached, 8 of 10 confident on the 20 newest tasks |
| Default budget 20 a day | Guess (about 9 new tasks a day, one day measured) |
| Copilot model | Labels now use the analysis models (gpt-6.1-sol first). Luna with a rewritten prompt measured 8 of 14 confident right |

## Accuracy (measured 2026-11-08)

Live, from Imran's laptop (read-only): 10 label guesses checked against the analysis, 6 right, so the
self-check held label tips from 11-04. Then `label_eval.py` (in the project files, sealed like `aiq label`)
labelled the 20 newest analyzed Copilot tasks once per variant, through his Copilot (about 88 premium requests
including a crashed first run):

| Variant | Answered | Kind right | Confident | Confident right |
| --- | --- | --- | --- | --- |
| label-1 prompt, gpt-6.1-sol (today) | 18 | 10 | 18 | 10 |
| rewritten prompt ("name the job the whole task ends in"), gpt-6.1-sol | 20 | 8 | 10 | 5 |
| rewritten prompt, gpt-6-luna | 20 | 9 | 14 | 8 |
| label-1 on gpt-6.1-sol, troubleshoot labels not coached | 18 | 10 | 10 | 8 |

The main miss: a first prompt that reports a problem reads as `ask_research/troubleshoot`, and the task then
fixes it, so the analysis says `code_change/fix_bug`. No prompt wording fixed it (the rewrite made those labels
unsure and broke others), so label-1 stays. 0.1.22 stopped troubleshoot labels coaching.

A confirmation run on 60 fresh tasks (17:04Z, 60 premium requests, label-1 on gpt-6.1-sol) disproved that
8 of 10: 13 of 29 confident labels right with troubleshoot left out (14 of 37 with it). The miss is wider than
troubleshoot. Any confident `ask_research` label was right 2 of 22 times (6 of 14 on the first 20). The task
the person starts with a question mostly becomes a code change. Labels naming any other job were right 12 of
15 (4 of 4 on the first 20), 16 of 19 together. So `label::NOT_COACHED_JOBS` keeps every `ask_research` label
from coaching. The rule was drawn from the 60, so the first 20 are its only out-of-sample check (4 of 4).
Coverage falls to about a quarter of new tasks (19 of 78). Word guesses and the analysis still coach questions.

| Rule | First 20 | Fresh 60 |
| --- | --- | --- |
| Every confident label (0.1.21) | 10 of 18 | 14 of 37 |
| No troubleshoot labels (0.1.22) | 8 of 10 | 13 of 29 |
| No `ask_research` labels | 4 of 4 | 12 of 15 |

Controls: `evals/label/golden.json` (59 synthetic first prompts) with recorded answers in `replay.json`
(`label_replay_gate`: a prompt change without a version bump and new recording fails; confident labels must
stay at 8 in 10 and cover a third of the set). `evals/label/measured.json` replays the real measured labels
(hashed ids, no text) through today's `accept` (`measured_labels_clear_the_bar`). `aiq label --eval` prints
whether the bar is cleared, names each miss, and exits 3 under the bar.

## Instrumentation

- `aiq status`: labels today of the budget, answered, confident, median call time, and labels checked
  against the judge (right of checked).
- `core.log`: `coach.guess` with `coach.outcome = label_requested | no_guess | decided | not_coachable` and
  `method`; `coach.label` with `label_not_confident | label_task_moved_on | label_self_check_off`.
- Counter `AIQ-COACH-LABEL-REQUESTED`.
- `label.log` (bounded): one line per label call or skip, with call time and wait time.
- `aiq export` includes `kind_label`.

## Failure modes

| Failure | Result |
| --- | --- |
| Tool missing, signed out, timeout, bad answer | Row stored with that outcome; no tip; counts against the budget |
| Core down when the label arrives | Row stays unused; nothing shown |
| Label after the session moved to a new task | Dropped (`label_task_moved_on`) |
| Label arrives after the first turn ended | Claude Code: the card shows at the next Stop of the same task. Copilot: the status line shows it |
| Labels wrong too often | Own self-check turns labels off below 8 in 10; word guesses unaffected |
| Process killed mid-call | Its Copilot home in `label-work` is swept by the next label run after 10 minutes |

## Next

- After install, run `aiq label --eval --sample 20` (about 20 premium requests on Copilot) for label
  accuracy before relying on live numbers, then read live labels against the judge in `aiq status`.
- If Copilot's default model is slow or costly, set `kind-label-copilot-model` to a smaller one after
  checking which models the person's Copilot offers.
- AIQ home does not show the label budget yet; `aiq status` does.
