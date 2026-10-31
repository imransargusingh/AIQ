---
name: migrate
description: Execute dependency upgrades, framework migrations, and large-scale refactors with a characterization-first discipline — pin current behavior with golden-master tests before any change, compute the blast radius from the knowledge graph, migrate in vertical increments behind the proof, and demonstrate behavioral equivalence (or cataloged intentional differences) at the end. Use for SDK/major-version bumps, library swaps, cross-cutting refactors, and platform moves.
runs_as: either
---

# /migrate — change everything, break nothing (provably)

Migrations and large refactors are the most common high-risk agentic task: broad blast radius, behavior that must *not* change, and an urge to "modernize while we're in here." This skill encodes the discipline that makes them safe: **behavior is pinned before it is touched**, the blast radius comes from the graph rather than from optimism, the work proceeds in vertical increments that each end green, and the result is proven equivalent — with every intentional difference cataloged, never smuggled.

**Spine:** the Rigor Protocol. **Authorities:** the Testing Strategy (characterization = the proof), the Observability Standard (telemetry is part of the contract being preserved — O-series), the Knowledge Visualization Standard (V15 grounding, V16 propagation), and the latest-stable-SDK policy in the Body of Knowledge (the *target* defaults to latest stable; the repo pin wins), and the **Solution-Selection Ladder** (`solution-selection-ladder.md`) — the cheapest migration is the code you delete instead of migrating. **Cast:** the relevant language **Developer** leads; the **Test Architect** holds the hard veto on characterization coverage; the Patterns Expert and Simplifier police "while-we're-here" scope creep; the SRE guards telemetry continuity; the Release Engineer owns rollout/rollback. **Tooling (V18):** graph mechanics via `docs/ai-forward-pack/scripts/docs-graph.py` (`inventory`, `flag`, `derive`).

## Grounding (first action)
CO-S0 applies first — the sentence is `reference/co-s0.md`. Load the artifacts that define the behavior being preserved — the specs, designs, ADRs, and proof packs of every touched component. Prefer **graph traversal** (`knowledge-visualization.md` V15): start from the migration's target components and follow the typed edges 1–2 hops; the **inbound** edges are the blast radius (V16 read for planning, before any flagging). Read the target's release notes / breaking-change list via the Spike Protocol — empirically, not from memory. Skip this grounding only if the user explicitly tells you not to consult prior artifacts.

## Stages

**Stage 0 — interdict the rush.** Two urges to kill: starting the rewrite before behavior is pinned, and "improving" things mid-migration. A migration's definition of success is **boring**: everything observable works exactly as before, except the differences the plan names.

**Stage 1 — OPEN (scope + blast radius).** Name the migration precisely (from-version → to-version / from-shape → to-shape). Compute the blast radius from the graph (`docs-graph.py inventory` + inbound edges of every touched component) plus the build's actual dependency closure. Decide the increments: **vertical slices** that each compile, pass, and could ship — never a months-long broken branch. **Apply the ladder to scope** (`solution-selection-ladder.md` L1): rung 1 — does each touched surface need to migrate at all, or can it be **deleted** (a `/ponytail-audit`-style over-engineering pass over the blast radius often turns migration into deletion); rung 5 — can the target now be covered by stdlib/platform so a dependency is **removed**, not swapped.

**Stage 2 — INTERROGATE (what behavior is load-bearing?).** For each touched surface: which behaviors are contractual (spec/design-slice-promised), which are incidental-but-depended-on (Hyrum's-law surfaces — observed outputs, error codes, log/telemetry shapes, ordering, timing tolerances), and which are free to change? The second category is where migrations break things; it is enumerated, not assumed empty.

**Stage 3 — EVIDENCE (pin, then change).**
- **Characterization first (the Test Architect's hard veto).** Before any migration edit, pin current behavior with **golden-master / characterization tests** over every load-bearing surface from Stage 2 — including error paths, emitted error codes, and telemetry shapes (O-series: a span or structured event silently disappearing is a regression). These tests assert *what is*, not *what should be*; they run green on the old stack first. **No characterization coverage on a touched surface → the migration of that surface does not start.**
- **Spike the target** (Spike Protocol): the breaking-change list verified empirically on a slice, not trusted from release notes.
- **Migrate by increment:** smallest vertical slice first (the walking skeleton of the new stack); characterization suite green after every increment; deviations from old behavior either fixed or **cataloged as intentional** with rationale — the catalog is part of the deliverable. New code follows the failure-mode and adversarial analyses of the components' designs; the migration MUST NOT silently weaken either. Each **intentional difference** carries a **ceiling and an upgrade trigger**, and where it is a deliberate simplification at a call site, an inline **`simplify:`** marker *there* (not only in the report) so it is discoverable in code (`solution-selection-ladder.md` L5).

**Stage 4 — DISCONFIRM (the gate).** Adversary Mode: the Test Architect attacks coverage (which load-bearing surface has no characterization? which Hyrum surface was assumed not to exist?); the SRE diffs telemetry before/after (codes, spans, events); the Simplifier strikes every "improvement" that isn't the migration; the Release Engineer demands the rollback story per increment. Authors do not self-clear.

**Stage 5 — CONVERGE (prove equivalence, update the graph).** Final characterization run green on the new stack. Produce the **equivalence report** in the migration's proof pack: surfaces pinned → results, plus the **intentional-difference catalog** (each with rationale and, where contractual, its upstream artifact updated). Update touched designs/ADRs; **propagate impact** (`docs-graph.py flag --changed <id> --reason "migrated to <target>"`) to inbound dependents (V16); `derive`; record the version pin (an ADR if the repo pins below latest stable, per the BoK policy).

**Status table (last output).** Completed | Remaining | Best next action — increments remaining are listed individually.

## Documentation & discoverability (last action)
Per the Discoverability Mandate (V10): the migration's artifacts (the plan, the proof pack with the equivalence report, updated designs/ADRs) carry frontmatter and land in the derived index via the script bundle — `python3 docs/ai-forward-pack/scripts/docs-graph.py derive`, with `flag` for V16 propagation — never ad-hoc scripts (V18); frontmatter wins wherever the two disagree. **Capture session exhaust (V17):** the judgment calls (which Hyrum surfaces were deemed load-bearing, why a deviation was accepted) become decision notes. Work that is not discoverable in the Explorer is not done.

## Definition of done
- [ ] Every touched load-bearing surface (contractual *and* enumerated Hyrum surfaces, error codes, telemetry shapes) had **characterization tests green on the old stack before migration began** (Test Architect's hard veto).
- [ ] Migration proceeded in vertical increments, each ending green with a rollback story (Release Engineer).
- [ ] Final suite green on the new stack; the **intentional-difference catalog** is complete, each entry with rationale and updated upstream artifacts where contractual.
- [ ] Telemetry continuity verified (no silently lost spans/events/codes — SRE).
- [ ] Failure-mode and adversarial analyses of touched components carried forward, not weakened.
- [ ] Inbound dependents flagged via V16 with the migration as provenance; index re-derived; version pin recorded per the BoK latest-SDK policy.
- [ ] No "while-we're-here" changes survived the Simplifier.
- [ ] **Ladder applied to scope** (`solution-selection-ladder.md` L1/L5): delete-don't-migrate considered (rung 1), dependency-removal-not-swap considered (rung 5); each intentional difference carries a ceiling + upgrade trigger and an inline `simplify:` marker where code-local.

**Audit & change (last action).** Append an audit-log entry for this run — `python3 docs/ai-forward-pack/scripts/audit-log.py append --shortname "migrate-<subject>" --session "<id>" --skill migrate --kind skill --prompt "<the prompt, verbatim>" --summary "<what it produced>" --artifact docs/proofs/<id>.md` — per the Audit Mandate (`knowledge/audit-and-change-log.md`, AL5). Because this skill shapes a load-bearing change, **also append a change-log entry** capturing the migration and its git context — `… audit-log.py change --title "<from → to>" --kind migration --skill migrate --prompt "<driving prompt>" --summary "<result + intentional-difference catalog>" --rationale "<why>" --artifact docs/proofs/<id>.md --git-before "<HEAD sha at grounding; from audit-log.py git-context>"` (Change Mandate, CL1–CL2). A run that left no trace in `docs/audit/` is, like an un-indexed artifact, not done.

**Handoff:** `/document --changed` over the touched surface; the flagged dependents' owners review per V16.
