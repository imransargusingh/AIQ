---
applyTo: "**"
---
# Execution-Graph Optimization

*Normative guidance for **planning the shape of the work before doing it** — modelling a prompt as a dependency graph, shortening its critical path, running independent work concurrently under a bounded contract, collapsing and promoting nodes to the right granularity, proving that every loop terminates, and recording cost against delivery so the next plan is better. The Rigor Protocol governs how you reason; the Solution-Selection Ladder governs how big the solution may be; **this governs the order, width, and boundedness of the execution itself.** The evidence base is `docs/knowledge/graph-and-loop-engineering/`; the skill that applies it is `/optimize-graph`.*

Normative keywords (**MUST**, **SHOULD**, **MAY**, **MUST NOT**) follow RFC 2119.

The governing idea, and the constraint that makes this document unusual: **optimization here may only ever increase completeness, rigor, and determinism.** Every other optimization discipline trades quality for speed at some exchange rate. This one is forbidden to. It exists because the naive execution shape — everything serial, in the order the tasks were thought of, with loops that stop by running out — spends the budget badly, and spending it *well* buys more verification, not less. **If a plan is faster because it checks less, it is not an optimized plan; it is a smaller one, and it is rejected.**

The measured warrant that this is achievable rather than aspirational: LLMCompiler's DAG-planned parallel execution delivered up to **3.7x latency and 6.7x cost improvement while accuracy went *up* ~9%** — the reordering itself improved the answer (`kb-graph-and-loop-engineering` finding #2).

---

## 0. When this applies, and who owns it

**Applies** to any non-trivial prompt before execution — that is, any prompt whose work is **more than two steps**, contains a **loop**, contains a **fan-out**, or triggers a **rigor gate**. It is *triggered* in the Testing-Strategy sense: a triggered-but-unmet directive is a gap the gate catches.

**Does not apply** below that threshold. A one-node prompt is executed, not planned — planning a task smaller than its plan is its own waste (GO16). In this fleet's own corpus, **36% of 750 audit entries were single-step work**, so the skip path is the common path, not an edge case.

**Owner:** the **Orchestrator** (which already owns sequencing, gates, and the mode-switch) with the **SRE & Systems Diagnostician** (profiling, resource bounds, backpressure), the **Test Architect** (hard veto: no plan may reduce what is proven), and **the Simplifier** (soft veto: no node, branch, or boundary that does not earn its place).

---

## 1. Prime directives

1. **The objective is lexicographic, and speed is last.** Rank every candidate plan in this strict order — **(1) completeness and rigor · (2) token cost · (3) speed**. Each axis is optimized only *subject to* every axis above it. Trade speed for tokens; trade tokens for completeness and rigor; **never trade completeness or rigor for anything** (GO4a).
2. **Optimization is a completeness amplifier, never a trade.** The rigor floors are **inputs** to the plan, not variables in it. A plan that meets its budget by dropping a gate is rejected, not scored (GO12).
3. **Optimize the span before the width.** No amount of parallelism can beat the critical path. Ask "can this chain be shorter?" before "what can run at once?" (GO4).
4. **A loop without a variant is not bounded — it is hoped to terminate.** A cap is a circuit breaker, not a termination argument (GO8).
5. **Independence is proven, not assumed; and "may" is not "should."** Concurrency needs the independence test; paying for it needs the coupling test (GO5–GO6).
6. **Profile before you reshape.** A plan that optimizes an unmeasured bottleneck is the Hunch Optimization anti-pattern with a bigger blast radius (GO3).
7. **Determinism is a first-class target.** A more reproducible plan is a better plan at equal cost (GO11).

---

## 2. Build the graph

**GO1 — Make the execution graph explicit before executing.** Enumerate the **nodes** (each with a goal, its inputs, its **exit condition**, and its capability **tier**) and the **edges** (each a *real* dependency). Nothing else in this document is possible without it — and the act itself is a correctness intervention, because MAST's largest failure category across 200+ traces of 7 frameworks is **specification**, including *ill-defined stopping conditions* (evidence base finding #5).

**GO2 — Classify every edge, and delete the ones that are not real.** An edge is legitimate only if it is a **data edge** (B consumes A's output) or a **decision edge** (A's result changes B's *shape*). Anything else is **incidental ordering** — two tasks in sequence because they were thought of in that order — and it **MUST** be removed. This is the cheapest win available and usually the largest.

**GO3 — Profile before reshaping.** Where cost data exists (prior audit entries, measured runs, known tool latencies), use it. Where it does not, **say so and label the estimate Inferred** rather than asserting a bottleneck. Never reshape around an assumed hot spot: the pack's own measured lesson is that a suite everyone blamed on a migration chain was **73% app-boot time** (CE1), and that parallelism layered on contention runs *slower* while costing more (CE3/CE16).

---

## 3. Shorten the span, then widen the graph

**GO4 — Compute and report work, span, and the ceiling.** State `T₁` (total node cost), `T∞` (the longest dependency chain), and the resulting bound `Tₚ ≤ (T₁ − T∞)/p + T∞`. Because `Tₚ ≥ T∞` **always**, the plan **MUST** state what parallelism can and cannot buy it. A plan reporting a wide graph with an unexamined long chain has optimized the wrong axis.

**GO4a — The objective function, and when a slower plan is acceptable.** Score every candidate plan on four axes and rank them **lexicographically**:

| Rank | Axis | Rule |
|---|---|---|
| **1** | **Completeness** and **rigor** | **Never decrease** — a decrease is a rejected plan, not a low-scoring one. Improving them is the highest-value outcome available and outranks every other consideration. |
| **2** | **Token cost** | Reduce it — but never at the expense of rank 1. |
| **3** | **Speed** | Faster is the ideal. But it is the **lowest-priority** axis and is never bought at the expense of rank 1 or rank 2. |

**The acceptance rule for a slower plan.** A plan that is *slower* than the alternative is acceptable **only when all three of these hold together**: completeness improves **and** rigor improves **and** token cost falls. This is a **conjunction, not a choice** — if any one of the three fails to improve, the slower plan is **rejected**, because a slowdown that is not fully paid for on every higher-ranked axis is simply waste.

**The corollary that catches the common error:** a change that is *slower* and leaves completeness, rigor and tokens **unchanged** is not a neutral trade — **it is pure loss**, and independence alone never justifies it. *Measured in this repo:* three genuinely independent verification gates were parallelised and the wave ran **19% slower** (0.84×) while completeness, rigor and tokens were all unchanged — because one gate was 83% of the work (ceiling only 1.20×) and fan-out overhead exceeded the entire available gain. Independence was necessary and **not sufficient**. Before widening a graph, **compare the ceiling `T₁ − T∞` against the cost of the fan-out itself**; where the overhead is the larger number, keep it serial and say so.

Among all plans that clear the floors and this rule, **prefer the fastest** — speed is genuinely the goal, it is simply the last thing traded for.

**GO5 — Independence test (may these run concurrently?).** All three **MUST** hold: **(a)** no data edge between them; **(b)** no decision edge — neither's result changes the other's shape; **(c)** no shared exclusive resource (same file to write, same lock, same rate-limited provider beyond its budget). If any fails, they stay serial **and the plan names which one failed**.

**GO6 — Coupling test (should they, given they may?).** Parallelism is a **cost multiplier**, not a saving, when branches are separate explorations: the production orchestrator-worker shape reports roughly **15x token usage**, justified only for **open-ended, breadth-first, loosely-coupled** work. For tightly-coupled or sequential work, a single coherent worker is both cheaper and more reliable. **Pay the multiplier deliberately or not at all.**

**GO7 — Every fan-out carries a contract.** A parallel node **MUST** declare all five:

| Field | Requirement |
|---|---|
| **Width cap** | a number. **Never unbounded.** Default ≤ 4 without explicit justification |
| **Transient-failure policy** | which errors are transient (429/529/timeout) and the retry-with-backoff rule |
| **Per-branch exit condition** | how one branch knows it is done |
| **Join rule** | all-must-succeed / quorum / best-effort-with-report — **and what the join does with a partial** |
| **Failure containment** | whether one branch failing fails the node (usually it must not) |
| **Per-branch budget** | a **tool-call and token budget** each branch carries in its own prompt (`max_tool_calls`, and a token or wall-clock bound where the runner exposes one). A branch that reaches it **stops and reports what it has** — the budget firing is a finding for the join, never a retry with a bigger number (GO9) |
| **Convergence condition** | the evidence at which a branch is *done gathering* and must converge, stated as a predicate ("three cited sources per claim", "every file in the list classified"), not as effort. Without it a research branch ends only when the parent interrupts it |

This is not theoretical. In this fleet, **five parallel model calls with no cap and no retry tripped 429/529 and failed the entire panel**; the fix was a concurrency cap of 2 plus retry-with-backoff (evidence base C1). A fan-out without this contract is an incident with a countdown. The last two rows come from a second measured shape: a research sub-agent ran **123 tool calls and 3.0M tokens** on a proposal iteration, and the parent had to send *"converge now — stop further investigation"* twice — the branch had no budget and no convergence predicate, so its only exit was the parent noticing (`session-profile.py` finding SP-07; class **CTX-F**). Record each branch's actual calls against its budget in the audit entry: `audit-log.py append ... --agent-run '<agent>|<start-iso>|<end-iso>|<calls>/<budget>'` (GO18). `audit-log.py selfcheck` then reports a delegation with **no** budget as a gap - an unbounded branch looks identical to a well-behaved one - and an over-run as a finding. It is a **record, not an enforcement**: no harness mediates a sub-agent's tool count, and a control that cannot stop the call must not be labelled as though it can.

---

## 4. Loop engineering — terminate by construction

**GO8 — Every cyclic node declares a variant, a floor, an exit condition, and a cap.** The **variant** (ranking function) is a quantity that **strictly decreases every iteration** over a **well-founded order** (no infinite descending chain). This is the only actual termination *proof* — total correctness is partial correctness **plus** termination, and they are separate obligations (Floyd/Hoare). Workable agent variants: items remaining in a bounded worklist; unresolved findings that must decrease each pass; residual error decreasing by a stated minimum delta; a lexicographic tuple for nested loops.

**GO9 — A cap is a circuit breaker, and its firing is a defect signal.** Caps are mandatory *in addition to* the variant, never instead of it. Frameworks agree with the theory here: LangGraph's `recursion_limit` (default **25**) exists to catch a bug, and its own documentation says never to rely on it for termination and that **hitting it at exactly the default means a missing or broken exit condition**. So when a cap fires, the response is to **investigate the variant, not raise the cap**.

**GO10 — "Refine until it's good" is rejected, not scored.** A loop with no variant, no floor, and no exit predicate is the *Unbounded Reflection Loop* (LOA Appendix C) and the *Unbudgeted Loop*. Replace it with a measurable variant and a stated stopping delta, or make it a fixed-count node.

---

## 5. Granularity and determinism

**GO11 — Prefer the deterministic node, and fix incidental order.** Where a deterministic (T0) node and a model node would both do, choose deterministic (LOA P1/P2): it is cheaper, reproducible, and reviewable. Eliminate order-dependence that is not semantic (the pack's own **PACK-I**: an unsorted directory walk made a generated artifact differ across platforms). Ensure **one authoritative producer per quantity** (DM7). **A more deterministic plan is a better plan even at identical cost.**

**GO12 — Rigor floors are immovable nodes.** The optimizer **MAY reorder** them; it **MUST NOT remove** them. The floor for a change includes: every triggered **hard veto** (`persona-audit.md` §8.4/§8.7), the **Testing Strategy** trigger-table union, the **end-to-end surface list** (E7) and reader trace (E8/DM15), **red-first** observation of any claimed control (CI6), the **audit/change** entries (AL5/CL1), and the **no-guessing** moves (NG1). Reordering a gate earlier is usually the *best* available optimization — a gate that fires early costs one node; the same gate firing late costs everything built on top of it.

**GO13 — Collapse and promote are both first-class.** **Collapse** two nodes when they share context, the boundary buys no independent verification, and neither is a gate — because per-node overhead in agent work is a whole context load, which pushes the optimum coarser than intuition suggests, and because every boundary is a place to lose information (MAST's inter-agent misalignment exists *only* because work was split). **Promote** a node out when it carries independent risk, needs its own budget or tier, is a verification gate hiding inside an implementation step, or is the reusable part of several nodes. **Never collapse across a gate.**

**GO14 — Every verification node declares its oracle.** A gate must state *what input would make it fail*. A check that cannot fail proves nothing: this fleet shipped a test that "stayed green with contributions zeroed because a 5 percent return makes the median rise on its own" (evidence base C3) — a node that passed while proving nothing. Oracle-less verification is Coverage Theater in graph form.

**GO14a — Transcription is a width-changing step; check the width at every hop.** A ruling becomes a plan clause and a clause becomes a guard, and **nothing checks that the scope survived the hop**. The two directions fail differently: a clause written **wider** than its ruling goes **red at the join** and announces itself; a guard written **narrower** than the clause above it stays **green while the promise it documents is violated**. So **(a)** for every surface a plan names as **shared** between nodes, list each node's fail-clause over it and show them **jointly satisfiable** — an unscoped clause (no *“in this node”* / *“on this path”*) over a declared shared surface is a defect in the plan whether or not it happens to collide; and **(b)** every **scan-shaped guard** — any check that enumerates files and matches tokens — **MUST** state its **scanned root**, whether it **recurses**, its **token set** and its **allowlist** in its doc comment, and the clause it discharges **MUST** carry the same qualifier. The mismatch is **mechanically checkable**: one sentence read against one enumeration call. Give the guard a **named allowlist constant** so a later node that legitimately needs the token adds an entry **citing its ruling** — *a guard that must be deleted to make progress is one people delete.* *Measured in a pack repo, both directions inside an hour:* a clause wider than its ruling became a **repo-wide** scan of a **shared** project file for a token a sibling node was authorised to add (green in both worktrees, red at the join, matching the sibling's explanatory *comment* rather than its code); and a guard whose doc comment quoted its clause verbatim — *“anything writes a run log **anywhere**”* — scanned one directory, non-recursively, for one token (class **DC-118**). Separate worktrees never hold each other's change, so (a) is not evaluable before the join: plan-time reading is the earliest detection, not a test run.

---

## 6. Bounds, revision, and learning

**GO15 — State the budget and the degradation path.** The plan carries a bound (steps, wall-clock, tokens, or spend as available) and says what happens when it is reached: **degrade gracefully to a cheaper tier or a narrower scope, or stop and report — never silently drop a gate** (LOA 2.4, 6.5; GO12).

**GO16 — Do not plan what is smaller than its plan.** Skip when the graph is **1–2 nodes with no loop and no gate**: execute directly and say the plan was skipped. Planning cost is real.

**GO17 — Plan to the next checkpoint; re-plan when the shape changes.** A frozen DAG is wrong for exploratory work, whose dependencies are discovered by doing it. Adopt plan → execute → observe → **replan** (LOA 1.3 + Archetype H), and **re-plan whenever a result changes the shape of the remaining work.** Re-planning is cheap; executing the wrong graph is not.

**GO18 — Record cost against delivery, and feed it back.** After execution, record **planned vs actual**: nodes, wall-clock, tokens where the runner exposes them, **rework passes** (repetitions the plan should have prevented), and whether the completeness and rigor floors were met. Where a plan change is validated red→green or by explicit human validation, capture it as a **mitigation record** (ADR-0003) so it is minable by `/dream`. **This is what makes the optimizer improvable rather than merely opinionated** — and it is subject to the same honesty rule as everything else: a *modeled* estimate is labelled Inferred, never reported as a measurement (NG6).

---

**GO19 — Allocate the model tier per PHASE, and re-allocate at every phase boundary.** A model choice made once at the top of a session is a choice made for the phase you were in *then*. Phases differ in the only two things that decide the tier: **novelty** (how much genuine reasoning a call needs) and **iteration count** (how many times the prefix will be re-read). Design, architecture and adversarial review are high-novelty and low-iteration — buy the expensive tier. Deployment, debugging, build-fix and test-fix loops are the inverse: low-novelty, very high iteration, and every turn re-reads the whole prefix. Putting the premium tier there multiplies the least valuable token by the largest number.

- **The tell:** a session whose most expensive model ran its highest call count in its most mechanical phase. In the profiled session `claude-opus-5` was switched on immediately *before* deployment and took 211 main-thread calls and 76.6% of spend for the most tool-heavy, highest-iteration stretch of the work.
- **Re-allocate at the boundary**, explicitly, and say which way and why: *"design done, dropping to the cheap tier for the deploy loop."* The boundary is the decision point; drifting through it is how the wrong tier gets paid for.
- **Tier and prefix multiply.** A large static prefix is a per-call tax, so it costs the most exactly where calls are most numerous. Cutting the prefix (`context-budget.py`) and dropping the tier are the *same* saving applied twice; neither substitutes for the other.
- **Declare a capability per NODE, not only a tier per phase.** A phase-level tier still buys a model for work that needs none. Every node in the graph carries one of three: **Reasoning** (genuine novelty — design, adversarial review, diagnosis), **Independent review** (a second party, which may be a person or a differently-scoped agent), or **Deterministic mechanics** (a script with a reviewer — regenerate, commit, push, merge, re-run the gate set). *Cheap tier* and *no tier* are different answers, and for closing work the second is usually available: a `Deterministic mechanics` node should be **executed, not prompted**. Measured: five closing turns in one session cost **16,765 AIU, 17% of the whole session**, and `/updatepack` ran twice on the same repo for **1,179 AIU and 8,890 AIU** — 7.5× for identical work (`session-profile.py` SP-22, class **CTX-P**). Phase-level allocation cannot reach these, because the boundary is *inside* the turn. A node with no declared capability is not admitted.
- **Never trade the floors for the tier.** Dropping to a cheaper model is a cost decision, never a rigor decision: the gates, the Testing-Strategy union and the hard vetoes are unchanged. If a phase genuinely needs the expensive tier to stay correct, that is the answer — say so and pay it (GO15's degradation path never drops a gate).

## 7. Self-verification checklist

- [ ] Graph made explicit: nodes with goal, inputs, **exit condition**, tier; edges classified (GO1–GO2).
- [ ] **Incidental ordering removed** — every remaining edge is a data or decision dependency (GO2).
- [ ] Bottleneck **measured, or the estimate labelled Inferred** (GO3).
- [ ] **Work, span, and the ceiling reported**; the span examined before the width (GO4).
- [ ] **Plans ranked lexicographically — completeness/rigor, then tokens, then speed** (GO4a).
- [ ] Any **slower** plan is justified by completeness ↑ **and** rigor ↑ **and** tokens ↓ — all three, or it is rejected (GO4a).
- [ ] Before widening, the **ceiling `T₁ − T∞` was compared against the fan-out overhead** (GO4a).
- [ ] Concurrency justified by the **independence test**, and paid for by the **coupling test** (GO5–GO6).
- [ ] Every fan-out carries **width cap, transient policy, per-branch exit, join rule, containment** (GO7).
- [ ] Every loop declares **variant, well-founded floor, exit condition, cap** — and no loop relies on the cap (GO8–GO10).
- [ ] Deterministic tier preferred; incidental order eliminated; one producer per quantity (GO11).
- [ ] **Every triggered rigor floor is present as an immovable node**; gates pulled as early as their inputs allow (GO12).
- [ ] Collapse/promote applied deliberately; **no collapse across a gate** (GO13).
- [ ] Every verification node **declares its oracle** (GO14).
- [ ] **Per declared shared surface**, each node's fail-clause over it is listed and shown **jointly satisfiable**; none is unscoped (GO14a).
- [ ] Every **scan-shaped guard** states its **root, recursion, token set and allowlist**, and the clause it discharges carries the **same qualifier** (GO14a).
- [ ] Budget stated with a **graceful degradation path that never drops a gate** (GO15).
- [ ] Planning **skipped** where the work was smaller than the plan (GO16).
- [ ] **Re-plan checkpoints** named where a result could change the shape (GO17).
- [ ] **Planned vs actual recorded**, rework passes counted, estimates labelled (GO18).
- [ ] **Model tier allocated per phase**, and re-allocated at each boundary — the premium tier is not left running through a high-iteration deploy or debug loop (GO19).

---

## 8. References

- **Evidence base:** `docs/knowledge/graph-and-loop-engineering/` — the sourced findings behind every directive here (work/span and Brent; LLMCompiler's measured 3.7x/6.7x/+9%; the orchestrator-worker 15x/+90.2% and its coupling boundary; MAST's 14 failure modes; Floyd/Hoare termination; the six in-fleet instances C1–C6). **Cite it rather than re-deriving it.**
- **`layered-optimized-architecture.md`** — Patterns **1.2** Cascade, **1.3** Plan/Execute Split, **1.5** Hierarchical Decomposition, **1.6** Speculative Execution, **2.4** Token Budget Throttle, **6.5** Graceful Degradation; Archetype **H**; anti-patterns *The Unbudgeted Loop* and *The Unbounded Reflection Loop*; principles **P1/P2** (cheapest sufficient tier, determinism at the floor).
- **`ci-and-test-efficiency.md`** — **CE1–CE3** profile-first and contention; **CE16** remove contention rather than out-core it; **CE22** a coverage-for-speed trade is a recorded deviation. This document is its planning-time sibling.
- **`solution-selection-ladder.md`** — smallest correct; **L4** the floors that are never simplified away; **L7** tier-gated ceremony.
- **`end-to-end-integrity.md`** — **E7** the change-surface list (an immovable node set), **E11/E13** prove the surface and verify a gate runs its contents.
- **`continuous-improvement.md`** — **CI2** class→sweep→derive→prevent, **CI6** the control ladder (GO18 feeds it).
- **`communication-and-task-discipline.md`** — **CT14–CT18**: ideas surfaced by planning are captured, not chased; proportionality never reaches the floors.
- **`no-guessing-protocol.md`** — **NG6** a modeled estimate is Inferred, never Verified (GO3, GO18).
