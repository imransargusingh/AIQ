---
id: design-one-call-analysis
title: "Design: one model call per analyzed task (measured against two)"
type: design
status: accepted
owner: "Imran"
phase: "5"
tags: [design, rung-model, judge, cost, eval]
links:
  - { to: design-rung-judge, rel: refines }
  - { to: design-judge-evals, rel: depends-on }
review-by: 2027-05-05
summary: >-
  Analysis asks the model twice per task and marks disputed facts unknown (rung-judge D1/D2). Measured on the
  golden set with 8 answers per task, one call scores the same as two: the answers repeat almost every time, so
  the second call rarely changes a placement. On Copilot (3 answers per task) the second call turned 2 of every
  100 sure-but-wrong results into "not sure" and lost no right ones, so the default stays 2 (accuracy first). A
  setting `analysis-calls 1` halves cost for anyone who prefers that.
---

# One call per analyzed task

**Goal.** Halve premium requests and time per analyzed task without losing accuracy. Imran (2026-11-05):
accuracy matters more than cost, so one call ships only if it is at least as accurate.

## What changes

| Piece | Today (2 calls) | `analysis-calls 1` |
|---|---|---|
| Calls per task | 2 (two answers, D2 merge) | 1; a failed call is tried once more, so never more than 2 |
| A fact the two answers dispute | unknown → Uncertain | no dispute possible |
| Only one answer arrived | Uncertain, never coached | the normal case: placed and coached |
| No-job / kind-other cause | from the answers | the same rules on one answer (`OneRun` removed) |

The judge prompt, facts, rubric and versions do not change, so no task is analyzed again when the setting flips.
`aiq analyze --eval --compare-calls [file]` scores both policies on recorded answers, free.

## Measurement (2026-11-05, measured)

Golden set g1, 34 tasks, Claude Code `claude-sonnet-5-5`, 8 answers per task (272 calls, $4.41, p50 6.8 s).
Each policy is scored over every way to pick its answers from the 8, through today's placement.

| Measure | 1 call | 2 calls |
|---|---|---|
| Job right | 89.0% | 88.2% |
| Kind right | 76.2% | 75.3% |
| Rung exact | 70.3% | 71.0% |
| Rung within one | 91.0% | 90.6% |
| Rung too high | 14.1% | 12.6% |
| Shown as sure but wrong | 21.0% | 20.5% |
| Injection resisted | 100% | 100% |

Three calls (56 triples) scored the same as two (rung exact 70.7%, sure but wrong 20.6%). The replay.json answers
(3 per task) show the same picture. Across 1,904 answer pairs the job never differed, the kind differed 6.7% and the
model's rung 9.1%. The misses repeat on every answer, so a second call cannot catch them; better rules can.

**Limits.** Claude only. Users run Copilot (gpt-6.1-sol). The 2026-11-03 Copilot eval (2 answers) agreed on
job/kind/rung for 32/31/33 of 34 tasks, which points the same way but is not this measurement. 34 tasks is small:
1-2 point gaps are noise.

## Copilot check (2026-11-05, measured)

Laptop, `copilot-cli` gpt-6.1-sol, golden g1, 3 answers per task: 102 premium requests, 1,222 s, p50 11.1 s.
Recording: `/mnt/project-files/one-call/copilot-gpt-6.1-sol-3-answers-20261105.json` (SHA-256 6f342a8d…781d).

| Measure | 1 call | 2 calls |
|---|---|---|
| Job right | 100.0% | 98.0% |
| Kind right | 95.8% | 94.8% |
| Rung exact | 65.6% | 65.6% |
| Rung within one | 90.6% | 90.6% |
| Rung too high | 15.6% | 15.6% |
| Shown as sure but wrong | 32.4% | 30.4% |
| Shown as sure and right | 61.8% | 61.8% |
| Marked not sure | 5.9% | 7.8% |
| Injection resisted | 100% | 100% |

## Decision (2026-11-05)

**Keep two calls as the default.** On both models the second call moves the same way: it marks a few wrong
answers "not sure" (Copilot 2.0 points, Claude 0.5) and costs almost no right ones (Copilot 0, Claude 0.7). The
gap is small and within noise for 34 tasks, but Imran ranks accuracy above cost, and a sure-but-wrong result is
the error a person acts on. One call stays available (`aiq config set analysis-calls 1`) and
`aiq analyze --eval --compare-calls` re-measures it after any rule or prompt change.

Revisit when: the golden set grows (more tasks narrow the noise), or a rule change makes answers vary more or less.
Accuracy gains come from the rules: the misses repeat on every answer (Copilot h08, h15, h16 rung; r04 job).
