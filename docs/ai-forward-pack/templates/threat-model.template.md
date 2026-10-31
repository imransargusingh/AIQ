---
id: threat-model
title: "Threat Model"
type: threat-model
status: draft
owner: "@<handle — typically the Security & Identity Architect's human counterpart>"
phase: ""
tags: [security, threat-model]
links:
  - { to: architecture, rel: documents }
  - { to: "<each design id>", rel: documents }
review-by: "<ISO date — 180 days or on any change to a linked design; V16 flags this doc automatically>"
review-suggested: []
summary: >-
  "<The system's security posture in two sentences: the trust boundaries that matter and the
  dominant accepted risks>"
---

# Threat Model

*The repo-level rollup of every component's adversarial analysis (design SKILL Stage 3,
STRIDE-lite). Maintained by `/document` (full refresh) and updated by `/design-slice`//`/implement`
when a component's analysis changes. **The per-boundary register below is generated** — refresh
it with the script bundle, never by hand:*

```bash
python3 docs/ai-forward-pack/scripts/docs-graph.py rollup --heading "Adversarial analysis (STRIDE-lite)" --type design
```

*Because this document links `documents →` every design, a material design change flags this
doc `review-suggested` automatically (V16) — a pending flag here means the model is behind
reality.*

## 1. System trust-boundary map

The C4-Context view annotated with trust boundaries: every place data or control crosses from a
less-trusted to a more-trusted context (public ingress, queue consumers, third-party calls,
admin surfaces, file/config ingestion).

```mermaid
C4Context
  %% annotate boundaries; keep at Context/Container zoom (V8)
```

| # | Boundary | Less-trusted side | More-trusted side | Owning design |
|---|---|---|---|---|
| "B1" | "<public HTTP ingress>" | "<internet>" | "<API container>" | "<design-id>" |

## 2. Threat register (generated — see command above)

<!-- paste the rollup output here; the trailing comment records the generation date -->

## 3. Accepted-risk register

Every `accept` disposition across the system, in one place — the list leadership reviews.

| Boundary / threat | Accepted by | Rationale | Residual risk | Revisit when |
|---|---|---|---|---|
| "<…>" | "<@owner>" | "<…>" | "<…>" | "<condition or date>" |

## 4. Cross-cutting controls

The controls multiple boundaries rely on (the authn middleware, the gateway WAF, secret
management, signing infrastructure) — each a *named* artifact or platform feature, with the
designs that depend on it linked. A `transfer` disposition anywhere must name its control here.

## 5. Gaps and flagged unknowns

Boundaries without an owning design, designs without an adversarial analysis (a gate failure
upstream — record it, don't absorb it), and threats awaiting triage.
