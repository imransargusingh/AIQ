---
id: "<kebab-id, unique in the repo>"
title: "<Title>"
type: investigation
status: draft
owner: "@<handle — the human accountable for this artifact's truth (V13)>"
phase: "<delivery phase / vertical slice, if applicable>"
tags: []
links:
  - { to: <upstream-artifact-id>, rel: implements }   # typed edges — registry in knowledge-visualization.md V14
review-by: "<ISO date — SLA for this type: frozen once resolved (immutable record — leave empty) (V13)>"
summary: >-
  <1–3 sentence real summary — shown in every Explorer view; not the title repeated>
---

<!--
TEMPLATE: Investigation / Root-Cause Report — produced by /investigate
Copy to docs/investigations/<id>.md. The discipline: do not propose a fix until the
root cause is verified by data as both NECESSARY and SUFFICIENT. A plausible cause is
not a verified cause. Confidence labels [Verified]/[Inferred]/[Flagged] apply throughout.
-->

# Investigation: <id> — <short symptom>

- **Status:** Open | Root cause verified | Fix proposed | Resolved
- **Severity / tier:**
- **Reported by / date:**
- **Related spec/design-slice:**

## Symptom
<!-- What was observed, precisely — not the suspected cause. Expected vs actual. -->

## Reproduction
<!-- The minimal, reliable steps (or the conditions under which it occurs). If not yet
reproducible, that is the first task; say so. A bug you cannot reproduce, you cannot
prove you fixed. -->

## Timeline
<!-- What changed and when (Change Analysis): deploys, config, data, dependency versions,
load. Correlate the symptom's onset to a change where possible. -->

## System map
<!-- The relevant slice of the system the symptom lives in (stocks/flows/feedback,
boundaries, the components on the path). Structure produces behavior. -->

## Hypotheses considered
<!-- Every candidate cause examined. RCA method chosen for the problem shape (5 Whys /
Ishikawa / Fault Tree / Change Analysis / Events-and-Causal-Factors) — name it. -->
| Hypothesis | Predicts we'd see… | Evidence for | Evidence against | Verdict |
|---|---|---|---|---|
|  |  |  |  |  |

## Verified root cause
<!-- The cause shown by data to be NECESSARY (remove it and the symptom goes away) and
SUFFICIENT (present, it produces the symptom). State the evidence explicitly. Distinguish
the proximate cause from the systemic cause — prefer the systemic fix over an
administrative band-aid. -->

## Causes ruled out
<!-- The hypotheses rejected, each with the specific evidence that rejected it. This is
the disconfirmation record (Rigor Protocol Stage 4) and it is load-bearing. -->

## Specific fix(es) for this instance
<!-- The systemic fix for the found instance, its blast radius, and its rollback. -->
- **Fix:**
- **Why systemic (not a band-aid):**
- **Rollback / mitigation:**
- **Failing-first regression test:**  <!-- the test that fails on today's code and passes
  on the fix — the proof the cause is understood and the bug cannot silently return. -->

## Generalization — the failure class
<!-- Step back from the instance to the class (Stage 6). -->
- **Failure class (named):**  <!-- the verified cause in its general form -->
- **Broader systemic solution:**  <!-- the reusable rule/pattern/guard; the analyzer or test
  that catches the class automatically (Testing Strategy / Observability Standard tie-in) -->

| Candidate sibling (where) | Same class because… | Verdict (confirmed / ruled out) | Evidence |
|---|---|---|---|
|  |  |  |  |

## Phased repair plan
<!-- Each item = an independently verifiable unit of CODE + TESTS, ordered by risk/blast
radius, each deployable on its own (vertical-slice discipline). Include the class-prevention
item (analyzer / shared guard / contract test) as its own phase. -->

| Phase | Repair item (code + tests) | Failure mode it eliminates | Validation | Depends on |
|---|---|---|---|---|
| 1 |  |  |  |  |
| 2 |  |  |  |  |

> **STOP — human review gate.** `/investigate` ends here, even when running autonomously.
> Review the root cause, the generalization, and this plan; explicitly approve which phases
> to execute. Implementation begins only after approval (→ `/implement` per approved phase).

## Residual risk & follow-ups

## Gate record
<!-- Adversaries: SRE, Distributed Systems, Security (if the fix touches a trust boundary —
hard veto), Test Architect (the fix must be proven — hard veto). Investigator did not
self-clear the fix. -->
`GATE investigate · <date> · <personas> · root cause verified: <y/n> · verdict: <…> · vetoes→resolution: <…>`
