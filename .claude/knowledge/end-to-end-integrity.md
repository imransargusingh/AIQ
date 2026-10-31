---
load: always
---
# End-to-End Integrity — the standing method

*Normative guidance for **how a decision is taken and how a change is verified to reach everywhere it must**. It is the answer to two failure modes that no amount of unit testing catches: the decision made in a silo that is locally right and globally wrong, and the change that is correct in the file it touched and incomplete on the path it belongs to. The Rigor Protocol governs how you think; the Testing Strategy governs what counts as proof; **this document governs the scope you must think and prove across** — the whole intent, end to end.*

Normative keywords (**MUST**, **SHOULD**, **MAY**, **MUST NOT**) follow RFC 2119.

The governing idea, in the words of a pack repo's owner after four simultaneous production defects with 1,123 green tests: **"these seem to be all simple mistakes where you are not looking at the entire site end-to-end but doing pointwise page or component thinking."** Every one of those defects was locally correct. Each component passed its own tests against its own expectation, and none of them ever looked at a sibling. Pointwise correctness is not correctness; it is correctness's most convincing counterfeit.

---

## 1. The standing method (the discipline is unconditional)

**E1 — The discipline applies whether or not a skill was invoked.** The absence of the words *"use the Rigor Protocol"*, *"convene the personas"*, or *"run `/design-slice` first"* in a prompt is **not permission to skip them**. A direct request — "add this field", "fix this bug", "wire this up" — carries the same standard as a skill run; only the *ceremony* scales with the tier (Rules of the Road §0.2), never the *rigor*. Concretely, on every non-trivial interactive request the agent still: grounds in the existing artifacts (Rigor Stage 0), interrogates before converging (Stages 1–2), establishes rather than asserts (Stage 3), attacks its own conclusion (Stage 4), and converges with confidence labels and residual risk (Stage 5).

**E2 — Route by the shape of the risk, not the shape of the request.** A defect report on a running system enters at **`/investigate`** — never at `/implement`, and never as a backlog item to be picked up later. "Do the next steps" does not downgrade a defect report. A request phrased as an implementation ("add a column for X") that actually changes a domain concept is a **model** decision and enters at `/design-slice` or `/specify` (see `domain-and-data-modelling.md` DM14). Accepting the requester's framing of the *kind* of work is the first way a silo forms.

**E3 — Adversarial review is convened by the risk, not by the request.** Peer Mode authors, Adversary Mode reviews, and **the author never clears its own hard veto** — including when the whole exchange is one interactive turn and the "adversary" is the same model switching lenses deliberately. Convene by the triggers in `persona-audit.md` §8.7, not by whether the user asked for a review. A change touching identity, money, personal data, an irreversible action, a schema, or a public contract convenes its veto-holder whether or not the word "review" appeared.

---

## 2. Ground in the whole intent

**E4 — Read the whole path before deciding any point on it.** Before changing a component, establish where it sits in the end-to-end flow the user actually cares about: what produces its inputs, what consumes its outputs, what the user is trying to accomplish across the whole journey, and what else already answers the same question. A decision taken with only the local file in view is a guess about the system, and it will be locally defensible and globally wrong. In graph terms this is V15 grounding: traverse 1–2 hops of typed edges from the artifact under work — **and then keep going to the surface the user actually touches**.

**E5 — Generalize to the broader intent before committing.** Ask, in writing: *does this solution serve the stated intent, or only the stated instance?* A fix that is correct for the one case in front of you and silently insufficient for the class it belongs to is the defect this rule prevents (one pack repo watched FR-050 become FR-051 within a day *"because the sweep stopped at the instance that was on fire"*). The mechanism — **class → sweep → derive → prevent** — is specified in `continuous-improvement.md` CI2 and is mandatory on any defect fix.

**E6 — Never decide in a silo: name what your decision constrains.** Any decision that fixes a contract, a name, a shape, a unit, a rounding rule, an ordering, or a default **MUST** name the other places that must now agree with it, and either change them in the same commit or record why they are unaffected. Two implementations of one rule drift silently — a pack repo shipped two implementations of one selection rule whose `ThenBy` and `ThenByDescending` disagreed, and nothing failed. If a rule has two producers, that is the defect; **derive it once** (`domain-and-data-modelling.md` DM7).

---

## 3. Change-surface completeness

**E7 — Trace every surface a change must reach, and enumerate them before you start.** A change to a data-carrying field is not done at the layer you touched. The canonical path — the one where the count is routinely underestimated — is:

> **store → domain/model → service → *projection/wire* → client type → UI render → compute reader**

*"When a DTO gains a field, the surfaces to update are five, not four."* A pack repo shipped a field that reached the DTO, the service, the TypeScript type and the column list, and missed **the projection**; the cell renderer tested `v === 0` (false for `undefined`), called a formatter on `undefined`, and took out the entire ledger screen in production. The rule: **write the surface list down first**, then tick it off. The list differs per architecture; the discipline does not. The **structured home** for that list — and the E8 writer/compute-reader trace — is the Proof Pack's *change reach & instrumentation* section (opt-in, `templates/proof-pack.template.md`).

**E8 — Every field has a traced writer *and* a traced reader.** Classify every reference to a new or changed field as **write** / **CRUD-or-DTO passthrough** / **schema** / **compute**. A field with no *write* path is an unimplemented design. A field with no *compute* reader is dead weight that round-trip tests will cheerfully prove works. Round-trip tests catch neither; **the reader trace does**. (This is `domain-and-data-modelling.md` DM15, stated here as a change-time obligation.)

**E9 — A declared shape with no implementation is not a design, it is a claim.** A constant, facet, enum member, status value, flag or route that the design declares and nothing writes will read as working for as long as nobody looks. Two pack-repo examples: facets `Score` and `Status` declared and never written, so a score change re-recorded everything; and a background task selecting rows where `Status = Live` when nothing ever set `Live` — so the task almost never fired, and its telemetry honestly reported *"skipped: no fixture is in play"*, which was **true, and for entirely the wrong reason**. Unit tests, composition proof and telemetry all agreed with each other, and none of them questioned the premise.

**E10 — Reachability is part of "done" for any user-facing change.** A screen, panel, action or route that exists but cannot be reached from where the user actually is has not shipped. A nav link to a screen that does not exist is worse — it blanks the app. Verify the path *to* the new thing, not only the new thing.

---

## 4. Proof that crosses the seams

**E11 — Prove the surface, not just the units.** A unit test constructs its subject by hand, so it never touches the composition root and never renders a page; a health endpoint proves the *process starts*. Neither proves the thing a user touches works. A pack repo shipped a setup page stuck on *"Loading your setup progress."* for every administrator with **253 unit tests green, the deploy succeeded, and health returning 200** — because a credential field bound to a dictionary indexer that throws on an unseen key, and two HTTP client registrations silently collided. *Nothing in the repository had ever rendered a page.* Every user-facing change therefore needs at least one proof that goes **through the real composition root to the real rendered surface**.

**E12 — Prove consistency *across* surfaces, not only within each.** Component tests assert a part against *its own* expectation and never against its siblings; that is precisely how four simultaneous defects survive 1,123 green tests. The countermeasure is a **cross-surface consistency test**: one seeded scenario traced through every surface that presents or computes from it, asserting the surfaces agree *with each other* — the horizon used by the projection equals the horizon shown in the header equals the horizon the simulation ran; the total on the summary equals the sum of the rows on the ledger. This is the test class that catches the "one quantity, two homes" defect, and no amount of per-component coverage substitutes for it.

**E13 — A gate's green result is evidence the gate passed, not that its contents passed.** Verify what a gate *actually executes*. A pack repo ran 19 control suites inside one multi-line shell block; the shell propagated only the final exit code, so 18 were advisory and one had **never executed in CI at all** — discovered only when a forensic review ran the suites independently. Aggregate green is a claim about the aggregator. Check that each control runs, fails when it should, and reports its own status.

**E14 — An exit code is not a result; read the state.** A command that returns success has told you it returned success. Cloud control planes report *Succeeded* while the resource is empty; async writes are not readable immediately; a shell can mangle a query expression into an error string on stdout that the caller then parses as data; environment/context selection does not persist across processes. After any consequential command, **read back the state you intended to change** and assert on *that*.

**E15 — Read the file; never assert the shape of our own code from memory.** The Body of Knowledge forbids guessing at *external* contracts (D2) and treating the agent's own recall of current-state facts as knowledge (§VI.1). This rule turns the same discipline inward on **our own codebase**: never claim a type has a member, a method has a signature, a property lives on a class, or a helper does what its name suggests, without opening the file. Not *"I recall"*, not *"it follows the pattern"*, not *"it must, because the caller works"*. Read it, or label the claim **Inferred**. This is written down because it recurred three times in one session in a single pack repo — each time in a **design document**, where the error is cheapest to make and most expensive to keep: a design built on a property that lives on a different type; a design asserting a field would carry data that its producer hard-codes to null; a design reusing a "general" name normaliser that in fact strips domain-specific words and collapsed two distinct competitions to the same key. *This is the highest-frequency special case of the **No-Guessing Protocol** (`no-guessing-protocol.md` NG1-NG11), which supplies the general mechanism: three permitted moves when you do not know, the `assume:` marker, and the rule that an assumption not written down beforehand is a guess.*

**E16 — Mechanical inventory from a delegated agent is evidence to verify, not evidence to cite.** A sub-agent's file listing, count, or "these five documents lack frontmatter" is a *lead*. Spot-check it before it enters an artifact — one pack repo's sub-agent reported five documents missing frontmatter, and reading the first line of each showed all five had it. Delegation multiplies reach, not reliability; the citing author owns the claim.

**E17 — The record is a claim with an expiry date.** A comment, a status table, a backlog row, a README line or a design statement asserting a constraint was true when written. When you touch the thing it describes, verify it still holds and correct it in the same change. Stale status is a defect, not documentation debt — and overstating what was done is the version of it that erodes trust fastest.

---

## 5. Closing a turn

**E18 — Close with Completed / Remaining / Next.** Every response that completes work, reports status, or answers a question about the repository ends with a short table: **Completed** (what actually changed this turn), **Remaining** (what is still open), **Best next action** (the single concrete next step). No exceptions, including short answers. A turn that leaves the reader guessing what comes next has externalised its own state into someone's memory — which is exactly what the audit log, the backlog and this rule exist to prevent. (Skills that already mandate a status table satisfy this rule with that table.)

---

## 6. Self-verification checklist

- [ ] Rigor, quality guidance and adversarial review were applied **because the work warranted it**, not because the prompt named them (E1, E3).
- [ ] The work was **routed by risk shape** — a defect went to `/investigate`; a concept change went to the model (E2).
- [ ] The **whole end-to-end path** was read before the local decision, out to the surface the user touches (E4).
- [ ] The solution serves the **intent**, not just the instance; the class was swept (E5, → `continuous-improvement.md` CI2).
- [ ] Everything the decision **constrains** was named and either changed or explicitly excluded; no rule has two producers (E6).
- [ ] The **surface list** for the change was written down first and ticked off — store → model → service → projection/wire → client type → UI → compute reader (E7).
- [ ] Every new field has a **traced writer and a traced compute reader** (E8); no declared-but-unwritten shape shipped (E9); the new surface is **reachable** (E10).
- [ ] At least one proof goes through the **real composition root to the real rendered surface** (E11), and a **cross-surface consistency** check asserts the surfaces agree with each other (E12).
- [ ] Gates were verified to **execute their contents** (E13); consequential commands had their **state read back** (E14).
- [ ] Every claim about our own code was **read from the file** or labelled Inferred (E15); delegated inventory was verified (E16); touched records were re-verified (E17).
- [ ] The turn closed with **Completed / Remaining / Next** (E18).

---

## 7. References

- **`rigor-protocol.md`** — the five stages this document makes unconditional (E1) and whose Stage 0 grounding it widens to the whole path (E4).
- **`agent-body-of-knowledge.md`** — D1 correctness over completion; D2 no guessing at contracts (E15 turns it inward); §VI.1 stale recall; §VI.2 the lost thread; Part VIII *The Unverified Green*, *Scope Drift*, *Silent Assumption*.
- **`testing-strategy.md`** — D0 hygiene, D4 real-infra, and the proof discipline that E11–E13 extend to the surface and across surfaces.
- **`domain-and-data-modelling.md`** — DM7 derive-don't-store (the rule behind E6), DM15 the reader trace (E8).
- **`continuous-improvement.md`** — CI2 class/sweep/derive/prevent (the mechanism behind E5) and the defect-class register that these rules were distilled from.
- **`persona-audit.md`** §8.4/§8.7 — the veto-clears-when predicates and convene-when triggers that make E3 mechanical.
- **`solution-selection-ladder.md`** — L2, comprehension before the ladder: the same "read before you build" stance at solution-size scale.
