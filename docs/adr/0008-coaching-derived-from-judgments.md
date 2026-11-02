---
id: adr-0008
title: "ADR-0008: Coaching is derived from judgments; only feedback and guesses are stored"
type: adr
status: proposed
owner: "Imran"
phase: "3b+5"
tags: [architecture, data-model, coaching, habits]
links:
  - { to: aiq-architecture, rel: refines }
  - { to: adr-0002, rel: refines }
  - { to: spec-coaching-loop, rel: implements }
  - { to: spec-aiq-home, rel: implements }
review-by: 2027-05-02
summary: >-
  Task types come from a fixed per-job list the judge picks from. Moves, opportunity rows, Home's
  finished / could-finish split and habit states are pure derivations over the append-only judgment
  facts, computed by one domain module for both the core and AIQ home. Only the person's feedback,
  "This doesn't fit" flags and the core's task-type guesses are new stored facts.
---

# ADR-0008: Coaching is derived from judgments; only feedback and guesses are stored

## Context

The rung judge (PR #24) stores one append-only `judgment` row per judgment version of a task, with its
facts. Coaching needs four more things: a recurring **task type** to count and rank, the **move** that
gets a type to its next rung, Home's **finished / could finish / needs you** split, and **habit**
states (Not yet, Tried, Adopted, with lapse). ADR-0002 sets the house rule: dimensions plus
append-only facts, derive what can be derived. Re-judgments, rubric updates and the person's deletions
all change the judgments a habit or a row rests on.

## Decision

1. **Task type is a categorical fact on the judgment**, chosen by the judge from a fixed per-job list
   in the pack (`judgment.task_type`, schema v6, expand only). Not free text, not clustered on device.
2. **Moves, task-type rows, the split, habit observations and habit states are derived** on read by
   `aiq_domain::coach`, a pure module. The core (in-the-moment tips) and AIQ home (views) call the same
   functions, so a tip and a page can never disagree about a move.
3. **Habit state is replayed**, not stored: observations in day order through the D5 rule. A milestone
   card is idempotent by a derived id in the existing `card_delivery`.
4. **New stored facts are only what can't be derived:** `tip_feedback` (the person's answers),
   `guidance_flag` ("This doesn't fit" and undo) and `type_guess` (the core's guess at a live task's
   type, needed to measure the guess later). All append-only, all covered by `forget`.

## Consequences

- A re-judgment, a pack update or a deletion changes every number and habit at once, with no stale
  state to repair and no backfill.
- Replay cost grows with history. `simplify:` the replay reads 30 days of judgments for rows and the
  split and all judgments for habits; upgrade to a per-day checkpoint when a snapshot's replay passes
  50 ms (measured in `home.log`).
- A habit can change state without the person doing anything new (a re-judgment). Accepted: the judgment
  is the record; the Habits page shows "seen in N of M tasks", so the cause is visible.
- Changing the task-type list is a pack change; judgments keep the type they were given, and types
  dropped from the list show as `other` until re-judged.

## Alternatives considered

- **Store habit state as a fact table** (one row per transition). Rejected: transitions would go stale on
  re-judgment and need a repair job; two definitions of one quantity (DM7).
- **Cluster first prompts on device into task types.** Rejected: names would drift, rows would split and
  merge week to week, and nothing could be labelled for the eval.
- **Use the judge's free-text gap as the tip.** Rejected: model text on a card is an injection path
  (TR-5), and free text has no identity to count as a habit.
