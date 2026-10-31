---
name: collectknowledge
description: Before (or at the start of) design, run deep research on the project's domain and problem and save it as a structured markdown knowledge base in the repository (docs/knowledge/). Captures industry state of the art, comparable solutions and problem framings, authoritative references, key data, and a glossary — every claim sourced and confidence-labeled. Bootstraps domain expertise for the whole team and for the personas. Use when starting a project in an unfamiliar or high-stakes domain, or whenever design would otherwise rest on assumptions.
runs_as: either
---

# Skill: /collectknowledge

Build the project's **domain knowledge base**: a researched, sourced, durable body of markdown in the repo that everyone — humans and personas — reasons from. It exists to replace *assumed* domain understanding with *established* understanding **before** design commits to anything, and to bootstrap domain-specific expertise the team may not yet have.

**Spine:** runs the Rigor Protocol (`knowledge/rigor-protocol.md`) on the *domain*, weighted hard toward **Stage 3 EVIDENCE** and **Stage 4 DISCONFIRM**. **Mode:** Peer Mode to gather, Adversary Mode to attack the evidence. **Lead:** the **Domain Researcher** (this is exactly its charter — ground claims in evidence; no new agent is minted).

> **Where it sits in the flow.** `/collectknowledge` runs **first**, before or as the opening move of design:
> `/collectknowledge` (build the evidence base) → `/adddomainexperts` (instantiate expert lenses that *cite* this base) → `/specify` → `/define-architecture` → `/design-slice` → `/implement` → `/document`.
> It is the complement of `/adddomainexperts`: that skill adds the *judgment* (personas) for a domain; this skill assembles the *knowledge* (evidence) about it. Run knowledge-first so the experts and the spec stand on sourced ground.

## Grounding (first action)
CO-S0 applies first — the sentence is `reference/co-s0.md`. Before researching, load what the repo already knows and treat it as the **authoritative source of truth** (Rigor Protocol Stage 0; BoK §III.1): any existing `docs/knowledge/<topic>/` for this topic and the spec or problem that motivated the research. Extend and reconcile existing knowledge rather than duplicating or silently contradicting it; if new sources overturn a prior finding, **flag the change explicitly**. Prefer **graph traversal** for this grounding (`knowledge-visualization.md` V15): start from this task's artifact(s) in the knowledge graph and follow the typed edges 1–2 hops (upstream `implements`/`refines`/`depends-on`, downstream `tested-by`/`documents`, and `uses-term` into the glossary), citing the traversal path; a missing edge, stale node, or orphan found here is a finding to surface. Skip this only if the user explicitly tells you not to consult prior artifacts.

## Input
From the prompt, the user states **the domain and the problem they are solving** (e.g. "real-time fraud scoring for card-present retail transactions"; "a CFD solver for transonic wing sections"; "a clinical triage assistant for emergency intake"). One or two sentences is enough; the skill expands it into a research frame.

## Cast
- **Peers (research together):** **Domain Researcher** (lead — searches, reads, synthesizes, sources), **Product Strategist** (keeps the research anchored to the problem that matters, not a literature tour), plus any **domain experts already added** by `/adddomainexperts` (they direct the research and vet domain claims).
- **Adversaries (attack the evidence at the gate):** **Domain Researcher** in Adversary Mode (source authority, currency, disconfirming evidence), **The Simplifier** (is this knowledge load-bearing for the project, or a tangent — soft veto on scope sprawl), **Security & Identity / Privacy** if the domain implies regulated data.

## Flow (Rigor Protocol, specialized to the domain)

**Stage 0 — Interdict the rush.** Do not start designing or recommending a solution. The deliverable is *understanding*, not a decision. Resist the urge to converge on the first framing the prompt suggests.

**Stage 1 — OPEN.** Frame the domain broadly: what *is* this domain; what are the sub-areas; who are the established players, standards bodies, and seminal sources; what are the canonical problem framings (is the user's framing the standard one, or idiosyncratic?); what reference data, regulations, and prior art plausibly bear on it. Produce the research map — the questions to answer.

**Stage 2 — INTERROGATE.** Turn the map into precise research questions (Precision Questioning types): *Clarification* — what exactly does this term mean in the domain; *Assumption* — what is the field taking for granted; *Evidence* — what does the state of the art actually show; *Cause* — why do existing solutions make the trade-offs they do; *Significance* — which of these facts changes our design.

**Stage 3 — EVIDENCE (deep research; the heart of /collectknowledge).** First inspect the active
harness and selected model for a native deep-research capability. When one is available, use it for
the broad research pass; otherwise use `web_search`/`web_fetch`. Deep-research output is a lead, not
authority: open and verify the cited primary sources before promoting a claim. The Domain Researcher
then gathers and **sources** with the source-of-truth hierarchy BoK §III.1: standards/specs/primary
sources > official docs > reputable secondary > forums:
- **State of the art** — current best-practice approaches, leading techniques, recent advances, and where the frontier is.
- **Comparable solutions / problem statements** — how existing products and the literature frame and solve this problem (named, with what each does well and badly); adjacent problems worth borrowing from.
- **Reference information & data** — the standards, regulations, formulae, benchmarks, datasets, and domain constants that pertain; the units, invariants, and edge cases of the domain.
- **Glossary** — the domain's ubiquitous language, defined.
- **Open questions & risks** — what the research could *not* settle, and the domain's known failure modes.
Every claim is labeled **Verified** (primary/authoritative source, current) / **Inferred** / **Flagged** (single weak source, dated, or contested), each with a citation. Recency matters — note the date of fast-moving facts.

**Stage 4 — DISCONFIRM (the gate).** Adversary Mode: attack the evidence base. Is each load-bearing claim from an authoritative, *current* source? Where is the disconfirming view — did we seek the strongest counter-argument, or just confirming sources (the disconfirmation principle)? Any claim doing real work that rests on one weak source is downgraded to Flagged and called out. The Simplifier strikes research that is interesting but not load-bearing for the project. The researcher does not bless their own sourcing.

**Stage 5 — CONVERGE — write the knowledge base.** Save structured markdown to **`docs/knowledge/<topic>/`** (from `templates/knowledge-base.template.md`):
`index.md` (the synthesis + how to use it), `state-of-the-art.md`, `comparables.md`, `references.md`, `data-and-constants.md`, `glossary.md`, `open-questions.md`, and `sources.md` (the full source list with access dates). The index states the headline findings, the confidence summary, and the design implications. Commit it.

> **Directory note.** This human-facing domain knowledge lives in **`docs/knowledge/`**, deliberately separate from the pack's own `.claude/knowledge/` (the reasoning internals). It is the project's evidence base; personas and the design skills cite it.

## Output artifact
`docs/knowledge/<topic>/` — a sourced, confidence-labeled markdown knowledge base (state of the art, comparables, references, data/constants, glossary, open questions, sources), with an index synthesizing the findings and their design implications. Committed alongside code; refreshed when the domain moves.

## Definition of done (exit gate)
- [ ] The domain and problem are framed; the canonical framing is identified (and any divergence from the user's framing noted).
- [ ] State of the art, comparables, references/data, and glossary are present and **sourced**.
- [ ] Every load-bearing claim is **confidence-labeled**, with the disconfirming view actively sought (no confirmation-only research).
- [ ] Open questions and domain failure modes are recorded.
- [ ] The Simplifier gate passed — the base is load-bearing, not a literature tour.
- [ ] The index states the **design implications** (so the next phase can use it).

## Documentation & discoverability (last action)
Per the **Knowledge Visualization & Docs Explorer Standard** (`knowledge/knowledge-visualization.md`, the Discoverability Mandate V10): after producing the knowledge base (`docs/knowledge/<topic>/`), **write the artifact's frontmatter** (the record, V2: id, title, type, status, **owner**, phase, tags, **typed links** per the relation registry, **review-by** per the type's SLA, and a real 1–3-sentence summary) and **sync the derived `docs/docs-index.js`** by running the script bundle — `python3 docs/ai-forward-pack/scripts/docs-graph.py derive` (and `flag --changed <id> --reason …` for V16 propagation) — never by generating ad-hoc scripts (V18); frontmatter wins wherever the two disagree. Ensure `docs/index.html` (the Docs Explorer) exists — instantiate it from `templates/docs-explorer.template.html` if missing — and verify the new entry has at least one typed link into the graph (an orphan is a finding). Index and diagrams land **in the same change** as the content (V11). **Propagate impact (V16):** if this change is material (a contract, requirement, decision, public shape, or proven claim changed), traverse the **inbound** edges and push `review-suggested: { by, on, reason }` into each inbound neighbor's frontmatter (and its index entry) in this same change. **Capture session exhaust (V17):** any mid-session decision, discovered assumption, or resolved question below ADR weight becomes a linked **decision note** (`docs/notes/`, from `templates/decision-note.template.md`) before close — promote to an ADR if it later bears load. Work that is not discoverable in the Explorer is not done.

**Audit & change (last action).** Append an audit-log entry for this run — `python3 docs/ai-forward-pack/scripts/audit-log.py append --shortname "collectknowledge-<topic>" --session "<id>" --skill collectknowledge --kind skill --prompt "<the prompt, verbatim>" --summary "<headline findings>" --artifact docs/knowledge/<topic>/index.md` — per the Audit Mandate (`knowledge/audit-and-change-log.md`, AL5). Because this skill establishes load-bearing knowledge that shapes design, **also append a change-log entry** capturing it and its git context — `… audit-log.py change --title "<the knowledge established>" --kind knowledge --skill collectknowledge --prompt "<driving prompt>" --summary "<design implications>" --rationale "<why it matters>" --artifact docs/knowledge/<topic>/index.md --git-before "<HEAD sha at grounding; from audit-log.py git-context>"` (Change Mandate, CL1–CL2). A run that left no trace in `docs/audit/` is, like an un-indexed artifact, not done.

**Handoff:** → `/adddomainexperts` (expert lenses that cite this base) → `/specify` / `/define-architecture` / `/design-slice`, which now reason from established knowledge rather than assumption.
