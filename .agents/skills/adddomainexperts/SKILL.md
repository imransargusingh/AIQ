---
name: adddomainexperts
description: Identify a project's domain from its own evidence and add domain-expert personas (peer + adversary) tailored to it — wiring in existing Claude domain skills where they fit, generating §8-conformant persona files, and updating every roster artifact locally in the repo. Use when a repo needs subject-matter lenses (finance, CFD, clinical, legal, ML, geospatial…) beyond the general engineering personas.
runs_as: either
---

# Skill: /adddomainexperts

Add **subject-matter experts** to the swarm. The twenty-three lenses in this pack are domain-*general* — architecture, testing, security, data, releasing. They do not know whether a journal entry balances, whether a turbulence model suits the flow regime, or whether a dosing calculation is clinically safe. This skill closes that gap for *one project*: it discovers the project's domain, proposes the domain-expert lenses that domain needs in **Peer Mode** (to co-author with real domain knowledge) and **Adversary Mode** (to attack domain *incorrectness*), wires in any existing Claude domain skills that already supply the capability, and adds the confirmed experts to the repo — updating every roster artifact for consistency, **local to that git repo**.

This is the **persona audit (`knowledge/persona-audit.md`) turned outward at the domain** rather than at engineering coverage. It runs the same discipline: a candidate earns a seat only if it catches a class of *domain* error no current lens catches (the Simplifier's test), it states its **seam** against the general lenses, and it conforms to the **Persona Operating Standard** (`persona-audit.md` §8) so it is as routable and auditable as every other persona.

**Spine:** runs the Rigor Protocol (`knowledge/rigor-protocol.md`) on the *domain*. **Mode:** Peer Mode to propose the roster delta, Adversary Mode at the gate (`knowledge/collaborative-personas.md`). **Schema:** every persona produced conforms to `knowledge/persona-cards.md` / §8.

## The one distinction that matters
A **Domain Expert** (what this skill adds) is *subject-matter judgment* — "is this financially/physically/clinically/legally correct per the domain's body of knowledge?" It is **not** the **Domain Researcher** (a peer this pack already has), whose lens is *research method*: establishing the contract of an unfamiliar API/SDK by reading and running it (the Spike Protocol). The Domain Researcher establishes the contract of a *ledger SDK*; the Finance Domain Expert judges the *accounting*. Keep this seam explicit in every expert you add, or the two will be confused.

## Grounding (first action)
Consume the compiled prompt when one is in hand (CO-S0, `knowledge/agent-coordination.md`; `/compile`): its goal state is the turn's goal state and its Not-in-scope is the interdiction — derive nothing from raw prose that a compiled prompt already fixed. Before deriving experts, load what already defines this domain and treat it as the **authoritative source of truth** (Rigor Protocol Stage 0; BoK §III.1): the spec and the domain knowledge bases (`docs/knowledge/`). The experts you mint must be consistent with the domain as the repo has already established it; reconcile against any existing `docs/domain-experts.md` rather than duplicating roles. Prefer **graph traversal** for this grounding (`knowledge-visualization.md` V15): start from this task's artifact(s) in the knowledge graph and follow the typed edges 1–2 hops (upstream `implements`/`refines`/`depends-on`, downstream `tested-by`/`documents`, and `uses-term` into the glossary), citing the traversal path; a missing edge, stale node, or orphan found here is a finding to surface. Skip this only if the user explicitly tells you not to consult prior artifacts.

## Input
A repository (the current one), optionally with a domain hint from the developer ("this is a derivatives-pricing engine"). If no hint, the skill derives the domain from the repo's own evidence.

## Cast
- **Peers (author the roster delta):** Orchestrator, **Product Strategist** (knows the project's purpose, users, and the stakes of a domain error), **Domain Researcher** (establishes the domain's authoritative standards *and* discovers existing Claude domain skills), and **any domain experts already present** (peer).
- **Adversaries (attack the roster delta at the gate):** **The Simplifier** (hard pressure on roster sprawl — soft veto), **Patterns Expert** (is there already a Claude skill or an existing persona for this?), **Tech Lead** (proportionality, maintenance burden), **Enterprise Architect** (fit and governability), and the relevant **general personas** (Data & Persistence, Security, Privacy) for overlap and seam.

## Flow (Rigor Protocol, specialized to the domain)

**Stage 0 — Interdict the rush.** Do **not** emit a generic list of domain roles. No persona is proposed before the domain and its high-cost failure modes are established from evidence. A plausible-sounding "you'll want a Finance Expert" is a hypothesis, not a finding (anti-pattern: Confident Guess).

**Stage 1 — OPEN.** Detect the domain(s) from the repo, not from assumption: read the README and any specs; inspect dependency manifests (`*.csproj`, `package.json`, `pyproject.toml`, `requirements*.txt`, `Cargo.toml`) for domain libraries; scan namespaces/directories and the ubiquitous language in code and tests. Then sketch the **domain failure map**: where is a domain error most *expensive*, most *silent*, or most *irreversible*? (For finance: a misstatement or a control gap. For CFD: a solver that converges to a physically wrong answer. For clinical: an unsafe dose.) Enumerate candidate expert lenses *broadly* — diverge before narrowing.

**Stage 2 — INTERROGATE.** Drill each candidate with precise questions:
- **Simplifier test:** does this lens catch a class of *domain* error that no current persona — general or already-added — catches? If an existing lens covers it, this is not a new seat.
- **Mode:** is it peer-useful (co-authors domain content), adversary-useful (attacks domain incorrectness), or both? Most are both, worn two ways.
- **Seam:** state precisely how it differs from the **Domain Researcher** (subject-matter vs research-method), and from **Data & Persistence**, **Security**, **Privacy**, **Product Strategist**, and any expert already present.
- **Criticality → veto:** does the cost of a domain error justify a hard veto, or is advisory right? A regulated or safety-critical domain (finance reporting, medical, aviation) warrants a hard veto on domain-correctness; a low-stakes domain does not.

**Stage 3 — EVIDENCE (establish, don't assert).** The Domain Researcher leads two tasks:
1. **Ground the lens in the domain's real body of knowledge.** Name the authoritative standards/sources the expert's interrogation will rest on (e.g., GAAP/IFRS/ASC 606 and the balance identity for accounting; the Navier–Stokes equations, a turbulence-model selection guide, and mesh-independence/CFL criteria for CFD; the relevant clinical guideline). A standard recalled from memory is **Flagged** until sourced (BoK §VI.1).
2. **Discover existing Claude domain skills/agents — and prefer wiring them in over reinventing** (Patterns Expert; BoK adopt-or-not). Search the environment for skills that already supply the *capability* (for finance: the `finance:*` skills — financial-statements, journal-entry, variance-analysis, reconciliation, sox-testing, audit-support; analogously `data:*`, `marketing:*`, `product-management:*` for other domains), and check connected MCP servers. **The persona supplies the lens/judgment; the existing Claude skill supplies the executable capability** — the expert is told to *use* those skills, not to re-implement them.

**Stage 4 — DISCONFIRM (the gate).** Switch to Adversary Mode on the *proposed roster delta itself*. The Simplifier attacks every candidate as potential sprawl ("does this project really need a separate X, or does Y already cover it?"); the Patterns Expert checks for an existing skill/persona; the Tech Lead weighs the maintenance cost of another seat; the general personas check for overlap. **Reject** what does not survive, and record each rejected candidate with its reason (Paul–Elder fairness). Then **present the surviving roster to the developer for confirmation** — adding personas is a change to their repo's standing configuration, so the developer confirms or trims the set before anything is written (BoK §VI.3; this is an explicit-permission action, not a silent one).

**Stage 5 — CONVERGE (add, and make everything consistent).** For each **confirmed** expert:
- **Generate the persona file** from `templates/domain-expert.template.md`, conforming to §8: Identity, Lens, **Convene-when** (a domain trigger predicate), **Peer deliverable** (the domain content it co-authors), the **Adversary interrogation** (grounded in the cited domain standards), **owned domain anti-patterns**, **severity ceiling**, **veto type + falsifiable veto-clears-when**, the shared output contract, and **Handoffs**. Write it to the repo's agent directory in both forms: `.claude/agents/<expert>.md` and `.github/agents/<expert>.agent.md`.
- **Wire in the backing Claude skills** discovered in Stage 3 (name them in the persona's tools/skills and instruct the expert to use them for capability).
- **Update every local roster artifact for consistency** — *in this repo's deployed copies, not the pack source*:
  - the casting sheet (`collaborative-personas.md` §5) — add the expert to the workflows it joins, in peer and adversary columns, with its veto;
  - the roster index (`persona-cards.md`) — add its card;
  - the trigger and anti-pattern tables (`persona-audit.md` §8.7 convene-when, §8.8 ownership) — add the expert's predicate and any domain anti-pattern it owns;
  - the **roster count** wherever it appears (README) — the swarm is now *twenty-three + N* lenses;
  - the install trees (`adapters/INSTALL.md`) — list the new agent files.
- **Write the domain-experts registry** `docs/domain-experts.md`: the domain(s) identified with their evidence, each expert added (lens, seam, veto, backing skills), and the rejected candidates with reasons — the project-local, domain-scoped analogue of the persona audit.
- **Record the gate** (Rules of the Road §3.2).

## Output artifacts
- `.claude/agents/<expert>.md` + `.github/agents/<expert>.agent.md` per confirmed expert (§8-conformant, dual-mode, backing skills wired).
- `docs/domain-experts.md` — the domain roster, rationale, seams, backing skills, and rejected candidates.
- Consistency updates to the repo's local `collaborative-personas.md` §5, `persona-cards.md`, `persona-audit.md` §8.7–8.8, `README`, and `adapters/INSTALL.md`.

## Definition of done (exit gate)
- [ ] Domain(s) identified from **repo evidence**, not assumed; the domain failure map is stated.
- [ ] Each added expert **passes the Simplifier test** and states its **seam** — especially vs the Domain Researcher (subject-matter vs research-method).
- [ ] Each expert's interrogation is **grounded in cited domain standards** (no Confident Guess about the domain).
- [ ] Existing Claude domain skills/agents are **wired in where they fit**; nothing already-available was reinvented.
- [ ] Each expert conforms to **§8** (convene-when, peer deliverable, severity, veto-clears-when, output contract); veto is **proportional to domain criticality**.
- [ ] The developer **confirmed** the roster before files were written.
- [ ] **All local consistency surfaces updated**; the casting sheet and roster count reflect the new experts; the registry doc is written; the gate is recorded.

## Documentation & discoverability (last action)
Per the **Knowledge Visualization & Docs Explorer Standard** (`knowledge/knowledge-visualization.md`, the Discoverability Mandate V10): after producing the domain-expert definitions (`docs/domain-experts.md`), **write the artifact's frontmatter** (the record, V2: id, title, type, status, **owner**, phase, tags, **typed links** per the relation registry, **review-by** per the type's SLA, and a real 1–3-sentence summary) and **sync the derived `docs/docs-index.js`** by running the script bundle — `python3 docs/ai-forward-pack/scripts/docs-graph.py derive` (and `flag --changed <id> --reason …` for V16 propagation) — never by generating ad-hoc scripts (V18); frontmatter wins wherever the two disagree. Ensure `docs/index.html` (the Docs Explorer) exists — instantiate it from `templates/docs-explorer.template.html` if missing — and verify the new entry has at least one typed link into the graph (an orphan is a finding). Index and diagrams land **in the same change** as the content (V11). **Propagate impact (V16):** if this change is material (a contract, requirement, decision, public shape, or proven claim changed), traverse the **inbound** edges and push `review-suggested: { by, on, reason }` into each inbound neighbor's frontmatter (and its index entry) in this same change. **Capture session exhaust (V17):** any mid-session decision, discovered assumption, or resolved question below ADR weight becomes a linked **decision note** (`docs/notes/`, from `templates/decision-note.template.md`) before close — promote to an ADR if it later bears load. Work that is not discoverable in the Explorer is not done.

**Audit (last action).** Append an audit-log entry for this run — `python3 docs/ai-forward-pack/scripts/audit-log.py append --shortname "adddomainexperts-<domain>" --session "<id>" --skill adddomainexperts --kind skill --prompt "<the prompt, verbatim>" --summary "<experts added>" --artifact docs/domain-experts.md` — per the Audit Mandate (`knowledge/audit-and-change-log.md`, AL5). A run that left no trace in `docs/audit/` is, like an un-indexed artifact, not done.

**Handoff:** → `/specify` or `/design-slice` — the new experts now join those workflows automatically by their convene-when triggers, in both peer and adversary modes.

---

## Appendix — worked examples (illustrative; always derive from the actual repo)

These show the *shape* of the output. They are starting points to interrogate, never a menu to copy.

**A finance project.** Candidate experts, filtered by the gate:
- **Financial-Reporting & Accounting Expert** — lens: GAAP/IFRS/ASC 606 and the double-entry balance identity. *Adversary:* attacks calculations that violate an accounting identity, mis-recognized revenue, intercompany double-counting. *Peer:* co-authors the recognition/measurement logic. *Backing skills:* `finance:financial-statements`, `finance:journal-entry`, `finance:variance-analysis`. *Veto:* **Hard** on a calculation that violates a fundamental accounting identity or a regulatory presentation requirement; **clears-when** the books balance and the basis is cited. *Seam:* Data & Persistence owns the ledger *schema/migration*; this expert owns the *accounting correctness* of what is stored. The Domain Researcher establishes a ledger *SDK's API*; this expert judges the *accounting*.
- **Controls & Audit Expert** — SOX/ICFR. *Backing:* `finance:sox-testing`, `finance:audit-support`. *Veto:* advisory, escalating a material control gap. *Seam vs Security:* Security owns technical access control; this expert owns financial-reporting controls.
- **Reconciliation Expert** — *Backing:* `finance:reconciliation`. Often advisory.

**A CFD project.** 
- **Fluid Dynamicist / Numerical-Methods Expert** — lens: the governing equations (Navier–Stokes), turbulence-model appropriateness for the regime, mesh independence, CFL/stability, boundary-condition validity, validation against benchmarks. *Adversary:* attacks a simulation reported as valid without convergence/validation evidence. *Peer:* co-designs the discretization, solver, and validation plan. *Veto:* **Hard** on a result reported valid without mesh-independence, residual convergence, and CFL-stability evidence; **clears-when** those checks are present and pass. *Seam vs Test Architect:* the Test Architect owns *software-test verifiability* (the code computes what was specified); this expert owns *physical/numerical validity* (a solver can pass unit tests and still be physically wrong) — a distinct and essential lens.

**A heuristic starter table** (domain → candidate lens → likely backing skills, *if any*):

| Domain | Candidate expert lens | Likely backing Claude skills |
|---|---|---|
| Finance / accounting | Financial-Reporting, Controls/Audit, Reconciliation | `finance:*` |
| Data science / ML | Statistical-Methods, ML-Evaluation (pairs with the AI Systems Engineer) | `data:statistical-analysis`, `data:*` |
| Marketing / growth | Brand/Performance, SEO | `marketing:*` |
| Product | Product-Research/Spec | `product-management:*` |
| Clinical / healthcare | Clinical-Safety (regulated → hard veto) | — (research the guidelines) |
| Legal / compliance | Regulatory/Contracts (often pairs with Privacy & Data Governance) | — |
| Geospatial, real-time/embedded, scientific computing, payments/e-commerce | the corresponding subject-matter lens | — |

The table is bait for interrogation, not a checklist: the project's *own* evidence decides which lenses it actually needs, and the gate decides which earn a seat.
