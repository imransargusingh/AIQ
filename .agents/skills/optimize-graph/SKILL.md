---
name: optimize-graph
description: Analyse a prompt BEFORE executing it and produce an optimized execution graph — dependencies made explicit, incidental ordering removed, the critical path shortened, independent work parallelised under a bounded fan-out contract, nodes collapsed or promoted to the right granularity, every loop given a termination variant, and cost recorded against delivery. It may only increase completeness, rigor and determinism, never trade them.
runs_as: Coordinator
---

# Skill: /optimize-graph

Analyse a prompt **before it is executed** and produce an **optimized execution graph**: the work modelled as a DAG of nodes and real dependencies, with incidental ordering removed, the **critical path shortened first**, genuinely independent work run concurrently under a **bounded fan-out contract**, nodes **collapsed or promoted** to the right granularity, every loop given a **termination variant** so it cannot run away, and a **cost-vs-delivery ledger** recorded afterwards so the next plan is better.

**The hard constraint, which inverts the usual optimization framing:** this skill **may only ever increase completeness, rigor and determinism**. It is forbidden to trade them for speed. It exists because the naive execution shape — everything serial in the order the tasks were thought of, with loops that stop by running out — spends the budget badly, and spending it well **buys more verification, not less**. A plan that is faster because it checks less is not an optimized plan; it is a smaller one, and it is **rejected, not scored**.

**The objective is lexicographic, and speed is last.** Rank candidate plans **(1) completeness and rigor · (2) token cost · (3) speed**, each axis optimized only subject to those above it. Faster is genuinely the goal — it is simply the last thing traded for. A *slower* plan is acceptable **only when completeness improves AND rigor improves AND tokens fall** — a conjunction, not a choice. A plan that is slower while those three are unchanged is not a neutral trade; it is pure loss (GO4a).

**Spine:** the Rigor Protocol applied to the *plan* — Stage 1 opens the space of possible shapes, Stage 3 grounds cost claims in evidence rather than assumption, **Stage 4 disconfirms** (the Simplifier strikes boundaries that do not earn their place; the Test Architect strikes any plan that reduces what is proven). **Authority:** `knowledge/execution-graph-optimization.md` (GO1–GO19) and its evidence base `docs/knowledge/graph-and-loop-engineering/`. **Mode:** Peer Mode to build the graph, Adversary Mode to attack it. **Lead:** the **Orchestrator** (it already owns sequencing, gates and the mode-switch), with the **SRE** (profiling, resource bounds, backpressure), the **Test Architect** (hard veto — no plan may reduce what is proven) and **the Simplifier** (soft veto — no node or boundary that does not earn its place).

> **Where it sits.** `/optimize-graph` runs **in front of** other work, on the prompt itself:
> `/optimize-graph` (plan the shape) → the actual skill or work → the cost-vs-delivery ledger → `/dream` mines the ledger so future plans improve.
> It is not a replacement for any workflow skill. It decides **the order, the width and the bounds** in which that skill's own stages run.

## Grounding (first action)
Load and treat as authoritative (Rigor Stage 0; BoK §III.1): **`knowledge/execution-graph-optimization.md`** (the directives), **`docs/knowledge/graph-and-loop-engineering/`** (the evidence — span and Brent, the coupling boundary, the termination obligation, the six measured in-fleet instances C1–C6), and **the repo's own history** for cost evidence — `docs/audit/audit-log.jsonl` for what work of this shape has actually cost and where it reworked (AL10), and `docs/lessons/defect-classes.md` (CI5) so a known failure class is designed out of the plan rather than rediscovered. Traverse the graph 1–2 hops from `kb-graph-and-loop-engineering` (V15) and cite the path. **Do not assert a bottleneck the history does not support** (GO3).

## Input
The prompt to be executed, verbatim. Optionally a stated budget (steps, wall-clock, tokens, spend), a tier if already known, and a focus (`--focus latency` · `--focus cost` · `--focus determinism`). With no focus, optimize the span first, then determinism, then cost — completeness and rigor are **not** focus options because they are never traded.

## The pass

### Stage 0 — Triage. Is planning worth it? (GO16)
Consume the compiled prompt when one is in hand (CO-S0, `knowledge/agent-coordination.md`; `/compile`): its goal state is the turn's goal state and its Not-in-scope is the interdiction — derive nothing from raw prose that a compiled prompt already fixed. Answer first, because the common case is "no". **Skip planning** when the work is **1–2 nodes with no loop, no fan-out and no triggered gate** — execute directly and say the plan was skipped. In this fleet's own corpus **36% of 750 audit entries were single-step work**, so the skip path is the normal path. Planning a task smaller than its plan is its own waste. **Never skip because the prompt merely *looks* small — a one-line prompt that triggers a hard veto is not small.**

### Stage 1 — Build the naive graph (what would happen with no planning)
Enumerate the nodes the prompt implies **in the order an unplanned agent would do them**. Each node gets a **goal**, its **inputs**, its **exit condition** (how it knows it is done), and its **tier** (T0 deterministic … T3 frontier). This is the baseline the optimization is measured against — and writing it is already an intervention, because *ill-defined stopping conditions* sit in MAST's largest failure category.

### Stage 2 — Add the mandatory floor nodes (GO12)
Before optimizing anything, add every node the change *requires*, whether or not the prompt asked for it: each triggered **hard veto** (`persona-audit.md` §8.7), the **Testing Strategy** trigger-table union, the **end-to-end surface list** (E7) and reader trace (E8/DM15), **red-first** observation for any claimed control (CI6), the **audit/change** entries (AL5/CL1). These are **immovable** — the optimizer may reorder them, never remove them. **The floor is added before the optimization so it can never be a casualty of it.**

### Stage 3 — Classify the edges and delete the fake ones (GO2)
For every edge, name it a **data edge** (B consumes A's output), a **decision edge** (A's result changes B's *shape*), or **incidental ordering** (they were merely thought of in that order). **Delete every incidental edge.** This is the cheapest and usually the largest win, and it is where the fleet's measured waste lives — three defects on three independent layers discovered one at a time (C4), two independent features taken serially through the same three-phase chain (C3).

### Stage 4 — Measure the graph and attack the span (GO4, GO4a)
Compute and report **work `T₁`**, **span `T∞`** (the longest chain), and the ceiling `Tₚ ≤ (T₁ − T∞)/p + T∞`. Because `Tₚ ≥ T∞` **always**, attack the chain *before* the width — can a node be removed, collapsed, overlapped, or pulled earlier? A plan that widens a graph without examining its long chain has optimized the wrong axis. **Pull gates as early as their inputs allow**: a gate that fires early costs one node; the same gate firing late costs everything built on top of it.

Then rank the candidate plans **lexicographically — completeness and rigor, then token cost, then speed** (GO4a). A *slower* plan is acceptable **only when all three hold together**: completeness ↑ **and** rigor ↑ **and** tokens ↓. Slower while those are unchanged is pure loss. **Before widening, compare the ceiling `T₁ − T∞` against the fan-out overhead itself** — measured here, three independent gates parallelised ran **19% slower** because one was 83% of the work and spawn cost exceeded the whole available gain.

### Stage 5 — Decide concurrency (GO5–GO7)
For each candidate pair run the **independence test** (no data edge, no decision edge, no shared exclusive resource) — if any fails, keep them serial **and name which failed**. For each surviving group run the **coupling test**: parallel exploration is a **cost multiplier** (~15x tokens in the production orchestrator-worker case), justified for open-ended, breadth-first, loosely-coupled work and *wrong* for tightly-coupled or sequential work. Then give every fan-out its **five-part contract** — width cap (never unbounded; ≤4 without justification), transient-failure policy, per-branch exit condition, join rule **including what it does with a partial**, and failure containment — plus a **termination condition, a deadline and a fallback**, so the node is directly dispatchable (CO-S1). *The fleet has already paid for skipping this — five uncapped parallel calls tripped 429/529 and failed the whole panel (C1).*

### Stage 6 — Granularity and determinism (GO11, GO13–GO14)
**Collapse** nodes whose separation buys no independent verification and costs a whole context load — but **never across a gate**. **Promote** a node that carries independent risk, needs its own budget or tier, or is a verification gate hiding inside an implementation step. Then push determinism — prefer the **T0 deterministic node** wherever it would do, remove order-dependence that is not semantic, ensure **one authoritative producer per quantity** (DM7). Finally, make every verification node **declare its oracle**, the input that would make it fail. Then check the **width** of every transcription (GO14a): per **declared shared surface**, list each node's fail-clause over it and show them **jointly satisfiable** — an unscoped clause there is a plan defect even when nothing collides; and make every **scan-shaped guard** state its **root, recursion, token set and allowlist**, matching the clause it discharges. Widening goes red at the join; **narrowing stays green while the promise is violated.** *A check that cannot fail proves nothing (C3's vacuous test).*

### Stage 7 — Bound every loop (GO8–GO10)
Before dispatch, refuse a compiled prompt whose `dispatchable` is false or whose text still carries an unanswered `DR-n` line — stop with `decision request unanswered: DR-n` (CO-S0). For each cyclic node declare all four: the **variant** (what strictly decreases each iteration), the **well-founded floor**, the **exit condition**, and the **cap** as a circuit breaker only. **A loop with a cap but no variant is flagged — it is *hoped* to terminate.** State plainly that a firing cap is a **defect signal, not a resource signal**. Reject "refine until it's good" and replace it with a measurable variant and a stopping delta.

### Stage 8 — DISCONFIRM (the gate)
Adversary Mode, and the author does not clear it. **Test Architect (hard veto):** does the optimized plan prove *everything* the naive plan proved? Name any check that got weaker — if one did, the plan fails. **Simplifier (soft veto):** does every node, boundary and branch earn its place, or has decomposition created handoffs that lose more than they buy? **SRE:** is any bottleneck claim *measured* or merely assumed (GO3); does every fan-out have backpressure; is any parallelism layered on top of contention (which runs slower *and* costs more)? **Orchestrator:** is every triggered floor still present, and can the plan actually be executed as written?

### Stage 9 — CONVERGE — emit the plan
Produce the optimized graph with, for each node, its id, goal, inputs, exit condition, tier, dependencies, and (for loops and fan-outs) its contracts. Report **before vs after**: nodes, span, parallel width, deterministic share, loops bounded, floors present. **Label every cost or duration figure Verified (measured) or Inferred (modeled)** — never present a model as a measurement (NG6). State the **budget and the degradation path** (degrade to a cheaper tier or a narrower scope, or stop and report — **never silently drop a gate**), and name the **re-plan checkpoints** where a result could change the shape of what remains (GO17).

### Stage 10 — Execute, then record cost vs delivery (GO18)
Execute the plan. Then record **planned vs actual** — nodes, wall-clock, tokens where the runner exposes them, **rework passes** (repetitions the plan should have prevented), and whether the completeness and rigor floors were met. Where a plan change is validated red→green or by explicit human validation, capture a **mitigation record** (`dream.py capture-mitigation`, ADR-0003) so `/dream` can mine it. **This is the loop that makes the optimizer improvable rather than merely opinionated.**

## Output artifact
For a T1/T2 run, `docs/plans/<slug>.md` — the optimized graph with its node table (**every node carries a `Capability`: Reasoning · Independent review · Deterministic mechanics** — a node with none is not admitted, and a `Deterministic mechanics` node is *executed, not prompted*, GO19), a **Mermaid** DAG, the before/after metrics, the loop and fan-out contracts, the floor nodes marked immovable, the budget and degradation path, the re-plan checkpoints, and (after execution) the cost-vs-delivery ledger. For a T0 run the plan is a short in-response table and no file is written (GO16, CT13).

## Definition of done (exit gate)
- [ ] Triage answered — either planning was skipped with a reason, or the graph was built (GO16).
- [ ] Every node has a **goal, inputs, exit condition and tier**; every edge is classified and **incidental ordering deleted** (GO1–GO2).
- [ ] **Every triggered rigor floor is present as an immovable node** (GO12), and the optimized plan proves **everything** the naive plan proved — the Test Architect's veto cleared by someone other than the author (Stage 8).
- [ ] **Work, span and the ceiling reported**; the span attacked before the width (GO4).
- [ ] Plans ranked **lexicographically — completeness/rigor, then tokens, then speed**; any slower plan justified by all three of completeness ↑, rigor ↑, tokens ↓, and the ceiling compared against the fan-out overhead (GO4a).
- [ ] Concurrency justified by the **independence** and **coupling** tests; every fan-out carries its **five-part contract** (GO5–GO7).
- [ ] Every loop declares **variant, floor, exit condition and cap**; none relies on the cap to terminate (GO8–GO10).
- [ ] Every verification node **declares its oracle** (GO14).
- [ ] **Transcription widths checked** (GO14a): per declared shared surface the fail-clauses are **jointly satisfiable** and none unscoped; every scan-shaped guard states root, recursion, token set and allowlist, matching its clause.
- [ ] Determinism improved or unchanged, never reduced (GO11).
- [ ] Budget stated with a **degradation path that never drops a gate** (GO15); **re-plan checkpoints** named (GO17).
- [ ] Every cost/duration figure **labelled Verified or Inferred** (GO3, GO18, NG6).
- [ ] After execution, **planned vs actual recorded**, rework passes counted, and a mitigation captured where a change was validated (GO18).

## Documentation & discoverability (last action)
Per the Discoverability Mandate (V10) — if a plan file was written, give it **frontmatter** (id, title, `type: doc`, owner, tags, **typed links** — at minimum `relates-to` the artifact it plans and `depends-on` `kb-graph-and-loop-engineering` — and `review-by`), then sync the derived index with `python3 docs/ai-forward-pack/scripts/docs-graph.py derive` (never an ad-hoc script, V18). Capture any planning judgement below ADR weight as a **decision note** (V17). Ideas the planning surfaces are **captured, not chased** (CT14).

**Audit (last action).** Append an audit entry — `python3 docs/ai-forward-pack/scripts/audit-log.py append --shortname "optimize-graph-<slug>" --session "<id>" --skill optimize-graph --kind skill --prompt "<the prompt, verbatim>" --summary "<before/after — nodes, span, width, floors, loops bounded>"` (AL5).

**Handoff:** → execute the plan with the workflow skill it wraps → record the cost-vs-delivery ledger → **`/dream`** mines the ledger and the mitigations so the next plan is better.
