---
id: "<kebab-id, unique in the repo>"
title: "<Title>"
type: adr
status: draft
owner: "@<handle — the human accountable for this artifact's truth (V13)>"
phase: "<delivery phase / vertical slice, if applicable>"
tags: []
links:
  - { to: <upstream-artifact-id>, rel: implements }   # typed edges — registry in knowledge-visualization.md V14
review-by: "<ISO date — SLA for this type: none while accepted — re-date only when superseded (V13)>"
summary: >-
  <1–3 sentence real summary — shown in every Explorer view; not the title repeated>
---

<!--
TEMPLATE: Architecture Decision Record
Copy to docs/adr/NNNN-<decision>.md (zero-padded, monotonically increasing).
This is the compact form. For the full field set, follow the ADR template in
Layered-Optimized Architecture Appendix F; this template is the same shape, abbreviated.
One decision per record. ADRs are immutable once Accepted — supersede, don't edit.
-->

# ADR-NNNN: <short decision title>

- **Status:** Proposed | Accepted | Superseded by ADR-MMMM
- **Date:**
- **Deciders:**
- **Context spec/architecture:** docs/architecture.md

## Context
<!-- The forces at play: the requirement, the constraints, the tension that forces a
choice. State the relevant LOA principles (P1–P11) in play. -->

## Decision
<!-- The choice, stated in one or two sentences in active voice: "We will …". -->

## Alternatives considered
<!-- Each real alternative and the specific reason it lost. A decision with no rejected
alternative was not a decision. -->
- **<Alternative>:** rejected because …

## Consequences
- **Positive:**
- **Negative / accepted trade-offs:**
- **Follow-ups / new risks:**

## Evidence
<!-- Any spike (spikes/…) or sourced contract (BoK §III.1) the decision rests on, with
confidence label. No guessed semantics. -->
