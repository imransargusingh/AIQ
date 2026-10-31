---
id: "ui-review-<surface>"
title: "UI review — <Surface>"
type: doc
status: draft
owner: "@<handle — accountable for this review's truth (V13)>"
phase: "<delivery phase, if applicable>"
tags: [ui-review, ux, accessibility]
links:
  - { to: <the-spec-or-design-id>, rel: documents }
  - { to: <the-mockup-hub-id>, rel: relates-to }
review-by: "<ISO date — 90-day SLA for a doc (V13)>"
summary: >-
  <1–3 real sentences: which surface was reviewed, in which mode, the headline verdict,
  and the single highest-leverage fix. Not the title repeated.>
---

# UI review — <Surface>

*Produced by `/ui-design` (mode: **review** | **elevate**). Governed by `ui-design-craft.md` DX22–DX25 over the floors in `ui-interaction-design.md` (U1–U20). Every finding carries location · dimension · severity · evidence · fix · confidence. Delete what does not apply; **do not delete a dimension to avoid reporting on it** — mark it `n/a` with a reason.*

**Surface(s) reviewed:** <screens/components, and the states inspected>
**Reviewed against:** spec Part B/C `<path>` · `DESIGN.md` · archetype `<Signature>`
**Reviewers:** UX & Accessibility (lead, a11y hard veto) · UX Researcher/IA (UX veto) · The Simplifier · <others>
**Date:** <ISO> · **Mode:** <review | elevate>

## 1. Verdict

> **<PASS | PASS-WITH-CONDITIONS | BLOCK>** — <one sentence>.
> **Highest-leverage change:** <the single fix with the best improvement-to-effort ratio> (DX25).

| | Count |
|---|---|
| Blockers (sev 4, or any a11y ≥3) | |
| Majors (sev 3) | |
| Minors (sev 2) | |
| Nits (sev 1) | |

**Accessibility veto:** <PASS / BLOCK> — clears when: *<the falsifiable predicate; U16>*. Cleared by **<name — not the author>**.

## 2. Measurements (DX23 — measure before you diagnose)

*"Cluttered" is a symptom; the count is the diagnosis.*

| Metric | Value | Note |
|---|---|---|
| Interactive controls on the primary screen | | |
| Simultaneous sections / cards | | |
| Network calls on first load | | |
| Competing focal points | | should be 1 (U6/DX18) |
| Distinct type sizes | | against the declared scale |
| Distinct colours in use | | against the declared palette |
| Modes/views doing the same job | | >1 is an unmade decision handed to the user |
| Arbitrary values (non-token) in the component code | | should be 0 (U3) |
| `design-lint.py --strict` | pass / fail | |
| Worst text/surface contrast pairing | `x.xx:1` | AA needs 4.5:1 (3.0:1 large/UI) |
| Performance: LCP / INP / CLS | | against the stated budget (U17) |

## 3. Findings

*Ordered **structure before surface** (DX24): archetype and flow first, then states, then accessibility, then craft. Severity 0–4 → Blocker(4) / Major(3) / Minor(2) / Nit(1); an accessibility finding at ≥3 is a Blocker regardless of usability impact.*

| # | Location | Dimension | Sev | Evidence (observed / measured) | Recommended fix | Confidence |
|---|---|---|---|---|---|---|
| 1 | | Archetype fit | | | | Verified/Inferred/Flagged |
| 2 | | Flow & IA / findability | | | | |
| 3 | | State completeness (U9) | | | | |
| 4 | | Accessibility (U16) | | | | |
| 5 | | Token discipline (U3) | | | | |
| 6 | | Craft (hierarchy/space/colour/alignment) | | | | |
| 7 | | Content & copy | | | | |
| 8 | | Motion & stability | | | | |
| 9 | | Performance (U17) | | | | |
| 10 | | AI-surface honesty (U13–U15) | | | | |

## 4. Scorecard by dimension

| # | Dimension | Verdict | Worst finding |
|---|---|---|---|
| 1 | Visibility of system status | | |
| 2 | Match to the real world | | |
| 3 | User control & freedom | | |
| 4 | Consistency & standards | | |
| 5 | Error prevention | | |
| 6 | Recognition over recall | | |
| 7 | Flexibility & efficiency | | |
| 8 | Aesthetic & minimalist design | | |
| 9 | Error recovery | | |
| 10 | Help & documentation | | |
| 11 | **Archetype fit** | | |
| 12 | **State completeness** | | |
| 13 | **Token discipline** | | |
| 14 | **Accessibility (WCAG 2.2 AA)** | | |
| 15 | **Performance & stability** | | |
| 16 | **Content & copy** | | |
| 17 | **Craft** | | |
| 18 | **AI-surface honesty** | | |

## 5. Generic-tells self-check (DX3)

| Tell | Present? | If present: the deliberate justification |
|---|---|---|
| Default violet/indigo gradient or lone saturated blue | | |
| Everything in same-radius, same-shadow cards | | |
| Three equal stat tiles | | |
| Uniform spacing (no grouping rhythm) | | |
| One or two type sizes, weight doing all the work | | |
| Lorem / placeholder names / placeholder numbers | | |
| Emoji as iconography | | |
| Happy-path-only screens | | |
| Symmetry everywhere with no earned asymmetry | | |
| Motion on everything, or none at all | | |

## 6. The Simplifier's delete-list

*Tagged, one line per finding; ends with the only metric that matters (`solution-selection-ladder.md` L9).*

```
delete:  <dead control / unused option / speculative feature>
yagni:   <abstraction, mode or config with one caller>
shrink:  <same job, fewer elements>
native:  <hand-rolled thing the platform already does>
net: -<N> elements possible.
```

## 7. Ranked plan

**Must fix before ship (Blockers)**
1. <finding #> — <fix> — owner: <> — est: <>

**Should fix next (Majors, ranked by user impact × effort)**
1. <finding #> — <fix> — owner: <> — est: <>

**Worth doing (Minors / Nits)**
- <finding #> — <fix>

> **Do this one first:** <the single highest improvement-to-effort change, restated> — because <reason>.

## 8. Residual risk & what this review did not cover

- <states, personas, viewports, locales or devices not inspected>
- <claims that remain Inferred or Flagged, and what would verify them>

## 9. Defect classes registered (CI1)

*Any defect found here that belongs to a recurring shape is registered in `docs/lessons/defect-classes.md` with its control — the class, not the instance (`continuous-improvement.md` CI2).*

| Class ID | Shape | Control added | Status |
|---|---|---|---|
| | | | controlled / partially / uncontrolled |
