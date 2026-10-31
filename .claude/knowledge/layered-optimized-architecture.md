---
load: reference
---
# Layered Optimized Architecture for AI-Integrated Systems

## Principles, Archetypes, and a Pattern Catalog

*Version 0.5 — A reference for human architects and AI code-generation assistants*

---

## Abstract

Layered Optimized Architecture (LOA) is a frame for designing AI-integrated systems under a constraint classical architecture did not face: **per-call inference cost varies by three to five orders of magnitude across the available computational substrate**, with capability, latency, and determinism profiles that map non-linearly to that cost. LOA treats the *capability tier* as an architectural dimension orthogonal to classical technical layering, and supplies a small set of principles, a catalog of canonical architectural shapes, and a pattern catalog with reference C# implementations and reference Roslyn analyzers. It is intended for both human architects and AI code-generation assistants, and includes mechanically checkable conformance criteria with RFC 2119 normative language.

## Reading Guide

This document is approximately twelve thousand words and serves several audiences. Each can navigate efficiently:

- **Architects making technology decisions** should read the Abstract, the Metamodel, Part I (the Frame), Part II (Principles), Part III (Archetypes), and Part VIII (Decision Framework). Parts IV and beyond become reference material.
- **Developers implementing components** should skim Parts II and III for context, then work primarily from Part IV (Pattern Catalog) and Appendix D (.NET Idiom Map). Appendix G provides reference Roslyn analyzers.
- **AI code-generation assistants** loaded with this document as context should treat Parts II, IV, VI, and VII as the operational core. Conformance criteria in Part VII are the test suite.
- **Reviewers and auditors** should read the Abstract, Part VII (Conformance), Appendix C (Anti-patterns with detection criteria), and Appendix F (ADR template).
- **Finance and budget partners** should read the Abstract, Part I, and Appendix H (System-Level Cost Model).

Cross-references use the form *Pattern 3.2 Adversarial Debater* on first mention within a section and *(3.2)* thereafter. Archetypes use letters A–I; patterns use numerals X.Y by category.

### Changelog

**v0.5.** Revision following a structured adversarial review (Enterprise Architect, Test Architect, Security & Identity Architect, SRE, Distributed Systems Architect, and language/simplicity/patterns lenses). Changes, in order of severity:
- **Correctness — Idempotent Action (5.3):** the reference wrapper had a check-then-act race (store-after-execute) that permitted the very double-submit it exists to prevent; replaced with atomic key reservation (`SET NX`-style) and a stated at-most-once invariant.
- **Correctness — Token Budget (2.4) and dependent patterns:** `BudgetContext.Sub` returned a *detached* child that never charged the parent, so concurrent children (Self-Consistency 3.5, Speculative 1.6, Reflection 3.6) could collectively overspend the ceiling, silently violating C2. Replaced with reservation semantics (`TryReserve`/`TryReserveFraction`) that draw down the parent atomically; fixed all call sites.
- **Security — new Principle P11 (Least-Privilege Delegated Identity)** and **C11 (Principal propagation)**; new anti-pattern **The Confused Deputy** (prompt-injection / untrusted-content-to-tool-call); Receipt Ledger now records the acting principal.
- **Operational — Graceful Degradation (6.5):** added `TimeoutRejectedException` (Polly v8) to the fallback catches, which the original missed.
- **Operational — Speculative Execution (1.6):** the loser task is now observed to avoid unobserved-exception escalation.
- **Currency:** platform target raised to .NET 10 (LTS) / C# 14; added a currency note that named products/model classes are illustrative and will age.

---

## Foreword

The Gang of Four documented patterns when the dominant constraint was developer time. The J2EE Blueprints documented patterns when the dominant constraint was network round-trips. Fowler's *Patterns of Enterprise Application Architecture* unified those traditions around mapping rich domain models onto relational stores.

AI-integrated systems operate under a different dominant constraint. The substrate spans deterministic compute that is effectively free through frontier reasoning ensembles that cost dollars per call. An architecture that ignores tier allocation wastes money and quality in equal measure. An architecture that treats AI as a single layer — "call the LLM" — forfeits the asymmetric economics that make these systems viable at scale.

This document establishes LOA as the response. Structure follows precedent: principles, canonical archetypes, pattern catalog. Three additions distinguish this from classical pattern books. A formal **metamodel** states how principles, archetypes, patterns, and anti-patterns relate. A **conformance section** states what LOA-compliance means in mechanically checkable terms with normative language. A **reference analyzer** demonstrates conformance enforcement at build time. These additions reflect the audience: human architects must justify designs to engineering, finance, risk, and audit; AI code-generation assistants must apply consistent architectural judgment when synthesizing implementations.

---

## The LOA Metamodel

LOA defines four element types and the relationships between them.

```
Principle  ─── constrains ───►  Archetype
   │                                │
   │                                │ composes
   │                                ▼
   └──── constrains ────────►  Pattern  ◄─── violates ─── Anti-pattern
                                    │
                                    │ verified by
                                    ▼
                              Conformance Criterion
```

- **Principle.** An axiomatic constraint on design. Principles do not prescribe implementation; they rule choices in or out. Eleven principles (P1–P11) appear in Part II.
- **Archetype.** A recurring high-level shape of an AI-integrated system, characterized by a dominant request pattern and tier allocation. Archetypes compose patterns. Nine archetypes (A–I) appear in Part III. Archetypes are not orthogonal: large systems often compose multiple archetypes.
- **Pattern.** A reusable structural solution to a recurring sub-problem. Patterns are concrete enough to implement directly and abstract enough to apply across domains. Thirty-three patterns appear in Part IV.
- **Anti-pattern.** A recurring shape that violates one or more principles. Each anti-pattern in Appendix C has a Roslyn-checkable detection criterion.
- **Conformance Criterion.** A mechanically checkable rule that asserts a Pattern is used correctly or an Anti-pattern is absent. Eleven criteria (C1–C11) appear in Part VII; reference analyzers appear in Appendix G.

A LOA-conformant implementation MUST satisfy all conformance criteria marked MUST in Part VII. SHOULD-level criteria represent strong guidance; deviation requires documented rationale. MAY-level criteria are discretionary refinements.

---

## Part I — The Frame

**Summary.** AI-integrated systems span five capability tiers from deterministic compute to composed reasoning ensembles. Each tier has distinct cost, latency, and determinism. The architectural problem is allocation: which work happens at which tier. This dimension is orthogonal to classical n-tier layering.

### The capability–cost surface

A modern AI-integrated system has access to a stratified computational substrate:

| Tier | Examples | Per-call cost | Latency | Determinism |
|---|---|---|---|---|
| **T0 Deterministic** | Math, rules, algorithms, indexes, validators | ~$0 | μs–ms | Total |
| **T1 Embedded specialist** | Embeddings, classifiers, OCR, ASR | ~$0.0001 | ms | High |
| **T2 Small Language Model** | Phi, Llama-class small, Mistral-7B | ~$0.001 | tens of ms | Moderate |
| **T3 Frontier LLM** | Claude, GPT, Gemini class (single call) | ~$0.01–0.10 | hundreds of ms–s | Lower |
| **T3+ Reasoning model** | o-series, Claude with extended thinking | ~$0.05–$1 | seconds–minutes | Lower |
| **T4 Composed ensemble** | Plan/execute + tools + critics + provers | ~$0.10–$10 | seconds–minutes | Lowest |

Two substrates often collapsed into one warrant explicit separation. T3+ reasoning models trade latency and tokens for capability at inference time — the cost is paid inside one provider call. T4 composed ensembles are architectures the integrator builds out of multiple calls. The first is configurable but opaque; the second is decomposable and auditable.

The spread from T0 to T4 is roughly five orders of magnitude in cost, three in latency, and inverse in determinism. No single tier is correct for a whole application. The architectural problem is allocation.

*Currency note.* Named model classes and product exemplars throughout this document (o-series, Phi, Devin, AlphaProof, and the like) are illustrative as of early 2026 and will age. The tiers, principles, archetypes, patterns, and conformance criteria are the durable core; the proper nouns are not.

### Why classical layering is insufficient

Classical n-tier architectures organize layers by technical concern (presentation, business, integration, persistence). AI-integrated systems still benefit from those separations, but they introduce an orthogonal axis: the cognitive capability tier. A single request may legitimately traverse five capability tiers — a deterministic policy check, an embedding lookup, an SLM classifier, an LLM planner, and a reasoning ensemble for the final synthesis. Each tier has its own scaling, caching, failure, and cost characteristics.

LOA treats the capability tier as an architectural dimension alongside the classical technical layering. The two axes are orthogonal: a presentation-tier component may legitimately span T0–T3; a business-tier component may legitimately span T0–T4.

---

## Part II — Principles

**Summary.** Eleven axiomatic constraints govern LOA designs. They constrain choices; patterns supply the prescriptive realizations. Each principle lists the patterns that realize it.

**P1 — Cheapest Sufficient Tier.** Each task is allocated to the lowest-cost tier that meets correctness and quality targets. Escalation is explicit and observable. *Realized by:* Capability Router (1.1), Cascade (1.2), Capability Escalator (6.1), Confidence-Calibrated Gating (6.4).

**P2 — Determinism at the Floor.** Anything computable deterministically is computed deterministically. Models are never asked to do arithmetic, enforce policy, or replay logic that a function can express. *Realized by:* Deterministic Verifier (3.1), Hot Path Bypass (1.4).

**P3 — Separation of Cognition and Execution.** Models reason; tools act. The model never executes side effects directly; it emits intents that pass through a typed boundary that validates, authorizes, and records. *Realized by:* Typed Tool Surface (5.1), Sandboxed Executor (5.2), Plan/Execute Split (1.3).

**P4 — Tool Surface Over Codegen.** Where a domain has a well-defined operation set, expose it as a typed tool surface rather than asking the model to emit code that calls an underlying API. *Realized by:* Typed Tool Surface (5.1).

**P5 — Verification Over Plausibility.** A model's claim is not accepted because it is plausible. It is accepted because a verifier — deterministic check, prover agent, executed test — has confirmed it. *Realized by:* Deterministic Verifier (3.1), Prover Agent (3.3), Self-Consistency (3.5).

**P6 — Adversarial Validation for High-Stakes Decisions.** Any decision with high cost-of-error involves at least one agent whose explicit role is to argue against it. Proposer and critic are structurally separate, ideally backed by different models. *Realized by:* Adversarial Debater (3.2), Red Team Probe (3.4).

**P7 — State Lives at the Edges.** Models are stateless. Conversation, workflow, and domain state lives in deterministic stores; the model receives the relevant slice as context per call. *Realized by:* Tool-Mediated State (4.2), Memory (4.4), Grounded Context Injector (4.1).

**P8 — Idempotency at Side-Effect Boundaries.** Every action a model can trigger is idempotent or carries an idempotency key. Models retry; orchestrators retry; users retry. *Realized by:* Idempotent Action (5.3).

**P9 — Typed Schema Boundaries.** Inter-agent and agent-to-tool communication is via typed schemas. Free-text between agents requires explicit justification. *Realized by:* Schema-Constrained Output (2.5), Typed Tool Surface (5.1).

**P10 — Audit Everything.** Every model call, tool invocation, and decision is logged with inputs, outputs, model identity, version, tier, and cost. Without this, distillation, debugging, and compliance are all impossible. *Realized by:* Receipt Ledger (4.3), Audit Trail (6.3).

**P11 — Least-Privilege Delegated Identity.** An agent acts under the identity of the *requesting principal* with the *minimum* authority the task requires — never a broad ambient service credential standing in for the user. Authorization is evaluated at the tool boundary against that principal, and the *acting principal* is recorded alongside the model identity on every side effect. This is the architectural counterpart to delegated-auth SDK surfaces (e.g. Entra-delegated Work IQ, per-user MCP tokens): the system must not silently broaden scope to "make it work." *Realized by:* Typed Tool Surface (5.1), Sandboxed Executor (5.2), Audit Trail (6.3).

---

## Part III — Canonical Architecture Archetypes

**Summary.** Nine recurring shapes characterize AI-integrated systems. Archetypes are not orthogonal — large systems compose multiple archetypes — but each captures a distinct dominant pattern. The Decision Framework (Part VIII) selects among them.

### Archetype A — Cascade Pipeline

**Intent.** Process high-volume input streams where the vast majority of items are routine and a small fraction warrant deep analysis.

**Structure.** Progressively more expensive filter/classifier stages. Deterministic filter eliminates obvious cases. SLM classifier handles the bulk. Frontier LLM analyzes only the residual. Adversarial verification gates output.

**Tier allocation.** Approximately 95% T0/T2, 5% T3, under 1% T4.

**Canonical domains.** Security event triage, content moderation, fraud detection, log analysis, customer support routing.

**Composes.** Capability Router (1.1), Cascade (1.2), Adversarial Debater (3.2), Distillation Loop (2.3), Audit Trail (6.3), Graceful Degradation (6.5).

```
[Stream] → [Deterministic Filter] → [SLM Classifier] → [LLM Investigator] → [Adversarial Validator] → [Action]
              99% eliminated         4% resolved        <1% escalated        confirms before act
```

### Archetype B — Adversarial Ensemble

**Intent.** Discover, validate, and prove findings in domains where false positives are expensive and false negatives are catastrophic.

**Structure.** Specialized agent roles — proposer, debater, prover, deduplicator — operate in stages on a shared evidence store. Frontier models do generation; distilled models do high-volume debate; deterministic provers do final validation. The proof step is the trust anchor: a finding is emitted only if a witness (exploit, test, proof object) can be executed.

**Tier allocation.** Mixed by stage; total budget per finding is high but bounded.

**Canonical domains.** Vulnerability discovery, scientific hypothesis generation, regulatory compliance audit, legal discovery, root-cause analysis.

**Composes.** Plan/Execute Split (1.3), Adversarial Debater (3.2), Prover Agent (3.3), Receipt Ledger (4.3), Capability Escalator (6.1), Audit Trail (6.3).

*Exemplars: Microsoft MDASH; Google Big Sleep / Project Naptime; DeepMind AlphaProof.*

### Archetype C — Tool-Mediated Constructor

**Intent.** Produce complex structured artifacts (CAD models, presentations, code, infrastructure) through composition of well-defined operations rather than code generation.

**Structure.** A typed tool surface exposes domain-meaningful operations. A planner LLM decomposes intent into a sequence of operations. An executor — often an SLM, since the action space is constrained — emits tool calls. The tool runtime validates, executes, and returns typed results or errors. State is server-side; the model sees handles, not geometry or binary blobs.

**Tier allocation.** Approximately 10% T3 for planning, 80% T2 for per-step execution, 10% T0 for tool runtime.

**Canonical domains.** Parametric CAD, presentation generation, IaC synthesis, structured document authoring, financial model construction.

**Composes.** Typed Tool Surface (5.1), Plan/Execute Split (1.3), Sandboxed Executor (5.2), Idempotent Action (5.3), Schema-Constrained Output (2.5).

*Exemplars: Model Context Protocol (MCP) servers; Onshape parametric APIs accessed by LLM agents.*

### Archetype D — Grounded Synthesizer

**Intent.** Answer or generate over a body of authoritative content the model was not trained on, with traceability to sources.

**Structure.** Hybrid retrieval (vector + lexical + structured + graph as appropriate to the corpus) feeds re-ranking, then context assembly, then LLM synthesis with citation. Modern implementations are increasingly agentic: the model decides what to retrieve, iterates on insufficient results, and chains retrievals. Retrieved sources are part of the output alongside the answer.

**Tier allocation.** Retrieval is T0+T1; re-ranking optionally T2; synthesis is T3. Agentic variants may iterate, raising aggregate cost.

**Canonical domains.** Enterprise search, customer-facing Q&A over docs, research assistants, legal and medical reference, decision-support over policy.

**Composes.** Hybrid Retrieval (4.5), Grounded Context Injector (4.1), Semantic Cache (2.1), Capability Router (1.1), Guardrail Filter (6.2).

### Archetype E — Generate-Verify-Select Loop

**Intent.** Search a large solution space for improvements to a problem with a verifiable objective function.

**Structure.** A population of candidate solutions. The LLM proposes mutations or new candidates. A deterministic evaluator scores them. Selection and replacement update the population. The loop iterates until convergence or budget exhaustion. The key property is that the evaluator is cheap and authoritative — without that asymmetry, the loop diverges in cost.

**Tier allocation.** T3 for proposal (modest per-call), T0/T1 for scoring (volume but cheap), T0 for orchestration.

**Canonical domains.** Algorithm discovery, scheduling, supply-chain optimization, prompt optimization, trading strategy search, hyperparameter tuning, materials discovery.

**Composes.** Plan/Execute Split (1.3), Deterministic Verifier (3.1), Token Budget Throttle (2.4), Receipt Ledger (4.3).

*Exemplars: DeepMind AlphaEvolve; FunSearch; AutoML systems.*

### Archetype F — Copilot Aside Hot Path

**Intent.** Embed AI assistance into systems where the hot path has hard latency or determinism constraints that AI cannot meet.

**Structure.** The hot path runs without AI dependency. A parallel advisory channel feeds AI-generated insights to humans, dashboards, or asynchronous decision points. The AI never blocks or directly mutates hot-path state; it informs. Hot-path configuration may be modified by AI-derived recommendations only after governance review.

**Tier allocation.** Hot path 0% AI; advisory channel mixed T0–T4.

**Canonical domains.** Trading systems, real-time control, payments authorization, ad serving, high-frequency telemetry, industrial control.

**Composes.** Hot Path Bypass (1.4), Grounded Context Injector (4.1), Adversarial Debater (3.2), Audit Trail (6.3), Graceful Degradation (6.5).

### Archetype G — Continuous Sentinel

**Intent.** Maintain ongoing oversight of a system's state against a policy, generating findings and remediations as conditions evolve.

**Structure.** Continuous telemetry ingestion feeds deterministic policy evaluation, then SLM enrichment and categorization, then LLM root-cause and remediation drafting, then adversarial verification, then human-in-loop or autonomous remediation. The loop never terminates; the system carries state across cycles.

**Tier allocation.** Heavy at T0/T2; T3 reserved for novel or escalated cases; T4 for the highest-severity, contested findings.

**Canonical domains.** Cloud incident DRI, compliance enforcement, security posture management, fleet health, supply-chain monitoring.

**Composes.** Continuous Sentinel composes other archetypes: Cascade Pipeline (A) handles routine inflow; Adversarial Ensemble (B) handles escalations. Additional patterns: Receipt Ledger (4.3), Capability Escalator (6.1), Idempotent Action (5.3), Confidence-Calibrated Gating (6.4).

### Archetype H — Long-Horizon Agent

**Intent.** Pursue a goal across hours, days, or thousands of model calls, with persisted memory, autonomous tool use, and iterative replanning under intermediate feedback.

**Structure.** A loop of plan, execute, observe, reflect, replan. Distinguished from Plan/Execute Split (1.3) by its iterative nature — plans are revised on contact with reality — and by its memory architecture: episodic memory of past actions and outcomes, semantic memory of accumulated facts, working memory of current focus. The agent operates over a long time horizon; durable state and resilience to model swaps are primary concerns.

**Tier allocation.** Reflection and replanning at T3 or T3+. Per-step execution often at T2. Memory operations at T0/T1. Aggregate cost depends on horizon length; budgeting and graceful degradation are mandatory.

**Canonical domains.** Software engineering agents (SWE-agent, Devin, Claude Code agentic mode, OpenHands), research agents, complex workflow automation, ops and SRE agents, long-running data analysis.

**Composes.** Plan/Execute Split (1.3), Hierarchical Decomposition (1.5), Reflection (3.6), Memory (4.4), Tool-Mediated State (4.2), Typed Tool Surface (5.1), Token Budget Throttle (2.4), Capability Escalator (6.1), Graceful Degradation (6.5), Audit Trail (6.3).

### Archetype I — Multimodal Pipeline

**Intent.** Process inputs or produce outputs that combine text with images, audio, video, or structured sensor data.

**Structure.** Modality-specific encoders (vision tower, ASR, video frame sampler) transform non-text inputs into representations the LLM consumes. Modality-specific decoders (image generators, TTS, structured renderers) produce non-text outputs. The LLM orchestrates and reasons; modality specialists handle perception and rendering. Verification varies by modality and is often weaker than text verification — perceptual claims rarely have deterministic verifiers.

**Tier allocation.** Modality encoders at T1 or T2. Cross-modal reasoning at T3 or T3+. Cost profile differs sharply from text: image and video tokens are dense and expensive; audio is moderate; sensor streams are highly compressible.

**Canonical domains.** Document understanding (text + diagrams + tables), medical imaging review with reports, video content moderation, voice-driven agents, robotics and embodied AI, multimodal accessibility tools.

**Composes.** Capability Router (1.1, modality-keyed), Hybrid Retrieval (4.5, multimodal embeddings), Schema-Constrained Output (2.5), Receipt Ledger (4.3), Guardrail Filter (6.2), Computer-Use Agent (1.7) when output is GUI interaction.

*Distinguishing concern.* P5 Verification Over Plausibility is harder to satisfy for perceptual claims. Multimodal systems SHOULD invest more heavily in Self-Consistency (3.5) and Adversarial Debater (3.2) where deterministic verifiers are unavailable.

---

## Part IV — Pattern Catalog

**The catalog is not held here.** Its ~64,000 characters are a *lookup* surface — you consult a
row when you have already chosen a pattern to evaluate — while the rest of this document is read
linearly. Holding both in one file meant every reader of the principles also paid for the whole
catalog, on every load.

> **Full catalog:** [`docs/knowledge/layered-optimized-architecture/pattern-catalog.md`](../../docs/knowledge/layered-optimized-architecture/pattern-catalog.md)
> — every pattern with its intent, structure, applicability, trade-offs and cost impact, unchanged.

Read it when you are **choosing or justifying a specific pattern**. Appendix A (Pattern Quick
Reference) and Appendix B (Archetype-to-Pattern Mapping) stay in this document and are usually
enough to *name* the candidate; the catalog is where you go to defend the choice.
## Part V — Composition Examples

**Summary.** Four complete worked examples showing how archetypes and patterns compose into real systems. Each example annotates the patterns it uses and the principles those patterns realize.

### Composition A — Day-Trader / CFD Application (Copilot Aside Hot Path)

The hot path — quote ingestion, signal computation, order placement, risk gating — is fully deterministic. No AI in the request/response cycle (P2, P3, Hot Path Bypass 1.4).

Aside the hot path, three AI-driven channels operate asynchronously:

1. **News and sentiment enrichment.** Streaming news events traverse a Cascade (1.2): deterministic ticker tagger → SLM sentiment classifier → LLM only on ambiguous or high-importance items. Output is structured (2.5) and joined to the deterministic feature store consumed by the hot path.

2. **Strategy advisor.** When the trader explores a new strategy, a Plan/Execute Split (1.3) decomposes the hypothesis: planner LLM proposes feature mix and parameters; executor (deterministic backtest) is the Prover Agent (3.3). An Adversarial Debater (3.2) argues regime risks. Verified strategies write to a strategy registry the hot path can adopt only after human approval.

3. **Post-trade post-mortems.** Closed positions feed through a Grounded Context Injector (4.1) over market context, news, and historical comparable trades. An LLM drafts narrative. A Receipt Ledger (4.3) records every step.

Token Budget Throttle (2.4) bounds the cost of any single advisory request. Graceful Degradation (6.5) ensures advisory channels fail to a "no recommendation, use prior" state, never blocking the hot path. Audit Trail (6.3) records every advisory output and every adopted change.

### Composition B — Cloud Incident DRI (Continuous Sentinel + Adversarial Ensemble)

Telemetry ingestion is deterministic. A Cascade Pipeline classifies alerts: deterministic rules → SLM categorizer → LLM only on novel patterns.

When an alert exceeds severity threshold, the system enters Adversarial Ensemble mode:

- **Auditor agent** (LLM) generates root-cause hypotheses from correlated signals via Grounded Context Injector over recent deployments, incident history, dependency graph.
- **Debater agent** (SLM) challenges each hypothesis: does it explain *all* observed symptoms? What evidence contradicts?
- **Prover agent** (deterministic + LLM): runs replay or canary test to confirm reproducibility.
- **Remediation planner** (LLM via Typed Tool Surface) drafts a fix using IaC and runbook tools.
- **Executor** runs the remediation in a Sandboxed Executor against a canary slice; Idempotent Action ensures safe retry.
- **Receipt Ledger** records the full chain; the post-incident report generates from it.

Capability Escalator (6.1) allows the SLM debater to flag a hypothesis as beyond its judgment. Token Budget Throttle (2.4) prevents runaway loops. Confidence-Calibrated Gating (6.4) decides when remediation can auto-execute vs. requires human approval. Guardrail Filter (6.2) prevents the remediation planner from emitting destructive operations outside scope.

### Composition C — Compliance Across the Dev Loop (Continuous Sentinel)

**Inner loop (developer machine, CI).** Deterministic policy-as-code, SAST, secret scanning. An SLM enriches violations with intent classification (refactor vs feature vs security-relevant) and drafts contextual explanations. An LLM is invoked only when the developer requests a remediation PR; it generates the fix via Typed Tool Surface over code-edit tools, then a Prover Agent runs the test suite.

**Outer loop (CD, runtime).** Deterministic gates remain authoritative — no AI in the deploy decision. A Red Team Probe (3.4) continuously tries to ship non-compliant artifacts through the pipeline, surfacing gaps in deterministic gates. An LLM advisor interprets ambiguous regulatory text against specific implementations and drafts auditor narratives. Audit Trail (6.3) is the system of record.

### Composition D — Software Engineering Agent (Long-Horizon Agent)

A multi-hour task to refactor a service across a repository:

- **Orchestrator** decomposes the goal using Hierarchical Decomposition (1.5): map call sites, plan migration, execute migrations file-by-file, run tests, write PR.
- **Memory** (4.4) carries semantic memory of codebase facts across sessions; episodic memory of which files have been touched; working memory of the current refactor.
- **Plan/Execute Split** (1.3) governs each phase; the plan is revised after each test run.
- **Typed Tool Surface** (5.1) exposes file edits, test runs, and git operations.
- **Reflection** (3.6) runs on each file edit; the agent reviews its own change before committing.
- **Sandboxed Executor** (5.2) runs tests in an isolated environment.
- **Token Budget Throttle** (2.4) caps total task cost; Graceful Degradation (6.5) returns a partial result with a documented stop-point if budget exhausts.
- **Audit Trail** (6.3) captures every edit, test result, and reasoning trace.

### Composition E — Document Understanding (Multimodal Pipeline)

Processing a portfolio of PDFs that mix text, tables, diagrams, and handwritten annotations:

- **Page classifier** (T1 specialist) tags each page by content type before further processing.
- **OCR + layout analyzer** (T1) extracts text with positional metadata.
- **Table extractor** (T2 SLM) emits structured tables via Schema-Constrained Output (2.5).
- **Diagram interpreter** (T3 vision-capable LLM) describes diagrams; Self-Consistency (3.5) runs N=3 to mitigate perceptual ambiguity.
- **Cross-modal synthesizer** (T3) integrates extracted text, tables, and diagram descriptions into a unified document representation, via Grounded Context Injector (4.1) with Hybrid Retrieval (4.5) over the document's own pages.
- **Guardrail Filter** (6.2) checks output for PII before downstream consumption.
- **Receipt Ledger** (4.3) and **Audit Trail** (6.3) capture per-page processing for compliance.

This composition illustrates the multimodal challenge: P5 Verification Over Plausibility is satisfied by deterministic verifiers for OCR (re-run, compare) and tables (count rows, validate types) but only by Self-Consistency for the diagram interpretation step. The architecture acknowledges this asymmetry rather than hiding it.

---

## Part VI — Guidance for AI Code Generation

When an AI assistant generates code or designs for an AI-integrated system, it MUST:

1. **Identify the archetype first.** Before generating components, determine which archetype best matches the user's intent. State the archetype explicitly.

2. **Allocate tiers explicitly.** For each component, name the tier (T0–T4) it operates at. Do not default to "the LLM does everything."

3. **Apply principles as filters.** Every generated design must be checked against P1–P10. A design that violates P2 by asking an LLM to compute arithmetic is wrong even if it appears to work.

4. **Name patterns in comments.** When implementing a pattern, name it: `// Pattern: Adversarial Debater (3.2)`. This makes the architecture legible to future humans and future AI consumers of the codebase.

5. **Pair every model call with a budget context.** No exceptions. The Unbudgeted Loop is a production hazard.

6. **Prefer typed boundaries.** Inter-agent and agent-to-tool communication should be via declared schemas. Free-text between agents requires explicit justification in a code comment.

7. **Compose, do not monolith.** A single function that "calls the LLM and does everything" is wrong. Decompose into the participants the relevant pattern names.

8. **Verify before acting.** Any generated code that takes a model output and uses it to trigger side effects must pass it through a Deterministic Verifier (3.1) or Schema-Constrained Output (2.5), never directly.

9. **Generate receipts.** Every model call writes a Receipt (4.3). Code that calls a model without logging the call is incomplete.

10. **Declare fallback.** Every handler that can fail or exceed budget has a declared Graceful Degradation (6.5) path. No bare provider calls.

11. **Document the deviation.** When the user requests something that violates a principle, state the violation, note the consequence, and proceed.

12. **Act under the request principal.** Side-effecting tool calls run under the requesting principal at least privilege (P11), and the acting principal is recorded with the receipt. Never resolve authority from an ambient service credential to "make it work."

---

## Part VII — Conformance Criteria

**Summary.** Eleven criteria define what LOA-conformance means. Each is mechanically checkable; Appendix G provides reference Roslyn analyzer skeletons. Normative keywords (MUST, SHOULD, MAY) follow RFC 2119.

**C1 — Tier annotation (MUST).** Every component that initiates a model call MUST be annotated with its operating tier(s) via `[CapabilityTier(Tier.T3)]` or equivalent.

**C2 — Budget propagation (MUST).** Every public method that initiates or chains model calls MUST accept a `BudgetContext` parameter or descend from one in scope. No method that calls a model lacks a path to a budget.

**C3 — Receipt emission (MUST).** Every model call site MUST be within an `Activity` scope and MUST feed a `ReceiptLedger`. Silent calls are non-conformant.

**C4 — Typed boundaries (SHOULD).** Cross-agent messages SHOULD be declared record types. Free-text passing requires an `[UntypedBoundary("justification")]` attribute and an entry in the deviation log.

**C5 — Side-effect protection (MUST).** Model output used to invoke a side-effecting tool MUST pass through a verifier or schema validation. The static-analysis rule flags `await tool.X(modelOutput)` where `modelOutput` came directly from a model call without intervening validation.

**C6 — Idempotency keys (MUST).** Tools with persistent side effects MUST accept an `IdempotencyKey`. Analyzers flag side-effecting tool calls without one.

**C7 — Fallback declaration (SHOULD).** Every handler in a critical path SHOULD have a non-null fallback or an explicit `[NoFallback("justification")]` attribute.

**C8 — Pattern naming (SHOULD).** Where a pattern is implemented, the implementing type SHOULD bear a `[Pattern("3.2 Adversarial Debater")]` attribute. The pattern catalog is the vocabulary; named usage is the conformance signal.

**C9 — Anti-pattern absence (MUST).** Code MUST NOT exhibit any anti-pattern signature in Appendix C. The reference analyzer flags each.

**C10 — Audit completeness (MUST for regulated; SHOULD otherwise).** For systems in regulated contexts, an automated check MUST confirm every model call has a receipt, every action has an audit entry, and the chain is queryable end-to-end. Non-regulated systems SHOULD meet this criterion.

**C11 — Principal propagation (MUST for regulated; SHOULD otherwise).** Every side-effecting tool call MUST carry the acting principal, and authorization MUST be evaluated at the tool boundary against that principal rather than an ambient service credential. The Receipt/Audit record MUST capture the acting principal alongside the model identity. Realizes P11; analyzers flag side-effecting tool invocations that resolve authority from process-wide credentials instead of the request's principal.

A LOA-conformant implementation satisfies all MUST-level criteria. Deviation from SHOULD-level criteria requires documented rationale in an Architecture Decision Record (see Appendix F).

---

## Part VIII — Decision Framework

**Summary.** Selecting an archetype is the first architectural decision; selecting patterns within it is the second. The decision proceeds top-down through dominant constraints.

### Archetype selection table

| Question | Answer = Yes | Archetype |
|---|---|---|
| Hard latency or determinism boundary in request path? | Yes | **F. Copilot Aside Hot Path** |
| High-volume stream with long tail of difficulty? | Yes | **A. Cascade Pipeline** |
| Search over solution space with cheap verifier? | Yes | **E. Generate-Verify-Select Loop** |
| Produce structured artifact via defined operations? | Yes | **C. Tool-Mediated Constructor** |
| Retrieval over corpus + synthesis step? | Yes | **D. Grounded Synthesizer** |
| Findings high-stakes; proof required, not plausibility? | Yes | **B. Adversarial Ensemble** |
| Ongoing oversight against a policy? | Yes | **G. Continuous Sentinel** |
| Long-horizon, multi-session, with persisted memory? | Yes | **H. Long-Horizon Agent** |
| Non-text modalities (image, audio, video)? | Yes | **I. Multimodal Pipeline** |

These are not mutually exclusive — large systems compose multiple archetypes. Continuous Sentinel composes Cascade Pipeline and Adversarial Ensemble; Long-Horizon Agent often composes with Tool-Mediated Constructor.

### Pattern selection rules

Within an archetype, walk the "Composes" list. For each item, apply:

| Condition | Apply |
|---|---|
| Operation is high-volume | Cascade (1.2), Distillation Loop (2.3) |
| Request distribution is heterogeneous | Capability Router (1.1), Confidence-Calibrated Gating (6.4) |
| Output drives side effects | Deterministic Verifier (3.1) or Adversarial Debater (3.2), Idempotent Action (5.3) |
| Response is latency-sensitive | Speculative Execution (1.6), Prompt Prefix Cache (2.2) |
| Correctness is verifiable | Prover Agent (3.3) preferred over Adversarial Debater (3.2) |
| Task is long-running | Memory (4.4), Tool-Mediated State (4.2), Graceful Degradation (6.5) |
| System is regulated | Audit Trail (6.3) MUST; Guardrail Filter (6.2) likely |
| Multiple providers available | Provider Portability (6.6) |
| Quality regressions on model updates likely | Evaluation Harness (Part IX) |

### Tier budgeting heuristic

Start from the cheapest tier. For each component:

1. Can a deterministic implementation solve this? (T0) → Use it.
2. Can a specialist classifier or embedder? (T1) → Use it.
3. Can an SLM handle this with structured I/O? (T2) → Use it.
4. Does this need general reasoning over diverse input? (T3) → Use it.
5. Does this require extended thinking or multi-step proof? (T3+ or T4) → Use it, with explicit budget and verification.

Most workloads use T0–T2 for over 90% of operations and reserve T3+ for the difficult residual.

---

## Part IX — Evaluation Harness

**Summary.** An evaluation harness is the test suite for an AI-integrated system. It is not optional. Without continuous evaluation, model updates silently degrade quality, distillation cannot be validated, and regressions surface only in production.

### Components

- **Eval suite.** A curated set of held-out test cases with expected behaviors. Cases cover happy paths, edge cases, known failure modes, regressions, and adversarial inputs. Each case has a deterministic scorer where possible; otherwise an LLM-judge with calibrated rubric.
- **Per-pattern metrics.** Each pattern under evaluation reports accuracy, latency, cost, and pattern-specific signals (cache hit rate for 2.1; routing accuracy for 1.1; verification pass rate for 3.1).
- **Regression detection.** New model versions run the eval suite before promotion. Statistically significant degradation on any metric blocks promotion.
- **A/B framework.** Production traffic splits between current and candidate; aggregate metrics compare; promotion follows confidence interval criteria.
- **Online metrics.** Production runs surface real-world quality signals (user thumbs-down, retry rate, escalation rate, downstream verification failure rate). These feed the eval suite as new cases.

### Architectural placement

The Evaluation Harness is a continuous service running alongside the production system, consuming receipts (Pattern 4.3) and producing dashboards and alerts. It composes with:

- **Distillation Loop (2.3)** — validates each distilled candidate before promotion.
- **Provider Portability (6.6)** — measures per-provider quality to maintain the capability matrix.
- **Red Team Probe (3.4)** — generates new adversarial test cases continuously.
- **Confidence-Calibrated Gating (6.4)** — provides the calibration data to keep confidence thresholds tuned.

### Conformance

Systems in production SHOULD operate an Evaluation Harness with at least:

- **E1.** A frozen eval suite, version-controlled.
- **E2.** Automated regression detection on model and prompt changes.
- **E3.** Online quality metrics piped from production receipts.
- **E4.** A documented promotion process for model and pattern changes.

Systems in regulated contexts MUST meet E1–E4 and additionally MUST maintain a record of every promotion decision linked to its eval results.

---

## Appendix A — Pattern Quick Reference

| # | Pattern | Category | Use when |
|---|---|---|---|
| 1.1 | Capability Router | Routing | Requests vary in complexity |
| 1.2 | Cascade | Routing | High-volume, long-tail difficulty |
| 1.3 | Plan/Execute Split | Routing | Multi-step workflows |
| 1.4 | Hot Path Bypass | Routing | Latency or determinism critical |
| 1.5 | Hierarchical Decomposition | Routing | Complex multi-disciplinary tasks |
| 1.6 | Speculative Execution | Routing | Latency-sensitive with mixed difficulty |
| 1.7 | Computer-Use / GUI Agent | Routing | Target system has no API |
| 2.1 | Semantic Cache | Cost | Repetitive query traffic |
| 2.2 | Prompt Prefix Cache | Cost | Stable prompt prefixes |
| 2.3 | Distillation Loop | Cost | Mature, narrow-distribution traffic |
| 2.4 | Token Budget Throttle | Cost | All agentic workflows |
| 2.5 | Schema-Constrained Output | Cost | Programmatic consumers |
| 2.6 | Test-Time Compute Budget | Cost | Reasoning models with adjustable depth |
| 3.1 | Deterministic Verifier | Verification | Domain has checkable claims |
| 3.2 | Adversarial Debater | Verification | High-stakes decisions |
| 3.3 | Prover Agent | Verification | Witnesses can be constructed |
| 3.4 | Red Team Probe | Verification | Coverage gaps matter |
| 3.5 | Self-Consistency | Verification | Limited answer set, modest cost budget |
| 3.6 | Reflection (Self-Critique) | Verification | Cheap accuracy improvement on drafts |
| 4.1 | Grounded Context Injector | State | Authoritative external corpus |
| 4.2 | Tool-Mediated State | State | Long-running or large state |
| 4.3 | Receipt Ledger | State | All production workflows |
| 4.4 | Memory (Working/Episodic/Semantic) | State | Long-horizon or multi-session agents |
| 4.5 | Hybrid Retrieval | State | Non-trivial corpus with mixed query types |
| 5.1 | Typed Tool Surface | Tools | Stable operation set |
| 5.2 | Sandboxed Executor | Tools | Side effects from model output |
| 5.3 | Idempotent Action | Tools | Retries possible |
| 6.1 | Capability Escalator | Safety | Tiered routing |
| 6.2 | Guardrail Filter | Safety | User-facing or regulated |
| 6.3 | Audit Trail | Safety | Compliance or auditability |
| 6.4 | Confidence-Calibrated Gating | Safety | Calibration data available |
| 6.5 | Graceful Degradation | Safety | Production availability requirements |
| 6.6 | Provider Portability | Safety | Multi-provider strategy |

## Appendix B — Archetype-to-Pattern Mapping

| Archetype | Primary patterns |
|---|---|
| A — Cascade Pipeline | 1.1, 1.2, 2.3, 3.2, 4.3, 6.5 |
| B — Adversarial Ensemble | 1.3, 3.2, 3.3, 4.3, 6.1, 6.3 |
| C — Tool-Mediated Constructor | 1.3, 2.5, 5.1, 5.2, 5.3 |
| D — Grounded Synthesizer | 4.1, 4.5, 2.1, 1.1, 6.2 |
| E — Generate-Verify-Select Loop | 1.3, 3.1, 2.4, 4.3 |
| F — Copilot Aside Hot Path | 1.4, 4.1, 3.2, 6.3, 6.5 |
| G — Continuous Sentinel | 1.2, 3.2, 3.4, 5.3, 6.1, 6.3, 6.4 |
| H — Long-Horizon Agent | 1.3, 1.5, 3.6, 4.2, 4.4, 5.1, 6.5 |
| I — Multimodal Pipeline | 1.1, 1.7, 2.5, 3.5, 4.5, 6.2 |

## Appendix C — Anti-Patterns with Severity and Detection Criteria

Each anti-pattern carries a severity tier: **Correctness** (the code is wrong), **Operational** (the code runs in production but creates incidents), **Compliance** (the code fails audit), or **Style** (the code works but obscures intent).

- **The Monolithic Agent** *(Operational)*. A single LLM call instructed to do planning, execution, verification, and output formatting. *Detection:* prompt string contains both "plan" and "execute" verbs and the call produces side effects. *Resolution:* decompose with Plan/Execute Split (1.3) and Adversarial Debater (3.2). *Violates:* P3, P5, P6.

- **The Trusting Consumer** *(Correctness)*. Code that takes model output and triggers side effects without a verifier or schema check. *Detection:* model-return value flows to a side-effecting call without intervening validation. *Resolution:* apply Deterministic Verifier (3.1) or Schema-Constrained Output (2.5). *Violates:* P5.

- **The Unbudgeted Loop** *(Operational)*. An agentic workflow without a token budget. *Detection:* model-call site not within a `BudgetContext` scope. *Resolution:* apply Token Budget Throttle (2.4). *Violates:* C2.

- **The Stateful Model** *(Operational)*. Code that relies on the model carrying state across calls. *Detection:* the same conversation ID is reused across calls with the expectation of memory; no Tool-Mediated State surface. *Resolution:* apply Tool-Mediated State (4.2) or Memory (4.4). *Violates:* P7.

- **The Free-Text Bus** *(Style)*. Agents communicating in unstructured natural language. *Detection:* agent-to-agent message types are `string`. *Resolution:* apply Schema-Constrained Output (2.5). *Violates:* P9.

- **The Silent Call** *(Compliance)*. A model invocation that is not logged. *Detection:* model-call site not within an `Activity` scope or with no Receipt Ledger output. *Resolution:* apply Receipt Ledger (4.3). *Violates:* P10, C3.

- **The Hot-Path LLM** *(Operational)*. A model in a sub-second or regulatory critical path. *Detection:* a model call within a method tagged with a tight SLA. *Resolution:* apply Hot Path Bypass (1.4). *Violates:* P2 and often SLA.

- **The Codegen Tax** *(Style)*. Asking the model to emit code that calls an API when a typed tool would do. *Detection:* prompt asks for "code" or "Python" or "SQL" to call a defined-elsewhere API; no Typed Tool Surface registered. *Resolution:* apply Typed Tool Surface (5.1). *Violates:* P4.

- **The Decrement-Then-Check Budget** *(Correctness)*. `Interlocked.Add` followed by sign check. *Detection:* exact code shape in budget classes. *Resolution:* use compare-and-swap (see Pattern 2.4). *Violates:* correctness.

- **The Unbounded Reflection Loop** *(Operational)*. Self-critique without termination criterion or budget. *Detection:* reflection invocation without budget or max-iteration cap. *Resolution:* bound by budget; cap iterations. *Violates:* operational hygiene; class of Unbudgeted Loop.

- **The Speculation Without Cancellation** *(Operational)*. Parallel cheap+expensive calls without cancellation propagation. *Detection:* `Task.WhenAny` over model calls without `CancellationTokenSource` linking. *Resolution:* propagate cancellation; verify provider honors it. *Violates:* cost discipline.

- **The Console-Log Audit** *(Compliance)*. Treating `ILogger.Log` as compliance evidence. *Detection:* compliance-tagged region uses standard logger instead of receipt ledger or audit trail. *Resolution:* apply Receipt Ledger (4.3) and Audit Trail (6.3). *Violates:* P10, C3, C10.

- **The Single-Provider Lock** *(Operational)*. Direct calls to one provider's SDK throughout the codebase. *Detection:* provider-specific types in business-logic methods. *Resolution:* apply Provider Portability (6.6). *Violates:* availability hygiene.

- **The Computer-Use-When-API-Exists** *(Operational)*. Using GUI agents against a system that exposes a programmatic interface. *Detection:* Computer-Use Agent (1.7) targeting an application listed in a known-API registry. *Resolution:* use Typed Tool Surface (5.1). *Violates:* cost and reliability hygiene.

- **The Confused Deputy** *(Correctness / Security)*. Untrusted content — retrieved documents, tool outputs, user-supplied files, web pages — flows into a side-effecting tool call without mediation, letting instructions embedded in that content act with the agent's authority. The signature attack on tool-using agents: prompt injection that turns the agent into a deputy for the attacker. *Detection:* data flow from an untrusted source into a side-effecting tool argument with no intervening Guardrail Filter (6.2) or verifier; or authority resolved from an ambient credential rather than the request principal. *Resolution:* treat retrieved and tool-returned content as *data, never instructions*; mediate untrusted spans through Guardrail Filter (6.2); enforce P3 (cognition/execution split), P5 (verify before acting), and P11 (least-privilege delegated identity). *Violates:* P3, P5, P11, C5.

## Appendix D — .NET / C# Idiom Map

| Concern | .NET realization |
|---|---|
| Resilience (retry, circuit breaker, bulkhead, timeout) | `Polly` v8+ `ResiliencePipeline` |
| Distributed tracing | `System.Diagnostics.Activity` + OpenTelemetry exporters |
| Metrics (token counts, costs) | `System.Diagnostics.Metrics.Meter` and `Counter<T>` |
| Token streaming | `IAsyncEnumerable<TokenChunk>` |
| Agent-to-agent backpressure | `System.Threading.Channels.Channel<T>` |
| HTTP connection pooling | `IHttpClientFactory` with named clients per provider |
| Testable time | `TimeProvider` (.NET 8+) |
| Cancellation | `CancellationToken` propagated through every async call |
| Schema-constrained generation | `System.Text.Json` source generators + JSON schema |
| Tool registration | Source generator emitting tool schemas from `[Tool]`-attributed records |
| Idempotency store | Redis with `SET NX EX` semantics or SQL with unique constraint |
| Conformance enforcement | Roslyn analyzers (`Microsoft.CodeAnalysis.CSharp`) keyed to pattern attributes |
| Background processing | `BackgroundService` + `Channel<T>` |
| Configuration | `IOptions<T>` + `IOptionsMonitor<T>` for live reload |
| Health checks | `AspNetCore.HealthChecks` per provider |

### Wiring example: composing a LOA service in `Program.cs`

```csharp
var builder = WebApplication.CreateBuilder(args);

// Tier handlers, each implementing IRequestHandler
builder.Services.AddSingleton<IRequestHandler, DeterministicHandler>();
builder.Services.AddSingleton<IRequestHandler, SlmHandler>();
builder.Services.AddSingleton<IRequestHandler, FrontierHandler>();

// Providers (for Provider Portability 6.6)
builder.Services.AddHttpClient<AnthropicProvider>(c => c.BaseAddress = new Uri("https://api.anthropic.com"));
builder.Services.AddHttpClient<OpenAIProvider>(c => c.BaseAddress = new Uri("https://api.openai.com"));

// Resilience pipeline (Polly)
builder.Services.AddResiliencePipeline("model-call", pipeline => pipeline
    .AddTimeout(TimeSpan.FromSeconds(30))
    .AddRetry(new() { MaxRetryAttempts = 3, BackoffType = DelayBackoffType.Exponential })
    .AddCircuitBreaker(new() { FailureRatio = 0.5, MinimumThroughput = 10 }));

// Observability (Receipt Ledger 4.3)
builder.Services.AddOpenTelemetry()
    .WithTracing(t => t.AddSource("LOA.ModelCall").AddOtlpExporter())
    .WithMetrics(m => m.AddMeter("LOA.ModelCall").AddPrometheusExporter());

// Budget (Token Budget Throttle 2.4)
builder.Services.Configure<BudgetOptions>(builder.Configuration.GetSection("Budget"));

// Router (Capability Router 1.1)
builder.Services.AddSingleton<CapabilityRouter>();

var app = builder.Build();
app.Run();
```

## Appendix E — Glossary

- **Agent.** A model instance playing a defined role in a workflow (proposer, debater, executor, etc.).
- **AI-integrated system.** The complete assembly of deterministic components and model-driven components that delivers an application.
- **Archetype.** A high-level recurring shape of an AI-integrated system, characterized by a dominant request pattern and tier allocation.
- **Calibration.** The property that a stated probability matches the empirical frequency of correctness at that probability.
- **Capability tier.** A position on the deterministic-to-frontier-reasoning cost-capability axis.
- **Conformance.** Satisfaction of the criteria in Part VII, mechanically checkable.
- **Episodic memory.** Persisted record of past interactions and outcomes, retrieved by recency and relevance.
- **Frontier model.** The highest-capability commercially available general-purpose LLM at a given time.
- **Long-horizon agent.** An agent operating over hours, days, or thousands of model calls with persisted memory.
- **Model.** The neural-network artifact itself (e.g., Claude Sonnet 4.7, GPT-5).
- **Pattern.** A reusable structural solution to a recurring sub-problem within an archetype.
- **Principle.** An axiomatic constraint that governs LOA designs.
- **Receipt.** An immutable record of one model or tool call sufficient to reconstruct its inputs, outputs, model identity, cost, and trace.
- **Reasoning model.** A model variant with built-in test-time compute (extended thinking) that trades inference time for capability.
- **Semantic memory.** Distilled facts and skills accumulated over time; retrieved by similarity, periodically consolidated.
- **Test-time compute.** Inference-time reasoning budget separate from prompt and output token budget.
- **Tier allocation.** The assignment of operations to capability tiers in a workflow.
- **Working memory.** Bounded, ephemeral state representing the current focus of an agent.

## Appendix F — Architecture Decision Record (ADR) Template

For AI-integrated architecture decisions, use the following ADR template:

```markdown
# ADR-NNNN: [Decision Title]

**Status:** Proposed | Accepted | Deprecated | Superseded
**Date:** YYYY-MM-DD
**Deciders:** [names/roles]

## Context

[The forces at play. What problem are we solving? What constraints?]

## Decision

[The choice made, stated clearly.]

## LOA Mapping

- **Archetype(s):** [e.g., A. Cascade Pipeline, with elements of B. Adversarial Ensemble]
- **Patterns applied:** [List, e.g., 1.1 Capability Router, 2.4 Token Budget Throttle, 6.5 Graceful Degradation]
- **Principles upheld:** [List, e.g., P1, P3, P5, P10]
- **Principles deviated from:** [List with justification, or "none"]
- **Tier allocation:** [Component-by-component table of T0–T4 assignment]

## Conformance Impact

- **Criteria satisfied:** [List]
- **Criteria with documented deviation:** [List with justification]
- **Anti-patterns explicitly avoided:** [List]

## Alternatives Considered

[For each alternative: what it was, why it was rejected.]

## Consequences

**Positive:** [...]
**Negative:** [...]
**Unknowns to monitor:** [...]

## Cost Model

[Use Appendix H formulas; show monthly run-rate at expected volume.]
```

## Appendix G — Reference Roslyn Analyzers

Skeleton implementations for the most important conformance criteria. Full production analyzers include code fixes, suppression handling, and configuration.

### C2 — Budget Propagation Analyzer

```csharp
[DiagnosticAnalyzer(LanguageNames.CSharp)]
public sealed class BudgetPropagationAnalyzer : DiagnosticAnalyzer {
    public const string DiagnosticId = "LOA002";
    private static readonly DiagnosticDescriptor Rule = new(
        id: DiagnosticId,
        title: "Model call requires BudgetContext",
        messageFormat: "Method '{0}' calls a model but does not accept or thread a BudgetContext",
        category: "LOA.Conformance",
        defaultSeverity: DiagnosticSeverity.Error,
        isEnabledByDefault: true,
        description: "Conformance Criterion C2: every model call must be in a budget scope.");

    public override ImmutableArray<DiagnosticDescriptor> SupportedDiagnostics =>
        ImmutableArray.Create(Rule);

    public override void Initialize(AnalysisContext context) {
        context.ConfigureGeneratedCodeAnalysis(GeneratedCodeAnalysisFlags.None);
        context.EnableConcurrentExecution();
        context.RegisterOperationAction(AnalyzeInvocation, OperationKind.Invocation);
    }

    private static void AnalyzeInvocation(OperationAnalysisContext context) {
        var invocation = (IInvocationOperation)context.Operation;
        if (!IsModelCall(invocation.TargetMethod)) return;
        var containingMethod = context.ContainingSymbol as IMethodSymbol;
        if (containingMethod is null) return;
        if (!HasBudgetParameter(containingMethod))
            context.ReportDiagnostic(
                Diagnostic.Create(Rule, invocation.Syntax.GetLocation(), containingMethod.Name));
    }

    private static bool IsModelCall(IMethodSymbol method) =>
        method.ContainingType.AllInterfaces.Any(i =>
            i.ToDisplayString() == "LOA.Contracts.IModelClient");

    private static bool HasBudgetParameter(IMethodSymbol method) =>
        method.Parameters.Any(p =>
            p.Type.ToDisplayString() == "LOA.Runtime.BudgetContext");
}
```

### C5 — Side-Effect Protection Analyzer

```csharp
[DiagnosticAnalyzer(LanguageNames.CSharp)]
public sealed class SideEffectProtectionAnalyzer : DiagnosticAnalyzer {
    public const string DiagnosticId = "LOA005";
    private static readonly DiagnosticDescriptor Rule = new(
        id: DiagnosticId,
        title: "Model output drives side effects without verification",
        messageFormat: "Value from model call '{0}' flows to side-effecting method '{1}' without an intervening verifier",
        category: "LOA.Conformance",
        defaultSeverity: DiagnosticSeverity.Warning,
        isEnabledByDefault: true);

    public override ImmutableArray<DiagnosticDescriptor> SupportedDiagnostics =>
        ImmutableArray.Create(Rule);

    public override void Initialize(AnalysisContext context) {
        context.ConfigureGeneratedCodeAnalysis(GeneratedCodeAnalysisFlags.None);
        context.EnableConcurrentExecution();
        context.RegisterCompilationStartAction(compilationContext => {
            // Track local-flow: identify variables that hold model output
            // and flag if they reach side-effecting calls without verification.
            // Full implementation uses DataFlowAnalysis API.
        });
    }
}
```

### C6 — Idempotency-Key Analyzer

```csharp
[DiagnosticAnalyzer(LanguageNames.CSharp)]
public sealed class IdempotencyKeyAnalyzer : DiagnosticAnalyzer {
    public const string DiagnosticId = "LOA006";
    private static readonly DiagnosticDescriptor Rule = new(
        id: DiagnosticId,
        title: "Side-effecting tool requires IdempotencyKey",
        messageFormat: "Tool '{0}' has persistent side effects but does not accept IdempotencyKey",
        category: "LOA.Conformance",
        defaultSeverity: DiagnosticSeverity.Error,
        isEnabledByDefault: true);

    public override ImmutableArray<DiagnosticDescriptor> SupportedDiagnostics =>
        ImmutableArray.Create(Rule);

    public override void Initialize(AnalysisContext context) {
        context.RegisterSymbolAction(AnalyzeNamedType, SymbolKind.NamedType);
    }

    private static void AnalyzeNamedType(SymbolAnalysisContext context) {
        var type = (INamedTypeSymbol)context.Symbol;
        if (!HasToolAttribute(type)) return;
        if (!IsSideEffecting(type)) return;
        if (!AcceptsIdempotencyKey(type))
            context.ReportDiagnostic(
                Diagnostic.Create(Rule, type.Locations.First(), type.Name));
    }

    private static bool HasToolAttribute(INamedTypeSymbol type) =>
        type.GetAttributes().Any(a => a.AttributeClass?.Name == "ToolAttribute");

    private static bool IsSideEffecting(INamedTypeSymbol type) =>
        type.GetAttributes().Any(a => a.AttributeClass?.Name == "SideEffectingAttribute");

    private static bool AcceptsIdempotencyKey(INamedTypeSymbol type) =>
        type.Constructors.Any(c => c.Parameters.Any(p =>
            p.Type.Name == "IdempotencyKey"));
}
```

A complete reference analyzer package implementing C1–C10 with code fixes is the natural next deliverable; the skeletons above demonstrate the technique.

## Appendix H — System-Level Cost Model

A finance-facing question — *what is the monthly run-rate of this AI-integrated system?* — requires a model that aggregates per-call costs by archetype and traffic profile.

### Per-archetype cost formulas

**Cascade Pipeline (A).** Let stages 1..N have per-call costs C_i and pass-through fractions f_i (the fraction of inputs that proceed to stage i+1). Per-input expected cost:

```
E[Cost] = Σ_{i=1..N} C_i × Π_{j=1..i-1} f_j
```

For a typical 3-stage cascade with f_1 = 0.05 (95% resolve at deterministic), f_2 = 0.20 (20% of remainder escalate from SLM), C_1 = $0.0001, C_2 = $0.001, C_3 = $0.02:

```
E[Cost] = 0.0001 + 0.05 × 0.001 + 0.05 × 0.20 × 0.02
       = $0.000150 + $0.00005 + $0.0002
       ≈ $0.000400 per input
```

For 10M inputs/month: **~$4,000/month**.

**Adversarial Ensemble (B).** Cost per finding is bounded by the per-stage budget B and the expected number of debate rounds R. With proposer cost P, debater cost D, prover cost V:

```
E[Cost per finding] = P + R × D + V
```

If only F% of inputs become findings, per-input cost is `F × E[Cost per finding] + baseline triage cost`.

**Grounded Synthesizer (D).** Per call: retrieval cost (T0+T1, near-zero) + re-ranking cost (T2, ~$0.0005) + synthesis cost (T3, ~$0.01–0.05 depending on context size). With Semantic Cache hit rate h:

```
E[Cost] = (1 - h) × (Retrieval + ReRanking + Synthesis) + h × Cache
       ≈ (1 - h) × $0.03 + h × $0.0001
```

At h = 0.4: ~$0.018 per call.

**Generate-Verify-Select Loop (E).** Per converged solution: N iterations × (proposal cost + verifier cost). Verifier cost is typically T0 (negligible). Total cost is roughly N × C_proposal where N is the number of iterations to convergence.

**Long-Horizon Agent (H).** Cost per task is the hardest to estimate ahead of time; this archetype mandates Token Budget Throttle (2.4) precisely to make the worst case bounded. With budget cap B per task and average utilization u (typically 0.3–0.7), expected cost per task ≈ u × B.

### Distillation effect

Distillation Loop (2.3) shifts traffic from frontier (cost C_f) to distilled SLM (cost C_s) for the in-distribution fraction α. New per-call cost:

```
E[Cost_post_distillation] = α × C_s + (1 - α) × C_f
```

For α = 0.85, C_f = $0.02, C_s = $0.001: pre = $0.02, post = $0.0039 — roughly 5× reduction.

### Worked example: Composition B (Cloud Incident DRI)

Assumptions:
- 100,000 alerts/month, 95% resolve at deterministic, 4% at SLM, 1% escalate to Adversarial Ensemble.
- Adversarial Ensemble: proposer $0.05, debater $0.005 × 3 rounds, prover $0.01. Per finding: $0.075.
- 60% of escalations become findings.

```
Cascade cost = 100,000 × $0.000400 ≈ $40
Ensemble entries = 100,000 × 0.01 = 1,000
Findings = 1,000 × 0.60 = 600
Ensemble cost ≈ 1,000 × $0.005 (triage in ensemble) + 600 × $0.075 (full process) ≈ $50
Total monthly cost ≈ $90
```

With audit trail storage at ~$0.1/GB and Receipt Ledger volume of ~1KB/call × 100,000 = 100MB ≈ negligible.

This kind of model belongs in every architecture decision record per Appendix F.

---

## Appendix I — Per-Pattern Cost-Impact Table

A trade-off-analysis aid for architects composing patterns. Each pattern is rated across six dimensions, grouped by category for readability. The ranges are typical, not absolute — they reflect production observations across security, finance, SRE, and developer-tooling workloads.

**Reading the columns:**

- **Cost effect** is a multiplier on per-call cost of the affected operation. `0.1×` means the pattern reduces cost to one-tenth; `3×` means it triples it.
- **Latency effect** is the typical change in end-to-end response time on a hit.
- **Accuracy delta** is the typical change in task success rate on workloads where the pattern is applicable, quoted as percentage-point change.
- **Implementation effort** ranks one-time engineering cost: Low (hours), Med (days), High (weeks).
- **Ongoing burden** ranks steady-state operational concern.
- **When it pays off** is the rough applicability heuristic.

### Category 1 — Routing & Composition

| Pattern | Cost effect | Latency effect | Accuracy delta | Impl effort | Ongoing burden | When it pays off |
|---|---|---|---|---|---|---|
| 1.1 Capability Router | 0.1–0.3× aggregate | +5–20 ms | ±0 if routed correctly | Med | Med (routing drift) | >20% of traffic is "easy" |
| 1.2 Cascade | 0.05–0.2× aggregate | +10–100 ms tail | ±0 | Med | Med (threshold tuning) | Power-law difficulty distribution |
| 1.3 Plan/Execute Split | 1.2–2.0× | +200–800 ms | +10 to +30 pp on multi-step | Med | Low | Steps independent or audit needed |
| 1.4 Hot Path Bypass | N/A (orthogonal) | 0 ms on hot path | N/A | High | Med (advisory ops) | Hard SLA on hot path |
| 1.5 Hierarchical Decomposition | 2–5× | +1–5 s | +20 to +40 pp on complex tasks | High | Med (decomposition quality) | Task exceeds single-context bounds |
| 1.6 Speculative Execution | 1.5–2.0× nominal; ~1.0× on cancel hits | −40 to −60% median | ±0 | Med | Med (calibration + cancel verify) | Latency-sensitive; cheap-tier hits >40% |
| 1.7 Computer-Use / GUI Agent | 10–100× vs typed tool | +2–30 s | −10 to −30 pp vs typed tool | High | High (UI brittleness) | No API exists; last resort |

### Category 2 — Cost Optimization

| Pattern | Cost effect | Latency effect | Accuracy delta | Impl effort | Ongoing burden | When it pays off |
|---|---|---|---|---|---|---|
| 2.1 Semantic Cache | 0.3–0.7× on hits | −90 to −99% on hits | −2 to +0 pp (stale risk) | Low | Med (TTL + threshold drift) | Repetitive traffic, hit rate >20% |
| 2.2 Prompt Prefix Cache | 0.1–0.3× on prefix tokens | −10 to −30% | ±0 | Low | Low | Stable prefix >500 tokens |
| 2.3 Distillation Loop | 0.05–0.2× on distillable share | −50 to −90% on distilled | −1 to +1 pp on in-distribution | High | High (MLOps pipeline) | Mature traffic, narrow distribution |
| 2.4 Token Budget Throttle | Caps tail; ~1.0× nominal | Bounded tail | −2 to ±0 pp (clipping) | Low | Low | All agentic workflows (mandatory) |
| 2.5 Schema-Constrained Output | 0.7–0.9× (fewer retries) | −5 to −20% | +5 to +15 pp (no parse failures) | Low | Low | Programmatic consumers |
| 2.6 Test-Time Compute Budget | 0.3–3.0× depending on tier | ±1–60 s | ±0 to +20 pp on hard cases | Med | Med (difficulty estimator) | Heterogeneous traffic, reasoning models |

### Category 3 — Verification

| Pattern | Cost effect | Latency effect | Accuracy delta | Impl effort | Ongoing burden | When it pays off |
|---|---|---|---|---|---|---|
| 3.1 Deterministic Verifier | 1.0–1.3× | +50–500 ms | +10 to +40 pp (FP elimination) | Med | Low | Domain has checkable claims |
| 3.2 Adversarial Debater | 2–3× | +500 ms–2 s | +5 to +20 pp | Med | Med (debater prompt drift) | High-stakes decisions |
| 3.3 Prover Agent | 2–4× | +1–10 s | +20 to +50 pp (eliminates FPs) | High | Med | Witnesses can be constructed |
| 3.4 Red Team Probe | Continuous overhead | N/A (out of path) | Coverage gain over time | Med | Med (red-team prompt rotation) | Coverage gaps matter |
| 3.5 Self-Consistency | N× (N=3–10) | +0 ms parallel; ×N serial | +5 to +15 pp | Low | Low | Limited answer set; hard cases |
| 3.6 Reflection / Self-Critique | 2–3× | +500 ms–2 s | +3 to +10 pp | Low | Low | Cheap quality bump on drafts |

### Category 4 — State & Context

| Pattern | Cost effect | Latency effect | Accuracy delta | Impl effort | Ongoing burden | When it pays off |
|---|---|---|---|---|---|---|
| 4.1 Grounded Context Injector | 1.1–1.5× (longer context) | +50–300 ms (retrieval) | +20 to +50 pp (hallucination drop) | Med | Med (retrieval quality) | External authoritative corpus |
| 4.2 Tool-Mediated State | ~1.0× per call | +20–100 ms per state hit | ±0 (enables long workflows) | Med | Low | State outlives one call |
| 4.3 Receipt Ledger | +~$0.0001/call storage | +1–5 ms | N/A | Low | Low (storage growth) | All production workflows |
| 4.4 Memory (W/E/S) | Variable (retrieval cost) | +50–200 ms recall | +10 to +30 pp on long-horizon | High | High (consolidation pipeline) | Multi-session or long-horizon agents |
| 4.5 Hybrid Retrieval | 1.2–1.5× retrieval cost | +30–100 ms | +10 to +25 pp recall/precision | Med | Med (re-ranker tuning) | Non-trivial corpus, mixed query types |

### Category 5 — Tool Integration

| Pattern | Cost effect | Latency effect | Accuracy delta | Impl effort | Ongoing burden | When it pays off |
|---|---|---|---|---|---|---|
| 5.1 Typed Tool Surface | 0.5–0.8× (fewer tokens) | −10 to −30% | +10 to +30 pp (no parse failures) | Med | Low | Stable operation set |
| 5.2 Sandboxed Executor | ~1.0× | +50–500 ms (sandbox overhead) | N/A (safety, not accuracy) | High | Med (sandbox maintenance) | Side effects from model output |
| 5.3 Idempotent Action | ~1.0× | +5–20 ms (key check) | N/A (safety on retry) | Low | Low | Retries possible (always) |

### Category 6 — Quality & Safety

| Pattern | Cost effect | Latency effect | Accuracy delta | Impl effort | Ongoing burden | When it pays off |
|---|---|---|---|---|---|---|
| 6.1 Capability Escalator | 1.0–1.5× on escalated subset | +100 ms–1 s on escalation | +5 to +20 pp on escalated | Low | Low | Tiered routing in use |
| 6.2 Guardrail Filter | 1.1–1.3× | +50–200 ms | Safety metric, not accuracy | Med | Med (filter red-teaming) | User-facing or regulated |
| 6.3 Audit Trail | +~$0.0002/call storage | +2–10 ms | N/A (compliance) | Med | Med (retention policy) | Regulated industries |
| 6.4 Confidence-Calibrated Gating | 1.0–1.3× | +0–500 ms (verifier on uncertain) | +5 to +15 pp | Med | High (calibration drift) | Calibration feedback loop exists |
| 6.5 Graceful Degradation | ~1.0× on success; fallback cost on failure | +0 on success | Availability metric | Med | Low | Production availability matters |
| 6.6 Provider Portability | ~1.0× (may save on arbitrage) | +5–20 ms (selection) | ±0 | High | Med (capability-matrix upkeep) | Multi-provider strategy |

### Reading the table together

Three observations a finance partner can pull from this table:

**Cost compounders to watch.** Stacking Plan/Execute Split (1.3) with Hierarchical Decomposition (1.5) with Self-Consistency (3.5, N=5) with Adversarial Debater (3.2) multiplies cost by roughly 1.5 × 4 × 5 × 3 = 90×. That stack is correct for a security finding worth thousands of dollars to verify; it is wrong for routing a support email. The compositions in Part V are deliberately budget-aware.

**Cost reducers that compound favorably.** Capability Router (1.1) at 0.2× aggregate, on top of Distillation Loop (2.3) at 0.1× on the distillable share, on top of Prompt Prefix Cache (2.2) at 0.2× on prefix tokens, on top of Semantic Cache (2.1) at 0.5× on hits, can drive total cost into single-digit percentages of a naive always-frontier baseline. The order matters: route first, distill second, cache prefix and response third.

**Patterns to apply universally.** Token Budget Throttle (2.4), Receipt Ledger (4.3), Schema-Constrained Output (2.5), Graceful Degradation (6.5), and Idempotent Action (5.3) have near-zero cost effect and high downside protection. They are the always-on baseline of any production LOA implementation.

---

## Appendix J — Reference Roslyn Analyzer Package

A production-grade analyzer package implementing the MUST-level conformance criteria from Part VII is provided alongside this document as the `loa-analyzers/` directory.

**Scope.** The package implements C1 Tier annotation, C2 Budget propagation, C3 Receipt emission, C5 Side-effect protection, C6 Idempotency keys, and C8 Pattern naming. It also detects the highest-severity anti-patterns from Appendix C: The Trusting Consumer, The Unbudgeted Loop, The Decrement-Then-Check Budget, and The Silent Call.

**Package structure:**

```
loa-analyzers/
├── LOA.Analyzers.sln
├── README.md
├── .editorconfig                     # Reference severity configuration
├── src/
│   ├── LOA.Contracts/                # Attributes, interfaces, marker types
│   │   ├── LOA.Contracts.csproj
│   │   ├── Attributes.cs             # [CapabilityTier], [Pattern], [Tool], [SideEffecting]
│   │   └── Interfaces.cs             # IModelClient, BudgetContext, IdempotencyKey
│   └── LOA.Analyzers/                # Roslyn analyzers + code fixes
│       ├── LOA.Analyzers.csproj
│       ├── Diagnostics.cs            # All DiagnosticDescriptor declarations
│       ├── Analyzers/
│       │   ├── TierAnnotationAnalyzer.cs           # LOA001 (C1)
│       │   ├── BudgetPropagationAnalyzer.cs        # LOA002 (C2)
│       │   ├── ReceiptEmissionAnalyzer.cs          # LOA003 (C3)
│       │   ├── SideEffectProtectionAnalyzer.cs     # LOA005 (C5)
│       │   ├── IdempotencyKeyAnalyzer.cs           # LOA006 (C6)
│       │   ├── PatternNamingAnalyzer.cs            # LOA008 (C8)
│       │   ├── DecrementThenCheckAnalyzer.cs       # LOA101 (anti-pattern)
│       │   └── TrustingConsumerAnalyzer.cs         # LOA102 (anti-pattern)
│       └── CodeFixes/
│           ├── BudgetPropagationCodeFix.cs
│           ├── DecrementThenCheckCodeFix.cs
│           └── PatternNamingCodeFix.cs
└── tests/
    └── LOA.Analyzers.Tests/
        ├── LOA.Analyzers.Tests.csproj
        └── BudgetPropagationAnalyzerTests.cs
```

**Diagnostic ID convention.** LOA001–LOA099 map to conformance criteria (C1 = LOA001, etc.). LOA101–LOA199 map to anti-patterns. The mapping is stable; new rules append.

**Configuration.** A reference `.editorconfig` ships severity settings: MUST criteria default to `error`, SHOULD criteria default to `warning`, and anti-patterns inherit from the criterion they violate. Teams adopting incrementally can demote `error` to `warning` per-rule and lift back as conformance reaches the codebase.

**Build integration.** The analyzer assemblies target `netstandard2.0` (Roslyn analyzer convention); contracts target `net8.0`. Consume via NuGet (recommended) or project reference. CI pipelines should treat `LOA001`–`LOA010` violations as build failures.

---

## Appendix K — Reference Implementation: Composition B

A working reference implementation of Composition B (Cloud Incident DRI — Continuous Sentinel + Adversarial Ensemble) is provided as the `loa-composition-b/` directory.

**Scope.** The reference demonstrates the full pipeline end-to-end: alert ingestion through deterministic filtering, SLM categorization, frontier-LLM investigation, Adversarial Ensemble escalation (proposer/debater/prover), tool-mediated remediation, sandboxed execution, and audit trail. Model-provider calls are stubbed behind an `IModelClient` abstraction with deterministic fakes so the reference can be run and tested locally without external dependencies.

**Solution structure:**

```
loa-composition-b/
├── README.md
├── LOA.CloudIncidentDri.sln
├── src/
│   ├── LOA.CloudIncidentDri.Api/             # HTTP entry point
│   │   ├── Program.cs                        # Full DI wiring (every pattern composed)
│   │   ├── Controllers/AlertController.cs
│   │   └── appsettings.json
│   ├── LOA.CloudIncidentDri.Core/            # Domain types
│   │   ├── Alerts.cs
│   │   ├── Findings.cs
│   │   └── Tier.cs
│   ├── LOA.CloudIncidentDri.Pipeline/        # Cascade + Adversarial Ensemble
│   │   ├── CascadeStages/
│   │   │   ├── DeterministicFilterStage.cs   # T0 rules
│   │   │   ├── SlmClassifierStage.cs         # T2 categorization
│   │   │   └── LlmInvestigatorStage.cs       # T3 investigation
│   │   ├── Ensemble/
│   │   │   ├── ProposerAgent.cs              # LLM root-cause hypothesis
│   │   │   ├── DebaterAgent.cs               # SLM challenger
│   │   │   ├── ProverAgent.cs                # Replay/canary
│   │   │   └── EnsembleOrchestrator.cs
│   │   └── IncidentOrchestrator.cs           # Top-level orchestrator
│   ├── LOA.CloudIncidentDri.Tools/           # Typed tool surface
│   │   ├── IacEditTool.cs
│   │   ├── RunbookTool.cs
│   │   └── CanaryDeployTool.cs
│   ├── LOA.CloudIncidentDri.Infrastructure/  # Cross-cutting LOA patterns
│   │   ├── BudgetContext.cs                  # 2.4
│   │   ├── ReceiptLedger.cs                  # 4.3 with OTel
│   │   ├── IdempotencyStore.cs               # 5.3
│   │   ├── CapabilityRouter.cs               # 1.1
│   │   ├── ResiliencePipelineFactory.cs      # Polly setup
│   │   └── FakeModelClient.cs                # Deterministic stub
│   └── LOA.CloudIncidentDri.Memory/          # 4.4 working/episodic/semantic
│       └── IncidentMemory.cs
└── tests/
    └── LOA.CloudIncidentDri.Tests/
        ├── CascadeIntegrationTests.cs
        └── EnsembleIntegrationTests.cs
```

**Patterns demonstrated.** 1.1 Capability Router, 1.2 Cascade, 1.3 Plan/Execute Split, 1.5 Hierarchical Decomposition (orchestrator-worker), 2.4 Token Budget Throttle, 2.5 Schema-Constrained Output, 3.1 Deterministic Verifier, 3.2 Adversarial Debater, 3.3 Prover Agent, 4.3 Receipt Ledger, 4.4 Memory, 5.1 Typed Tool Surface, 5.2 Sandboxed Executor, 5.3 Idempotent Action, 6.1 Capability Escalator, 6.5 Graceful Degradation.

**Conformance.** The reference passes all MUST-level conformance criteria from Part VII when checked against the `loa-analyzers/` package. The build fails on `LOA001`–`LOA010` violations; this is the canonical demonstration that LOA conformance is achievable in real code.

**Running it.** `dotnet run --project src/LOA.CloudIncidentDri.Api` starts a local HTTP service. POSTing an alert JSON to `/api/alerts` exercises the full pipeline. The reference logs Receipt Ledger entries to console (and to OpenTelemetry if exporters are configured). The `tests/` project exercises the pipeline against canned alert fixtures.

The reference is deliberately scoped to one composition. Adapting it to other compositions (A, F, H) is a translation exercise rather than a redesign.

---

*End of document. Version 0.4. Adds Appendix I (per-pattern cost-impact table), Appendix J (reference Roslyn analyzer package), and Appendix K (reference implementation of Composition B). Companion source for the analyzer package and reference implementation is provided alongside this document.*

