---
applyTo: "**"
---
# The Rigor Protocol

*A staged broad-to-narrow reasoning discipline for coding agents. Version 1.0 — composes on top of Body of Knowledge §II (Coning + Iterative Critical Thinking).*

This document exists to defeat one failure mode above all others: **the rush to a plausible answer.** A language model is built to emit the most probable next token, and the most probable answer is often the most *plausible-sounding* one — which is not the same as the most *correct* one (BoK D2). Speed is the enemy of rigor here. This protocol deliberately slows the agent down, replacing "answer first, justify later" with **"establish first, conclude last,"** and it makes the establishing *inspectable* at every step.

It does not replace Coning or Iterative Critical Thinking (BoK §II.1–II.2). It **deepens** them: Coning says *open wide then narrow*; this protocol supplies the named techniques for *how* to open, *how* to interrogate, *how* to gather evidence, *how* to attack the conclusion, and *how* to narrow with conviction — with an evidence bar at each transition. It is the reasoning spine that the five workflow skills (`/specify`, `/define-architecture`, `/design-slice`, `/implement`, `/investigate`) all run on.

Normative keywords (**MUST**, **SHOULD**, **MAY**) follow RFC 2119.

---

## 0. The discipline in one line

> Do not answer the question. First map it, then interrogate it, then ground it in evidence, then try to break your own conclusion — *only then* answer, and answer with your confidence and your residual risk attached.

---

## 1. Why this works (the evidence base, condensed)

The protocol is an engineering synthesis of disciplines that each independently raise reasoning quality. They are not decoration; each stage below cites the one it operationalizes.

- **The funnel / cone** (qualitative interviewing; investigative interviewing). Start broad and open, narrow deliberately. Premature specificity introduces bias and skips the data you didn't know you needed. (BoK §II.1 already names this; here it becomes the macro-shape of the whole protocol.)
- **The Paul–Elder framework.** Reasoning has eight **elements** (purpose, question, information, inference, concepts, assumptions, implications, point of view) judged against nine **intellectual standards** (clarity, accuracy, precision, relevance, depth, breadth, logic, significance, fairness). The standards are the *quality bar*; the elements are the *checklist*.
- **Precision Questioning** (Matthies & Worline, Vervago). A taxonomy of analytic question types worked in a **one-question/one-answer** rhythm. Its central warning is the one this protocol takes most seriously: *"Why?" five times is vague.* Precision beats repetition.
- **Systems thinking** (Meadows). Behavior emerges from structure: stocks, flows, feedback loops, delays, and leverage points. The highest-leverage interventions are rarely the obvious parameters. A correct local fix to a structural problem is still a wrong answer.
- **Root-cause analysis** (5 Whys / Ishikawa / fault-tree / change / events-and-causal-factors). The method is chosen by problem shape, and **every candidate cause is verified against data before it is believed** — the universal RCA step that the "5 Whys" alone routinely skips.
- **Cognitive interviewing & the PEACE model** (evidence-based investigative interviewing). Free recall before probing; vary the order; change the perspective; summarize each segment and surface contradictions. The literature is unambiguous that **rapport and structured open-then-narrow questioning outperform pressure and accusation** — "tough tactics fail."
- **Strategic elicitation — the disconfirmation principle** (Scharff/SUE, Granhag et al.). To test a claim, *state it as a confident counter-claim and see whether it survives*, rather than asking an open question that telegraphs what you don't know. Applied here only to **interrogating problems, designs, and claims — never to deceiving the human.** Where a person is in the loop, the show-your-work and honesty directives (Rules of the Road §3) govern absolutely.
- **Chain-of-Verification & self-consistency** (the AI-native forms of the above). Draft → generate verification questions → answer them *independently of the draft* → regenerate keeping only what verified. This is BoK D3 ("verification is never self-certified") expressed as a reasoning move, and its known limitation — *a model that cannot find its own error gains nothing* — is exactly why the protocol routes high-stakes conclusions through execution and a structurally separate adversary, not self-review alone.

---

## 2. The Rush Interdiction (Stage 0 — non-negotiable)

Before any other work, for any task above T0 (Rules of the Road §0.2), the agent **MUST NOT** state a solution, a conclusion, a root cause, or an API contract. It produces a **frame** first (Stage 1). This is a hard ordering constraint, not a suggestion.

Three interdiction rules apply throughout:

1. **No conclusion without a confidence label.** Every claim the agent commits to is tagged **Verified** (checked against a cited source or an executed result), **Inferred** (reasoned but not run), or **Assumed/Flagged** (unconfirmed, carried as a risk). An unlabeled conclusion is a defect (BoK §II.2 Q1; Rules of the Road §3).
2. **No "Why?" without a "Why do I believe that?"** Every causal or contract claim is paired with the evidence that distinguishes it from its most plausible competitor. A claim with no disconfirming test behind it is a hope (Test Architect, Persona Catalog §2).
3. **First-plausible is a trap, not a finish.** The first framing, the first cause, the first API that "should" work — these are hypotheses to be tested in Stages 2–4, never answers to be shipped. (Anti-pattern: Premature Convergence; The Confident Guess; BoK §VIII.)

> Practical tell: if the agent finds itself writing the answer in the first paragraph of its reasoning, it has skipped Stage 0. Stop, delete it, and frame.

**Stage 0 also grounds the work in what already exists.** Before framing, the agent **MUST** load the upstream artifacts the task answers to — and it does so by **traversing the knowledge graph, not re-discovering files** (`knowledge-visualization.md` V15): starting from the artifact(s) under work, follow the typed edges 1–2 hops out — upstream via `implements`/`refines`/`depends-on` (the spec, the architecture, the ADRs), downstream via `tested-by`/`documents` (the proofs and docs that constrain change), and `uses-term` (the glossary definitions in play) — citing the traversal path as the grounding's provenance. Where the graph is absent or incomplete, fall back to the source-of-truth chain (BoK §III.1): the relevant domain knowledge bases (`docs/knowledge/`), the specification (`docs/specs/`), the architecture (`docs/architecture.md`), the component design (`docs/design/`), and, in an existing codebase, the touched code and its tests — and surface the missing edges as findings. These artifacts are **authoritative**: the work conforms to them, quotes the specific statements it must satisfy, and carries them forward without silent reinterpretation. A stale (`review-by`-past) node or an orphan discovered during grounding is likewise a finding to surface, not something to silently route around. If the current request, the code, or a downstream artifact conflicts with an upstream one, that is **drift** — the agent stops and surfaces it (re-spec, or record a deviation per Rules of the Road §4), rather than quietly diverging. This is the single most common way generated work goes wrong: it builds a *plausible* thing instead of the *specified* one (anti-pattern: Specification Drift). The agent honors this grounding on every skill unless the human explicitly instructs it not to consult prior artifacts.

---

## 3. The five stages (the cone)

Each stage states its **purpose**, the **technique** it operationalizes, the **work** it does, the **exit evidence** that lets it proceed, and the **anti-rush rule** specific to it. The stages are worked in order; a later stage may re-open an earlier one (the cone is iterative — BoK §II.2 Q3), but it may not be skipped.

The protocol scales with the tier. **T0** runs Stage 1 and Stage 5 as a quick self-check. **T1** runs all five, lightly. **T2** runs all five with full evidence and an external adversary at Stage 4 (Persona Catalog).

### Stage 1 — OPEN: map the whole before touching a part

**Purpose.** Defeat premature convergence. Produce a wide, unbiased picture of the problem space before any narrowing.

**Technique.** The funnel's wide mouth + cognitive-interview **free recall** + Paul–Elder **element decomposition** + a **systems sketch**.

**Work.**
- **Restate** the request in the agent's own words, and name the ambiguities. (Paul–Elder: *purpose*, *question at issue*.)
- **Free-recall the space.** Enumerate candidate framings, the scenarios (baseline / adversarial / wildcard), and the boundary set (empty, null, max, concurrent, malformed, hostile) — *without yet judging any of them.* Do not interrupt the enumeration to solve. (BoK §II.1 Stroke 1.)
- **Decompose by element.** For the problem as framed, lay out the eight Paul–Elder elements explicitly: purpose, question, the information you have vs. need, the concepts in play, the assumptions you are making, the inferences you would draw, the implications/consequences, and whose point of view matters.
- **Sketch the system, not just the task.** Name the stocks (what accumulates: queues, balances, state), the flows (what moves: requests, messages, money), the feedback loops (retries, backpressure, rate limits, caches), the delays (latency, eventual consistency), and the boundary you are drawing around the problem (and what you are excluding). Ask the systems-thinking question early: *is the obvious intervention point the high-leverage one, or just the visible one?*
- **List the unknowns.** Every contract (API, library, protocol, schema) this work will depend on and that is not yet established (BoK Part III). Every product or domain fact you are assuming.

**Exit evidence.** A written frame containing: the restated question, the candidate framings, the boundary set, the element decomposition, the system sketch, and the unknowns list. No solution code, no chosen approach, no named root cause yet.

**Anti-rush rule.** *Breadth is mandatory before depth.* If only one framing was produced, the cone was never opened — produce at least one genuine alternative and say why it is plausible.

### Stage 2 — INTERROGATE: drill each element with precision

**Purpose.** Turn the broad frame into sharp, answerable questions. Find the gaps in thinking *before* spending evidence-gathering effort on the wrong ones.

**Technique.** **Precision Questioning** (seven analytic question types, one-question/one-answer) governed by the **Paul–Elder standards**, with cognitive-interview **probing** of each segment.

**Work.** Take the frame from Stage 1 and interrogate it one precise question at a time. Use the seven PQ types deliberately rather than asking a vague "why":

| PQ type | The precise question it asks | Catches |
|---|---|---|
| **Clarification** | What exactly does this term/scope/requirement mean? Where is it ambiguous or vague? Can it be segmented? | Equivocation, scope blur |
| **Assumption** | What is being taken as given — about existence, uniqueness, measurement, value, audience, time-constancy, category, similarity? | Silent premises (BoK §II.2 Q1) |
| **Basic-critical (data/source)** | What is the data behind this, and what is the source? Is the source authoritative (BoK §III.1)? | The Confident Guess; unsourced claims |
| **Cause** | By what *specific* mechanism does X produce Y? Which competing cause is being ruled out, and how? | "5 Whys" hand-waving; correlation-as-cause |
| **Effect / implication** | If this is true, what *must* also be true? What downstream behavior, failure mode, or cost follows? | Unconsidered consequences |
| **Evidence (for/against)** | What weighs *for* this, what weighs *against*, and on balance? | Confirmation bias |
| **Go / No-Go** | Is this decided enough to act on, or is it still open? What would make it actionable? | Acting on the undecided; stalling on the decided |

Apply the **nine intellectual standards** as the quality bar for every answer: is it *clear, accurate, precise, relevant, deep* (does it address the real complexity), *broad* (does it consider the other viewpoint), *logical, significant* (are we drilling the thing that matters), and *fair* (steel-manned, not strawmanned)?

Run the cognitive-interview moves on the frame: **probe each segment** (don't accept a summary where a mechanism is needed), **vary the order** (does the reasoning still hold if you start from the failure and work back?), and **change perspective** (how does this read from the attacker's seat, the operator's seat at 3 a.m., the future-maintainer's seat?).

**Exit evidence.** A ranked list of the few questions that actually determine the answer, each phrased precisely (not as "why"), each tagged with what kind of evidence would resolve it. Assumptions are now explicit and individually marked *to-verify* or *accepted-with-rationale*.

**Anti-rush rule.** *One question, one answer.* Do not bundle five concerns into one sweeping question and answer it with one sweeping paragraph. Precision is the point.

### Stage 3 — EVIDENCE: establish, do not assert

**Purpose.** Convert the sharp questions into grounded facts. This is where the agent earns the right to a conclusion.

**Technique.** The **Contract Due-Diligence Protocol** (BoK Part III) + **verify-by-execution** (BoK §III.2; the Spike Protocol) + the **RCA verify-cause-by-data** step + Paul–Elder *accuracy/precision* standards. For unfamiliar APIs, SDKs, MCP servers, or protocols, the Spike Protocol (`spike-protocol.md`) is **mandatory at this stage** — read the source and run a minimal PoC; a contract established from naming intuition is not established.

**Work.**
- For each **contract** the answer depends on: establish identity/version, semantics, nullability, boundaries, error model, security model, concurrency, lifecycle, idempotency, cost, and lifecycle status. **Cite the source** (BoK §III.1). Where cheap, **confirm by execution** rather than trusting the doc.
- For each **causal claim** (especially in `/investigate`): do not stop at the first plausible cause. Reproduce the effect, vary the suspected input, and confirm the cause is necessary and sufficient — *if removing it removes the effect and restoring it restores the effect, it is verified; otherwise it is still a hypothesis.* Choose the RCA method by problem shape (linear chain → 5 Whys with evidence at each step; multi-category → Ishikawa; safety/probabilistic → fault tree; "it broke after a change" → change analysis; complex timeline → events-and-causal-factors).
- For each **product/domain fact**: find the comparable, the reference, the standard, the user evidence — name it.
- **Promote or flag.** Each Stage-2 assumption is now either *Verified* (with source/result) or remains *Flagged* as a carried risk. Unknowns that survive due diligence are reported as findings, never papered over with a guess (BoK §III.1).

**Exit evidence.** A **confidence ledger**: one row per load-bearing claim, each with its evidence, its source/version (or `file:line`, or run output), and its label (Verified / Inferred / Flagged). This ledger is the raw material for the **Proof Pack** (Rules of the Road §3.1) at implementation time.

**Anti-rush rule.** *No claim outruns its evidence.* "I expect X" and "the docs/source/run show X" are different sentences and must be written differently. If the evidence isn't there yet, the label is *Inferred* or *Flagged* — not silently *Verified*.

### Stage 4 — DISCONFIRM: try to break your own conclusion

**Purpose.** Self-review is structurally insufficient (BoK §II.3): the mind that formed a conclusion is the worst-placed to find its flaw. This stage manufactures the missing adversary — first internally, then, for T2, externally.

**Technique.** **Chain-of-Verification** + Popperian **falsification** + the **disconfirmation principle** (state the failure as a confident claim and see whether the design survives) + **self-consistency** + the **adversarial Persona Catalog**.

**Work.**
- **Chain-of-Verification pass.** Write the draft conclusion. Generate the verification questions it implies. Answer each one *independently of the draft* (so the draft cannot bias the check). Regenerate the conclusion keeping only what survived. Discard the rest.
- **Falsification pass (the oracle test).** For each correctness claim, state the input or interleaving that *would make it false*. Is that case in your evidence or your test set? A claim whose only tested inputs are the ones expected to pass is unproven (Test Architect).
- **Disconfirmation pass.** For each load-bearing belief, assert its negation as if confident — *"On redelivery this consumer double-charges, because the idempotency key is reserved after the side effect"* — and require the design (or the data) to defeat the claim with evidence. A belief that cannot be disconfirmed by any available test is either solid or untestable; decide which.
- **Self-consistency pass.** Where the cost-of-error is high and the question admits more than one reasoning route, reason it a second time by a different route (e.g., from the boundary inward, or from the failure backward). Convergence raises confidence; divergence is a finding to resolve, not to average away.
- **Convene the adversaries (T2).** Run the relevant personas' interrogation sets against the spec/plan/design-slice — proportional to cost-of-error (Persona Catalog → "How to convene"). The author does not clear its own hard veto (Security, Test Architect, Distributed Systems). Conflict between personas is the feature; resolve it explicitly and record it (do not average it into mush).

**Exit evidence.** A short adversarial record: the verification questions and their independent answers, the falsifying input for each claim (and whether it's covered), the disconfirming claims attempted and the evidence that defeated or upheld them, and — for T2 — the persona verdicts with any veto and its resolution (Rules of the Road §3.2 gate records).

**Anti-rush rule.** *Attack before you commit.* If no genuine attempt was made to break the conclusion, Stage 4 did not happen. "I reviewed it and it looks right" is not a disconfirmation pass.

### Stage 5 — CONVERGE: commit with conviction and stated residual risk

**Purpose.** Close the cone. Deliver the answer the evidence supports — no wider, no narrower — with its confidence and its limits attached.

**Technique.** Coning Stroke 2 (BoK §II.1) + Paul–Elder *significance/logic* + the **re-grounding** discipline (BoK §VI.2).

**Work.**
- **Name the conclusion** (the chosen approach / the root cause / the contract / the spec). State the strongest one or two alternatives you rejected and *why* — a discarded option that was never named cannot be defended later, and is the raw material for an ADR (LOA Appendix F).
- **State the conviction honestly.** Lead with what is *Verified*, then what is *Inferred*, then what remains *Flagged*. Honesty about uncertainty outranks the appearance of competence (Rules of the Road §3).
- **State the residual risk.** What does the evidence *not* cover? Which boundary cases remain open? What is the one piece of new information that would change this conclusion? (If you can name it, you understand the conclusion's limits; if you can't, you don't yet.)
- **Re-ground.** On long tasks, restate the original request and constraints and confirm the conclusion has not drifted from or contradicted an earlier decision (BoK §VI.2; anti-pattern: The Lost Thread).

**Exit evidence.** The conclusion, the rejected alternatives with reasons, the confidence ledger summary, the residual-risk statement, and the "what would change my mind" line.

**Anti-rush rule.** *Match the answer to the evidence — no overreach.* Do not state as certain what is merely inferred, and do not hedge into uselessness what is actually verified. Calibration is the deliverable.

---

## 4. The confidence ledger (the artifact that ties it together)

The protocol's durable output is a small table the agent maintains from Stage 3 onward and hands off at the end. It is the bridge to the Proof Pack and the gate records.

| Claim | Stage it entered | Evidence | Source / version / `file:line` / run | Disconfirming test tried | Label |
|---|---|---|---|---|---|
| *e.g. "consumer is idempotent on redelivery"* | 3 | reservation is atomic `SET NX` before side effect; redelivery test green; was seen red before fix | `OrderConsumer.cs:88`; `RedeliveryTests.cs:40` | replayed ack-after-write duplicate | **Verified** |
| *e.g. "Work IQ query returns within budget"* | 3 | docs state minutes-scale; not yet measured against our budget | vendor docs (preview) | none yet | **Flagged** |

A claim that cannot fill the **Evidence**, **Disconfirming test**, and **Label** columns is not a conclusion — it is a hypothesis still in Stage 2, and it must be sent back there before it ships.

---

## 5. How the protocol maps onto the five workflow skills

Each skill is the Rigor Protocol specialized to one kind of work. The stages are constant; the personas, the dominant techniques, and the artifacts change.

| Skill | Stage-1 framing emphasis | Dominant technique at Stage 3 | Adversaries at Stage 4 | Output artifact |
|---|---|---|---|---|
| `/specify` | user/problem space, comps, scenarios | comparable & user-evidence gathering | Simplifier, Test Architect, Security (if data/identity) | spec + acceptance criteria |
| `/define-architecture` | system sketch, archetype, tier allocation | **Spike/PoC + read-the-code** for unfamiliar SDKs | Enterprise, Distributed Systems, Security architects | architecture.md + ADRs |
| `/design-slice` | component boundaries, contracts, patterns | contract due diligence + **spikes** | Patterns Expert, Simplifier, language dev | design doc |
| `/implement` | task decomposition, boundary set | TDD red→green + execution | Test Architect, language dev, SRE | code + tests + Proof Pack |
| `/investigate` | reproduce, timeline, system map | **RCA method + verify-cause-by-data** | Distributed Systems, SRE, Security | investigation report + go-forward |

These five carry a piece of work from idea to shipped code. **Four supporting skills** run the same protocol specialized to a different end: **`/collectknowledge`** (before design — Stage 3 gathers and sources the domain's state of the art; the artifact is a knowledge base, not a work product), **`/adddomainexperts`** (Stage 1 establishes the domain from repo evidence, Stage 4 attacks the proposed roster as sprawl; the artifact is a roster delta), **`/document`** (the subject is the codebase itself; Stage 3 extracts the API reference and diagrams from the source, Stage 4 verifies every diagram and example against the code), and **`/forensicreview`** (the subject is the entire existing repository; it reconstructs architecture/docs, attacks architecture/design-slice/implementation claims, and converts evidenced gaps into a prioritized backlog). All four are documented with the personas they concern in `collaborative-personas.md` §5.

---

## 6. The protocol, in one breath

> Don't answer yet. Open the whole space and sketch the system before touching a part. Interrogate each element with precise, one-at-a-time questions — never a vague "why." Establish every load-bearing claim against a cited source or an executed result; read the code and run a spike for anything unfamiliar. Then try to break your own conclusion — verify it independently, name the input that would falsify it, assert its negation and see if it survives, and for anything that can hurt, let a separate adversary attack it. Only then converge — to exactly what the evidence supports, with your confidence and your residual risk written down, and the one thing that would change your mind named out loud.

