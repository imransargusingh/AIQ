---
id: glossary
title: "Domain Glossary"
type: glossary
status: accepted
owner: "@<handle>"
phase: ""
tags: [glossary, ontology]
links: []
review-by: "<ISO date — 90-day SLA (knowledge-visualization.md V13)>"
summary: >-
  The governed vocabulary of this repository: one entry per load-bearing domain term, with the
  definition in the repo's voice, the near-miss disambiguation, and links to the artifacts that
  anchor it.
---

# Domain Glossary

*The ontology layer's term half (`knowledge-visualization.md` V14; the relation half — the `rel`
registry — lives in that doc). Specs, designs, and investigations link the load-bearing terms
they use via `uses-term` edges, so terminology resolves identically across artifacts and across
agent sessions. One entry per term; alphabetical; an entry is atomic and quotable.*

---

## <Term>

**Definition.** What it means *in this repository*, in one to three precise sentences — the
repo's voice, not a textbook's. If the repo's usage narrows or differs from industry usage, say
so explicitly.

**Not to be confused with.** The near-miss: the adjacent term people conflate it with, and the
distinction that matters (e.g. *settlement* is not *capture*; a trace-id is not a correlation-id).

**Anchored by.** The artifacts that define or constrain this term — link them:
`docs/specs/<…>.md`, `docs/design/<…>.md`, `docs/adr/<…>.md`.

**Canonical type/shape (if code-level).** The type, table, or message that *is* this term in the
codebase (`Order`, `orders.status`, `OrderShipped` event) — so the word and the code never drift.

---

## <Next term>

…

---

### Maintenance rules

- A term enters the glossary the first time two artifacts need it to mean the same thing.
- Renaming a term is a **supersede**: keep the old entry with `status: superseded`, link
  `supersedes` from the new one, and update `uses-term` edges in the same change (V11).
- Every entry's *Anchored by* links are real typed links — they appear in this file's
  frontmatter `links` (or per-entry inline links) so the Explorer's graph shows the term web.
- Stale per V13 after 90 days: re-read against reality, fix or confirm, bump `review-by`.
