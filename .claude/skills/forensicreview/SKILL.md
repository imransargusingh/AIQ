---
name: forensicreview
description: Perform a deep evidence-based architecture, design, implementation, and documentation review of an existing repository, then produce a prioritized risk and remediation backlog.
runs_as: either
---

# Skill: /forensicreview

Reconstruct an existing repository's architecture and documentation from the code, review the architecture, design, and implementation against the pack's full engineering standards, and turn every evidenced gap into a prioritized, testable backlog. This is a **forensic assessment**, not a drive-by code review: it establishes what the system actually is, where its risks are, and what work should happen next.

**Spine:** runs the Rigor Protocol (`knowledge/rigor-protocol.md`) on the repository as evidence. **Authority:** the Body of Knowledge, Engineering Governance, Testing Strategy, Observability Standard, Knowledge Visualization Standard, and LOA for AI-integrated systems. **Composition:** applies `/adopt`'s honest architecture recovery and `/document`'s truth-to-code full documentation sweep before judging the system. **Mode:** Peer Mode to reconstruct and assess, Adversary Mode to attack every finding (`knowledge/collaborative-personas.md`).

## Grounding (first action)
CO-S0 applies first — the sentence is `reference/co-s0.md`. Capture the target commit, branch, and dirty-worktree state. Read the repo front door and local instructions, then inventory the code, tests, build/package manifests, CI/CD, deployment/configuration, database schemas and migrations, public contracts, runtime entry points, dependency graph, existing docs/ADRs/specs/designs, recent load-bearing history, and the audit/change log. If a knowledge graph exists, run `docs-graph.py inventory` and traverse the relevant artifacts 1–2 hops per V15. Treat code and executed behavior as the ground truth; existing documents are evidence of intent and must be checked against it. Skip this only if the user explicitly says so.

## Input
A repository or path (default: the whole current repo), optionally narrowed to a subsystem or risk theme. The user may supply incidents, audit concerns, architectural claims, or external requirements to test. Default mode is comprehensive. The skill may regenerate documentation but **MUST NOT modify production code, dependencies, schemas, CI behavior, or runtime configuration**; it ends with findings and a backlog for human triage.

## Cast
- **Peers (author together):** Orchestrator; **Enterprise Architect** and **Documentation Steward** (co-leads); the relevant language Developer; Patterns Expert; Test Architect; SRE; Security & Identity; Data & Persistence; Distributed Systems; AI Systems, Privacy, UX/Accessibility, Mobile, or Native Desktop when triggered; project domain experts when present.
- **Adversaries (attack at the gate):** Enterprise Architect (architectural coherence), Documentation Steward (truth and coverage), Simplifier (speculation/bloat), Test Architect (**hard veto** on unsupported findings), Security, Distributed Systems, AI Systems, Data, and Privacy with their normal hard-veto scopes. A hard-veto **BLOCK** describes the reviewed repository's readiness; it does not suppress the report.

## Flow (Rigor Protocol, specialized)

**Stage 0 — Interdict the rush.** Do not start with a generic checklist or a list of stylistic complaints. First reconstruct the system and establish its local contracts. Do not rewrite history, invent missing intent, infer severity from a filename, or label a hypothesis as a defect. Never "fix while reviewing"; remediation starts only after human triage.

**Stage 1 — OPEN (inventory and baseline).**
- Run the repo's existing, smallest representative build/test/lint/analysis commands without installing new tooling. Record failures as baseline evidence; do not attribute them to the review.
- Map languages, frameworks, deployables, entry points, modules, public APIs, stores, queues, external systems, trust boundaries, data flows, release paths, and ownership.
- Inventory existing docs and graph health (`docs-graph.py inventory` when available): missing/invalid frontmatter, stale/flagged/orphan nodes, doc-code contradictions, and traceability gaps.
- Enumerate the boundary set for the review: security/identity, data integrity/migration, concurrency/distribution, reliability/operations, performance/cost, accessibility/UX, supply chain, testing, and maintainability.

**Stage 2 — RECONSTRUCT (architecture and documentation before judgment).**
- Apply `/adopt` discipline where the repo lacks a reliable graph: recover `docs/architecture.md` from code at C4 Context + Container, add Component detail where it earns its place, and label every claim Verified/Inferred/Flagged.
- Apply `/document` discipline for the full bundle: API reference, sequence/class/layered/component diagrams, architecture overview, Map of Content, security/privacy rollups, Docs Explorer index, and coverage/confidence metadata. Preserve useful existing history; replace false claims with truth-to-code documentation and record the discrepancy.
- Recreate only from evidence. An undocumented public surface or unprovable architecture edge is a finding, not an invitation to fabricate.

**Stage 3 — INTERROGATE (review the three layers).**
- **Architecture:** system boundaries, dependency direction/cycles, coupling and cohesion, deployment topology, data ownership, consistency boundaries, trust boundaries, scaling/resource limits, availability and recovery, platform fit, LOA archetype/tier allocation for AI, and ADR alignment.
- **Design:** contracts and nullability, API/schema evolution, state and lifecycle, concurrency/idempotency, failure-mode dispositions, security/privacy controls, observability/error model, performance budgets, rollout/rollback, accessibility and complete UI states where applicable, and named-pattern fit.
- **Implementation:** correctness hotspots, unsafe boundary handling, error/cancellation/resource lifecycle, duplicated or dead logic, dependency and supply-chain posture, migrations, tests and mutation/fidelity strength, CI gates, operational telemetry, TODO/FIXME/HACK markers, and divergence from local conventions.
- **Traceability:** requirement/spec -> architecture -> design -> code -> test/proof. Missing links in either direction are findings.

**Stage 4 — EVIDENCE (prove each finding).**
- Every finding receives a stable `FR-###` id, category, kind, location (`file:line` or artifact id), observed evidence, violated contract/standard, consequence, confidence (**Verified/Inferred/Flagged**), and the disconfirming check attempted.
- Use existing analyzers, dependency reports, test runners, history, and focused probes. Run the Spike Protocol for unfamiliar/version-sensitive contracts. Do not add scanners or packages just to make the review look deeper.
- Distinguish: **risk** = plausible future adverse outcome not yet observed; **issue** = verified present defect or governance gap; **todo** = improvement work that is not itself a defect.

**Stage 5 — DISCONFIRM (the gate).** Switch to Adversary Mode. The Enterprise Architect attacks the recovered architecture; the Steward walks every diagram/document claim against code; Security/Privacy/Data/Distributed/AI/SRE attack findings in their domains; the Test Architect rejects any finding without an oracle or reproducible evidence; the Simplifier removes duplicates, preferences disguised as defects, and speculative backlog items. Record each persona's PASS/BLOCK and residual risk. Authors do not self-clear.

**Stage 6 — PRIORITIZE AND CONVERGE.**
- Assign priority without false precision: **P0** exploitable/active data-loss/safety/irreversible blocker; **P1** high-likelihood correctness, security, reliability, or migration risk; **P2** material maintainability, operability, performance, testing, or design debt; **P3** localized hygiene, documentation, or low-impact improvement.
- Deduplicate findings by root cause. Convert each accepted finding into a backlog item with: id, kind, priority, title, evidence link, affected scope, consequence, recommended remediation, falsifiable acceptance criteria, validation method, dependencies, suggested owner, recommended next skill (`/investigate`, `/define-architecture`, `/design-slice`, `/migrate`, `/implement`, or `/document`), and status `proposed`.
- Order the backlog into independently deliverable phases: contain P0/P1 exposure first; restore missing proof/observability next; repair architecture/design-slice debt in vertical slices; finish with maintainability/docs hygiene.
- End with an overall readiness verdict and a status table: Completed | Remaining | Best next action. Then **STOP for human triage**. (the stop is a message — CO-S2, `knowledge/agent-coordination.md`) Do not implement backlog items or create remote issues unless the user explicitly requested that separate action.

## Output artifact
- Recreated truth-to-code documentation under `docs/`, including `docs/architecture.md`, the documentation bundle, diagrams, graph index, and Explorer.
- **Naming: rev-numbered, newest-wins.** Write `docs/reviews/forensic-review-rev<N>.md` and `docs/backlog/forensic-review-rev<N>.md`, where `<N>` is the pack/repo revision under review — **not** a generic `forensic-review.md`. A single always-newest filename rots: the older review keeps the name a reader trusts while its content goes stale. Give the new review `id: forensic-review-rev<N>`, a `supersedes` edge to the previous one, and **set that previous review's `status` to `superseded`** — exactly one forensic review may be non-superseded at a time. `check-consistency.py :: check_forensic_review_currency()` enforces both halves mechanically, defining "newest" as the highest `rev<N>` so a script can fail it.
- `docs/reviews/forensic-review-rev<N>.md` (`type: doc`) — scope and baseline, recovered system map, architecture/design-slice/implementation assessments, persona verdicts, deduplicated findings, readiness verdict, confidence ledger, and residual risk.
- `docs/backlog/forensic-review-rev<N>.md` (`type: doc`) — the canonical proposed backlog of risks, open issues, and todos using `FR-###` ids and the schema above. **`FR-###` ids continue across reviews** — never restart the numbering; scan prior backlogs for the highest id first.

## Definition of done (exit gate)
- [ ] Target commit, scope, baseline commands/results, and dirty-state caveat recorded.
- [ ] Architecture and documentation regenerated from code with C4/UML claims confidence-labeled; docs-vs-code discrepancies recorded.
- [ ] Architecture, design, implementation, traceability, testing, security/privacy/data, operations, release, performance, accessibility, and supply-chain lenses marked reviewed or explicitly N/A with rationale.
- [ ] Every finding has evidence, location, confidence, consequence, a disconfirming check, and no duplicate root cause.
- [ ] Backlog contains separated `risk` / `issue` / `todo` items, P0–P3 priority, falsifiable acceptance criteria, validation, dependencies, owner, and recommended next skill.
- [ ] `docs-graph.py validate` passes; report and backlog have valid frontmatter, typed links, and derived index entries.
- [ ] No production code, dependencies, schemas, runtime config, or CI behavior changed.
- [ ] Adversarial gate passed; vetoes resolved or recorded; authors did not self-clear.
- [ ] Stopped for human triage before implementation or remote issue creation.

## Documentation & discoverability (last action)
Per the Discoverability Mandate (V10): write full frontmatter on the report and backlog (ids `forensic-review-rev<N>` and `forensic-review-rev<N>-backlog`, `type: doc`, owner, review-by, real summaries, and typed links to the architecture, to each other, and a `supersedes` edge to the previous review), then run `python3 docs/ai-forward-pack/scripts/docs-graph.py derive`, `validate`, and `snapshot` — never ad-hoc scripts (V18). Set the previous review's `status` to `superseded` in the same change. Propagate material documentation corrections with `flag` (V16), capture sub-ADR review decisions as decision notes (V17), and verify both artifacts appear in the Explorer.

**Audit (last action).** Append an audit entry via `audit-log.py append --shortname "forensicreview-<repo>" --session "<id>" --skill forensicreview --kind skill --prompt "<verbatim>" --summary "<docs rebuilt; findings/backlog counts; readiness verdict>" --artifact docs/reviews/forensic-review-rev<N>.md --artifact docs/backlog/forensic-review-rev<N>.md`. If the review establishes a load-bearing architectural decision rather than merely recommending one, record it separately with `audit-log.py change`.

**Handoff:** human triage of the backlog -> run the recommended skill per approved item. Use `/investigate` before fixing any unverified defect; use `/implement` only after the governing design and acceptance criteria are approved.
