---
load: always
---
# The Solution-Selection Ladder

*The constructive procedure for "smallest correct" — an ordered, checkable ladder for picking the least solution that fully works, an inline marker for the deliberate simplifications it leaves behind, and a ledger that keeps "later" from becoming "never". Version 1.0.*

The pack already has the *value* — **The Simplifier** persona ("the simplest thing that is still correct"), and the **Gratuitous Dependency** and **Cargo-Cult Pattern** anti-patterns (BoK Part VIII). What it lacked was the *procedure*: an ordered algorithm the author climbs **while building**, the way the Testing Strategy trigger table is the procedure for *what to test*. This doc is that algorithm. The Simplifier catches over-engineering **adversarially, at the gate**; the ladder prevents it **constructively, before the gate**. They are the two halves of one discipline.

Normative keywords (**MUST**, **SHOULD**, **MAY**, **MUST NOT**) follow RFC 2119.

The governing idea: **minimalism is about the solution's *size*, never the reasoning's *rigor* or the correctness *floors*.** Be lazy about the solution; never about the reading, the proof, or the safety of the thing. The code ends up small because it is *necessary*, not because it is golfed — and the moment "smaller" means "flimsier", the ladder has been misread.

*Provenance: the rung ladder and the inline-marker convention are adapted from the "lazy senior dev" pattern (the ponytail skill, MIT) and hardened into the pack's discipline — composed with the Simplifier persona, the BoK anti-patterns, and the pack's correctness mandates (failure-mode / STRIDE / LINDDUN / UI / Testing Strategy), which are exactly the floors the ladder must never cut.*

---

## 1. The ladder (the forcing function)

**L1 — Climb the ladder before writing the solution.** For any unit of work that builds or changes behavior, the author **MUST** stop at the **first rung that holds**, in order:

```
1. Does this need to exist at all?      → speculative need: skip it, say so in one line (YAGNI)
2. Already in this codebase?            → reuse the helper/util/type/pattern; do not re-implement it
3. Standard library does it?            → use it
4. Native platform feature covers it?   → use it (<input type="date"> over a picker lib, a DB constraint over app code, CSS over JS)
5. An already-installed dependency?      → use it; never add a new dependency for what a few lines do
6. Can it be one line?                  → one line
7. Only then:                           → the minimum code that works
```

Two rungs hold → take the **higher** one and move on. The ladder is a reflex, not a research project — but it is a *forcing function*, not a suggestion: a design or implementation that skipped an applicable lower rung without a recorded reason is a **Simplifier finding**, exactly as a skipped Testing-Strategy directive is a Test-Architect finding.

**L2 — The ladder runs *after* comprehension, never instead of it.** It shortens the *solution*, never the *reading*. Read the task and the code the change touches, trace the real flow end to end, **then** climb. "Lazy about the solution, never about the problem" — a small diff in the wrong place is not lazy, it is a second bug. This is the pack's Stage-0 grounding (Rigor Protocol) restated for solution size: the comprehension is mandatory, the ladder only governs what you build once you understand.

**L3 — Rung 5 *is* the Gratuitous-Dependency gate.** A new dependency is new transitive surface, new supply-chain risk, and a permanent maintenance obligation (BoK Part III adopt-or-not; Security owns the supply-chain half). Adding one to do what the stdlib, the platform, or a few owned lines already do is a finding. The cheapest dependency is the one not added; the cheapest code is the code deleted.

## 2. The floors the ladder never cuts

**L4 — Lazy, not negligent.** The ladder **MUST NOT** simplify away any of: input **validation at trust boundaries**; **error handling** that prevents data loss; **security** controls; **accessibility** basics; the real-hardware **calibration** a minimal model can't see (a clock drifts, a sensor reads off); and **anything explicitly requested**. In pack terms these floors are the **failure-mode dispositions** (`/design-slice`), the **STRIDE** mitigations, the **LINDDUN** controls, the **UI state/WCAG/perf** mandates, and the **Testing Strategy** union — all of which stand at full strength regardless of how small the solution is. The merge is safe *because* both disciplines protect the same floors: the ladder only ever removes what is *gratuitous*, never what is *load-bearing*. When the user insists on the fuller version, build it — do not re-argue.

## 3. The intentional-simplification marker

**L5 — Mark every deliberate shortcut inline, with its ceiling and its trigger.** When the chosen rung is *correct but bounded* — a global lock, an O(n²) scan that is fine at the current `n`, a naive heuristic, a single-tenant assumption — the author **MUST** leave an inline marker naming **the ceiling** (the limit) and **the upgrade trigger** (the condition that says "revisit now"):

```
// simplify: global lock, ok at current write volume — go per-account if throughput becomes the bottleneck
# simplify: O(n²) match, fine for n<1k batches — index it when batch size grows
```

The token is **`simplify:`** (the pack-native marker); the harvest (L6) also recognizes **`ponytail:`** so a repo running the ponytail plugin shares one ledger. A marker with a named ceiling reads as *intent*, not ignorance, and is the **code-local, lightweight sibling** of two heavier pack records: a `/design-slice` failure-mode dispositioned **"consciously accept (rationale + residual risk)"** (BoK; Engineering Governance) and the **Deviation Protocol** (Rules of the Road §4). They are the same idea at three weights — use the marker for a code-local shortcut, a **decision note** (V17) for a session-level judgment, an **ADR** when it bears architectural load. A marker that names **no trigger** is a latent rot: it silently becomes permanent — which `scripts/marker-lint.py` now flags (`simplify-no-trigger`).

## 4. The debt ledger (so "later" ≠ "never")

**L6 — Harvest the markers; flag the triggerless ones.** The deliberate shortcuts are tracked, not left to rot. The ledger is a one-shot harvest — grep the tree (skip `node_modules`, `.git`, build output) for the marker, one row per hit, grouped by file:

```
grep -rnE '(#|//) ?(simplify|ponytail):' .
```

Each row reads `<file>:<line>, <what was simplified>. ceiling: <limit>. upgrade: <trigger>.`; any marker with **no trigger** gets a `no-trigger` tag — *those are the ones that rot*. This maps onto existing machinery rather than inventing a store: a triggerless or stale marker is a **freshness-style finding** (V13), a marker worth durable capture becomes a **decision note** (V17), and the harvest itself is recordable in the **audit log**. The harvest is now a **first-class command** — `scripts/marker-lint.py` (`--json`, `--gate`; the sibling of `design-lint.py`) classifies every marker and flags the triggerless/incomplete ones with the same warn-by-default posture; the grep above remains the zero-dependency fallback.

## 5. Tier-gated ceremony (where minimalism and rigor meet)

**L7 — Match the *output* ceremony to the tier, never the *floors* to the tier.** The pack's tiers (Rules of the Road §0.2) already dial rigor by cost-of-error; the ladder dials *solution size*, and the two compose:
- At **T0 (trivial)** the ladder *is* the procedure: build the least thing that works, leave **one runnable check** behind for any non-trivial logic (an `assert`-based self-check or one small test — YAGNI applies to tests too; a one-liner needs none), and keep the prose minimal — **code first, then at most a couple of lines**: what was skipped, when to add it. At T0 the full Proof-Pack / status-table / artifact ceremony is *not* required (running it on a one-line change is its own over-engineering — *Coverage Theater*, a BoK anti-pattern).
- At **T1/T2** the full discipline stands: the design doc, the Proof Pack, the failure-mode/STRIDE/LINDDUN analyses, the audit/change entries — these durable artifacts **are the product** at high tiers and are never thinned (the anti-prose rule below does **not** touch them).

**L8 — The anti-prose rule (T0 and code-local only).** If a *code-local* explanation is longer than the code it defends, delete the explanation — every paragraph defending a simplification is complexity smuggled back as prose. This governs inline/PR prose at low tiers; it **MUST NOT** be used to thin a required T1/T2 artifact, an explanation the user asked for, or any of the L4 floors.

## 6. The Simplifier's adversarial procedure

**L9 — At the gate, the Simplifier emits a delete-list, not an essay.** The constructive ladder has an adversarial mirror: in Adversary Mode the Simplifier reviews the diff/design-slice as a tagged, one-line-per-finding delete-list, ending with the only metric that matters — `net: -<N> lines possible.` (or `Lean already. Ship.`). The tags:
- **`delete:`** dead code, unused flexibility, a speculative feature → replaced by nothing.
- **`stdlib:`** a hand-rolled thing the standard library ships → name the function.
- **`native:`** a dependency or code doing what the platform already does → name the feature.
- **`yagni:`** an abstraction with one implementation, a config nobody sets, a layer with one caller → inline it until a second caller exists.
- **`shrink:`** the same logic in fewer lines → show the shorter form.

A single smoke test or `assert`-based self-check is the *minimum*, never bloat — do not flag it. This is the Simplifier's required output when convened on a build (it composes with, and does not replace, the persona's verdict contract in `persona-cards.md`).

---

## 7. Self-verification checklist

- [ ] The ladder was climbed **after** comprehension; the first holding rung was taken; any skipped lower rung has a recorded reason (L1–L2).
- [ ] No new dependency for what stdlib / platform / a few owned lines do (L3).
- [ ] No L4 floor was simplified away — validation, data-loss handling, security, accessibility, calibration, explicit requests all intact (L4).
- [ ] Every deliberate bounded shortcut carries an inline `simplify:` marker naming its **ceiling and trigger**; none is triggerless (L5).
- [ ] The shortcuts are harvestable into the ledger; triggerless ones flagged; durable ones promoted to a decision note / ADR (L6).
- [ ] Ceremony matched the tier: T0 = code-first + one runnable check; T1/T2 = full artifacts, never thinned (L7–L8).
- [ ] At the gate, the Simplifier emitted a tagged delete-list with `net: -N` (L9).

## 8. References

- **The Simplifier** — `agent-persona-catalog.md` §10, `persona-cards.md`, `persona-audit.md` §8 (the adversarial mirror of this constructive procedure).
- **BoK Part VIII** — the *Gratuitous Dependency*, *Cargo-Cult Pattern*, and *Coverage Theater* anti-patterns this doc prevents; **Part III** adopt-or-not (rung 5).
- **Rigor Protocol** Stage 0 (comprehension before the ladder, L2); **Rules of the Road** §0.2 (tiers, L7) and §4 (the Deviation Protocol, the heavyweight sibling of the marker).
- **Knowledge Visualization** V13 (freshness — the rot-flag), V17 (decision notes — durable shortcut capture), V18 (the script-bundle home for a future harvest command).
- **Testing Strategy** — the floor the T0 "one runnable check" scales down to and the T1/T2 union scales up to.
- **ponytail** (MIT, DietrichGebert/ponytail) — the source of the rung ladder, the inline marker, and the debt-ledger idea, adapted here.
