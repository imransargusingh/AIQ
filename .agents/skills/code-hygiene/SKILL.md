---
name: code-hygiene
description: Find and quantify violations of the coding guidelines — dead code, commented-out code, and the anti-pattern classes — as a measured backlog (lines of code and % of codebase per class) for review, then build a TDD-guarded, git-labelled remediation strategy that removes them without introducing regressions. `review` yields the analysis; `fix` yields (and, once approved, executes) the plan. Use to hold a codebase to the pack's coding standards and keep it clean.
runs_as: either
---

# Skill: /code-hygiene `review` | `fix`

Hold a codebase to the pack's **coding guidelines and directives** — and keep it there. This skill finds the accumulated clutter those standards forbid (dead code, commented-out code, the anti-pattern classes), **measures** it honestly (lines of code and percent of codebase, per class), and turns it into a reviewable backlog. Its `fix` mode then builds a remediation strategy whose entire design goal is the one the user cares about most: **remove the rot without breaking anything** — proven by tests, decomposed into atomic, labelled commits so git tooling can pinpoint and revert any regression.

It is the operational enforcement of the standards that only ever lived as prose before: **HYG-A** (commented-out / dead code, `continuous-improvement.md` §6), **CT18a** (a turn does not close with dead code in it, `communication-and-task-discipline.md`), the C# guide's **§1.6**, and the Simplifier's **delete-list** (`solution-selection-ladder.md` L9). *A lesson recorded as prose is a memoir* (CI6); this skill is the control.

**Spine:** the Rigor Protocol (`knowledge/rigor-protocol.md`), with **measure-before-diagnose** (IO1/CE1) and, in `fix`, **characterize-first / red-first** (BoK Part V.2, Testing Strategy). **Authority:** the coding guidelines themselves — `csharp-style-guide.md` §1.6, the per-language sections of `agent-body-of-knowledge.md` Part VII, its Part VIII anti-pattern catalogue, `solution-selection-ladder.md`, and the repo's own `docs/lessons/defect-classes.md`. **Mode:** Peer Mode to analyse and plan, Adversary Mode at the gate; the author never clears their own hard veto.

## The two modes

| Mode | What it does | Touches code? |
|---|---|---|
| **`review`** *(default)* | Detect → classify → **measure** → backlog + aggregate (LOC and % of codebase per violation class). Read-only analysis. | **No** |
| **`fix`** | From the backlog, build a **TDD-guarded, git-labelled, phased remediation strategy** (each removal/fix proven behaviour-preserving and independently revertible), stop for human approval, then execute the approved phases as atomic green commits. | Only after explicit approval |

`review` is the safe default and always runnable. `fix` **consumes** a `review` backlog (running `fix` with no current backlog first runs `review`).

## Grounding (first action)
Consume the compiled prompt when one is in hand (CO-S0, `knowledge/agent-coordination.md`; `/compile`): its goal state is the turn's goal state and its Not-in-scope is the interdiction — derive nothing from raw prose that a compiled prompt already fixed. Load the standards that **define** a violation and treat them as the authoritative source of truth (Rigor Protocol Stage 0): `csharp-style-guide.md` (esp. §1.6), `agent-body-of-knowledge.md` Part VII (the language idioms) and Part VIII (the reasoning/anti-pattern catalogue), `solution-selection-ladder.md` (L4 floors that are **never** removed, L9 the delete-list), `communication-and-task-discipline.md` CT18a, and — critically — the repo's live **`docs/lessons/defect-classes.md`** register (its classes are this repo's *observed* anti-patterns, and HYG-A is the dead/commented-out-code class). Read the register at grounding (CI5). Then **detect the languages and build system** present (so the right analyzers are chosen) and establish the **total codebase size** with a real counter (§Detection). Also establish, and record, **(a) the source-of-truth map** — which trees are hand-authored *source* vs *generated* mirrors vs *vendored/derived data* — from the repo's own deployment/build map (in this pack `pack/adapters/INSTALL.md`; elsewhere `tsconfig` `outDir` / `dist/` / `Directory.Build.props` / a `sync`/`build` step), because a fix to a generated copy is silently overwritten (E15/E14); and **(b) the repo's test-run command and its pre-commit gate** (in this pack `python -m unittest discover tests/docs_explorer`, the JS tests, then `tools/verify-bundle.ps1`), so `fix` can prove green-before / green-after. Skip grounding only if the user explicitly says so.

## Input
`review` or `fix`, plus an optional **scope**: a path, a module, a language, a single violation class (e.g. `dead-code`, `commented-out`, `swallowed-exception`), or a backlog item id. No scope means the whole repository. Absent a mode, default to `review`.

## Cast
- **Peers (analyse and plan together):** Orchestrator, **The Simplifier** (lead — this *is* its delete-list at repo scale, L9), the relevant **language developer(s)** (idiom and the language's own dead-code diagnostics), **Test Architect** (the safety net that makes `fix` non-breaking), **SRE & Systems Diagnostician** (is this "dead" branch actually a guard, a fallback, or telemetry?).
- **Adversaries (attack the backlog and the plan at the gate):**
  - **Test Architect** — **hard veto**: no removal or fix is accepted as safe without proof. Pure removal needs a proof of *deadness* + a green suite seen green before and after; a behaviour-adjacent fix needs a test that was **observed failing** first (red-first).
  - **Security & Identity Architect** — **hard veto** if a path proposed for removal is actually a trust-boundary guard, an input validation, an authz check, or a fail-closed default that only *looks* unreachable.
  - **Tech Lead** — proportionality: the smallest correct clean-up; reviewer findings are advice, not automatic scope (CT16); don't turn a hygiene pass into a rewrite.
  - the **language developer** — the replacement is idiomatic, not merely shorter.

## Flow (Rigor Protocol, specialized to code hygiene)

**Stage 0 — Interdict the rush.** Do **not** start deleting. A finding is a *candidate*, not a verdict: commented-out code may be an intentional `simplify:` / `assume:` marker (`solution-selection-ladder.md` L5, `no-guessing-protocol.md` NG4) — which is tracked intent, **not** a violation — and a "dead" branch may be a load-bearing guard. Classify and prove before you touch anything.

**Stage 1 — OPEN (inventory the surface).** Enumerate the codebase: the languages, the source roots, and the standards in play per language. **Analyse the source, never a generated copy** — using the source-of-truth map from grounding, exclude *generated mirrors* (byte-copies produced by a build/sync — in this pack `docs/ai-forward-pack/`, `.claude/`, `.github/{instructions,prompts,agents}`), *vendored* third-party code (`node_modules/`, `web/vendor/`), and *derived data* (accumulated indexes like `docs/docs-index.js`, `docs/audit/audit-data.js`). Counting a generated copy triple-counts the same file and inflates the denominator; **fixing one is overwritten by the next build**. Establish the **denominator** — total lines of *source* — with a real counter over the deduplicated source roots, not an estimate (§Detection). Record the source-of-truth map in the backlog so `fix` inherits it.

**Stage 2 — INTERROGATE (choose the detectors, per class, per language).** For each violation class, name the **deterministic detector** that finds it — measurement over judgement (IO1). Do not eyeball what a tool can prove.

| Class | What it is | Detector (deterministic first) |
|---|---|---|
| **Dead / unused code** (HYG-A) | Unreferenced method/field/local, unreachable branch, unused import, unused parameter | **.NET** IDE0051/IDE0052/IDE0059/CS0219 + Roslynator; **TS/JS** `knip` / `ts-prune` / ESLint `no-unused-vars`; **Python** `ruff` (F401/F841) / `vulture`; **Rust** `cargo clippy` (`dead_code`); **Go** `staticcheck` / `deadcode` |
| **Commented-out code** (HYG-A) | A comment whose body is code, not prose | **Python** `ruff` ERA001 (eradicate); **JS/TS** ESLint `no-commented-out-code`-style rules; **Ruby** Rubocop; language-agnostic: the **code knowledge graph** (Graphify, `code-knowledge-graph.md`) which flags it natively; else a heuristic (a comment line that parses as code / ends in `;`/`{`/`)` or holds an assignment/call) — **label heuristic hits Inferred** and route them through a false-positive review |
| **Anti-pattern classes** | Swallowed exception, primitive obsession, boolean-flag param, magic value, God method, duplicated logic, bare `except`, `.Result`/`.Wait()` blocking, etc. | The pack's Part VIII catalogue + the language analyzers above (Roslynator / clippy / ruff / ESLint) map most; the rest via a targeted search of the **structural signature** (E15: open the file, never assert its shape from memory) |
| **Register classes** | Any class already recorded in `docs/lessons/defect-classes.md` with a signature | Its recorded signature; a recurrence is a *failed control*, not carelessness (CI4) |

Where the repo has no analyzer wired for a language, say so and either run the tool ad-hoc or fall back to the heuristic **with the finding labelled Inferred** — never present a heuristic guess as Verified (NG6).

**Stage 3 — EVIDENCE (detect, classify, and MEASURE).** Run the detectors. For each finding record: file, line span, the **class**, the **specific guideline it violates** (traceability: finding → rule → class), and the **line count** it represents. Then **measure the aggregate** — this is the deliverable the user reviews:
- **Total LOC** from a real counter (`cloc` / `tokei` / `scc`, or `git ls-files` piped through a line count on source files) — recorded as a number, with the tool named.
- **Violating LOC and instance count per class**, summed from detector output.
- **% of codebase per class** = violating LOC ÷ total LOC, and the overall total.
- **Instrumentation-over-inference (IO7/IO8):** every number is *measured*; where a counter genuinely cannot run, the cell reads **"not recorded"** and the figure is labelled **Inferred** — never a plausible wrong number. State which detector produced each class's count.

**Stage 4 — DISCONFIRM (triage the backlog — the gate).** Adversary Mode. Before anything is called a violation to fix:
- **The Simplifier** confirms each item earns removal (dead / unused / cargo-cult), not a false positive.
- **Security / SRE** rule out that any "dead" path is actually a guard, a fallback, a fail-closed default, or telemetry — **hard veto** on removing a live safety control mislabelled dead.
- **Clear the known false-positive families** before calling anything dead — a static detector cannot see these, so each requires a real reference check (`git grep` **including tests and adapters**, or the code graph) first:
  - **framework / callback overrides** invoked by name, not by call site — `HTMLParser.handle_*`, dunders, event/hook handlers, ORM/serializer hooks;
  - **FFI / `ctypes.Structure` fields** consumed by memory layout, not by reference (removing one corrupts the struct);
  - **reflection / dynamic dispatch** — `getattr`, registries, plugin loaders, string-keyed dispatch tables, CLI subcommand maps;
  - **entry points & public/plugin API surface** — a member unreferenced *in-repo* may be a deliberate contract;
  - **test-only references** — a function used solely by the test suite is **not** dead (last run: `hook_decision_of` was flagged 60% by vulture yet exercised by `test_harness_conformance.py`);
  - **commented-out-code false positives** — section-banner/divider comments and *illustrative code-in-prose* trip even deterministic detectors (ruff `ERA001`); read each hit, do not trust the flag.
- Intentional **`simplify:` / `assume:`** markers and genuine *why* comments are excluded — they are tracked intent, not rot (L5, NG4).
- Each surviving finding gets a **disposition**: `remove` (dead / commented-out), `refactor` (anti-pattern → the idiom), or `accept-with-rationale` (a consciously-kept exception, recorded like a deviation — Rules of the Road §4). Record the gate verdict.

**Stage 5 — CONVERGE.** Emit the **backlog** and the **aggregate**. In `review` mode the skill **stops here** (read-only) for human review. In `fix` mode, continue to Stage 6.

**Stage 6 — STRATEGIZE (the non-breaking remediation plan — `fix` only).** Build the plan whose whole purpose is *no regressions*. Its rules:

1. **Prove-before-remove.** A removal is only safe when the code is *proven* dead: no references (analyzer / code graph), not reflectively or dynamically reached, not part of a published API/ABI contract, not a build-conditional (`#if`, feature flag). Anything not provably dead is treated as a **behaviour-risking refactor**, not a deletion.
2. **Characterize first, then TDD.** For a behaviour-risking change, pin current observable behaviour with a **characterization test** seen green first, so the change is proven behaviour-preserving green-to-green (BoK V.2). For an anti-pattern fix that *intends* to change behaviour, write the test encoding the intended behaviour and **observe it fail (red)** before the fix (Testing Strategy). The Test Architect's hard veto: *no proof, no removal.*
3. **Separate refactor from behaviour change.** Dead-code and commented-out-code removal are **pure refactors** — a commit that preserves behaviour, never mixed with a feature or fix. One concern per commit.
4. **Atomic, labelled, single-class commits (the git-remediation contract).** Each backlog item becomes **one atomic commit that keeps the full suite green**, carrying a structured label so git tooling can find and undo it precisely if a regression surfaces later:
   - subject: `chore(hygiene): <what> in <area> [<CLASS>]` (e.g. `[HYG-A]`)
   - trailers: `Hygiene-Class: <ID>` · `Hygiene-Item: <backlog-id>` · `Hygiene-Scope: <files>`
   - so `git log --grep 'Hygiene-Item: <id>'` finds it, `git bisect` isolates it, and `git revert <sha>` rolls back exactly that removal with no collateral. **One logical unit per commit** so a revert is surgical.
5. **Green-to-green vertical increments.** Every commit is independently green *and* independently revertible; a large sweep is decomposed so a bisect can pinpoint a single offending change. A refactor that is red in the middle cannot be bisected — forbidden.
6. **Order by risk / blast radius.** Lowest-risk first (commented-out-code deletion, provably-dead private members) → higher-risk last (public surface, dynamically-reached, cross-module). Phased so each phase lands deployable on its own.
7. **Never cut a floor (L4).** Validation, error handling that prevents data loss, security controls, accessibility, and anything explicitly requested are **not** hygiene targets even when they look removable.
8. **Edit source, then regenerate, then gate (the generated-mirror rule).** Every change lands in the **hand-authored source** identified at grounding — never in a generated mirror or derived artifact (a fix there is silently overwritten). In a repo with a build/sync step, the atomic commit is *source + regenerated output together*: apply the fix in source, run the regeneration (in this pack `tools/sync-pack.ps1`), run the pre-commit gate (`tools/verify-bundle.ps1`) **and** the test suite green, then commit the whole set. A generated file that drifts from its source is itself a defect.

**Stage 7 — REPORT & STOP (`fix`, hard gate).** Emit the remediation plan (below) and **STOP for human approval — even on autopilot.** (the stop is a message — CO-S2, `knowledge/agent-coordination.md`) The human approves which phases/classes to execute. Only then execute, phase by phase: each item as its labelled atomic commit, the suite run green before and after, the removed LOC confirmed, and the aggregate re-measured so the report shows the codebase getting cleaner. A phase that turns the suite red is reverted (its own commit) and returned to the backlog, never forced.

## Output artifacts
- **`review`** → `docs/hygiene/backlog.md` (graph node, V2 frontmatter): the **aggregate table** (class · guideline violated · instances · violating LOC · **% of codebase** · severity · detector used), the **totals**, and the **itemised backlog** (one row per finding: id · class · `file:line` span · guideline · severity · LOC · disposition). Intentional markers and L4 floors are listed as **explicitly excluded**, with why.
- **`fix`** → `docs/hygiene/remediation-plan.md` (graph node): the **phased plan** — one row per item: phase · class · scope · **test strategy** (characterize / red-first / proof-of-deadness) · **commit label** · blast radius · **rollback** (`git revert <sha>`) — plus the labelling scheme, ordered by risk. Ends at the approval stop; execution results (LOC removed, suite green, re-measured aggregate) are appended as phases land.

## Definition of done (exit gate)
**Both modes**
- [ ] Grounding read the coding guidelines **and** the live defect-class register; the languages and total LOC were established with a **named real counter** (not estimated).
- [ ] Detection used the **deterministic detector** per class/language where one exists; heuristic-only findings are **labelled Inferred** and were false-positive-reviewed.
- [ ] Every finding is traced to the **specific guideline it violates** and grouped by class.
- [ ] The **aggregate** reports violating LOC and **% of codebase per class** and overall, each number measured (or explicitly "not recorded" + Inferred — never a plausible wrong number).
- [ ] Intentional `simplify:`/`assume:` markers and L4 floors are **excluded** and listed as such; a "dead" path was not proposed for removal without ruling out that it is a live guard (Security/SRE).
- [ ] `docs/hygiene/backlog.md` written with frontmatter and linked into the graph.

**`fix` additionally**
- [ ] Each removal is **proven dead** (no reference / not dynamic / not public API / not build-conditional) *or* reclassified as a behaviour-risking refactor.
- [ ] Each change carries its **test proof**: a characterization test seen **green first** for behaviour-preserving changes, or a test **observed failing first** for intended behaviour changes; the Test Architect veto is cleared.
- [ ] Refactor is **separated from behaviour change**; each item is an **atomic, single-class, green commit** carrying the `Hygiene-Class` / `Hygiene-Item` labels so `git revert`/`bisect`/`--grep` can remediate a regression precisely.
- [ ] The plan is **ordered by blast radius**, phased to land deployable; no floor (L4) is targeted.
- [ ] Every fix was applied in **hand-authored source** (never a generated mirror); the build/sync was re-run, the **pre-commit gate and test suite were green**, and source + regenerated output were committed together (Stage 6.8).
- [ ] **Stopped at the plan for human approval**; execution (if approved) kept the suite green per phase and re-measured the aggregate.
- [ ] Adversarial gate passed; hard vetoes resolved or recorded; the author did not self-clear.

## Documentation & discoverability (last action)
Per the Discoverability Mandate (V10): write the artifact's frontmatter (id, title, type `doc`, owner, tags, typed links — `relates-to` the coding-guideline docs and `docs/lessons/defect-classes.md` — review-by, a real summary) and **sync the derived index** via `python3 docs/ai-forward-pack/scripts/docs-graph.py derive` — never ad-hoc scripts (V18); verify the artifact is linked into the graph (an orphan is a finding). If a hygiene sweep surfaces a **new** recurring anti-pattern the register does not yet name, register it as a class (CI1) and, if it would help any repo, raise it upstream via `/extendaibundle` (CI8).

**Audit (last action).** Append an audit-log entry — `python3 docs/ai-forward-pack/scripts/audit-log.py append --shortname "code-hygiene-<review|fix>" --session "<id>" --skill code-hygiene --kind skill --prompt "<the prompt, verbatim>" --summary "<classes found; LOC and % aggregate; plan/executed>" --artifact docs/hygiene/backlog.md` — per the Audit Mandate (AL5). A run that left no trace in `docs/audit/` is, like an un-indexed artifact, not done.

**Handoff:** `review` → human triage of the backlog → `/code-hygiene fix <approved class|scope>` → per approved phase, `/implement` if a fix is load-bearing enough to need its own design; a newly-found recurring class → `/investigate` (if it caused a defect) or `/extendaibundle` (if it should become a pack-level control).
