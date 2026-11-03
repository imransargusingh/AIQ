---
id: design-no-job-remainder
title: "Design note: the 51 tasks still without a job"
type: design
status: accepted
owner: "Imran"
phase: "5"
tags: [design, rung-model, judge, coaching, pack]
links:
  - { to: design-no-job-tasks, rel: depends-on }
  - { to: design-run-or-ship-pack, rel: depends-on }
  - { to: spec-rung-model, rel: implements }
review-by: 2027-05-02
summary: >-
  After the Run or ship it pack, 51 of 476 judged tasks have no job. They are two different groups. 31 are
  work no pack fits (chat tests, planning, brainstorming, odds and ends): no group is big enough for a
  fifth pack, and they already get no tips. 20 are pack tasks the judge read without confidence: that is a
  rule question (R2), not a pack. Measured on the laptop 2026-11-03: no kind reaches the bar and R2 agrees
  7 of 15, so no fifth pack, no catch-all, and R2 stays off.
---

# Design note: the 51 tasks still without a job

Cloud-only, started under Imran's standing rule. Built from the repo only: the design notes
[no-job-tasks](no-job-tasks.md) and [run-or-ship-pack](run-or-ship-pack.md), and the judge code in
`crates/aiq/src/judge.rs`. I could not read the laptop store, so every number below is either quoted from a
measured laptop run (named) or worked out from those runs (marked).

## Outcome

**Recommendation: leave the 51 as they are. Build no fifth pack and no catch-all.** One survey and one check
on the laptop confirm or overturn this. Neither needs Imran's time beyond the premium requests.

- A catch-all already exists in effect. A task with no job is `unclassified`: Home shows it as
  "Unclassified" (`apps/home/src/format.ts`) and no tip can fire for it. A named catch-all pack with no tips
  would change a label, not anyone's habits.
- A fifth pack would need one kind of work big enough to coach and to check. The numbers below say no such
  kind exists.

## What the 51 are

Measured on Imran's laptop (build 3393c97, 2026-11-02 22:42Z, recorded in run-or-ship-pack): 51 of 476
judged tasks have no job.

| Group | Count | What it likely is | Basis |
|---|---|---|---|
| C1: both judge runs said `other` | **31** | Work outside every pack | Measured count. "Outside every pack" was measured before run_ship: a different model agreed 18 of 18 |
| C3: both runs named the same pack job, below 0.7 | 17 | Pack tasks, mostly Ask or research | Measured count. Earlier second reading of 7 such tasks: ask_research 5, code_change 1, other 1 |
| C2: the runs named different jobs | 3 | Pack tasks, mostly Ask or research | Measured count. Earlier second reading of 5: ask_research 3, write_doc 1, other 1 |

### C1 (31 tasks): what kind of work

The only measured reading is the survey of 30 "both said other" tasks, taken **before** run_ship existed
(2026-11-02 19:39Z). 20 of those 30 were operate systems or set up tools, which run_ship now covers. The
other 10:

| Survey kind | Of 30 | Example labels (from the survey) | Coachable? |
|---|---|---|---|
| chat_or_test | 3 | retry request; single-word reply test; agent behavior inquiry | No. There is no habit to change in a one-word test |
| plan_or_manage | 3 | advance work priorities; managing team tasks; plan execution | Possibly |
| other | 2 | capture full page screenshot; resuming interrupted work | No common shape |
| learn_brainstorm | 1 | interface design exploration | Possibly |
| review_feedback | 1 | feedback on review process | Possibly |

Worked out, not measured:
- 10 of 30 scaled to the 76 C1 tasks of that time is about 25. The store now has 31. The gap of about 6 is
  either new tasks judged since, or operate-systems tasks that run_ship still calls `other`. Which one is
  not known.
- The biggest coachable kind, plan_or_manage, is 3 of 30 (Wilson 90%: 4% to 23%). On the old 76 that is 3
  to 17 tasks, so at most about 3.6% of 476. run_ship had 17 of 30 behind it.

## Why no fifth pack (the bar)

A pack's tips stay on only if a different-model check agrees on its job **8 of 10, with at least 10
checked** (rung-model US-7, judge-accuracy-check K6b). A pack with fewer than 10 tasks can never pass that
check, so its tips would never be proven. Tips also need a kind repeated 3 times in 30 days.

So the bar for a fifth pack: **one coachable kind (not chat_or_test, not other) on at least 10 of the 31
C1 tasks.** On the survey proportions the largest kind would be about 3 of 31. That is far below the bar.

## Why no new pack for C2 and C3 (20 tasks)

These are not outside the packs. The judge named a pack job but was not sure. The fix, if any, is rule R2
(both runs agree on a pack job at 0.5 or more), already built and measured but not live (no-job-tasks N2).
R2 would give a job to **15 of the 51** (measured, run-or-ship-pack). None of those 15 has been checked
since run_ship; the earlier 4 of 7 was on a different set. R2 goes live only at 8 of 10 with at least 10
checked (N3).

## Laptop readings that settle it

Both are measurements only. Neither changes a judgment, a tip or a setting.

**1. Survey all C1 tasks (decides the fifth-pack question).** Stores nothing; prints counts and generic
labels. About 31 premium requests.

```
aiq judge --survey-other --sample 31
```

- If one coachable kind reaches 10 of 31: draft a fifth pack for it and ask Imran.
- If operate_systems or set_up_tools reach 4 or more: run_ship is missing tasks. The fix is the run_ship
  wording in the judge prompt (a judge version bump), not a new pack.
- Otherwise: leave C1 as it is. This note becomes accepted.

**2. Check every no-job task (decides R2 for C2 and C3).** Stores the check answers, like every check run.
About 51 premium requests, outside the judging budget. A sample of 30 would give R2 only about 9 checks
(5 per cause first, then random), short of the 10 the bar needs, so take all 51.

```
aiq judge --check --no-job --sample 51
aiq judge --check-report
```

- In the report, read the line `R2 (...): gives a job to N of 51; second reading agreed k of n`.
- If R2 agrees at least 8 of 10 with at least 10 checked: propose switching R2 on (Imran's card).
- If not: R2 stays off, and these 20 stay uncoached (about 4% of tasks).

## Result (Imran's laptop, build 444dcda, 2026-11-03 ~00:00Z, check run 01a19eb3)

Counts are measured. The kinds and the second readings are Inferred: model readings, no person checked.

**Reading 1, survey.** 22 tasks were eligible, not 31, and all 22 answered. The gap of 9 is not explained
here (likely tasks the survey's task list does not include; not checked).

| Kind | Of 22 | Example labels |
|---|---|---|
| chat_or_test | 8 | casual greeting; minimal test prompt; agent retry request |
| other | 7 | full page screenshot capture; photo background editing; resume interrupted work |
| plan_or_manage | 4 | manage team actions; plan execution; work status check |
| analyze_data | 1 | enrich organizational spreadsheet |
| review_feedback | 1 | review process feedback |
| set_up_tools | 1 | network access recovery |

- No coachable kind reaches 10. The largest, plan_or_manage, has 4. **No fifth pack (L1).**
- operate_systems is 0 and set_up_tools 1, below the 4 that would point to run_ship missing tasks. run_ship
  wording stays as it is.
- 15 of 22 are chat tests or odds and ends with no habit to coach.

**Reading 2, check of every no-job task.** 43 of 43 eligible were read, second readers claude-opus-5-5 and gpt-6-astra (no job now 52 of 477: C1 31, C3 17,
C2 4).

| Cause | Second reading |
|---|---|
| C1 both said other | other 22 of 22 |
| C3 agreed, not confident | other 8, ask_research 5, run_ship 3, code_change 1 |
| C2 runs differed | write_doc 2, code_change 1, other 1 |

| Rule | Gives a job to | Second reading agreed |
|---|---|---|
| R1 | 0 of 52 | none checked |
| R2 | 15 of 52 | **7 of 15** (90%: 28-67%) |

- R2 is far below 8 of 10. **R2 stays off (L3).** Half the C3 tasks the second reader calls `other`, so
  the judge's low confidence there is mostly right.
- C1 is confirmed as work outside every pack, now 40 of 40 across both checks.

## Decisions

| # | Question | Decision | Why |
|---|---|---|---|
| L1 | Fifth pack? | **No**, unless reading 1 shows a coachable kind on 10 or more of 31 | Below 10 tasks a pack cannot pass its own accuracy check |
| L2 | Catch-all with no tips? | **No** | `unclassified` already is one: no tips, labelled on Home |
| L3 | C2/C3 | Decide R2 from reading 2 against the N3 bar | They are pack tasks read without confidence, not new work |

## Status and next action

| | |
|---|---|
| **Completed** | Grouped by cause; bars set; both readings run; no bar cleared |
| **Remaining** | None. No-job tasks stay `unclassified`, with no tips |
| **Best next action** | None now. Re-run the survey only if no-job tasks grow past about 20% of judged tasks |
| **Imran's decision** | None needed: no reading cleared a bar |
