---
name: adopt
description: Bootstrap the AI-Forward Pack's knowledge graph in an existing (brownfield) repository — inventory what's already there, recover the architecture from the code, bring existing docs under frontmatter, mine an initial glossary from the code's own vocabulary, build the first index and Docs Explorer, and produce a phased adoption plan for the gaps. Use once per legacy repo, right after dropping the pack in. Run /document afterward for the full documentation bundle.
runs_as: either
---

# /adopt — bring an existing repository into the knowledge graph

Take a repository that predates the pack and give it a working knowledge graph: every existing doc becomes a frontmattered artifact, the recovered architecture and glossary become first nodes, the Explorer renders it all, and the gaps (no spec, no designs, no proofs) become an explicit, phased adoption plan instead of silent absences. This is the highest-friction moment of rolling the pack across an estate — the skill exists so it is done once, well, and the same way every time.

**Spine:** the Rigor Protocol (`knowledge/rigor-protocol.md`) on the repository as the subject. **Authority:** the Knowledge Visualization & Docs Explorer Standard (`knowledge/knowledge-visualization.md`, V1–V18). **Cast:** the **Enterprise Architect** and **Documentation Steward** co-lead; the relevant language Developer and the Patterns Expert read the code; the Simplifier keeps the bootstrap minimal; gate by the Steward + Test Architect in Adversary Mode. **Tooling (V18):** all graph mechanics run through `docs/ai-forward-pack/scripts/docs-graph.py` (`stub`, `derive`, `inventory`, `snapshot`) — no ad-hoc scripts.

## Grounding (first action)
CO-S0 applies first — the sentence is `reference/co-s0.md`. The grounding *is* the repository: the code, its tests, its build, every existing document (READMEs, wikis exported in-repo, `docs/**`, ADRs in any format), commit history for the load-bearing areas, and any external artifacts the human supplies. Treat what exists as **evidence of intent**, not noise — adoption recovers and records, it does not rewrite history. Where a pack graph already partially exists, traverse it per V15 before adding nodes. Skip this grounding only if the user explicitly tells you not to consult prior artifacts.

## Stages

**Stage 0 — interdict the rush.** The urge is to generate a beautiful fake history — specs and designs the team never wrote. Adoption records **what is**, labels its confidence honestly (Verified from code / Inferred / Flagged), and plans what's missing; it never fabricates provenance.

**Stage 1 — OPEN (inventory both surfaces).** Run `docs-graph.py inventory` for whatever graph exists. Enumerate: the code surface (modules, public contracts, dependency edges, entry points); every existing document and where it lives; the build/test/CI reality; the team's actual vocabulary (names that recur in code, docs, and commit messages — glossary candidates). List what must be read per artifact.

**Stage 2 — INTERROGATE.** For each existing document: what *is* this (spec? design? ADR-in-prose? runbook?), is it still true, who owns it, and what does it link to? For the code: what architecture does it *actually* exhibit (not what anyone wishes)? Which areas are load-bearing and under-documented — the adoption plan's priority order comes from here.

**Stage 3 — EVIDENCE (recover and record).**
- **Architecture recovery:** write `docs/architecture.md` from the code as it is — C4 Context + Container (V8), the real dependency edges, the tiers actually in use; every claim Verified-against-code or labeled Inferred/Flagged.
- **Docs under frontmatter:** for each existing document worth keeping, add V2 frontmatter *in place* (`docs-graph.py stub` for net-new homes; targeted frontmatter prepend for existing files) — honest `status`, real `owner` if known (else the area's CODEOWNERS owner, Flagged), typed links to what it demonstrably relates to, `review-by` per its type's SLA (V13). Do not move files unless the human asks; the graph links them where they live.
- **Glossary mining (V14):** seed `docs/knowledge/glossary.md` from the recurring vocabulary — only terms two or more artifacts/areas need to agree on; each entry anchored to the code shape that is that term.
- **Decision archaeology (V17, light):** where commit history or comments reveal a clearly load-bearing past decision, record it as a decision note marked **Inferred** with its evidence — never invented.

**Stage 4 — DISCONFIRM (the gate).** Adversary Mode: the Steward verifies every frontmatter claim against the file and every architecture edge against the code; the Test Architect attacks the confidence labels (anything Verified must have a citation); the Simplifier strikes speculative nodes — a smaller honest graph beats a larger guessed one. Authors do not self-clear.

**Stage 5 — CONVERGE.** Run `docs-graph.py derive` (the first real index), ensure the **Docs Explorer** at `docs/index.html`, run `docs-graph.py snapshot` (the baseline health record — adoption is day zero of the trend). Write the **adoption plan** into `docs/ai-forward-pack-adoption.md` (type `doc`): the gap table (missing specs/designs/proofs per area), **phased vertically** by load-bearing priority — Phase 1 is the area where an agent will work next — with each phase naming the skills to run (`/specify`, `/design-slice`, `/document`) and ending in a working, navigable graph increment.

**Status table (last output).** Completed | Remaining | Best next action — the remaining column *is* the adoption plan's head.

## Documentation & discoverability (last action)
Per the Discoverability Mandate (V10): the adoption artifacts themselves (architecture, glossary, adoption plan, notes) carry full frontmatter and land in the derived index via the script bundle — `python3 docs/ai-forward-pack/scripts/docs-graph.py derive` — never ad-hoc scripts (V18); frontmatter wins wherever the two disagree. **Propagate impact (V16):** not applicable on first adoption (there are no prior dependents), but any re-adoption pass over an existing graph follows it. **Capture session exhaust (V17):** judgment calls made during adoption (what counted as an ADR, which areas were deprioritized and why) become decision notes. Work that is not discoverable in the Explorer is not done.

## Definition of done
- [ ] Every kept existing document carries valid V2 frontmatter with honest status/owner/links and a type-correct `review-by` (V13); nothing fabricated — confidence labels on every recovered claim.
- [ ] `docs/architecture.md` recovered at C4 Context+Container with every edge Verified or labeled (V8).
- [ ] Glossary seeded with anchored, multi-artifact terms only (V14).
- [ ] `docs-graph.py validate` reports no schema findings; `derive` ran; the Explorer renders; `snapshot` recorded the baseline.
- [ ] The adoption plan exists, phased vertically by load-bearing priority, each phase naming its skills.
- [ ] Gate held: Steward verified claims, Test Architect attacked confidence labels, Simplifier struck speculation; no self-clearing.

**Audit (last action).** Append an audit-log entry for this run — `python3 docs/ai-forward-pack/scripts/audit-log.py append --shortname "adopt-<repo>" --session "<id>" --skill adopt --kind skill --prompt "<the prompt, verbatim>" --summary "<what was recovered + adoption plan>" --artifact docs/architecture.md` — per the Audit Mandate (`knowledge/audit-and-change-log.md`, AL5). A run that left no trace in `docs/audit/` is, like an un-indexed artifact, not done.

**Handoff:** run `/document` for the full documentation bundle over the adopted base, then the adoption plan's Phase 1 skills.
