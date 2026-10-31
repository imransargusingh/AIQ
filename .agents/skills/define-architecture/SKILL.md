---
name: define-architecture
description: Produce the top-level architecture from a prompt or spec. Selects the LOA archetype and tier allocation, establishes unfamiliar SDK/protocol contracts via mandatory spikes, runs the architect council adversarially, and records decisions as ADRs. Use for new systems or load-bearing architecture.
runs_as: either
---

# Skill: /define-architecture

Produce the **top-level architecture** — the shape of the system, its major components and boundaries, its archetype and capability-tier allocation, and the load-bearing decisions captured as ADRs. This is the highest-leverage work in the loop: systems-thinking teaches that **behavior emerges from structure**, so a wrong structure cannot be fixed by good code downstream. Architecture is authoritative for any architectural question (Rules of the Road §0.1, precedence rank 3).

**Spine:** Rigor Protocol with a heavy **Stage-3 spike** emphasis (`knowledge/spike-protocol.md`). **Authority:** the **LOA** (`layered-optimized-architecture.md`) governs AI-integrated architecture — identify the archetype, allocate tiers, apply principles P1–P11 as filters. **Mode:** Peer Mode for authoring, full architect council in Adversary Mode at the gate.

## Grounding (first action)
Consume the compiled prompt when one is in hand (CO-S0, `knowledge/agent-coordination.md`; `/compile`): its goal state is the turn's goal state and its Not-in-scope is the interdiction — derive nothing from raw prose that a compiled prompt already fixed. Load the artifacts this architecture must answer to and treat them as the **authoritative source of truth** (Rigor Protocol Stage 0; BoK §III.1): the spec(s) (`docs/specs/`), the domain knowledge bases (`docs/knowledge/`), and any existing `docs/architecture.md`. Quote the spec requirements and quality attributes the architecture must satisfy; if a new decision conflicts with an existing architectural decision or the spec, **surface the drift** (supersede it explicitly via an ADR, don't silently contradict it). Prefer **graph traversal** for this grounding (`knowledge-visualization.md` V15): start from this task's artifact(s) in the knowledge graph and follow the typed edges 1–2 hops (upstream `implements`/`refines`/`depends-on`, downstream `tested-by`/`documents`, and `uses-term` into the glossary), citing the traversal path; a missing edge, stale node, or orphan found here is a finding to surface. Skip this only if the user explicitly tells you not to consult prior artifacts.

## Input
A spec (`docs/specs/<feature>.md` from `/specify`) or a prompt. If only a prompt is given, run a lightweight `/specify` first — *architecture without a problem statement is architecture-astronaut work* (Tech Lead; the Simplifier).

## Cast
- **Peers (author together):** Orchestrator, **Enterprise Architect** (fit/longevity/archetype), **Distributed Systems Architect** (async/messaging/consistency), **Security & Identity Architect** (trust boundaries/delegated identity — from the start, not bolted on), **Tech Lead** (smallest correct shape), **Domain Researcher** (spikes unfamiliar SDKs/protocols).
- **Adversaries (attack the architecture at the gate):** the full architect council + **Patterns Expert** + **SRE & Systems Diagnostician**. Hard vetoes: Security (auth/identity/secrets/PII), Distributed Systems (unsafe delivery/ordering/idempotency).

## Flow (Rigor Protocol, specialized to system shape)

**Decision closure.** Read `knowledge/decision-interrogation.md`. Research technical facts rather
than asking the human. Before handoff, run DI6 for unresolved product constraints or trade-offs:
show the Question / Needed for / Recommendation table, then ask one question at a time.

**Stage 0 — Interdict the rush.** Do not name a tech stack or draw boxes yet. The first plausible architecture is a hypothesis.

**Stage 1 — OPEN.** Sketch the system as a system (Meadows): the **stocks** (what accumulates — queues, state, balances), the **flows** (requests, messages, money), the **feedback loops** (retries, backpressure, rate limits, caches, circuit breakers), the **delays** (latency, eventual consistency), and the **boundary** you draw (and what you exclude). Enumerate candidate architectures (at least two genuinely different shapes). Ask the leverage-point question: *where is the high-leverage structural decision, vs. the merely visible one?* List every external contract the system will depend on.

**Stage 2 — INTERROGATE.** Drill with precise questions: *Cause/Effect* — by what mechanism does each component meet its requirement, and what downstream behavior/cost/failure follows; *Assumption* — what scale, latency, consistency, and identity model are we assuming; *Evidence* — for each candidate shape, what weighs for and against. Select the **LOA archetype** (A–I) and the **tier allocation** (T0–T4) explicitly, and name the rejected archetypes.

**Stage 3 — EVIDENCE (spikes mandatory).** **Settle the durable data representation as a first-class architecture decision (`domain-and-data-modelling.md`)** — it is harder to reverse than any component choice, because code is refactorable and a schema that has taken data is not. Name the **bounded context(s)** and the ubiquitous language; identify the **aggregates** and the invariant each protects (DM1–DM3); then decide the **durable representation** and record it as an **ADR** (DM13): by default core entities as **dimensions** and change-over-time as **append-only facts** — so history and the audit trail *are* the data rather than a shadow schema, and new measures are new rows/columns rather than rewrites of history. State the layering honestly (DM6): one integrated operational store whose durable shape is dimensional; any analytical layer is a *derived projection*, never a second source of truth. Accept the trade explicitly — current-state reads become "latest row per key" and must be indexed or materialised as a labelled, rebuildable cache — or record the reasoned alternative (normalised + system-versioned temporal tables; event sourcing) with its own ADR. The **Data & Persistence Architect** authors this in Peer Mode and attacks it at the gate. For **every unfamiliar, preview, or version-sensitive SDK/API/MCP server/protocol** the architecture depends on, run the **Spike Protocol**: read the source/signatures, then run a minimal PoC against the *installed* version to confirm semantics, error model, concurrency, idempotency, and cost — *before* committing to it. This is non-negotiable: do not choose a transport, an AI SDK, a delegated-identity surface, or an MCP integration you have not exercised. Establish all external contracts (BoK Part III) with cited sources. Apply the LOA principles as filters: cheapest sufficient tier (P1), determinism at the floor (P2), cognition/execution separation (P3), verification over plausibility (P5), idempotency at side-effect boundaries (P8), least-privilege delegated identity (P11). Maintain the confidence ledger.

**Stage 4 — DISCONFIRM (the gate).** Switch to Adversary Mode and convene the council (proportional to cost-of-error). The Distributed Systems Architect attacks delivery/ordering/idempotency with disconfirming claims (*"on redelivery this corrupts state because…"*); Security attacks trust boundaries and delegated-identity scope (hard veto); the Enterprise Architect attacks fit, reuse, and LOA conformance; the SRE attacks observability and the 3-a.m. debug story; the Simplifier attacks unjustified architectural complexity. Run the LOA conformance criteria (C1–C11) as the test suite. Resolve or record every veto; architecture forks go to an **ADR**. Authors do not clear their own hard veto.

**Stage 5 — CONVERGE.** Write the architecture to `templates/architecture.template.md` and one **ADR** per load-bearing decision (`templates/adr.template.md`; LOA Appendix F): the decision, the chosen option, the strongest rejected options and why, the consequences. State the archetype, the tier allocation, the composition contract, the cross-cutting concerns (auth, observability, idempotency), and the residual architectural risk.

**Define completely, phase vertically (mandatory).** The architecture is defined *whole* — every component, boundary, and contract named — but its **delivery is partitioned into vertical slices**: each phase is a thin end-to-end path through the system (UI/API edge → logic → persistence/integration) that can be **deployed, exercised end-to-end by automated tests, and validated by a human**, even if that requires mocks, stubs, or fakes at the edges not yet built. Phase 1 is the **walking skeleton**: the thinnest complete path that touches every architectural layer and proves the composition. Order the slices so that *serial implementation always leaves a meaningful, demonstrable, end-to-end deployable* — never a pile of horizontal layers that only integrates at the end (that is Big-Bang Integration, and it hides the riskiest defects until last). For each phase state: the user-visible capability it proves, what is real vs. mocked at that phase, how a human validates it (the demo script), how tests validate it (the E2E checks), and what it unblocks. Mocked seams are *contracts* — define them as such so replacing the mock with the real component later is a substitution, not a redesign.

**Close with the status table (mandatory).** End the response with:

| | |
|---|---|
| **Completed** | what this run produced (architecture, ADRs, phasing plan) |
| **Remaining** | the delivery phases not yet built, in order |
| **Best next action** | the single concrete next step (typically `/design-slice` of the Phase-1 slice's components) |

## Output artifacts
- `docs/architecture.md` (top-level architecture, from `templates/architecture.template.md`).
- `docs/adr/NNNN-<decision>.md` per load-bearing decision.
- Spike findings recorded in the confidence ledger; `spikes/` scaffolding disposed.

## Definition of done (exit gate)
- [ ] LOA archetype and tier allocation named; rejected archetypes stated.
- [ ] **Domain model and durable representation settled and recorded as ADRs** (`domain-and-data-modelling.md` DM1–DM6, DM13): bounded context(s) and ubiquitous language named; aggregates identified with the invariant each protects; the durable representation chosen (dimensions + append-only facts by default) with the read-path cost accepted, or a reasoned alternative recorded; the ODS/analytical layering stated honestly with any analytical layer a derived projection, not a second source of truth (Data & Persistence Architect).
- [ ] **Every unfamiliar/preview SDK/protocol spiked** (read + run); contracts established with sources; preview surfaces flagged.
- [ ] LOA principles applied as filters; conformance criteria (C1–C11) checked.
- [ ] Cross-cutting concerns designed in (auth/delegated identity, observability, idempotency, failure modes), not deferred.
- [ ] **Delivery phased vertically**: the whole architecture is defined, and a phasing plan partitions it into end-to-end vertical slices — Phase 1 a walking skeleton; each phase deployable, test-validated, and human-validatable (with mocked seams defined as contracts); no horizontal-layer phasing.
- [ ] Architect council passed in Adversary Mode; hard vetoes (Security, Distributed Systems) resolved or formally deviated; authors did not self-clear.
- [ ] ADRs written for load-bearing decisions; residual architectural risk stated.
- [ ] **Status table emitted**: completed / remaining phases / best next action.

## Documentation & discoverability (last action)
Per the **Knowledge Visualization & Docs Explorer Standard** (`knowledge/knowledge-visualization.md`, the Discoverability Mandate V10): after producing the architecture (`docs/architecture.md`) and its ADRs, **write the artifact's frontmatter** (the record, V2: id, title, type, status, **owner**, phase, tags, **typed links** per the relation registry, **review-by** per the type's SLA, and a real 1–3-sentence summary) and **sync the derived `docs/docs-index.js`** by running the script bundle — `python3 docs/ai-forward-pack/scripts/docs-graph.py derive` (and `flag --changed <id> --reason …` for V16 propagation) — never by generating ad-hoc scripts (V18); frontmatter wins wherever the two disagree. Ensure `docs/index.html` (the Docs Explorer) exists — instantiate it from `templates/docs-explorer.template.html` if missing — and verify the new entry has at least one typed link into the graph (an orphan is a finding). Index and diagrams land **in the same change** as the content (V11). **Propagate impact (V16):** if this change is material (a contract, requirement, decision, public shape, or proven claim changed), traverse the **inbound** edges and push `review-suggested: { by, on, reason }` into each inbound neighbor's frontmatter (and its index entry) in this same change. **Capture session exhaust (V17):** any mid-session decision, discovered assumption, or resolved question below ADR weight becomes a linked **decision note** (`docs/notes/`, from `templates/decision-note.template.md`) before close — promote to an ADR if it later bears load. Work that is not discoverable in the Explorer is not done.

**Audit & change (last action).** Append an audit-log entry for this run — `python3 docs/ai-forward-pack/scripts/audit-log.py append --shortname "define-architecture-<subject>" --session "<id>" --skill define-architecture --kind skill --prompt "<the prompt, verbatim>" --summary "<what it produced>" --artifact docs/architecture.md` — per the Audit Mandate (`knowledge/audit-and-change-log.md`, AL5). Because this skill shapes a load-bearing decision, **also append a change-log entry** capturing the decision and its git context — `… audit-log.py change --title "<the decision>" --kind architecture --skill define-architecture --prompt "<driving prompt>" --summary "<result>" --rationale "<why>" --artifact docs/architecture.md --artifact docs/adr/<id>.md --git-before "<HEAD sha at grounding; from audit-log.py git-context>"` (Change Mandate, CL1–CL2). A run that left no trace in `docs/audit/` is, like an un-indexed artifact, not done.

**Handoff:** → `/design-slice` (detailed design of each component).
