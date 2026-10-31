---
id: "<kebab-id, unique in the repo>"
title: "<Title>"
type: design
status: draft
owner: "@<handle — the human accountable for this artifact's truth (V13)>"
phase: "<delivery phase / vertical slice, if applicable>"
tags: []
links:
  - { to: <upstream-artifact-id>, rel: implements }   # typed edges — registry in knowledge-visualization.md V14
review-by: "<ISO date — SLA for this type: 180 days, or with the component's next change (V13)>"
summary: >-
  <1–3 sentence real summary — shown in every Explorer view; not the title repeated>
---

<!--
TEMPLATE: Detailed Component Design — produced by /design-slice
Copy to docs/design/<component>.md. The *how* for one component, within an accepted
architecture. Confidence labels apply to all contracts; unfamiliar ones established by
the Spike Protocol first.
-->

# Design: <component>

- **Status:** Draft | In review | Accepted
- **Spec / architecture:** docs/specs/<…>.md · docs/architecture.md
- **Delivery phase / vertical slice:**  <!-- which phase of the architecture's phasing plan this
  component serves; what is mocked around it at that phase; which of its seams are
  mock-substitutable contracts so the slice deploys end-to-end -->
- **Author(s) / date:**

## Responsibility
<!-- One paragraph: what this component is responsible for, and explicitly what it is not. -->

## Contracts
- **Exposed (what it offers):**  <!-- signatures, schemas, guarantees -->
- **Consumed (what it depends on):**  <!-- each with source + confidence; spikes linked -->

## Patterns
<!-- The established idiom/pattern used, NAMED and justified (Patterns Expert). Reinvention
must be defended; misapplied patterns are a finding. -->

## Data shapes
<!-- Types/DTOs/persistence shapes. Typed boundaries (P9). -->

## Error & concurrency model
<!-- Failure modes, retries, idempotency at side-effect boundaries (P8), cancellation,
threading/async correctness (Distributed Systems lens). -->

## Failure-mode analysis
<!-- Derive failure modes FROM the design choices (each pattern / dependency / boundary /
concurrency decision). Walk the categories so the common ones aren't missed: inputs
(null/empty/malformed/boundary/oversized/duplicate/out-of-order), each dependency
(timeout/unavailable/slow/partial or error response/rate-limited/contract drift), concurrency
(race/lost update/ordering/partial failure mid-operation), state & data
(staleness/inconsistency across stores/partial write/retry idempotency), resources
(exhaustion/leak/backpressure), and time (timeout/expiry/clock skew/retry storm). -->

| Failure mode | From which choice | Disposition | How it's addressed (or why accepted) | Detection (telemetry) | Test |
|---|---|---|---|---|---|
| <e.g. payment gateway times out> | <synchronous charge-on-checkout> | mitigate + detect | <Polly timeout + breaker; surface `ORD-0007`> | <error code `ORD-0007`, span status=error, alert> | <`Charge_GatewayTimeout_Returns503`> |

<!-- Disposition ∈ {prevent, detect, mitigate/degrade, recover, accept}. Nothing is left
implicit. "accept" MUST carry a rationale and the residual risk. Every handled load-bearing mode
becomes a negative/error-path entry in the test plan below — these are the cases the urge to
complete skips. -->
## Adversarial analysis (STRIDE-lite)
*Trust boundaries this component owns or crosses, and the threats walked per boundary
(design SKILL Stage 3). Disposition: mitigate (named control) / transfer (named upstream control) / accept (rationale + residual risk). Every mitigation has a negative security test in the plan.*

| Trust boundary | STRIDE threat | Disposition | Control / rationale | Negative test |
|---|---|---|---|---|
| "<e.g. public HTTP endpoint>" | "<e.g. S: forged bearer token>" | mitigate | "<authn middleware, audience+expiry validation>" | "<expired/forged token → 401, RFC 9457 body>" |

## Privacy analysis (LINDDUN-lite)
*Mandatory when the component touches personal data; otherwise one explicit line: "This
component touches no personal data (verified — list what was checked)." Disposition: mitigate
(named control) / accept (rationale + residual risk). The repo Privacy Review rolls this table
up mechanically (`docs-graph.py rollup`).*

| Data flow / category | LINDDUN finding | Disposition | Control / rationale | Retention & rights path |
|---|---|---|---|---|
| "<e.g. email in receipt flow>" | "<e.g. D: address leaks into request logs>" | mitigate | "<structured-log redaction; no-PII negative test>" | "<90d post-fulfilment; erasure via deletion job>" |

## UI & interaction design
*Mandatory when the component has a user-facing interface (UI Standard `ui-interaction-design.md` U19); otherwise delete this section. Judge excellence relative to the declared medium.*

**Medium(s) & platform guidelines:** "<web / native desktop / mobile / CLI / voice — and the authoritative HIG per medium: Apple HIG, Material, Fluent, GNOME, CLI conventions>" (U2)

**Tokens used / defined:** "<the semantic tokens this UI references, or the new primitive/semantic/component tokens it introduces — no arbitrary values>" (U3–U5)

**Key screens / flows:** "<each view's single focal point and visual hierarchy; the flow's steps and progressive disclosure>" (U6–U8)

**Component states (complete set — the polish gate):**

| Component | default | hover/focus | active | disabled | loading | empty | error | success | first-run / overflow |
|---|---|---|---|---|---|---|---|---|---|
| "<name>" | "<…>" | "<…>" | "<…>" | "<…>" | "<skeleton/optimistic>" | "<guides to first action>" | "<humane, actionable>" | "<…>" | "<…>" |

**Motion:** "<key transitions/micro-interactions and their purpose; prefers-reduced-motion behavior>" (U10)

**UI copy (load-bearing strings, real and in-voice):** "<labels, primary CTAs, empty-state text, error messages, confirmations>" (U11)

**Medium adaptation:** "<how the experience changes across the declared media — not one medium's shape forced onto another>" (U2)

**Accessibility (WCAG 2.2 AA) & performance budget:** "<contrast at token level, keyboard path, focus, semantics/labeling, target size; perceived-load / latency / frame-rate / payload budget for the medium>" (U16–U17)

**AI-UX (AI-facing UIs only):** applicable **HAX** guidelines "<G1–G16 across initially / during / when-wrong / over-time>" and **Shape-of-AI** patterns "<Wayfinders / Tuners / Governors (action plan, stream-of-thought, verification before irreversible) / Trust builders (disclosure, citations, provenance) / Identifiers>"; wrong-answer + uncertainty as first-class states "<correction, regeneration, honest confidence, graceful degradation>" (U13–U15)


## Telemetry
<!-- Per the Observability & Instrumentation Standard (observability-and-instrumentation.md):
the spans, the structured log events + trace context, the stable error codes, the RFC 9457
responses (HTTP), and the metrics (O1–O13) that make this observable and debuggable (P10). -->

## Test plan
<!-- Map to Testing Strategy directives (testing-strategy.md): the triggers that apply
(T1–T14) and the directives they pull in (D0–D7, A1–A6). The boundary set from the spec
becomes concrete tests. -->

## Conformance notes
<!-- Relevant LOA criteria (C1–C11) and how this design meets them; any recorded deviation. -->

## Flagged risks & residual unknowns

## Status & next action
<!-- Updated at the end of every run. -->
| | |
|---|---|
| **Completed** |  |
| **Remaining** |  |
| **Best next action** |  |

## Gate record
<!-- Adversaries: Patterns Expert + Simplifier (mutual check), Security, Distributed
Systems, Test Architect (hard vetoes as applicable). Author did not self-clear. -->
`GATE design · <date> · <personas> · criteria met: <…> · verdict: <…> · vetoes→resolution: <…>`

---
**Handoff:** → `/implement`.
