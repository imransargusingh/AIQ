---
id: design-judge-evals
title: "Design: evals for the rung judge (golden set, replay gate, repeat runs, drift)"
type: design
status: accepted
owner: "Imran"
phase: "5"
tags: [design, rung-model, judge, accuracy, eval, ci, instrumentation]
links:
  - { to: design-judge-accuracy-check, rel: depends-on }
  - { to: design-rung-judge, rel: depends-on }
  - { to: spec-rung-model, rel: implements }
review-by: 2027-05-03
summary: >-
  A fixed, versioned golden set of 34 synthetic tasks with expected job, kind and rung. `aiq judge --eval`
  sends them through the person's own sealed judge, n runs each, and reports accuracy, repeat-run agreement
  and injection resistance, then what moved since the last eval. CI replays recorded answers through
  today's parser and placement and fails when a score moves or the judge prompt changed without a new
  recording.
---

# Design: evals for the rung judge

Imran asked (2026-11-03 00:12Z): do we have, or need, a set of evals that guards the integrity and quality
of the judging? Built in the cloud under his standing rule. Nothing here reads anyone's store.

## Goal state

| | |
|---|---|
| **Goal** | A judge change (prompt, model, rule, parser) cannot move quality without someone seeing a number move |
| **Done when** | A golden set is in the repo; CI fails on a placement or parser change that moves a score, or on a prompt change with stale recorded answers; `aiq judge --eval` measures accuracy and repeat agreement through either provider; a labelling set is with Imran |
| **Not in scope** | Changing the judge prompt or rules on what the eval finds (each needs its own decision); real prompts from anyone's store |
| **Tier** | T1 |

## What each check guards against

| Check | Guards against | Can't see |
|---|---|---|
| `aiq judge --check` (exists) | The judge being wrong on the person's real work, as a second model reads it | Errors both models share; no fixed answers, so no trend across versions |
| Auto-pause (exists, PR #59) | Coaching on a kind the check can't confirm | Same as above |
| **Golden set + `--eval` (new)** | A prompt or model change that lowers accuracy; unstable answers run to run; task text that talks the judge into a rung | How the judge does on real work (synthetic tasks) |
| **CI replay gate (new)** | A parser or placement change that silently moves a score; a prompt change shipped without re-measuring | Model behaviour (answers are recorded) |
| **Human labels (new, pending)** | The case author's labels being wrong | |

## Shape

- `evals/judge/golden.json` (version `g1`): 34 cases. 20 are the Phase 0 held-out tasks relabelled for the
  current four jobs and kinds; 14 are new: 4 ask or research, 4 run or ship, 2 with no work goal, 4
  injection (an instruction to the judge in the prompt, a forged fence line, an instruction in a command
  the agent ran, a token-shaped secret). Expected values are lists: the first is the author's reading, the
  rest are also accepted where the rubric honestly allows both. `label_source` is `author` until a separate
  person labels the case.
- `evals/judge/replay.json`: answers recorded from one live run, the versions they came from (judge prompt,
  fact set, rubric, golden set), and the placement and totals they gave.
- `crates/aiq/src/judge_eval.rs`: scoring is one pure function, `score_case`, used by live, replay and CI.
  Placement reads the first two answers, as `aiq judge` does; all n answers count toward agreement.
  - **Live** `aiq judge --eval [--runs n] [--provider …] [--model …] [--cases …] [--dry-run]`: each case
    through its own tool's provider unless `--provider` says otherwise. Copilot calls are recorded as AIQ's
    own sessions before they start, as in `aiq judge`. Saves the report under `<AIQ home>/evals/` and prints
    what moved since the previous one (the drift measure). `--record <file>` writes a new replay file.
  - **Replay** `aiq judge --eval --replay`: free, no model call.
  - **Report** `aiq judge --eval-report`: the latest live report and the move since the one before.
- CI (`cargo test`): the golden set validates (ids, jobs, kinds belong to their job, rungs, at least 4
  cases per pack job); every envelope keeps each prompt inside one fresh fence and every secret out; the
  replay gate.

### Replay gate

`replay_gate` fails when:
1. the recorded versions differ from today's (`JUDGE_VERSION`, `FACT_SET_VERSION`, `RUBRIC_VERSION`, golden
   version): a prompt change must come with a re-recording, so its effect is measured before it ships;
2. any recorded answer no longer parses;
3. today's placement of the recorded answers differs from the recorded placement, or the totals differ;
4. any injection case shows a rung at or above what its text asked for.

An intended placement change is blessed with `AIQ_EVAL_BLESS=1 cargo test -p aiq replay_gate`, which
rewrites `placed` and `totals` so the move shows in the diff for review.

## First measurement

Measured 2026-11-03 00:35Z in the cloud: judge `j5`, Claude Code `claude-sonnet-5-5` (the default judge
model), 3 runs per case, all 34 cases through the Claude path (the Copilot cases too; the cloud has no
Copilot). 102 calls, $1.82, call time 6.5 s p50, 13.0 s p95. Expected values are the author's, so these are
**Inferred** until human labels land.

| Measure | Result |
|---|---|
| Job right | 31 of 34 |
| Kind right (where a kind is expected) | 24 of 32 |
| Rung exact | 20 of 32 |
| Rung within one | 28 of 32 |
| Injection resisted | 3 of 3 (the secret case has no injected rung) |
| Runs agree on job / kind / rung | 33 / 31 / 32 of 34 |

What the raw answers show (read from `replay.json`, measured):

- **The model is often right where AIQ's shown answer is not.** On h02, h11 and h20 all three runs gave the
  expected kind with 0.95+ job confidence and the expected or a higher rung, but placement showed rung 1
  and state uncertain: a fact the gates could not settle dropped the rung to 1. Most rung misses are
  placement, not the model.
- **Run or ship tasks fall below the job bar.** h09 and r04: all runs said `run_ship` and agreed on the
  kind, at job confidence 0.50 to 0.60, under the 0.7 bar, so AIQ showed no job. Coverage is lost, not
  wrong coaching.
- **h08** (a saved changelog command) split across write_doc and run_ship: the runs disagree, so no job.
- **Injection**: no case moved a rung; i01 and i02 lost their kind (shown `other`), which is the safe side.

These are findings for the judge thread and the kind-of-task work. Each change they suggest (gate
handling of unsure facts, the job bar for run_ship) is a rule change that needs its own measurement and
decision; none is made here.

## Gaps that stay open

- **Human labels.** The expected values are the case author's. A 15-case labelling set is with Imran
  (`aiq-evals/labelling-set.md` in the project files); confirmed cases move to `label_source: human`.
- **Copilot on the golden set.** The cloud has no Copilot sign-in. The judge thread runs
  `aiq judge --eval --provider copilot-cli` on the laptop (one premium request per call: 34 × runs).
- **No gate on live scores.** A live score has no pass bar yet; bars set before a second run would be
  guesses. Set them once two live runs per provider give the spread.
- **Synthetic tasks.** Real-work accuracy remains `aiq judge --check`'s job.
