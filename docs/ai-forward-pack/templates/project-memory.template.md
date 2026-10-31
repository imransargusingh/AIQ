---
id: project-memory
title: "Project Memory"
type: doc
status: draft
owner: "@<handle — the human accountable for this ledger's truth (V13)>"
phase: ""
tags: [memory, continuity]
links:
  - { to: architecture, rel: relates-to }
review-by: "<ISO date — 90 days (V13); the freshness gate flags a rotting ledger>"
summary: >-
  <1–3 sentences: the durable, append-only record of what this project has learned and decided —
  read at every skill's grounding, appended to at every skill's convergence.>
---

<!--
TEMPLATE: Project memory ledger — the rolling, graph-linked record of decisions & learnings.
Copy to docs/project-memory.md. Design: docs/design/project-memory.md.

HOW IT WORKS (the convention that makes it load-bearing):
- READ at grounding: every skill, as its first action, reads this file if present, so prior
  decisions/learnings inform the work (alongside the V15 graph traversal).
- APPEND at convergence: every skill, as part of its last action (the Discoverability Mandate
  V10 / decision-note step V17), adds an entry for any material decision or learning.
- Entries are APPEND-ONLY (history is the value); never rewrite a past entry.
- Frontmatter/graph WINS over the ledger (V2) — this is narrative, not authority.
- NO secrets / PII in entries (reference @handles, not emails). Run `scrub.py` over this file;
  use gitleaks/Presidio in CI for real enforcement.
- OBSIDIAN (optional lens): docs/ is already a valid Obsidian vault — open it to get Properties,
  Dataview queries, and a graph view over this ledger. Obsidian is a reader, never required.
-->

# Project memory

## Project facts (durable)
*Standing invariants a contributor or agent should know before touching this project. Edit in place as facts change; this block is the exception to append-only.*

- <e.g. "Source of truth is `pack/`; `.claude/`+`.github/`+`docs/` are generated — never hand-edit.">
- <e.g. "All scripts are stdlib-only Python; the CLI is `tools/aiforward.py`.">

## Log (newest first)
*Append an entry at each skill's convergence. Format: `### <ISO date> — <headline>` + 1–3 sentences + a confidence label + back-links to touched artifacts.*

_No entries yet — skills append here at convergence._

<!-- entry template:
### 2026-06-14 — <what was decided or learned, in a headline>
<1–3 sentences of context: the decision/learning and why it mattered.> *(Verified | Inferred | Flagged)*
Touches: [[<artifact-id>]], [[<artifact-id>]]
-->
