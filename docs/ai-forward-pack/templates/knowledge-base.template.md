---
id: "<kebab-id, unique in the repo>"
title: "<Title>"
type: knowledge
status: draft
owner: "@<handle — the human accountable for this artifact's truth (V13)>"
phase: "<delivery phase / vertical slice, if applicable>"
tags: []
links:
  - { to: <upstream-artifact-id>, rel: implements }   # typed edges — registry in knowledge-visualization.md V14
review-by: "<ISO date — SLA for this type: 90 days (V13)>"
summary: >-
  <1–3 sentence real summary — shown in every Explorer view; not the title repeated>
---

# Domain knowledge base — structure

*Template for the `/collectknowledge` skill. Saved to `docs/knowledge/<topic>/`. Every claim carries a source and a confidence label: **Verified** (authoritative, current primary source) · **Inferred** (reasoned from sources) · **Flagged** (single/weak/dated/contested). Replace placeholders; delete what does not apply.*

```
docs/knowledge/<topic>/
├─ index.md               # synthesis: headline findings, confidence summary, design implications
├─ state-of-the-art.md    # current best practice, leading techniques, the frontier
├─ comparables.md         # how existing products / the literature frame & solve this (named)
├─ references.md          # standards, regulations, specs, seminal papers (with links)
├─ data-and-constants.md  # formulae, benchmarks, datasets, domain constants, units, invariants
├─ glossary.md            # the domain's ubiquitous language, defined
├─ open-questions.md      # what research could NOT settle; known domain failure modes
└─ sources.md             # full source list with access dates
```

---

## `index.md`

```markdown
# <Topic> — domain knowledge

**Domain & problem:** <one-paragraph statement of what this project addresses>.
**Canonical framing:** <how the field frames this problem> — <note any divergence from our framing>.
**Compiled:** <date> · **Lead:** Domain Researcher · **Status:** <fresh | needs refresh>

## Headline findings
1. <finding> — *(Verified, [src])*
2. <finding> — *(Inferred)*
3. <finding> — *(Flagged — single source / dated)*

## Confidence summary
- Verified: <n> · Inferred: <n> · Flagged: <n>. The Flagged claims that are load-bearing: <list>.

## Design implications (what the next phase should do with this)
- <implication for /specify, /define-architecture, or /design-slice>
- <constraint, standard, or invariant the design MUST honor>
- <a comparable worth borrowing / avoiding, and why>

## How to use this base
Personas and the design skills cite these files as evidence (BoK §III.1). Refresh when the domain moves; re-run `/collectknowledge` and bump this date.
```

## `state-of-the-art.md`

```markdown
# State of the art — <topic>

## Current best-practice approaches
- **<approach>** — what it is, where it wins, where it fails. *(Verified, [src, dated])*

## Leading techniques / recent advances
- **<technique>** — maturity, adoption, trade-offs. *(label, [src])*

## The frontier / open research
- <what is unsolved or actively moving> *(label, [src])*
```

## `comparables.md`

```markdown
# Comparable solutions & problem framings

| Solution / source | How it frames the problem | Approach | Does well | Does badly | Confidence |
|---|---|---|---|---|---|
| <product / paper> | <framing> | <approach> | <strength> | <weakness> | Verified [src] |

## Adjacent problems worth borrowing from
- <adjacent domain> — <transferable idea> *(label)*
```

## `references.md`

```markdown
# Reference information

## Standards & regulations
- **<standard/reg>** — scope, what it requires of us. *(Verified, [official src])*

## Specifications & primary sources
- **<spec>** — the authoritative definition of <X>. *([src])*

## Seminal / foundational works
- **<author, year>** — why it matters. *([src])*
```

## `data-and-constants.md`

```markdown
# Domain data, constants & invariants
- **Formula / model:** <name> — <expression>, with units. *(Verified, [src])*
- **Benchmark / dataset:** <name> — what it measures, where to get it. *([src])*
- **Constant:** <name> = <value> (<units>). *([src])*
- **Invariant / edge case:** <statement that must always hold, or the boundary that breaks things>.
```

## `glossary.md`

```markdown
# Glossary — ubiquitous language
- **<Term>** — precise domain definition. *(label, [src])* <!-- use this exact term in code & specs -->
```

## `open-questions.md`

```markdown
# Open questions & domain failure modes
## Unresolved by research
- <question> — what we'd need to settle it; current best guess (Flagged).
## Known failure modes of this domain
- <how things go wrong here, where errors are silent or expensive>.
## Disconfirming views we deliberately sought
- <the strongest counter-argument to our headline findings, and how it fared>.
```

## `sources.md`

```markdown
# Sources
| # | Title / source | Type (primary/standard/secondary) | URL | Accessed | Used for |
|---|---|---|---|---|---|
| 1 | <title> | standard | <url> | <date> | <claim it supports> |
```
