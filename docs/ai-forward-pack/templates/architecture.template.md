---
id: "<kebab-id, unique in the repo>"
title: "<Title>"
type: architecture
status: draft
owner: "@<handle — the human accountable for this artifact's truth (V13)>"
phase: "<delivery phase / vertical slice, if applicable>"
tags: []
links:
  - { to: <upstream-artifact-id>, rel: implements }   # typed edges — registry in knowledge-visualization.md V14
review-by: "<ISO date — SLA for this type: 180 days, or on any change to a linked downstream artifact (V13)>"
summary: >-
  <1–3 sentence real summary — shown in every Explorer view; not the title repeated>
---

<!--
TEMPLATE: Top-Level Architecture — produced by /define-architecture
Copy to docs/architecture.md. This is the top-level shape: boundaries, archetype,
tier allocation, the load-bearing decisions and their ADRs. Detailed component design
belongs in /design-slice, not here. Confidence labels [Verified]/[Inferred]/[Flagged] apply
to every contract. Unfamiliar contracts MUST be established by the Spike Protocol
(knowledge/spike-protocol.md) BEFORE they are committed here.
-->

# Architecture: <system name>

- **Status:** Draft | In review | Accepted
- **Tier:** T0 | T1 | T2
- **Driving spec:** docs/specs/<feature>.md
- **Author(s) / date:**

## Context & constraints
<!-- What the system must do (link the spec), and the hard constraints it lives under:
delegated identity boundary, data residency, latency/cost budgets, platform, existing
systems it must fit. Quality attributes that shape the structure go here. -->

## Archetype & rationale
<!-- LOA Part V. Which archetype (A–I) fits, and why. If none fits cleanly, say so and
justify the composition. Name the alternative archetype(s) considered and why rejected. -->

## Capability-tier allocation
<!-- LOA Tiers T0–T4. For each major capability, the tier it runs at and the justification
for the cheapest sufficient tier (P1). Determinism at the floor (P2); cognition and
execution separated (P3). -->
| Capability | Tier | Why this tier (not cheaper / not dearer) |
|---|---|---|
|  |  |  |

## Component map & boundaries
<!-- The major components, their responsibilities, and the boundaries between them.
Typed schema boundaries (P9); state pushed to the edges (P7); least-privilege delegated
identity (P11). A diagram (Mermaid) is welcome. -->

## Contracts at the seams (sourced)
<!-- Every API/library/protocol/MCP server/schema the architecture depends on, with its
source in the hierarchy of truth (BoK §III.1) and confidence label. Anything unfamiliar:
link the spike under spikes/ and the observed result. No guessed semantics (D2). -->
| Seam / dependency | Contract relied on | Source / spike | Confidence |
|---|---|---|---|

## Cross-cutting concerns
- **Identity & trust boundaries:**  <!-- delegated identity is a hard boundary -->
- **Failure & resilience:**  <!-- idempotency at side-effect boundaries (P8) -->
- **Observability:**  <!-- audit everything (P10); the debug story up front -->
- **Data governance / privacy:**

## Load-bearing decisions → ADRs
<!-- Each significant, hard-to-reverse decision gets an ADR (templates/adr.template.md →
docs/adr/NNNN-*.md). List them here with one-line summaries. -->
- ADR-NNNN: <decision> — <one line>

## Delivery phasing (vertical slices)
<!-- Define completely, phase vertically: the architecture above is whole; delivery is
partitioned into END-TO-END vertical slices. Phase 1 is the walking skeleton — the thinnest
complete path through every layer that proves the composition. Each phase must be deployable,
test-validated end-to-end, and human-validatable (a demo script), with mocks/stubs/fakes at
not-yet-built edges. Mocked seams are contracts: replacing the mock later is a substitution,
not a redesign. Never phase by horizontal layer (Big-Bang Integration). -->

| Phase | End-to-end capability it proves | Real | Mocked/stubbed (seam = contract) | Human validation (demo) | Test validation (E2E) | Unblocks |
|---|---|---|---|---|---|---|
| 1 (walking skeleton) |  |  |  |  |  |  |
| 2 |  |  |  |  |  |  |

## LOA conformance check
<!-- Walk conformance criteria C1–C11. Note any deliberate, recorded deviation
(Deviation Protocol). -->

## Flagged risks & residual unknowns

## Status & next action
<!-- Updated at the end of every /define-architecture run. -->
| | |
|---|---|
| **Completed** |  |
| **Remaining** |  <!-- phases not yet built, in order --> |
| **Best next action** |  |

## Gate record
<!-- Adversarial review: full architect council proportional to tier, Patterns Expert,
SRE; hard vetoes Security and Distributed Systems. Authors did not self-clear. -->
`GATE define-architecture · <date> · <personas> · criteria met: <…> · verdict: <…> · vetoes→resolution: <…>`

---
**Handoff:** → `/design-slice` per component.
