---
id: design-reclaim-low-confidence
title: "Design: reclaim pack tasks the judge read without confidence"
type: design
status: proposed
owner: "Imran"
phase: "5"
tags: [design, rung-model, judge, accuracy, coaching]
links:
  - { to: design-no-job-tasks, rel: depends-on }
  - { to: design-judge-accuracy-check, rel: depends-on }
  - { to: spec-rung-model, rel: implements }
review-by: 2027-05-02
summary: >-
  About 15 of Imran's 51 no-job tasks are pack tasks (mostly Ask or research) where both judge runs named
  the same job but below 0.7. Rule R2 (both runs agree at 0.5 or more) would give them a job. This change
  lets R2 switch itself on only when the accuracy check proves it (a different-model reader agrees 8 in 10,
  at least 10 checked), and off again if a later check falls under that bar. Off by default; every later
  coaching bar is unchanged.
---

# Design: reclaim pack tasks the judge read without confidence

Cloud-only, started under Imran's standing rule. Built from the repo, design notes
[no-job-tasks](no-job-tasks.md) and the no-job remainder note (PR #79). I could not read the laptop store, so
every number is quoted from a named laptop run or marked as worked out.

## Goal state

| | |
|---|---|
| **Goal** | The pack tasks AIQ leaves without a job only because the judge was unsure get a job and tips, without giving tasks the wrong job |
| **Done when** | R2 is live exactly when the accuracy check backs it (N3 bar), stored judgments follow the gate, and the report says whether it is live |
| **Not in scope** | The 31 "both said other" tasks (no pack fits; PR #79); the 3 "runs disagree" tasks (R1 covers almost none of them, see below); new packs; the kind-of-task rules |
| **Tier** | T1: a judging rule that can change which tasks get tips, so Imran's OK before merge |

## What is measured

Measured on Imran's laptop (build 3393c97, 2026-11-02 22:42Z): 51 of 476 judged tasks have no job.

| Group | Count | Covered here? |
|---|---|---|
| C1 both runs said `other` | 31 | No. A different model also said other on 18 of 18 sampled (run 01a19d99) |
| C3 both runs named the same pack job, one below 0.7 | 17 | Yes, where both are 0.5 or more: R2 gives a job to **15 of 51** (measured) |
| C2 the runs named different jobs | 3 | No. R1 (one confident pack job, the other `other`) gave a job to 3 of 101 before and agreed 1 of 2 |

R2's only second reading so far: **agreed 4 of 7** (run 01a19d99, before Run or ship it existed, a different
set of tasks). That is under the bar, so R2 would **not** be live today.

## Options considered

| Option | Why not (or why) |
|---|---|
| Turn R2 on now | 4 of 7 is under the bar. A wrong job means wrong coaching, which is worse than none (no-job N3) |
| Lower `JOB_THRESHOLD` from 0.7 to 0.5 for every task | Same risk, and it would also move tasks whose runs disagree. Not measured at all |
| Accept one run's job when the other is unsure | That is R1's shape; 1 of 2 agreed, and it covers 3 tasks |
| **R2 behind a self-switching gate (chosen)** | AIQ decides from its own checks, as the auto-pause does (PR #59). No setting for Imran, nothing live until the bar is met, and it turns itself off if a later check drops |

## Decisions

| # | Question | Decision | Why |
|---|---|---|---|
| R2-1 | Which rule? | R2 unchanged: both runs name the same pack job, each at 0.5 or more, where the strict rule gave none (`rung::reclaimed_job`) | Already built and measured (no-job N2); one definition |
| R2-2 | When is it live? | The newest whole check runs hold at least **10** second readings of tasks R2 covers, and the reader named R2's job on at least **8 in 10** (`check::reclaim_gate`) | The no-job N3 bar, and the same window as the pause rule (one helper, `window`) |
| R2-3 | Which readings count? | Every check of a task R2 covers at **today's judge version**, whether that judgment is still current or was judged again since | Reclaiming a task judges it again; if its check then dropped out, the gate would turn itself off and flip back and forth. A new judge version starts the evidence again (safe direction: off) |
| R2-4 | Stored judgments | A task R2 covers whose stored job does not match the gate (no job while live, R2's job after it turned off) is out of date and goes **first** in the judging queue. Judged again, it lands on the gate's side and leaves the queue | One judging pass reaches the old tasks. Each one costs 2 judge calls (about 30 for 15 tasks; on Copilot about one premium request each, from the check runs) |
| R2-5 | What stays the same | The kind still needs 0.7; a tip still needs potential confidence **0.8** and no unknown gate; the per-job and per-kind auto-pause (PR #59) still applies once the reclaimed tasks are checked | The 0.8 coaching bar is on the rung step, not the job. R2 only decides the job |
| R2-6 | What people see | A reclaimed judgment says why: "Both judge runs named Ask or research, though not confidently. AIQ uses that because a second reader agreed on 8 in 10 such tasks." `--check-report` prints "R2 in use: yes/no" with the agreement and the bar | Explainable from the judgment alone |

## Change surfaces (E7)

| Surface | Change |
|---|---|
| Domain | `rung::reclaimed_job`, `rung::place_with(.., reclaim)` (`place` = strict, unchanged); `check::RuleReading`, `RuleGate`, `reclaim_gate`, `RECLAIM_MIN`; `window` shared with the pause rule |
| Runner | `judge::rule_readings`, `reclaim_gate`; the queue and `current_version` take the gate; the judging pass reads it once; `--check-report` line |
| Store, contracts, Home | None. The job, state and note are the judgment's own fields, so Home and tips follow without change |

## Residual risks

- **Lag after the gate turns off.** Reclaimed tasks keep their job, and their tips, until the next judging
  pass judges them again. They go first, but only within the daily budget. Inferred, not measured: how long
  that is on the laptop.
- **Small evidence.** 10 readings at 8 in 10 still allows a true rate of about 54% (Wilson 90% low end, computed). That is
  the same bar every AIQ rule uses; the pause rule's later checks are the backstop.
- **Version reset.** A judge version bump discards the evidence, so R2 turns off and reclaimed tasks lose
  their job until a new check. Safe, but it costs a check run.

## Laptop reading that decides it

Already asked of the judge thread for PR #79 (about 51 premium requests). Read-only apart from storing the
check answers, like every check:

```
aiq judge --check --no-job --sample 51
aiq judge --check-report
```

After this change is installed, the report line `R2 in use: ...` says whether the bar is met. Before it, read
`R2 (...): gives a job to N of 51; second reading agreed k of n`: R2 goes live on install only if k of n is at
least 8 in 10 with n at least 10.

## Gate record

`GATE design-slice · 2026-11-02 · Simplifier, Test Architect, Data & Persistence (seats played by Claude; not independent) · pass`

- Simplifier: no setting, no column; the gate is derived from stored checks each pass.
- Test Architect: domain tests for the rule, the gate's bar and window; one runner test drives off → live →
  reclaim → off → give back through the real store.
- Data & Persistence: the gate is derived, never stored; the judgment stays the one record of the job.

## Status and next action

| | |
|---|---|
| **Completed** | Rule, gate, queue and report built and tested (PR) |
| **Remaining** | Imran's OK to merge; the no-job check on the laptop |
| **Best next action** | Merge on Imran's OK; the judge thread stages it in the next upgrade |
