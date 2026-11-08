---
id: design-tip-funnel
title: "Design: the tip funnel and the dry-day note"
type: design
status: proposed
owner: "Imran"
phase: "5"
tags: [design, coaching, tips, instrumentation]
links:
  - { to: design-coaching-from-rungs, rel: depends-on }
  - { to: design-kind-label, rel: relates-to }
review-by: 2027-05-08
summary: >-
  AIQ showed no tip from 2026-11-04 to 11-08 and nothing said so. Every new task's tip path now ends at a recorded
  step, and `aiq status` and Home count new tasks down to tips opened, say why each task had no tip and why each
  ready tip was held, and note any working day with no tip shown.
---

# The tip funnel

## Why

Imran (2026-11-08): "It's super important that AIQ offers tips when it is supposed to." From 11-04 16:00Z to 11-08
the label self-check held every label tip and the words check had nothing to match, so no tip was decided on any
surface for four days. The only trace was one log line per core run. A stop that silent is a defect class:
*a hold that is invisible to the person and to us*. The control is a count that cannot be missed.

## Data model

Grain: **one row is exactly one point where a live new task's next-step tip path ended or began to wait**
(`tip_path`, store v11, append-only, no text). The latest row per task is its state. A task waiting on a kind
label has `label_asked`, then the label's end. Everything after a tip is decided is already stored:
`card_decision` (held, ready, expired), `card_delivery` (shown, by tool), `card_open`, `tip_feedback`.

Steps (`aiq_domain::funnel::Step`), each written at exactly one exit of the core's path:

| Step | Where | Means |
|---|---|---|
| `decided` | `decide_kind` | A tip was decided (see its card decision) |
| `kind_too_few` · `kind_right_rung` · `kind_blocked` · `kind_not_sure` · `kind_flagged` | `decide_kind` | Kind known, its tip held on purpose by the kind's state (D6) |
| `kind_new` | `decide_kind` | Kind known, none of the person's analyzed tasks has it yet |
| `no_tip_for_kind` | `decide_kind` | Kind coaches a move the pack lacks (pack defect) |
| `label_held` | `on_kind_label` | Label named the kind; label self-check under 8 in 10 |
| `guess_held` | `next_rung_with` | Word guess made; words check under 8 in 10; labels can't run |
| `no_match` | `next_rung_with` | No word match; labels can't run |
| `label_asked` | `next_rung_with` | Waiting for a label; as the latest step, it never came back |
| `label_failed` · `label_unsure` · `label_not_coached` · `label_late` | `on_kind_label` | Label call failed / not sure / sure, but of a job labels never coach from (questions, `label::NOT_COACHED_JOBS`) / after the next task started |
| `tips_off` | `next_rung` | Next-step tips turned off |
| `late_first_prompt` | `record` | Session set aside first; a person's second prompt brought it back |

Measures: every count is additive over tasks or cards. Periods: since local midnight, and the last 7 days. One
reader (`aiq_core::funnel::tip_funnels`) feeds `aiq status` and Home; one place holds the words
(`aiq_domain::funnel::words`).

## Dry day

A local day before today, in the last 7, with **3 or more** new tasks and no tip shown. `assume:` 3 tasks is a day of
work (not measured). If it warns on light days or misses dry ones, change `DROUGHT_MIN_TASKS`.

## Surfaces

- `aiq status`: the dry days first, then the week (tasks, kind known, tip found, tips, shown, opened, answered), why
  tasks had no tip, why tips were held, expired, waiting, shown per tool; then today's line.
- Home: a closable note for the newest dry day ("No tips yesterday across 6 tasks. Main reason: …", See why).
  Closed stays closed for that day.
- Habits and tips: "Tips this week": one sentence, five bars (new tasks, kind known, tip ready, shown, opened) with
  the step that lost the most marked, why tasks had no tip, and why ready tips were held.

## Tests (the controls)

- Domain: every step and every hold reason has words; each step counts under its own code; latest step wins; a dry
  day is found, named by its main reason, and cleared by a shown tip, a light day, today, or age.
- Core: every exit of the path writes its step end to end (`next_rung.rs`, `service.rs`); status carries the
  funnel and a dry day.
- CLI: the status text leads with the dry day and uses no "judge".
- Home: unit tests of the sentence, the marked step and the day words; e2e of the card, the note, See why, close,
  and axe in light and dark.
