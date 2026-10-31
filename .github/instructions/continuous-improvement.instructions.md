---
applyTo: "**"
---
# Continuous Improvement — the defect-class discipline

*Normative guidance for the agent's standing obligation to **get better**: every bug it creates, every assumption it gets wrong, and every correction it receives is captured, generalised to a **class**, and converted into a control that makes the class unable to recur. The Rigor Protocol governs how you reason on the way in; `end-to-end-integrity.md` governs the scope you reason across; **this document governs what happens after you are wrong** — which is the only part of the loop that compounds.*

Normative keywords (**MUST**, **SHOULD**, **MAY**, **MUST NOT**) follow RFC 2119.

The governing idea: **a lesson recorded as prose is a memoir.** Writing "we should remember to check the projection layer" into a plan document changes nothing, because the next session will not read that plan at the moment it matters. A lesson only becomes improvement when it lands somewhere that **fires at the moment of the mistake** — a test, a gate, a lint rule, or a file that is always loaded into the instruction path. Everything below exists to force that conversion.

This is a **primary directive**, not a retrospective ritual. It ranks with correctness: an agent that solves the problem and learns nothing has done half the job, because it has guaranteed it will pay for the same mistake again.

---

## 1. Prime directives

1. **Every defect and every mistaken assumption is captured.** Including — especially — the ones the agent itself introduced, and the ones a human caught before they shipped. A correction received is the cheapest possible lesson; wasting it is the most expensive possible choice.
2. **Capture the class, not the instance.** "I forgot the projection for `OtherIncome`" is an anecdote. "A field can reach the DTO and miss the wire, and round-trip tests do not catch it" is a class, and classes are what controls can be built against.
3. **A lesson must become a control.** Prose is a memoir; a test, a gate, a lint rule, or an always-loaded instruction is a control. Every captured class carries a named control, or a written reason why one is not yet possible.
4. **Recurrence is the metric.** The register is working when the same class does not appear twice. A second occurrence of a known class is itself a finding — it means the control was wrong, not that the agent was careless.
5. **Be honest about the mistake, including in the record.** Correct overstatements in committed artifacts as their own change. An improvement loop built on a flattering account of what happened improves nothing.

---

## 2. The mechanism: class → sweep → derive → prevent

**CI1 — Capture on every defect, correction, or falsified assumption.** The trigger is any of: a bug found in running software; a test that failed for a reason the design did not anticipate; a human correction of the agent's output or reasoning; an assumption labelled Verified that turned out false; a review veto that found something real; a "that's not what I meant" moment. Each **MUST** produce a register entry (§3) as part of the same change that fixes it — not a follow-up task.

**CI2 — Answer four questions in writing, in order.** This is the mechanism; it is mandatory on any defect fix and **SHOULD** be run on any correction.

| Step | The question | What a good answer looks like |
|---|---|---|
| **Class** | What *class* of problem does this instance belong to? | A shape, stated without the specifics: "a persisted field with no compute reader"; "two producers of one rule"; "a gate whose contents don't run". If you can only describe the instance, you have not found the class yet. |
| **Sweep** | Where else does that shape already exist? | A search of the codebase for the *shape*, with the results listed — including the ones that turned out fine. A sweep that finds nothing is a result; a sweep that was not done is a gap. |
| **Derive** | Can this read from a single source of truth instead of being duplicated? | The fix that removes the possibility, not the fix that corrects the value. Prefer deriving over synchronising (`domain-and-data-modelling.md` DM7). |
| **Prevent** | What control fails when the *shape* recurs? | A named test, gate, lint rule, analyzer, schema constraint, or always-loaded instruction — and the demonstration that it **fails on the un-fixed code** (red observed). A control never seen to fail is not a control. |

> A fix that stops at the instance is not finished. One pack repo watched **FR-050 become FR-051 within a day** *"because the sweep stopped at the instance that was on fire."*

**CI3 — Sweep the class, then fix the class.** Where the sweep finds siblings, fix them in the same change if they are small and safe, or register them explicitly as known instances with an owner if they are not. Silently leaving a discovered sibling unfixed and unrecorded is the worst of the three options.

---

## 3. The defect-class register

**CI4 — Maintain `docs/lessons/defect-classes.md` as a committed, graph-indexed artifact.** It carries V2 frontmatter (`type: doc`, an owner, a `review-by`, typed links) so it appears in the Docs Explorer and cannot rot invisibly. One entry per class — **not** one per bug. A new occurrence of an existing class appends to that class's *Instances* list and triggers a control review; it does not create a new entry.

**Entry schema:**

```markdown
### <ID> — <the class, stated as a shape>
- **Signature:** how you recognise it in the wild — the smell, the code shape, the symptom.
- **Why it survives:** which controls it passes. (This is the field that makes the class useful:
  every durable class survives something, and naming what it survives is how you build the control.)
- **Instances:** <date/ref> — one line each. Newest first.
- **Control:** the named test / gate / lint / instruction that now fails when the shape recurs,
  with its location. Or: `NONE YET — <why, and what would be needed>`.
- **Status:** `controlled` | `partially-controlled` | `uncontrolled`
```

**CI5 — Read the register at grounding.** Every skill's grounding step (Rigor Stage 0) **SHOULD** glance at the register — at minimum the classes tagged to the area under work — so known classes are designed out rather than rediscovered. This is what turns the register from an archive into an input. Pair it with the audit-log history read (`audit-and-change-log.md` AL10): the audit log says *what was done*, the register says *what goes wrong here*.

---

## 4. Where a lesson must land

**CI6 — Climb the control ladder; take the highest rung that actually holds.** A lesson's home is chosen by where it will fire, not by where it is easiest to write.

```
1. Make it impossible      → type, schema constraint, permission, API shape (best: the shape can't be expressed)
2. Automated control       → test, analyzer/lint rule, CI gate, script check (fails on recurrence)
3. Always-loaded instruction → a repo instruction file the agent loads every session
4. Knowledge doc / standard → a directive with a self-verification checklist
5. Register entry only     → `uncontrolled`, with the reason written down
```

Rungs 1–2 are controls; rungs 3–5 are memory. **Prefer a control.** A pack repo's clearest single commit on this reads *"convert ten recorded lessons into two gates and one loaded file"* — ten prose lessons became two enforced gates and one always-loaded instruction. That commit is the pattern.

**CI7 — Rung 3 means the instruction path, not a plans folder.** A lesson that must be *remembered* rather than *checked* belongs in a file the agent loads every session — a repo-local operational-gotchas or standing-method file referenced from `CLAUDE.md` / `AGENTS.md` (or `.github/instructions/`), **not** buried in `docs/plans/`. A lesson filed where nobody reads it at the moment of action is indistinguishable from a lesson not learned.

**CI8 — Repo-local first, pack upstream when it generalises.** A lesson specific to this codebase or environment stays local. A lesson that would have helped *any* project running this pack is a **pack change**: raise it through `/extendaibundle`, add or amend the knowledge doc, and bump the INSTALL revision so every installed repo inherits it. The classes seeded in §6 arrived exactly this way — two independent repos hitting the same shapes is the signal that a class is general.

---

## 5. Adjacent obligations

**CI9 — Harvest the `simplify:` and `assume:` markers when you investigate.** Every deliberate bounded shortcut carries an inline `simplify:` marker naming its ceiling and upgrade trigger (`solution-selection-ladder.md` L5–L6). When investigating a defect in a subsystem, **grep its markers first** — a pack repo found that not one marker was harvested in the session that produced four defects, *in a subsystem carrying a marker that described one of those defects exactly*. The marker had already predicted the bug; nobody looked. A triggered marker found during investigation is a register entry. The same applies to **`assume:`** markers (`no-guessing-protocol.md` NG4/NG9): an assumption that has come true is the bug, already written down and unread.

**CI10 — Every skill run leaves an audit entry; every defect leaves a register entry.** These are different records with different jobs: the **audit log** (`audit-and-change-log.md`) records *what happened*, the **change log** records *what was decided*, and the **defect-class register** records *what goes wrong here and what now stops it*. An investigation that produces a fix but no register entry has repaired the instance and left the class live.

**CI11 — Correct the record honestly.** When a committed artifact overstates what was done, understates a risk, or asserts a cause that later proved wrong, correct it in its own change with the correction visible. Keep the corrected reasoning where it is instructive rather than editing it away — a pack repo deliberately kept a wrong aggregate design visible in its design doc *"because it is the exact mistake this repository's model-first directive is meant to catch."* The visible correction is the teaching.

**CI12 — Improvement is reported, not assumed.** When work closes, the summary states what class (if any) was learned and what control now holds it. "Fixed" is a claim about an instance; "fixed, class registered, control added and observed failing on the old code" is a claim about the future.

---

## 6. Seed register — classes already established

A new repo adopting this pack **SHOULD** seed `docs/lessons/defect-classes.md` with these. Each was observed in production across two independent codebases running this pack; each is `uncontrolled` in a fresh repo until that repo builds the control.

| ID | Class | Signature | Why it survives | Control to build |
|---|---|---|---|---|
| **DM-A** | **One quantity, two homes** | A value is produced in two places; a new call site wires to whichever it found first | Both implementations pass their own tests; nothing compares them | Derive once (DM7); a cross-surface consistency test asserting the surfaces agree |
| **DM-B** | **Stored fact that nothing writes** | A persisted flag/field beside a function computing the same thing; the stored copy is maintained by nothing | Reads succeed and return a plausible default | Reader/writer trace (DM15); a test asserting the stored value equals its derivation |
| **DM-C** | **Persisted field with no compute reader** | Stored, entered on a real screen, round-tripped, tested — and read by nothing that computes | Round-trip tests pass perfectly | Reader trace classifying every reference as write / CRUD / schema / **compute** |
| **DM-D** | **Grain inferred, not declared** | "Which period does this belong to" derived from a date rule; rescheduled/backdated items break it | Works for every non-exceptional record | Declare the grain, put the key in the row, ask the authority (DM8) |
| **DM-E** | **Semi-additive measure summed across time** | Balances/positions/headcounts added up over periods | The arithmetic is valid; only the meaning is wrong | Declare additivity per measure (DM9); a test on a known multi-period fixture |
| **DM-F** | **Type-1 where Type-2 was needed** | A mutable attribute on a current-state row silently rewrites the meaning of past records | Nothing errors; history just quietly changes | History rule per attribute before it ships (DM10); a point-in-time test |
| **E2E-A** | **Field reaches the DTO but not the wire** | Present in model, service, client type and column list; missing from the projection | Every layer's own test passes | Write the surface list first (E7); one end-to-end assertion per field |
| **E2E-B** | **Declared shape with no write path** | A facet/status/enum/route declared in the design that nothing ever sets | Telemetry reports "nothing to do", which is *true* for the wrong reason | Test that the shape is written under the condition the design claims |
| **E2E-C** | **Green suite, broken surface** | All units green; the page never renders; the composition root is never exercised | Units construct their subject by hand | One proof through the real composition root to the real rendered surface (E11) |
| **E2E-D** | **Component tests that can't see each other** | Every part correct against its own expectation; parts disagree with each other | Per-component coverage is high | Cross-surface consistency test on one seeded scenario (E12) |
| **E2E-E** | **The gate passed, its contents didn't** | Multiple checks in one block; only the last exit code propagates | The gate is green | Run each control independently; assert each reports its own status (E13) |
| **E2E-I** | **Tool-not-run rendered as tool-found-nothing** | A wrapper treats an external checker's empty output as empty findings | The report is well-formed and the exit code is 0; a failed invocation is indistinguishable from a clean scan | Treat no-output as an error, surface stderr, exit non-zero; test tool-missing / non-JSON / clean / findings |
| **E2E-F** | **Exit code taken as result** | A command returned success; the state was never read back | Success is success-shaped | Read back the state and assert on it (E14) |
| **E2E-H** | **Static control scanned the shell, not the surface** | A client-rendered page whose HTML is a shell; the linter/detector reports few findings because there is little to see | The run is green and the tool did nothing wrong — nobody asks what fraction of the surface was in the corpus | Strip `<script>` and count the remaining body before trusting a clean run; where it is a small fraction, scan the **rendered DOM** instead (`ui-craft-detection.md` CD20) |
| **E2E-G** | **Reachability not verified** | The screen/panel/action exists but nothing links to it — or a link points at nothing | The feature works if you can get to it | Assert the navigation path to any new surface (E10) |
| **RIG-A** | **Own-code shape asserted from memory** | "This type has that member" / "this helper is general" — written in a *design*, never opened | Designs aren't compiled | Read the file or label Inferred (E15); adversarial review that opens the cited file |
| **RIG-B** | **Delegated inventory cited as fact** | A sub-agent's counts/listings enter an artifact unverified | Sub-agent output is fluent and specific | Spot-check before citing (E16) |
| **RIG-C** | **Sweep stopped at the instance** | The fix works; the sibling ships the same bug days later | The reported symptom is gone | class → sweep → derive → prevent (CI2) |
| **RIG-E** | **"It works" cited as conformance** | An artifact is observed functioning, and that observation is treated as proof it matches the contract | The evidence is first-hand, so it is more convincing than a guess — but it shows the thing is *tolerated*, not *specified* | Check the observation against the authoritative source before promoting it (BoK III.1); a tolerant path is not a conformant one |
| **REC-A** | **Stale record / overstated claim** | A comment, status table or doc asserts something that was true once | Documentation isn't executed | Re-verify records you touch (E17); correct overstatements in their own change (CI11) |
| **HYG-A** | **Commented-out or dead code left in the tree** | Blocks commented out "in case", an unreferenced method/field, an unreachable branch, an unused import — often the residue of code an agent commented out while working and never removed | It compiles and every test passes; a comment executes nothing, so nothing fails — it just goes stale silently, misleads the next reader, and clutters what a code-graph builder / linter / `grep` reads | Delete it — version control is the archive, not the working tree; the turn does not close with it (`communication-and-task-discipline.md` CT18a); a commented-out-code / unused-symbol detector wired as a CI gate + the Simplifier's `delete:` sweep (L9), IDE0051/IDE0052/CS0219 for compiled code; sweep the class, not the line (CI2) |
| **PACK-E** | **Install instruction with no source** | A deployment map names a destination file the project does not actually ship | The source repo has the file, so every check passes there; the failure appears only in a target repo on the adopter's first command | Ship a template for every destination named, with portable frontmatter, and verify the install on a scratch repo |
| **OPS-A** | **Migration compiles but is never applied** | The migration builds; the deployer doesn't run it | The build is green | Migrate-before-publish enforcement; exercise the down path (DM16) |
| **OPS-B** | **Ignore rule written, never verified to take effect** | A `.gitignore` pattern with a trailing `# reason` on the same line — `#` opens a comment only at line start, so the pattern matches nothing | The write succeeds, the tool reports success, the exit code is 0; the first evidence is a leaked credential | Read the state back, not the exit code (E14): `git check-ignore --quiet <path>` exits **0 when the path is ignored**, and `git status` does not list it. Never verify with `-v` — there the exit code means *"some pattern matched"*, a `!` negation counts as a match, so `-v` exits 0 for a **re-included** path and inverts the answer (measured) |
| **PACK-C** | **Documented command assumed portable** | An instruction hard-codes an interpreter or path form correct on the author's platform and absent on a supported one (`python3` where python.org Windows ships only `python.exe`) | Nothing executes documentation, so the first evidence is an adopter reporting a "missing" dependency that is installed | State the convention once, add a **detection control** that names the working form for the current machine, and verify any substitution on *every* supported platform before applying it |
| **RIG-D** | **External contract guessed from the vendor name** | Env vars, flags or model IDs asserted from what a product is *called* rather than from its source (`HIGGSFIELD_API_KEY` where the server reads `HF_API_KEY`) | Plausible, type-checks, and nothing fails until the integration actually runs | Open the source or the docs and read the identifier (`no-guessing-protocol.md` NG1/NG3) |
| **UX-A** | **Archetype mismatched to the task** | A dashboard/bento archetype applied to a data-entry task: everything visible at once, nothing sequenced | Each component is individually fine | Verify the archetype against the job-to-be-done on *changes to existing screens*, not only new ones (`ui-archetype-grammar.md`) |
| **UX-B** | **Shipped with data-only states** | Loading/empty/error states never designed; "it looked fine with data" | Demos and tests use populated fixtures | Complete component state set as a design gate (U9) |
| **UX-C** | **Craft rule documented but not controlled** | A named anti-pattern lives in a knowledge doc's prose table while the repo's own artifacts violate it | Prose is not executed; a reviewer has to remember to consult a table at exactly the right moment | The **deterministic detector** in CI (`ui-craft-detection.md` CD8, `ui-craft-gate.py`) — this class was found *by* adopting it: it caught four real instances in this pack's own templates |
| **VA-A** | **Generated asset referenced by provider URL** | A mockup or page links a generation-provider CDN URL instead of a committed file | It renders perfectly today; provider retention is typically days | Generate once, download, optimize, commit, reference by relative path (`ui-visual-assets.md` VA4) |
| **VA-B** | **Generated interface passed off as design** | A generated "screenshot" or UI panel embedded in a design artifact | It looks finished, so nobody inspects the illegible text or the invented controls | Never generate the interface, only what it shows (VA5); mockups stay hand-authored dependency-free HTML (DX8) |
| **PACK-A** | **Conditional standard wired as prose, not as a trigger** | A standard declares itself "triggered", but the skill that should apply it references it only in narrative or a definition-of-done checkbox | The reference genuinely exists, so link-checks and consistency gates both pass; it is merely in the wrong place in the flow, and the cost stays invisible until someone makes the decision it should have shaped | An explicit **trigger table** with union semantics, mapped at the stage the trigger reshapes rather than at the gate (`testing-strategy.md` §3 is the model) |
| **OPS-CI** | **Unbounded per-item CI cost on a premium runner** | A required gate whose wall-clock and billable-minute cost grow with the *product* (each migration/test adds fixed cost to every integration class), billed at 2× on Windows, run on every push and re-run on merge | Every individual test is fast and every green run looks fine; the only guard warns near the *timeout*, not the *budget*, so cost creeps up with nobody watching until a job is cancelled at its limit | Profile the bill (CE2); trend the aggregate billable-minutes and warn before the budget, not the timeout (`ci-and-test-efficiency.md` CE26) |
| **TEST-BOOT** | **The bottleneck is the suspect nobody measured** | A suite is "slow" and the obvious cause (a migration chain, "too many tests") is optimized, moving nothing — because the real cost is elsewhere (app boots re-created per test were 73% of CPU-time; the DB tests were 0.5–3s) | Every class looks locally reasonable; without a self-time profile the dominant cost is invisible and intuition points at the wrong place | Profile self-time per class **before** optimizing; share expensive fixtures instead of rebuilding per test (`ci-and-test-efficiency.md` CE1/CE14) |
| **TEST-CONTEND** | **Parallelism on top of contention spends money to go slower** | A unit is cheap in isolation and expensive under load (a 3s boot → 36s at parallel width) because concurrent workers serialise on one shared resource and/or retry a missing one | Green in CI (2-core runners rarely hit the width that contends); the blow-up appears only on a many-core machine, so evidence points at the developer's box | Measure isolation-vs-load (CE3); remove the contention (real server / per-worker instance) and kill retry-storms against missing resources, don't out-core it (`ci-and-test-efficiency.md` CE16/CE17) |
| **PARALLEL-BLIND** | **A concurrency defect the gate's runners are too small to reach** | A race that fails locally at 8–16 cores and passes in CI at 2, with the failing tests changing run to run on the same commit | The gate is *structurally incapable* of observing it, and its green is honest about what it ran and silent about what it could not | Run the concurrency-sensitive suite at a width that can hit the race (a larger runner or a forced high thread count); a coverage-for-speed width reduction is a recorded deviation (`ci-and-test-efficiency.md` CE22) |
| **RES-LEAK-TEST** | **A per-test durable resource nothing destroys** | A helper creates a database/container/blob/temp-dir that outlives the process with no teardown; the count only ever goes up (one repo reached 28,799 orphaned test databases) | Every test passes on every run; CI runners start empty and are discarded, so the accumulation is invisible to the gate and surfaces only on a long-lived dev machine | Teardown every durable per-test resource; a **janitor** control that counts before/after a run and fails when the number only climbs (`ci-and-test-efficiency.md` CE18) |
| **PACK-O** | **A turn begun with no stated goal state or exit condition** | Work continues past the point it should have stopped because nothing defined that point; the symptom is either ceremony (work past sufficiency) or under-validation (no acceptance criteria) — opposite-looking, same cause — and a gap found en route is silently promoted to a work order | Every step after the satisfaction point is individually useful, correct and defensible and nothing errors, so scope is invisible from inside the work; a completion-forcing harness *rewards* continuing while a turn-ends-naturally harness *masks* it, so neither surface reports it | The **two-step front matter** (`communication-and-task-discipline.md` CT19–CT24): open every turn by writing the goal state and stop point, then plan the whole turn with `/optimize-graph` once — its GO16 Stage-0 triage is the termination argument. CT19–CT24 is rung-3 (always-loaded). **Rung-2 (built):** the audit entry records `done_when` (AL5b) and `/dream`'s PACK-O miner flags substantive turns that recorded none (presence — mechanical) and surfaces `done_when`→summary pairs for scope-drift review; a rung-1 belt is the autopilot step cap (`--max-autopilot-continues` / `--max-turns`, see `kb-agent-autopilot-controls`) |
| **CI-ENV** | **A control proven only where it was authored** | A control is red-then-green on a dev machine and fails on the first CI run — not because the checked thing is broken but because the control borrowed ambient local state (git identity, a global tool, a credential, a locale) | Red-then-green *is* the discipline and it was followed — but both colours were observed in one environment full of invisible accumulated config a fresh runner lacks | The control **supplies its own** environment (identity/tools/fixtures) and is proven on a clean runner; spike a runner change on a throwaway job before flipping the protected gate (`ci-and-test-efficiency.md` CE23/CE24) |

---

### 6.1 Context, ceremony and coordination classes (seeded from session profiling)

These seven were measured on **one** profiled codebase's Copilot CLI session (`session-profile.py`, 2026-09-05), not two — they are seeded because the controls are generic to the pack, not because the classes met the two-codebase bar above. Each is `partially-controlled` in the source repo and `uncontrolled` in a fresh one until the profiler has re-flagged it there.

| ID | Class | Signature | Why it survives | Control to build |
|---|---|---|---|---|
| **CTX-A** | **Several tasks in one session re-read on every request** | Context grows monotonically across unrelated tasks; no compaction fires | Nothing errors; the permissive tier looks like generosity | WT1a (new task → new session); `pack-doctor` `copilot settings`; profiler SP-01 |
| **CTX-B** | **Budget gate measured its own part, reported the whole green** | Knowledge-only budget ~44k while the assembled prefix was ~105k+; a scope-less vendored copy reported 461 | The gate is honest about what it scans; nobody asks the fraction | `context-budget.py prefix --gate`; scope-declaring discovery; `CLAUDE.md` = `@AGENTS.md` |
| **CTX-C** | **Council convened on a turn with no declared tier** | Goal state without `Tier:`/`Fan-out cap:`; nine sub-agents on a proposal iteration | Proportionality was prose; goal presence was checkable, tier was not | CT19 tier + cap fields; `audit-log.py --tier --fan-out`; `selfcheck`; profiler SP-06 |
| **CTX-D** | **Same file read again while still in context** | A file viewed 4× in three minutes; a paged output viewed whole twice | A read never fails; "check first" is prose | `reread-guard.py` hook on both hosts; profiler SP-04 |
| **CTX-E** | **Knowledge at hand re-fetched whole** | A 30 KB skill re-injected six times; four craft docs read whole for one mockup | A skill has no size budget; glob docs attach late | Progressive-disclosure skills; `context-budget.py skills --gate`; rule indexes; profiler SP-05/SP-16 |
| **CTX-F** | **Delegation with no budget and no convergence predicate** | 123 tool calls / 3.0M tokens; the parent had to say "converge now" twice | GO7 bounded width, not depth; "enough evidence" was undefined | GO7 per-branch budget + convergence condition; card budget rule; profiler SP-07 |
| **CTX-G** | **Persona reads the roster to find out what it is** | ~170 KB of roster docs per sub-agent spawn | The card pointed there; the cost hides inside the sub-agent | Self-sufficient cards with a do-not-read list; profiler SP-08 |

## 7. Self-verification checklist

- [ ] Every defect, correction and falsified assumption from this work produced a **register entry** in the same change (CI1, CI4).
- [ ] Each one answered **class / sweep / derive / prevent** in writing, and the sweep results are listed (CI2).
- [ ] Siblings found by the sweep were **fixed or explicitly registered with an owner** (CI3).
- [ ] Each class carries a **named control** taken from the highest holding rung of the ladder, and the control was **observed failing** on the un-fixed code (CI6).
- [ ] Lessons that must be remembered went into the **instruction path**, not a plans folder (CI7).
- [ ] Anything that generalises beyond this repo was raised as a **pack change** (CI8).
- [ ] `simplify:` markers in the affected subsystem were **harvested** during investigation (CI9).
- [ ] The register was **read at grounding** for the area under work (CI5).
- [ ] Overstatements in committed artifacts were **corrected in their own change**; instructive wrong turns were kept visible (CI11).
- [ ] The closing summary states **what class was learned and what control now holds it** (CI12).

---

## 8. References

- **`end-to-end-integrity.md`** — the in-flight discipline these classes were distilled from; E5 defers to CI2 for the generalisation mechanism.
- **`domain-and-data-modelling.md`** — DM7/DM8/DM9/DM10/DM15/DM16, the model-side controls for the DM-* classes.
- **`solution-selection-ladder.md`** — L5–L6, the `simplify:` marker and debt ledger that CI9 harvests.
- **`audit-and-change-log.md`** — AL5 the Audit Mandate, AL10 read-history-at-grounding; the register is its defect-shaped sibling.
- **`testing-strategy.md`** — where most controls land (rung 2), including mutation resistance as the check that a control can actually fail.
- **`agent-body-of-knowledge.md`** — Part VIII names the *reasoning* anti-patterns; this register names the *observed* defect classes, and the two are meant to grow into each other (a class that recurs across projects is a candidate anti-pattern).
- **`/investigate`** — the skill that must produce a register entry; **`/extendaibundle`** — the route for a lesson that belongs upstream in the pack.
