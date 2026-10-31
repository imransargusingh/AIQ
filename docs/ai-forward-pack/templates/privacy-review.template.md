---
id: privacy-review
title: "Privacy Review"
type: privacy-review
status: draft
owner: "@<handle — typically the Privacy & Data Governance lens's human counterpart>"
phase: ""
tags: [privacy, data-governance]
links:
  - { to: architecture, rel: documents }
  - { to: "<each design id that touches personal data>", rel: documents }
review-by: "<ISO date — 180 days or on any change to a linked design; V16 flags this doc automatically>"
review-suggested: []
summary: >-
  "<What personal data the system touches, the dominant privacy risks, and the open
  compliance items — two or three sentences>"
---

# Privacy Review

*The repo-level rollup of every component's privacy analysis (design SKILL Stage 3,
LINDDUN-lite — the privacy analog of STRIDE). Maintained by `/document` (full refresh) and
updated by `/design-slice`//`/implement` when a component's data handling changes. **The findings
register is generated** — refresh it with the script bundle:*

```bash
python3 docs/ai-forward-pack/scripts/docs-graph.py rollup --heading "Privacy analysis (LINDDUN-lite)" --type design
```

*This document links `documents →` every personal-data-touching design, so a material design
change flags it `review-suggested` automatically (V16).*

## 1. Personal-data inventory

One row per personal-data category the system stores or moves. If this table is empty and
true, the rest of this document collapses to "no personal data — re-verify on any new ingress."

| Category | Examples | Where stored | Where it flows | Lawful basis / purpose | Retention | Owning design |
|---|---|---|---|---|---|---|
| "<contact>" | "<email>" | "<orders DB>" | "<receipt mail, PSP>" | "<contract>" | "<90d post-fulfilment>" | "<design-id>" |

## 2. Findings register (generated — see command above)

<!-- paste the rollup output here -->

## 3. Data-subject rights paths

How access / rectification / erasure / portability requests are actually fulfilled — the
concrete mechanism per category (not "we comply"): the deletion job, the export endpoint, the
audit event that proves execution (Observability Standard audit events).

## 4. Telemetry alignment (O-series)

The Observability Standard forbids PII in logs/traces/metrics. Record here the verification:
which structured events were checked, the redaction/hashing controls in place, and the negative
test that proves a sample PII value does not survive into telemetry.

## 5. Transfers and processors

Third parties receiving personal data (the PSP, mail provider, analytics), the legal mechanism
per transfer, and the design link that introduces each.

## 6. Accepted privacy risks

| Finding | Accepted by | Rationale | Residual risk | Revisit when |
|---|---|---|---|---|
| "<…>" | "<@owner>" | "<…>" | "<…>" | "<condition or date>" |

## 7. Gaps and flagged unknowns

Personal data with no owning design, designs touching personal data with no privacy analysis
(an upstream gate failure — record it), unresolved retention or basis questions.

---

### LINDDUN-lite reference (the categories the per-design analyses walk)

**L**inkability (can records be tied together across contexts?), **I**dentifiability (can a
person be singled out?), **N**on-repudiation (is a person *unable to deny* an action where they
should be able to?), **D**etectability (does the existence of a record leak something?),
**D**isclosure (does data reach parties it shouldn't — overlaps STRIDE-I, including via logs?),
**U**nawareness (does the person not know what's collected?), **N**on-compliance (retention,
basis, or rights paths violated?).
