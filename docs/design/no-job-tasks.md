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

## Cause, measured (Imran's laptop, 2026-11-02 ~18:30Z, build 7434187)

Every no-job judgment has now been re-judged at j4. **No job: 100 of 436** current judgments. The count
went up from 70 because more never-judged tasks have been judged since; about 23% of the new ones get no
job. That share is the laptop thread's figure, which I did not recompute.

| Cause | Count |
|---|---|
| C1 both runs said `other` | **75** |
| C3 agreed on a pack job, not confidently | 18 |
| C2 the runs named different jobs | 7 |

The candidate rules, scored against the old check sample (run 01a19ce8):

| Rule | Tasks it gives a job | Agreed with the second reading |
|---|---|---|
| R1 | 3 of 100 | 0 of 1 |
| R2 | 18 of 100 | 1 of 2 |

What this means:
- R1 can't help: it covers only 3 tasks. R2 covers 18 but has only 2 checks behind it, far short of the
  N3 bar. Neither goes live.
- C1 dominates, so by N4 the fix is in the prompt (R3) or in coverage (a pack for whatever those tasks are).
  Choosing between them needs to know what a different model calls the C1 tasks. In run 01a19ce8 it gave
  4 of 9 no-job tasks Ask or research.

**N5 (added):** `aiq judge --check --no-job` samples only no-job judgments, spread across the three causes.
The report then splits "the second reading said" by cause. One run of 30, at about 30 premium requests
(worked out from one request per task), shows:
- what the C1 tasks are: a pack the judge misses, which points to R3, or `other`, which points to coverage
- whether R2 clears the bar on C3

## Result of the no-job check (run 01a19d99, build 762a61c, 2026-11-02 18:55Z, measured)

`aiq judge --check --no-job` read 30 tasks with no job, spread by cause. The second readers were
`gpt-6-astra` and `claude-opus-5-5`, neither of them the judge's model. Cost: Claude Code $0.08 and about
29 Copilot requests. Agreement is Inferred: two models, no person.

| Cause (store: 101 of 473 with no job) | In store | Second reading of the sampled ones |
|---|---|---|
| C1 both runs said `other` | 76 | **other 18 of 18** |
| C3 agreed on a pack job, not confidently | 18 | ask_research 5, code_change 1, other 1 |
| C2 the runs named different jobs | 7 | ask_research 3, write_doc 1, other 1 |

| Rule | Tasks it gives a job | Second reading agreed |
|---|---|---|
| R1 | 3 of 101 | 1 of 2 (12-88%) |
| R2 | 18 of 101 | 4 of 7 (29-81%) |

**Decisions (2026-11-02):**
- **C1 is right, not a miss.** A different model also called all 18 sampled tasks `other`. That rules out
  rewording the prompt (R3): both readers agree these tasks fit none of the three packs. They stay
  uncoached, which is the designed behaviour. About 76 of 473, or 16% of Imran's judged tasks, are work
  outside every pack.
- **No rule goes live.** R2 agrees 4 of 7 and R1 1 of 2, both below the N3 bar of 8 of 10. R4, a third run
  for the 25 C2 and C3 tasks, isn't worth it: that is about 5% of tasks, and they would cost an extra call
  each.
- **What would change the coverage:** a fourth pack for whatever the C1 tasks are. Nobody knows what they
  are yet, and the second reader only says `other`. Finding out needs a short private label per task, kept
  on the device. Built as N6 below.

## N6: what the "both said other" tasks are (built 2026-11-02)

`aiq judge --survey-other [--sample n] [--dry-run]` takes a seeded random sample, 30 by default, of
judgments where both runs said `other`. For each one it asks the tool that already saw the task (the
sealed judge path, ADR-0007) for two things:
- a broad **kind** from a fixed draft list: communicate, plan or manage, analyze data, set up tools,
  operate systems, review or feedback, learn or brainstorm, personal or admin, chat or test, other
- a **label** of 2 to 5 generic words, with no names, paths or quotes

AIQ keeps only letters in the label and prints counts per kind with up to five labels each. **Nothing is
stored**, and no prompt text appears in the output. The kind list is a guess at useful buckets, and the
counts show whether a fourth pack would cover enough tasks. Cost: one call per task, about one premium
request each on Copilot (from the check runs).

## N6 result (Imran's laptop, build bf1c5cc, 2026-11-02 19:35-19:39Z)

30 of 30 sampled "both said other" tasks answered. The counts are measured. The kinds and labels are
Inferred: the tool that saw each task read it, and no person checked.

| Kind | Count | Example labels |
|---|---|---|
| operate_systems | **17** | application restart troubleshooting; package application release; publish clean project repository; publish project source; refresh data and launch |
| chat_or_test | 3 | agent behavior inquiry; retry request; single-word reply test |
| plan_or_manage | 3 | advance work priorities; managing team tasks; plan execution |
| set_up_tools | 3 | authenticate tool connection; network access configuration; preserve agent session continuity |
| other | 2 | capture full page screenshot; resuming interrupted work |
| learn_brainstorm | 1 | interface design exploration |
| review_feedback | 1 | feedback on review process |

What this means:
- One kind dominates: running, shipping or fixing a working system (restart, release, publish, launch).
  17 of 30 is 57% (Wilson 90%: 42-70%). Scaled to the 76 such tasks, that is about 32 to 53 tasks, or
  7% to 11% of Imran's 473 judged tasks (worked out, not measured).
- No other kind reaches 4 of 30, so no second new pack is supported.
- A fourth pack, "Run or ship it", is the only coverage change these numbers back. Whether to add it is a
  product decision for Imran.

## Questions for Imran

Whether to add a fourth pack, "Run or ship it", for the operate-systems tasks above.

## Gate record

`GATE design-slice · 2026-11-02 · Simplifier, Test Architect, Data & Persistence (seats played by Claude; not independent) · pass`
- Simplifier: measure first, and switch no rule on yet. Adding a field to `read` is cheaper than a column.
- Test Architect: the cause and both rules are unit-tested. The runner test checks that the runs are recorded and that the report explains them.
- Data & Persistence: `read` is the judgment's own JSON, and the cause is derived from it, not stored.

## Status and next action

| | |
|---|---|
| **Completed** | Design; measurement build; cause measured; no-job check run; survey run; decisions above |
| **Remaining** | Fourth-pack decision |
| **Best next action** | Imran decides on a "Run or ship it" pack; if yes, design it with `/design-slice` |
