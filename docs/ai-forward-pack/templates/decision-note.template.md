---
id: "<note-YYYYMMDD-kebab-slug>"
title: "<Decision or assumption, as a statement>"
type: decision-note
status: draft
owner: "<@handle — whoever made or relies on the call>"
phase: "<delivery phase, if applicable>"
tags: [decision-note]
links:
  - { to: "<artifact-this-shaped>", rel: relates-to }
review-by: "<ISO date — 180-day SLA; for assumptions, sooner if the validation condition can trip earlier (V13)>"
review-suggested: []
summary: >-
  "<The decision/assumption in one sentence, plus its blast radius in a second>"
---

# <Decision or assumption, as a statement>

*A decision note (`knowledge-visualization.md` V17): below ADR weight, above chat-scrollback
weight. One note per call; written before the session that made it closes.*

- **Kind:** decision | assumption | resolved-question
- **Confidence:** Verified | Inferred | Flagged  *(BoK labels — how was this established?)*
- **Made during:** `<skill run / session context — e.g. /implement of design-payment-gateway>`

## The call
What was decided or assumed, precisely enough that a stranger could apply it. One short
paragraph of **why** — the constraint, evidence, or trade-off that settled it.

## Alternatives dismissed
- `<alternative>` — `<one line: why not>`

## Validation condition *(assumptions only — when does this need re-checking?)*
"Holds until/unless `<the observable condition — a dependency upgrade, a load threshold, a
contract change, a date>`." When the condition trips, re-validate: confirm (bump `review-by`),
retire (`status: superseded`), or escalate.

## Promotion rule
If this call starts bearing load — multiple artifacts depend on it, or reversing it would be
expensive — **promote it to an ADR**: write the ADR, link it `supersedes` this note, set this
note's status to `superseded`. The note remains as the decision's origin story.
