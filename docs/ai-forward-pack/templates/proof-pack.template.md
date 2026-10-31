---
id: "<kebab-id, unique in the repo>"
title: "<Title>"
type: proof-pack
status: draft
owner: "@<handle — the human accountable for this artifact's truth (V13)>"
phase: "<delivery phase / vertical slice, if applicable>"
tags: []
links:
  - { to: <upstream-artifact-id>, rel: implements }   # typed edges — registry in knowledge-visualization.md V14
review-by: "<ISO date — SLA for this type: regenerate per release (V13)>"
summary: >-
  <1–3 sentence real summary — shown in every Explorer view; not the title repeated>
---

<!--
TEMPLATE: Proof Pack — produced by /implement (and any task that asserts "done")
Copy into the change (PR description or docs/proof/<change>.md). The Proof Pack is how a
claim of correctness is substantiated rather than self-certified (BoK D3; Rules of the
Road §3.1). One row per non-trivial claim the change makes. Verification is never
self-certified: the oracle and the observed red→green are the substance, not the verdict.
-->

# Proof Pack: <change / feature>

- **Change:** <PR / commit / branch>
- **Spec / design:** docs/specs/<…> · docs/design/<…>
- **Tier:** T0 | T1 | T2  <!-- T1/T2 require this pack committed with the change -->
- **Author / date:**

## Claims & evidence
<!-- For each claim the change makes (a behavior met, a bug fixed, a constraint held):
- Claim         — what is asserted true.
- Evidence      — what was actually observed (test output, run, profile, query result).
- Source/Oracle — the test or check that decides it, and WHY that oracle is trustworthy
                  (what it would catch; not "it passed" but "it fails when X breaks").
- Red observed  — for new tests: confirmation the test failed before the code (an
                  Unverified Green is a test that never could fail).
- Confidence    — [Verified] (observed) / [Inferred] / [Flagged].
- Residual risk — what this evidence does NOT cover. -->

### Claim 1: <…>
- **Evidence:**
- **Oracle (and why it's trustworthy):**
- **Red observed before green:** yes / n-a (why)
- **Confidence:**
- **Residual risk:**

### Claim 2: <…>
- **Evidence:**
- **Oracle:**
- **Red observed before green:**
- **Confidence:**
- **Residual risk:**

## Test coverage of the boundary set
<!-- The boundary set from the spec (empty/max/malformed/hostile/concurrent/unhappy):
which boundaries are covered by which test. Coverage % is a floor, not the claim
(testing-strategy.md). Note any uncovered boundary as a Flagged risk. -->

## Change reach & instrumentation (opt-in — delete whichever table does not apply)
<!-- Tier-2 of the prose->structure review (docs/proposals/prose-to-structure-review.html):
the structured home for E7/E8 (a change that adds/changes a data-carrying field) and IO2
(a new capability). Opt-in and piloted, NOT a mandatory block — the surface list differs per
architecture (E7), so adapt the rows to this change rather than treating them as fixed. -->

### Change-surface completeness (E7/E8) — for a data-carrying field change
<!-- Tick one row per surface the change actually reached; the canonical path below is a
default to adapt, not a fixed set. Then trace every new/changed field to a WRITER and a
COMPUTE reader: a field with no writer is an unimplemented design; a field with no compute
reader is dead weight round-trip tests will pass anyway (E8 / DM15). -->

| Surface | Reached? | Where (file / member) |
|---|---|---|
| store / schema | | |
| domain / model | | |
| service | | |
| projection / wire | | |
| client type | | |
| UI render | | |
| compute reader | | |

| New / changed field | Writer traced (where) | Compute reader traced (where) |
|---|---|---|
| "<field>" | | |

### Operator questions instrumented (IO2 / IO10) — for a new capability
<!-- Each question an operator asks within a month → its named emitting source on the normal
path (no flag, no re-run), observed at least once. A question with no source is a NAMED
instrumentation gap, not a blank (IO4 / IO7). -->

| Operator question | Emitting source (metric / span / log field) | Observed once? |
|---|---|---|
| how long does it take? | | |
| how often does it run? | | |
| how much does it cost? | | |
| which path did it take? | | |
| did it fail, and where? | | |

## Failure modes addressed
<!-- Carry the design's failure-mode analysis forward (design.template.md) plus any the
implementation introduced. One row per mode: how the code addresses it, and the negative/
error-path test that proves it (or why it is consciously accepted). -->

| Failure mode | Handled in code by | Proven by (test) | or Accepted (rationale + residual risk) |
|---|---|---|---|
| <timeout on dependency X> | <Polly timeout + `ORD-0007`> | <`Charge_GatewayTimeout_Returns503`> | — |
## Threats addressed (adversarial analysis)
*Every design-mitigated STRIDE threat → the enforcing code + the negative security test that proves it (red-first). Transfers name the upstream control; accepts carry the recorded rationale.*

| Boundary / threat | Disposition | Enforcing code | Negative security test | Result |
|---|---|---|---|---|
| "<boundary: threat>" | mitigate | "<file/member>" | "<test name>" | red→green |

## Privacy findings addressed (LINDDUN-lite)
*Every design privacy disposition → the enforcing code + the privacy test that proves it
(red-first): the no-PII-in-telemetry probe, the erasure that deletes, the retention job.*

| Data flow / finding | Disposition | Enforcing code | Privacy test | Result |
|---|---|---|---|---|
| "<flow: finding>" | mitigate | "<file/member>" | "<test name>" | red→green |

## UI states & floors proven (if the component has an interface)
*Each specified component state built and verified, and the accessibility + performance floors proven red-first (UI Standard U20). Delete if no UI.*

| Item | Built | Test (red→green) | Result |
|---|---|---|---|
| "<component> empty state" | yes | "<render test>" | red→green |
| "<component> loading state" | yes | "<render test>" | red→green |
| "<component> error state" | yes | "<render test>" | red→green |
| Keyboard-only path | — | "<a11y test>" | pass |
| Contrast / labeling (WCAG 2.2 AA) | — | "<a11y test>" | pass |
| Performance budget "<metric>" | — | "<threshold assertion>" | pass |
| AI-UI pattern "<e.g. action plan before irreversible action>" | yes | "<test>" | red→green |


## Testing Strategy directives applied
<!-- The triggers (T1–T14) that fired and the directives (D0–D7, A1–A6) they pulled in.
For AI-composed surfaces, the probabilistic-content vs deterministic-structure split. -->

## Verification commands
<!-- The exact commands a reviewer can run to reproduce the evidence (build, test,
analyzers, mutation if applicable). Reproducibility is the point. -->
```
```

## Flagged risks / residual unknowns

## Status & next action
<!-- Updated at the end of every run. -->
| | |
|---|---|
| **Completed** |  |
| **Remaining** |  |
| **Best next action** |  |

## Gate record
<!-- Pre-merge adversarial review: Test Architect (hard veto on unverifiable claims),
SRE, relevant architects, language Developer. Author did not clear its own hard veto. -->
`GATE implement · <date> · <personas> · criteria met: <…> · verdict: <…> · vetoes→resolution: <…>`
