---
id: design-kind-of-task
title: "Design: kind-of-task accuracy"
type: design
status: accepted
owner: "Imran"
phase: "5"
tags: [design, rung-model, judge, accuracy, coaching]
links:
  - { to: design-judge-accuracy-check, rel: depends-on }
  - { to: design-no-job-tasks, rel: relates-to }
  - { to: design-rung-judge, rel: depends-on }
  - { to: spec-rung-model, rel: implements }
review-by: 2027-05-02
summary: >-
  Kind of task is the judge's weakest answer (about 4 in 5 against a different-model second reader), and
  it picks the coaching people see. The store cannot yet say where the misses come from. This step keeps
  each run's kind pick, explains every fallback "other" kind, scores two candidate rules before and after,
  and fixes the confident-kind count. No rule changes yet.
---

# Design: kind-of-task accuracy

Started without waiting, under Imran's standing rule. Cloud-only; it changes no rule anyone sees yet.

## Goal state

| | |
|---|---|
| **Goal** | Kind of task agrees with an independent reading at least 8 in 10, so coaching names the right kind |
| **Done when** | (this change) every kind miss in a check is explained from data, and the report shows each candidate rule's agreement before and after; (next) one fix is chosen from those numbers, or none |
| **Not in scope** | No-job rules (no-job-tasks design); new kinds or packs; rung and fact rules |
| **Tier** | T1 (no schema change, no judge version bump, no new model calls) |

## Grounding

Measured on Imran's laptop (check runs 01a19ca2 and 01a19ce8, 2026-11-02, recorded in the rung-judge design):

- Kind, where the job matched: 15 of 18 (same-model reader), then **14 of 17 (63-93%)** with a different
  model: ask_research 9 of 11, code_change 3 of 4, write_doc 2 of 2. Six misses in 35 checks, three kinds
  of job. Too few to name one weak kind.
- The second run printed **confident kind 14 of 18** while kind was 14 of 17. A subset can't be larger.
  Cause (verified in code, `check::agreement`): confident kind also scored tasks whose *job* differed.
  Every judgment with a pack job was confident, so the extra 1 is the one confident job miss.
- The merge (`rung::place`) keeps a kind only if both runs agree and both are at 0.7 or more. Otherwise
  it stores **`other`**. The second reader always picks a specific kind. So a fallback `other` counts as a
  miss even when both runs leaned to the reader's kind.
- **Not measured:** how many of the six misses were fallback `other` and how many were a different named
  kind. The store keeps only the merged kind and the lower confidence, not each run's pick.

## First breakdown (measured, Imran's laptop, build a4b9331, 2026-11-02 18:47Z, no model calls)

Pooled over check runs 01a19ca2 and 01a19ce8: kind **29 of 35 (70-91%)** where the job matched.

| Judge's kind | Checks | Second reading agreed |
|---|---|---|
| A named kind | 27 | **27 of 27** (no named miss) |
| `other`, not confident (stored confidence below 0.7) | 6 | 2 (reader also said other) |
| `other`, runs differed or both said other | 2 | 0 |

- **Every kind miss (6 of 6) is AIQ's fallback `other`, none is a wrong named kind.** So K-c is not seen
  and T5 (re-word the prompt, re-judge everything) is not indicated.
- A fallback `other` is not wrong coaching: the live tip never guesses a kind of `other`
  (`coach::guess_type` returns none for `other`). It is a task AIQ could have named and did not: lost
  coverage, not a wrong answer.
- Not yet known: what each run picked in those 8 (judged before runs were kept), so K1 and K2 are not
  scored. Most were "not confident", which points at K2 (Inferred; 8 checks).

## Possible causes (before the first breakdown)

| Cause | What it looks like | Fix it points to |
|---|---|---|
| K-a Fallback: runs differ | Run 1 says `how_to`, run 2 says `explain`; stored `other` | Tie-break rule K1, or sharper definitions |
| K-b Fallback: not confident | Both runs say `find`, one at 0.6; stored `other` | Lower bar K2 |
| K-c Named the wrong kind | Judge and reader both confident, different kinds | Definitions in the prompt (T5) |

Overlaps in the list that could drive K-a and K-c (Inferred from the labels, not from data): `explain`
appears under both code_change and ask_research; ask_research `how_to` vs `explain` vs `troubleshoot`;
`find` vs `research` vs `analyse`; code_change `fix_bug` vs `build_fix`.

## Decisions

| # | Question | Decision | Why |
|---|---|---|---|
| T1 | How do we learn the cause? | The judgment's `read` JSON gains `type_runs` (each run's kind and confidence). No version bump, so nothing is judged again; new judgments carry it | Measure, don't infer. Re-judging old tasks would cost premium requests for a measurement that new tasks give free |
| T2 | How is a fallback explained? | `rung::kind_other_cause`: both said other, runs differed, not confident, not on the job's list, one run. A test proves it matches the merge rule case by case. Old judgments fall back to what the stored confidence can say | Derived, never stored |
| T3 | Which rules are measured? | **K1:** runs named different kinds; take the more confident listed kind if it is at 0.7 or more and at least 0.1 ahead. **K2:** both runs name the same kind at 0.5 or more. Each only replaces a fallback `other` | Cheap and deterministic. Neither runs live |
| T4 | What does the report show? | Under kind: kind pooled over **all** check runs, each fallback `other` by cause, each named miss as "job: judge kind read as reader kind", and per rule "gives a kind to n; before -> after" | One run has about three misses; pooling gives the cause a chance to show. Before/after is the number a fix is judged by |
| T5 | Not yet | Re-wording kind definitions in the prompt waits for T4's numbers: only if named misses (K-c) dominate. It changes the rubric, so every task would be judged again | Pick the fix from the cause |
| T6 | Bar for making a rule live | Same as no-job N3: a different-model reader agrees at least 8 of 10 after the rule and at least 10 checked | A wrong kind means wrong coaching |
| T7 | Confident-kind count | Compared only where the job matched, the same base as kind | One definition of one quantity |

## Change surfaces (E7)

| Surface | Change |
|---|---|
| Domain | `rung::TypeRun`, `KindOther`, `kind_other_cause`, `candidate_type`, `KIND_CANDIDATE_RULES`; `check::agreement` confident-kind base |
| Store | `Store::check_runs` (read only) |
| Runner | The judgment's `read.type_runs`; `--check-report` adds the pooled kind lines |
| Contracts, schema, UI, coaching | None |

## Run plan

1. Merge. The judge thread installs it with the next laptop upgrade.
2. `aiq judge --check-report` right away: pooled kind and the fallback split from stored confidence. No premium requests.
3. After a few days of new judgments, the next `aiq judge --check` scores K1 and K2 before and after.
4. Choose K1, K2, T5 or none against T6, and record it here and in the rung-judge accuracy table.

## Questions for Imran

None. Cause and fix are settled from data.

## Gate record

`GATE design-slice · 2026-11-02 · Simplifier, Test Architect, Data & Persistence (seats played by Claude; not independent) · pass`
- Simplifier: one JSON field, no bump, no re-judge, no live rule.
- Test Architect: cause matches the merge rule in a table test; rules unit-tested; a runner test proves the runs are recorded and the report pools two check runs and shows before -> after; confident-kind base has its own test.
- Data & Persistence: `read` is the judgment's own JSON; cause and rule results are derived.

## Status and next action

| | |
|---|---|
| **Completed** | Design; measurement build; first breakdown (all misses are fallback `other`) |
| **Remaining** | New judgments on this build; a check; score K1/K2; pick one or none |
| **Best next action** | The judge thread runs the next `aiq judge --check` after a few days of judging |
