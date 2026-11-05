---
id: design-one-call-analysis
title: "Design: which model analyzes Copilot tasks, and how many calls (measured)"
type: design
status: proposed
owner: "Imran"
phase: "5"
tags: [design, rung-model, judge, cost, eval]
links:
  - { to: design-rung-judge, rel: refines }
  - { to: design-judge-evals, rel: depends-on }
review-by: 2027-05-05
summary: >-
  Copilot tasks are analyzed by gpt-6.1-sol, two calls (Imran: OpenAI models, Sol default), falling back to
  Copilot's own pick when it is not offered or a call fails. Measured on the golden set: the number of calls with
  one model barely matters; the model matters a lot (sonnet sure-but-wrong 7% vs Sol 30%, recorded, not adopted).
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

## Second call to a different model, and the model itself (2026-11-05 14:22Z, measured)

Imran asked about two different models. Everyday analysis sent the same task twice to the same model; a
different model only reads tasks in the accuracy check (`aiq analyze --check`). So the test: claude-sonnet-5.5
through Copilot (same "medium" price tier as gpt-6.1-sol; gpt-6-astra skipped, priced "very high"), 3 answers
per task, 102 calls, p50 6.9 s. `aiq analyze --eval --compare-calls A --with B`.

| Measure | sol once | sol twice | sonnet once | sonnet twice | sol + sonnet |
|---|---|---|---|---|---|
| Job right | 100.0% | 98.0% | 92.1% | 91.0% | 89.1% |
| Kind right | 95.8% | 94.8% | 80.0% | 77.7% | 78.9% |
| Rung exact | 65.6% | 65.6% | 82.1% | 80.9% | 75.8% |
| Rung too high | 15.6% | 15.6% | 3.2% | 3.2% | 3.2% |
| Shown as sure and right | 61.8% | 61.8% | 71.3% | 70.0% | 63.4% |
| Shown as sure but wrong | 32.4% | 30.4% | 6.9% | 6.0% | 16.8% |
| Marked not sure | 5.9% | 7.8% | 21.8% | 24.0% | 19.8% |

On the 15 tasks whose labels Imran confirmed (11-03): sol twice rung exact 60.0%, sure but wrong 35.6%; sonnet
once 77.3%, 2.3%. Caveats: every expected label was written by a Claude session, which may favour a Claude model
(the result holds on Imran's 15); sonnet names the job and kind wrong more often and says "not sure" more, so
fewer tasks get a tip; 1 request per sonnet call is inferred from its price tier, not measured.

## Decision (2026-11-05 14:37Z, Imran)

**Copilot tasks stay on OpenAI models, gpt-6.1-sol by default, two calls.** Imran: "For sessions in Copilot,
prefer OpenAI models (and Sol as the default)." The sonnet numbers above stay as a measurement, not a plan. The
default model is now named (`gpt-6.1-sol`) rather than left to Copilot; when it is not offered or a call to it
fails, Copilot's own pick answers, and the model that answered is recorded and shown on the task page. Two calls
stay because on Sol the second call measurably catches a few confidently wrong results. Next: a second call on a
different OpenAI model (gpt-6-luna, "low" price tier), scored with `--compare-calls sol --with luna`.

### Earlier decision (08:30Z): keep two calls on gpt-6.1-sol


**Keep two calls as the default.** On both models the second call moves the same way: it marks a few wrong
answers "not sure" (Copilot 2.0 points, Claude 0.5) and costs almost no right ones (Copilot 0, Claude 0.7). The
gap is small and within noise for 34 tasks, but Imran ranks accuracy above cost, and a sure-but-wrong result is
the error a person acts on. One call stays available (`aiq config set analysis-calls 1`) and
`aiq analyze --eval --compare-calls` re-measures it after any rule or prompt change.

Revisit when: the golden set grows (more tasks narrow the noise), or a rule change makes answers vary more or less.
Accuracy gains come from the rules: the misses repeat on every answer (Copilot h08, h15, h16 rung; r04 job).
