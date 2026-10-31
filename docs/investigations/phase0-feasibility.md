---
id: spike-phase0-feasibility
title: "Spike: Phase 0 feasibility of task grouping, job classification and rung judging"
type: investigation
status: in-review
owner: "Imran"
phase: investigate
tags: [rung-model, task-grouping, job-classification, hooks, phase-0]
links:
  - {rel: refines, target: spec-coaching-loop}
review-by: 2026-11-30
summary: >-
  On synthetic Claude Code and Copilot CLI sessions, a model judge groups turns into tasks almost
  perfectly, classifies the job reliably when it may read a short excerpt of the prompts, and places
  rungs 1-3 well. Rung 5 needs repo reuse signals. A facts-first judge (rung-model spec) gives stable facts
  but needs rubric fixes and calibrated thresholds. A fact check on real sessions is the remaining gate.
---

# Spike: Phase 0 feasibility (task grouping, job, rung)

**Verdict: GO to real-data validation, with two design changes.** On synthetic data, all three
questions pass for rungs 1 to 3. Rung 5, and half of rung 4, need signals that are not in session
transcripts. Job classification needs a short prompt excerpt; structured signals alone are not
enough.

| Question | Verdict (synthetic) | Best configuration | Key number |
|---|---|---|---|
| (a) Group session events into tasks | **Go** | Model grouping, one call per window | B-cubed F1 0.98 to 1.00 (rules: 0.93 to 0.95) |
| (b) Classify each task's job | **Go, with an excerpt** | Sonnet or Haiku, signals + minimised excerpt | Job accuracy 93 to 98%; 0 other-job tasks leaked into an alpha pack at threshold 0.7 |
| (b') Classify the job from signals only (no prompt text) | **No-go** | n/a | Job accuracy 77 to 80% |
| (c) Judge current rung, rungs 1 to 3 | **Go** | Sonnet, excerpt | 88% (dev) and 100% (held-out) exact |
| (c) Judge current rung 4 | **Conditional** | Sonnet, excerpt | 4 of 8 exact, rest one rung low |
| (c) Judge current rung 5 | **No-go from transcripts** | n/a | 0 of 8; always judged 3 or 4 |
| (c) Judge potential rung | **Conditional** | Sonnet, excerpt | 77 to 80% exact, 100% within one rung |

**Confidence: Inferred.** Every number is measured, but on synthetic data whose labels were written
by Claude (dev set: this session; held-out set: a separate model call). The judge is also Claude, so
the numbers measure agreement between Claude and Claude using the same rubric, not agreement with
expert humans. The Phase 0 probes in the spec ("label a week of the team's own sessions") still have
to run on real data before alpha. See [What would change this verdict](#what-would-change-this-verdict).

## What was tested

- **Reader.** `prototypes/phase0-spike/aiq_spike/normalize.py` reads both surfaces into one list of human
  turns. It reuses AI-Forward's `session-profile.py` for discovery and parsing
  (`claude_sessions`, `copilot_sessions`, `_load_events`, `_text_of`, `_tool_result_texts`,
  `_args_of`) by loading it from the installed pack, not by copying it. All 107 labelled turns were
  recovered from both formats.
- **Corpus.** Two synthetic corpora in the on-disk formats AIQ will read
  (`~/.claude/projects/<slug>/<session>.jsonl`, `~/.copilot/session-store.db` +
  `session-state/<id>/events.jsonl`). No real person's data was read.
  - *Dev:* 30 hand-written tasks in 14 sessions, 66 turns (14 Code a change, 10 Write a spec or doc,
    6 other jobs; rungs 1 to 5). The session plan is designed to be hard: several tasks per session a
    few minutes apart, a task resumed hours later on the other surface, a 70-minute idle gap inside
    one task, and a design note followed by its implementation on the same feature.
  - *Held-out:* 20 tasks in 10 sessions, 41 turns, written by a separate Sonnet call that never saw
    the dev tasks, the grouper or the judge (`prototypes/phase0-spike/data/heldout_scenarios.json`). Different
    domain (rail timetabling), terser phrasing.
- **Grouping.** Three strategies: a 5-minute gap baseline, a deterministic heuristic (time gap,
  shared files, prompt similarity, cue words), and one model call that assigns every turn in the
  window to a task.
- **Judge.** One call per task returns job, current rung, potential rung, confidences, evidence, gap
  and blockers against the design doc's rubric (`prototypes/phase0-spike/aiq_spike/judge.py`). Two input modes:
  - *signals:* counts and flags only (prompts, re-prompts, structure markers, tools, tests and
    checks run, sub-agents, file kinds). No prompt text and no paths.
  - *excerpt:* signals plus each prompt cut to 300 characters (at most 8), file paths, and commands
    the agent ran.
- **Models.** Claude Haiku 5.5 and Sonnet 5.5 through the local `claude -p` with tools, settings and
  MCP off, and schema-checked JSON output. This matches ADR-0007 on the architecture branch (judge
  runs through the person's own AI account). Every configuration ran twice to measure repeatability.
  Judging used the gold task grouping, so judge errors are measured apart from grouping errors.

## Results

### (a) Task grouping

| Strategy | Dev F1 | Dev tasks exact | Held-out F1 | Held-out tasks exact |
|---|---|---|---|---|
| 5-minute gap baseline | 0.90 | 18 / 30 | 0.92 | 13 / 20 |
| Heuristic (rules) | 0.95 | 24 / 30 | 0.93 | 15 / 20 |
| Model, Haiku (2 runs) | 1.00, 0.98 | 30, 28 | 1.00, 1.00 | 20, 20 |
| Model, Sonnet (2 runs) | 0.98, 0.98 | 28, 28 | 1.00, 1.00 | 20, 20 |

F1 is B-cubed over turns.

- The model's only error: it joined the redis question (`o01`) to the rate-limiting task resumed just
  before it (`c05`). A human could argue for that grouping too.
- The model kept the design note and its implementation as two tasks, and joined the task resumed
  across surfaces and hours. The heuristic failed both of these.
- The heuristic was tuned on the dev set (one rule added after the first run). Its held-out F1
  (0.93) is barely above the baseline (0.92), so the rules overfit.
- Latency: 6 to 13 s per call for a window of 41 to 66 turns.

**Recommendation:** group with the model in the slow path. Use rules only as a cheap live estimate
of "is this still the same task" for in-the-moment detectors, which do not wait for the model.

### (b) Job classification

| Configuration | Dev accuracy | Held-out accuracy | Repeat agreement (job) |
|---|---|---|---|
| Haiku, signals | 0.77 | 0.80 | 1.00 / 1.00 |
| Haiku, excerpt | 0.98 | 0.98 | 0.97 / 0.95 |
| Sonnet, signals | 0.80 | 0.80 | 1.00 / 1.00 |
| Sonnet, excerpt | 0.93 | 0.95 | 1.00 / 1.00 |

The product rule is *Unclassified* when the job is "other" or confidence is below the pack
threshold. With excerpts and a threshold of 0.7:

- Precision on classified tasks was 100% in all four excerpt runs.
- No other-job task reached an alpha pack.
- 80 to 97% of alpha-job tasks were classified. The rest fell back to core Opportunities, which is
  the designed safe failure (US-7).

The misclassified tasks were short and context-free: a one-line typo fix, and a pasted snippet with
"what's wrong with this". The judge gave them low confidence (0.4 to 0.6), so a 0.7 threshold
routes them to Unclassified rather than the wrong pack.

**Recommendation:** set the alpha pack threshold at **0.7**. The judge needs the minimised excerpt to
classify the job. ADR-0007 already sends task content to the person's own model account, so this
costs no extra privacy. It should still be shown in the capture log and be possible to turn off. With
the excerpt off, expect about 20% wrong jobs.

### (c) Rung and potential rung

| Configuration | Rung exact (dev / held-out) | Within 1 | Mean signed error | Potential exact | Repeat agreement (rung) |
|---|---|---|---|---|---|
| Haiku, signals | 0.57 / 0.53 | 0.92 / 0.90 | −0.52 / −0.53 | 0.57 / 0.48 | 0.77 / 0.95 |
| Haiku, excerpt | 0.77 / 0.55 | 0.95 / 0.90 | −0.12 / −0.05 | 0.77 / 0.58 | 0.83 / 0.90 |
| Sonnet, signals | 0.53 / 0.60 | 0.93 / 0.83 | −0.53 / −0.58 | 0.67 / 0.55 | 1.00 / 0.95 |
| **Sonnet, excerpt** | **0.80 / 0.85** | **0.97 / 0.90** | −0.23 / −0.25 | **0.77 / 0.80** | **1.00 / 1.00** |

Sonnet with excerpt, by gold rung (both corpora, 2 runs each):

| Gold rung | Exact | Errors |
|---|---|---|
| 1 Assisted | 26 / 26 | none |
| 2 Directed | 32 / 34 | 2 judged 1 |
| 3 Delegated | 20 / 24 | 4 judged 2 |
| 4 Orchestrated | 4 / 8 | 4 judged 3 |
| 5 Factory | 0 / 8 | 2 judged 4, 6 judged 3 |

- **Rung 5 cannot be seen in a transcript.** The judge always saw the typed slash command and the
  checked-in workflow file. It noted both, then stopped at 3 or 4, because the rubric requires that
  "others can trigger it", and nothing in the session shows that. This is the rubric working as
  written, not a model failure. AIQ needs a **reuse signal from the repo**: is the workflow file
  versioned, how many authors has it had, and do CI or other people's sessions run it (counts only,
  as the design doc's Reuse signals already say).
- **Rung 4 is judged one rung low half the time.** Two Sonnet runs on two of the four rung-4 tasks
  called them 3, despite sub-agents, a spec and automated gates. Fix the rubric wording before
  tuning the model: say what counts as "several agents coordinated" when the sub-agents are visible.
- **Signals-only mode biases low by about half a rung.** It also reaches only 53 to 60% exact.
  Prompt structure is the main signal for rungs 1 to 3, and counts cannot carry it.
- **Potential rung** was within one rung in 100% of Sonnet excerpt runs. For coaching, the judge
  flagged 95 to 100% of tasks that have a next rung (recall). But 15 to 27% of its "next rung"
  calls were tasks already at their right rung (precision 0.73 to 0.85). The coaching loop should
  not nudge on potential until precision is measured on real data. Coaching only on
  potential-confidence ≥ 0.8 is the first thing to try.
- **Haiku is cheaper but less stable.** It costs about $0.001 per task against about $0.01 for
  Sonnet (list-rate estimates reported by the CLI; on a subscription this is quota, not dollars).
  Its rung agreement between two runs was 0.83 to 0.90, against 1.00 for Sonnet, and held-out
  exact accuracy dropped to 0.55. Sonnet answered in 5.4 s p50 and 7.5 s p95. Haiku was slower in
  this environment, at 8.3 s p50.

### Hook probe (requested by the architecture thread)

`prototypes/phase0-spike/hook_probe/` installs a throwaway Claude Code project with async `UserPromptSubmit`
and `PostToolUse` hooks and a sync `Stop` hook. It ran 3 live headless sessions (Claude Code 2.1.316).

- All three hooks fired on every run. The payloads carry the prompt, tool name, input and response,
  `duration_ms`, `session_id` and `transcript_path`, enough to feed grouping without reading the
  session store.
- The Stop hook's `{"systemMessage": ...}` reached the person as a notice ("Stop says: …") and did
  not enter the model's context.
- A Python collector process costs **35 ms p50, 47 ms p95, 58 ms max** (50 cold runs, wall time).
  That is inside the 50 ms p95 budget with almost no margin. Use a compiled collector, or a tiny
  client that hands off to the running core, for the sync Stop hook.
- **Not tested:** Copilot CLI hooks (Copilot CLI is not installed or signed in in this environment)
  and rendering in the interactive TUI (only the headless stream).

### Facts-first judge (rung-model spec, PR #3)

After the first run, the rung-model spec moved to **facts first**: the model answers narrow fact
questions, and deterministic per-job rules place the rung (decision Q6). The prototype in
`aiq_spike/facts.py` and `run_facts.py` reproduces this:

- **Model facts:** F2, F4, F5, F6, F7 and F12, each with a confidence.
- **Log facts:** F1, F3, F7 and F8.
- **F9 (reuse):** repo reuse counts, simulated per corpus because a session cannot supply them.
- **Rules:** the spec's illustrative rubric table.

Sonnet and Haiku each made two runs on both corpora (200 calls). The rule variants below are applied
to the same saved facts, so any difference between variants comes from the rules alone.

| Variant (Sonnet facts) | Exact (dev / held-out) | Within 1 | Uncertain |
|---|---|---|---|
| A: spec as written (F2 from logs only) | 0.27 / 0.30 | 0.70 / 0.60 | 100% |
| B: F2 estimated by the model | 0.38 / 0.30 | 0.77 / 0.63 | 48% / 45% |
| E: B, plus Factory without Orchestrated, plus one sub-agent counts, at fact threshold 0.5 | 0.63 / 0.55 | 0.92 / 0.80 | 0% |
| *Holistic judge, for comparison (Sonnet, excerpt)* | *0.80 / 0.85* | *0.97 / 0.90* | n/a |

What this shows:

1. **The facts are stable.** Sonnet's two runs agreed on F4, F5, F6, F7 and F12 in 85 to 100% of
   tasks, and on F2 in 75 to 93%. So facts-first is a sound base.
2. **The model's confidences are low, not its answers.** It mostly answers 0.5 to 0.6, even when the
   value looks right. At the spec's 0.7 fact threshold, most rules count as not met and tasks sink
   to rung 1. The planned fact check should set this threshold; picking it a priori is a guess.
3. **F2 as specified makes every CLI task Uncertain (variant A).** With no git diff, nothing in a
   CLI log gives the person's share of the change, and the CLI rows in the corpus have no git. AIQ
   needs the post-turn git diff for this fact.
4. **F2 does not separate Directed from Delegated in CLI agents.** The agent writes almost all of the
   change in both. In variant E, 16 of 34 Directed runs were placed on Delegated. The rubric needs a
   fact for what the rubric actually means: *was a whole outcome handed over, with done criteria,
   or one piece or step?*
5. **Factory should not require Orchestrated.** All 4 Factory tasks, across both corpora, are a
   team's versioned single-agent workflow. They also start with a typed command rather than a brief,
   so F4 fails. Factory should rest on F9 (reuse by others) plus automated checks.
6. **Rung 4 became reliable.** Once one sub-agent with automated gates counts, the facts plus rules
   placed all 8 Orchestrated runs correctly (holistic judge: 4 of 8).

Recommendation for the rung-model spec:

- Keep facts-first. Its errors are in the rules, which can be fixed and explained.
- Add the "whole outcome handed over" fact.
- Base Factory on F9 plus checks.
- Take F2 from git where it exists.
- Set the fact threshold from the fact check.

**Collecting the week of real sessions is now a fact check, not a rung labelling exercise**: the
person answers yes or no on the model-read facts of 20 to 30 of their own tasks.

## What would change this verdict

- **Real sessions, checked by facts (the main gate).** Follow the rung-model spec's Q6, not a large
  golden set. Collect one week of the team's own Claude Code and Copilot CLI sessions on each
  person's own machine. Each person confirms, for 20 to 30 of their own tasks:
  - the task boundaries and the job;
  - the model-read facts (yes or no).

  Log facts are checked automatically. A 30 to 50 task expert sanity check per job covers rung
  placement. **No-go if** model facts are right on less than 90% of tasks (the spec's bar), job
  precision at 0.7 is below 90%, or grouping F1 is below 0.85. The synthetic tasks are cleaner than
  real work. Real sessions mix topics, wander, and include long auto-compacted runs.
- **Format check.** The reader matches every field `session-profile.py` reads. Fields it does not
  read were modelled from memory: Claude Code's `<command-name>` wrapper for slash commands, and
  Copilot tool argument names. Confirm them on one real transcript per surface.
- **Human edits are invisible.** The rubric's "the person makes most of the edits" (the line between
  rung 2 and 3) cannot be seen in CLI transcripts when the person edits in their editor. The VS Code
  collector or a git diff after the agent's turn would supply it. The synthetic tasks state it in
  the prompts, which flatters the judge.
- **Window size.** Model grouping ran on the whole corpus in one call (41 to 66 turns). Real weeks
  are larger, so the window and the rule for carrying tasks across windows still need designing.

## Recommended changes

1. **Rung model / rubric (rung-model spec):** add a repo reuse signal for rung 5, clarify rung 4
   when sub-agents and gates are visible, and keep "mixed signals → lower rung" (it worked).
2. **Job packs:** set the classification threshold to 0.7, and require the minimised excerpt for job
   classification (the person can turn it off, at a known accuracy cost).
3. **Architecture:** use model grouping in the slow path, rules only for live detectors, and a
   non-Python collector for sync hooks. Default judge model: Sonnet, with Haiku as a cheaper option
   once its stability is measured on real data.
4. **Next probe:** run the fact check on a week of real sessions (above), plus one Copilot CLI hook
   end to end.

## Reproduce

```bash
python3 prototypes/phase0-spike/run_spike.py --out /tmp/aiq-spike \
  --endpoint claude-cli:claude-haiku-5-5 --endpoint claude-cli:claude-sonnet-5-5 --reps 2
python3 prototypes/phase0-spike/hook_probe/run_probe.py --runs 3 --bench 50
```

The measured outputs from this run are committed in `prototypes/phase0-spike/results/`: `results.json`,
per-call judgments with latency, tokens and evidence, and `hook-probe.json`. Model outputs vary
between runs; Sonnet's did not vary across two runs here.
