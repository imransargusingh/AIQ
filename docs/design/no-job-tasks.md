---
id: design-no-job-tasks
title: "Design: tasks the judge gives no job"
type: design
status: accepted
owner: "Imran"
phase: "5"
tags: [design, rung-model, judge, accuracy, coaching, copilot-cli]
links:
  - { to: design-judge-accuracy-check, rel: depends-on }
  - { to: design-rung-judge, rel: depends-on }
  - { to: spec-rung-model, rel: implements }
review-by: 2027-05-02
summary: >-
  70 of Imran's 200 judged tasks have no job, so AIQ never coaches them. The second reading put half of a
  sample of them into a pack. Step one (built here) records each run's job pick, so the cause is measured.
  It also measures two candidate rules against the second reading. A rule becomes live only if it clears
  the bar below.
---

# Design: tasks the judge gives no job

Produced with AI-Forward `/design-slice`, then `/implement` for the measurement step. Started without
waiting, under Imran's standing rule: it is cloud-only and changes no rule anyone sees yet.

## Goal state

| | |
|---|---|
| **Goal** | Fewer of Imran's tasks left with no job, without giving tasks the wrong job |
| **Done when** | (this change) the cause of every new no-job judgment is recorded, and the report shows what each candidate rule would change; (next) one rule is chosen from those numbers, or none |
| **Not in scope** | New packs; tuning the type or rung rules; F6 (separate gap) |
| **Tier** | T1 (judge version bump, no schema change) |

## Grounding (measured on Imran's laptop, 2026-11-02, check run 01a19ca2)

- **No job: 70 of 200** current judgments (35%). Every judgment with a pack job was confident (0
  Uncertain among 130). So all of Imran's Uncertain tasks are no-job tasks.
- The rule (rung-judge design): the job needs both runs to agree, both at confidence 0.7 or more, and a
  job that has a pack. Otherwise the task is `unclassified`.
- Second reading of the 10 no-job tasks in the sample: other 5, ask_research 4, code_change 1. On 29 of
  the 30 tasks, though, the second reader was the judge's own model. PR #40 now picks a different model.
- **Not measured:** which part of the rule fails. Before this change, AIQ kept only the lower of the two
  runs' job confidences, not each run's pick.

## Possible causes (Inferred until N1 is measured)

| Cause | What it would look like | Fix it would point to |
|---|---|---|
| C1 Both runs say `other` | The judge sees no pack fit (for example, a quick question) | Prompt wording for Ask or research (R3), or the task really is outside every pack |
| C2 The runs disagree | One run says a pack job, the other says `other` or another pack | A tie-break rule (R1) or a third run (R4) |
| C3 Agreed, but not confident | Both runs name the same pack job, one is below 0.7 | A lower bar (R2) |

## Decisions

| # | Question | Decision | Why |
|---|---|---|---|
| N1 | How do we learn the cause? | The judgment's `read` JSON gains `job_runs` (each run's job and confidence). Judge version **j4**. Only no-job judgments of j2/j3 are judged again, and those go first in the queue. No schema change | Measure, don't infer. The re-judge costs 2 calls per no-job task (about 140 calls for Imran's 70, under his 100-task daily cap; guessed from the counts) |
| N2 | Which rules are measured? | **R1:** one run names a pack job at 0.7 or more, and the other says `other`. **R2:** both runs name the same pack job at 0.5 or more. The report shows, for each: how many no-job tasks it would give a job, and how often the second reading agrees | Both rules are cheap and deterministic. Neither runs live |
| N3 | Bar for making a rule live | The second reading **by a different model** agrees on at least 8 of 10, with at least 10 checked. Agreement on confident judgments must not drop | The same bar as the coaching self-check. A wrong job means wrong coaching, which is worse than none |
| N4 | Not yet | R3 (re-word the Ask or research job in the prompt) and R4 (a third run as tie-break, one more call) wait for N1. R3 if C1 dominates, R4 if C2 does and R1 fails the bar | Pick the fix from the cause, not before |

## Change surfaces (E7)

| Surface | Change |
|---|---|
| Domain | `rung::JobRun`, `NoJob`, `no_job_cause`, `candidate_job`, `CANDIDATE_RULES`; judge version j4, `JUDGE_VERSIONS_KEPT` j2 and j3 |
| Runner | The judgment's `read.job_runs`; `--check-report` adds "Why" under No job, plus one line per rule |
| Store, contracts, UI | None |

## Run plan

1. Merge. The laptop thread sends the upgrade line.
2. Background judging re-judges the no-job tasks first. Allow about a day at Imran's cap.
3. `aiq judge --check` (new sample, different second reader), then `aiq judge --check-report`.
4. Pick R1, R2, R3, R4 or none against N3, and record it here.

## Questions for Imran

None. The cause and the rule are both settled from data.

## Gate record

`GATE design-slice · 2026-11-02 · Simplifier, Test Architect, Data & Persistence (seats played by Claude; not independent) · pass`
- Simplifier: measure first, and switch no rule on yet. Adding a field to `read` is cheaper than a column.
- Test Architect: the cause and both rules are unit-tested. The runner test checks that the runs are recorded and that the report explains them.
- Data & Persistence: `read` is the judgment's own JSON, and the cause is derived from it, not stored.

## Status and next action

| | |
|---|---|
| **Completed** | Design; measurement build |
| **Remaining** | Laptop re-judge and a check; pick a rule |
| **Best next action** | Upgrade the laptop, wait for re-judging, then run `aiq judge --check` |
