---
applyTo: ".github/workflows/**,**/*.yml,**/*.yaml,**/Dockerfile,**/Makefile"
---
# CI & Test Execution Efficiency — best coverage at minimum time and cost

*Normative guidance for the **economics of verification**: getting the strongest coverage per minute and per dollar out of a test suite and its CI gate, without ever weakening the gate. The **Testing Strategy** (`testing-strategy.md`) governs *what* to test and *what counts as proof*; **`end-to-end-integrity.md`** (E13–E14) governs *that a gate actually runs its contents*; **this document governs how the verification is executed** — the runner it runs on, how often it runs, what it rebuilds, where its time actually goes, and the control that stops its cost creeping up with nobody watching.*

Normative keywords (**MUST**, **SHOULD**, **MAY**, **MUST NOT**) follow RFC 2119.

The governing idea, learned the expensive way: **a test suite's cost grows with the product, not with the change under review, and every individual green run looks fine right up until a required job is cancelled at its timeout.** Coverage is a floor (Testing Strategy prime directive 1); *coverage per minute per dollar* is the number that decides whether the floor is affordable enough to keep. The two failure modes this document exists to prevent are (a) **paying more than you need** for the coverage you have — the wrong runner, the same gate run twice, the whole solution rebuilt in every job, a suite whose time goes somewhere nobody measured — and (b) **buying speed by quietly weakening the gate** — muting a step, path-filtering the required check into silence, or shipping a half-validated change to the protected merge gate.

Every directive below is backed by a real, measured investigation in a repository running this pack: a single required job that had crept from ~8 to ~37 minutes, ran on a 2×-billed runner, ran twice per change, and was ~95% of the monthly Actions bill — reduced ~60–75% with **no gate weakened**, and, decisively, whose dominant cost turned out to be *not* the thing everyone suspected. Where a consuming repo has run this investigation, cite its recorded CI-cost report and defect-class register rather than re-deriving the numbers here.

---

## 0. When this applies, and who owns it

**Applies** to any work that **adds, changes, or reviews CI workflows, a test suite's execution shape, or the cost/duration of the gate** — a new test class, a new integration dependency, a workflow edit, a runner choice, a migration that every test class re-applies, a required check, or an investigation into "CI is slow / expensive." At **T0** (adding one fast unit test to an already-cheap suite) the directives collapse to CE1 and CE19. Everything that changes the *execution economics* of the gate runs the full discipline.

**Owner:** the **SRE & Systems Diagnostician** (design-time performance budgets & profiling, `persona-audit.md` §5) with the **Release / Deployment Engineer** (the CI/CD path and rollout) and the **Test Architect** (that cheaper never means weaker — the gate still proves what it claims). The **Data & Persistence Architect** owns the test-database levers (CE15–CE17). No persona may trade coverage for speed without recording it (CE22, Deviation Protocol).

---

## 1. Prime directives

1. **Measure before you optimize; the bottleneck is rarely the suspect.** A "faster" CI change with no profile behind it is the Hunch Optimization anti-pattern (BoK Part VIII) with a bigger blast radius. Profile the suite's self-time per class and the gate's billable minutes per job *first* (CE1).
2. **Optimize the runner and the schedule before the tests.** The cheapest minute is the one never billed: the wrong runner multiplier and the gate run twice are usually larger, lower-risk wins than any test rewrite (CE8, CE11).
3. **Cheaper is never weaker.** Every efficiency move preserves the gate's fail-closed contract and its coverage. A speed-up that mutes a step, skips a suite, or ships unvalidated to the protected gate is a defect, not an optimization (CE19–CE22).
4. **Cost that grows with the product needs a guard that watches the aggregate.** A per-job timeout warns near the *timeout*, not near the *budget* — so cost creeps up with nobody looking. Trend the minutes; warn before the wall (CE26).
5. **A control must run everywhere the gate runs.** A check proven green only on the machine that authored it is not yet a control (CE23).

---

## 2. Measure first — profile the suite and the bill

**CE1 — Profile the suite by self-time per class before optimizing anything.** Run the whole suite once with per-test timing (a TRX/JUnit report, `--logger trx`, `pytest --durations`, `cargo nextest`'s timings) and rank classes by *self-time*, not test count. The dominant cost is almost never where intuition points: in the anchoring investigation, everyone suspected the migration chain (`O(classes × migrations)`), but the profile showed **eight `WebApplicationFactory` boot classes were 73% of the suite's CPU-time while the actual database-integration tests were 0.5–3s each**. *"The migration chain is not the bottleneck; app boots are"* — a conclusion only the measurement could produce. Optimizing the suspected cause would have spent effort and moved nothing (Hunch Optimization).

**CE2 — Profile the bill by billable-minutes per job over a real window.** Establish where the money goes with data, not reasoning: `gh run list` / `gh run view` over a 7-day window, per-job wall-clock × the runner multiplier, and the event split (how many runs are `pull_request` vs `push`). The anchoring investigation found **one job was 76% of every run and ~72% of the whole bill**; without the split it would have been invisible which lever mattered. Where the billing API is reachable it is authoritative; where it is not, say so and treat the duration-derived figures as an order-of-magnitude estimate with a named residual risk.

**CE3 — Watch for super-linear contention, and measure it in isolation vs under load.** A cost that looks intrinsic is often contention: in the investigation a full-app boot measured **~3s in isolation and ~35.9s under the parallel suite — a ~10× blow-up** — because concurrent boots serialised on one shared LocalDB process *and* each retried a connection to a database that did not exist. The tell is a per-item cost that rises with parallel width. Measure the unit both alone and under full load before deciding whether the fix is "make it faster" or "make it not contend" (CE16).

---

## 3. The rings-of-integration model — run the right tests at the right time

**CE4 — Separate a fast path from a slow path.** The single flat gate that runs everything on every push is the shape that creeps. Split verification into **rings** by cost and feedback value, and let the protected required check *aggregate* them rather than *be* them:

- **Ring 0 — fast, every push, cheap runner, target < ~6 min, required.** Build once + publish artifacts; unit-only tests; the lint/graph/secret/control suite. This is the feedback loop, so it must stay fast.
- **Ring 1 — slow, at merge readiness, required.** Integration (real infra, Testing Strategy D4) + e2e, **reusing Ring 0's build artifacts** (`--no-build`). Runs **once per PR at readiness** (merge queue or a "ready" trigger), **not** on every work-in-progress push and **not** again on push-to-main.
- **Ring 2 — post-merge / scheduled.** Deploy-on-merge whose health check *is* the post-merge integration proof; an optional single nightly full sweep; trimmed scheduled controls.

**CE5 — The required check stays fail-closed; rings change what feeds it, never its contract.** Keep one protected check (e.g. `delivery / required`) with an aggregator whose rule is *any non-success ⇒ red, neutral ⇒ red*. Rings change *what runs and when*; they never rename the required check, never make it optional, and never let a skipped ring report as green (CE20).

**CE6 — Do not run the full gate twice for one change.** With protected `main` + "require branches up to date," the merge tree is identical to the just-passed PR tree, so a full gate on `push:[main]` re-proves an already-green tree. In the anchoring case that was **~116 redundant premium runs/week (~34% of the job's spend)**. Drop the full gate on push-to-main; let the deploy-on-merge health check be the post-merge proof. (Verify the "up to date" branch-protection setting is actually on before relying on this — it is the premise.)

**CE7 — Path-filter premium work, but never into a false green.** A docs-only or tracker-only change should not pay for a premium build+test — *but* a `paths:` filter that skips a job feeding a fail-closed aggregator can turn "skipped" into "red" or, worse, "silently green." Path-filtering is real money only when the aggregator still resolves correctly for the skipped case; prove a docs-only PR both skips the expensive job **and** still reports the required check green for the right reason (E13). When in doubt, the cheaper correct move is often CE6, not CE7.

---

## 4. Runner economics — the cheapest minute is the one never billed

**CE8 — Know the multiplier, and prefer the 1× runner.** GitHub-hosted minutes bill at **Linux 1×, Windows 2×, macOS 10×**. A required job on Windows costs double for the same work. The single largest lever in the anchoring investigation was moving the integration suite off Windows: **LocalDB (Windows-only) → a Linux SQL Server container cut the job's billable minutes ~69%** and was ~38% faster in wall-clock too. Pick the runner deliberately; a premium runner is a decision that needs a reason (a genuinely platform-locked dependency), not a default.

**CE9 — A platform lock is a cost to remove, not a constant to accept.** "We must run on Windows because the tests use a Windows-only dependency" is a *finding*, not a floor. Name the specific dependency (here: SQL Server LocalDB), and check whether a cross-platform equivalent exists (a `mcr.microsoft.com/mssql/server` container, Testcontainers, an emulator). The lock is usually one seam away from removable — and removing it halves the recurring cost of the whole ring.

**CE10 — Right-size the runner against a measured need.** A larger runner (more cores) is worth electing when the suite is CPU-bound *and* parallel-efficient enough to use the cores (CE14); it is wasted money when the suite is contention-bound (CE3/CE16), because more cores just deepen the contention. Record a larger-runner option as a deliberate, measured election, not a reflex — and re-measure parallel efficiency after fixing contention, because that is what makes the cores pay.

---

## 5. Don't pay twice — build once, share artifacts

**CE11 — Build once per run; downstream jobs consume the artifact.** Re-checking-out and re-building the whole solution in every job is duplicated premium work. Build in Ring 0, `upload-artifact` the output, and have integration/e2e `download-artifact` + run with `--no-build` / `--no-restore`. One Release build per run instead of three.

**CE12 — Cache only what a measurement shows is worth caching.** Caching is not free: it has upload/download/restore cost and a staleness surface. The anchoring repo *measured and rejected* NuGet caching (warm restore 6s vs 127s to populate the cache) — the cache was slower. Establish the restore cost before adding a cache key; a cache that costs more than the thing it caches is negative work.

**CE13 — Collapse work-in-progress runs.** Use `concurrency:` with `cancel-in-progress: true` so intermediate pushes to a branch cancel their own superseded runs, and use a merge queue so Ring 1 runs once at readiness rather than on every WIP push. Both convert a stream of premium re-runs into one.

---

## 6. Test-infrastructure levers — where the time usually actually is

*These are the highest-leverage moves because, per CE1, the dominant cost is usually test infrastructure (app boots, shared resources, per-class setup), not the product logic under test — so they are a test-infrastructure change, not a product change, and they preserve coverage exactly.*

**CE14 — Share expensive fixtures instead of rebuilding them per test.** Booting the whole application (`WebApplicationFactory<Program>`, a DI graph, hosted services) *per test* is the most common single waste. Share one instance across same-configuration classes via a collection/class fixture; in the anchoring case this collapsed **60+ boots to ~8–15** and, because fewer concurrent boots also cut the per-boot contention, it was a **double win** (fewer × cheaper boots), turning a ~1798s block into an estimated ~150–300s. This is a fixture-lifetime change; the tests and their assertions are untouched.

**CE15 — Do per-class setup once, then clone.** A pattern where *every* integration test class migrates its database from scratch in its constructor is `O(classes × migrations)` and slows measurably with every migration added — the "8→37 minute creep." The structural fix is **migrate once into a template database and clone it per class** (or per collection), making per-class setup O(1). Treat a `simplify:`-style "treat the symptom with a bigger timeout" note in the setup code as the triggered marker it is (CI9) — it is the defect, already written down.

**CE16 — Remove shared-resource contention, don't just add cores.** When the profile shows a unit cheap in isolation and expensive under load (CE3), the fix is to remove the contention, not to parallelize harder. Options: a real server that handles concurrent connect/login (a SQL container vs LocalDB's single `sqlservr`), per-worker resource instances instead of one shared instance, or reduced parallel width on the contended resource. Parallelism on top of contention spends money to make things slower.

**CE17 — Kill retry-storms against a missing or unreachable resource.** A resilience policy that treats "resource does not exist" as transient turns every startup into a retry storm: in the anchoring case, `EnableRetryOnFailure` treated SQL error 4060 ("cannot open database") as retryable, so **every app boot retried a connection to a database that was never created** before degrading. Give the test environment a reachable, pre-migrated default resource, or set retries to zero in the Testing configuration. A retry against a permanent absence is pure waste, multiplied by every boot.

**CE18 — A test that creates a durable external resource must destroy it (RES-LEAK).** A helper that creates a database/container/blob/temp-dir that outlives the process, with no teardown, leaks silently forever — *CI runners start empty and are discarded, so the accumulation is invisible to the gate* and surfaces only on a long-lived developer machine as "works in CI, broken on my box." The anchoring repo accumulated **28,799 orphaned test databases**. Every durable per-test resource gets a teardown (`IAsyncLifetime`/fixture `Dispose`, a `finally`, a container-per-run), and a **janitor** control that counts the resource before/after a run and fails if the number only ever climbs.

---

## 7. Gate integrity while cheap — the fail-closed floor

*These are the guardrails that keep §§3–6 from becoming a silent weakening of the gate. Several are CI-specific instances of `end-to-end-integrity.md` E13/E14 and the seed defect classes; they are restated here because the pressure to violate them comes precisely when you are trying to make CI cheaper.*

**CE19 — A gate step must be able to fail (no muting).** A trailing `$LASTEXITCODE = 0`, a `|| true`, a swallowed `catch`, or piping a command somewhere its status is lost makes a step report PASS on the runs where its contents failed — *more* dangerous than an absent check, because it is a false assurance. The mute is usually added for a real reason (a tool that writes progress to stderr, or whose exit code is unreliable) and silences the noisy neighbour *and everything after it in the same block*. Fix the specific noise; never blanket-zero the status (defect class GATE-MUTED; E13).

**CE20 — One command per gate step, or an explicit fail-fast shell (no shell-swallowed failures).** A multi-command `run: |` block with no explicit `shell:` reports only the *last* command's exit code on pwsh (Windows) while `bash -e` (Linux) stops at the first — so the same repository state is green on one platform and red on the other, and the green is the wrong answer. Run each control as its own step (so each reports its own status), or set `shell: bash` / `set -euo pipefail` / `$ErrorActionPreference='Stop'; exit $LASTEXITCODE` explicitly. Verify a control *fails the run when it should* (defect class GATE-SHELL; E13).

**CE21 — The gate must run the aggregate suite, not a hand-picked subset (CTRL-D), and count what it runs.** CI invokes the aggregate control/test runner; an agent or contributor running "the scripts I know about" reports "controls green" and the gate then fails on a control that was never in their list. **Derive the suite/control count from the filesystem and assert every discovered suite ran** — a suite that exists in one place and is silently not executed is the failure this catches (defect class CTRL-D; the "derive the suite count" control). Equally, verify the required check *can report its own absence*: a conflicted PR with no merge ref, or a workflow that never triggered, must resolve the required check **red**, never as reassuring silence (defect classes CI-A / GATE-MUTED).

**CE22 — Trading coverage for speed is a recorded deviation.** Any efficiency move that reduces what is verified on the path to merge — dropping a suite from the required set, lowering parallel width past where a concurrency defect is observable (defect class PARALLEL-BLIND: *a 2-core runner is structurally incapable of observing a race an 8-core machine hits*), relaxing a threshold — is a deviation under the Deviation Protocol (Rules of the Road §4): name what is no longer covered, the residual risk, and where it *is* covered instead (e.g. a nightly Ring 2 sweep). The Test Architect's hard veto applies to an un-recorded reduction in proof.

---

## 8. Change the gate safely — spike before you flip

**CE23 — A control must supply its own environment; prove it on a fresh runner, not only locally (CI-ENV).** Red-then-green observed only on a developer machine is not proof: a dev machine has years of ambient state a fresh runner does not — a configured git identity, a global tool, a credential, a locale, a path. In the anchoring case a control used `git commit-tree` (which needs an author); locally the identity came from `.git/config` and was invisible, and it failed anonymously on the first CI run. A control **supplies** its identity/tools/fixtures rather than borrowing them, and is proven on the clean runner (defect class CI-A/CI-ENV).

**CE24 — Spike a runner/infra change on a throwaway job before touching the protected gate.** Before flipping the required gate to a new runner or new infra, prove the premise on a disposable `workflow_dispatch`/push job that runs the real suite against the new environment. The anchoring investigation took "move to a Linux SQL container" from an *Inferred* code-read to a **spike-proven 99.4% green (1779/1789), ~69% billable cut**, and — critically — the real run surfaced a latent Linux-incompatibility in a control script that *a static scan had missed*. Prove the surface (E11); do not reason your way onto the protected gate.

**CE25 — Ship the change behind a landed, inert seam; iterate with local infra, not CI round-trips.** Land the seam that the change hangs on (e.g. an env-selected connection string) **inert** first — default path unchanged, validated — so the follow-up is a small workflow edit plus named fixes, not a from-scratch change. Do the tuning iterations with **local Docker/containers** where each cycle is seconds, not a 20-minute CI round-trip; flip the protected gate only once it is green and characterized.

---

## 9. The prevention — a cost/duration budget control

**CE26 — Trend the aggregate cost and warn before the wall (OPS-CI).** The class this whole document defends against is *unbounded per-item cost on a premium runner, invisible until it hits a wall* — every individual test fast, every green run fine, and the only guard a timeout that warns near the *timeout* rather than the *budget*. The control (CI6 rung 2, the one that makes the class self-evident next time): a check that records the long-pole job's minutes and the total billable-minute burn per run to a committed trend file, and **warns when the trend is rising or approaches a stated budget** — not when a single run nearly times out. A budget the gate watches is the difference between "CI got expensive over six months and nobody noticed" and "the trend line went up in week two and we looked."

---

## 10. Self-verification checklist

- [ ] The suite was **profiled by self-time per class** and the bill by **billable-minutes per job over a real window** before any optimization; the dominant cost was measured, not assumed (CE1–CE2).
- [ ] Contention was checked by measuring a unit **in isolation vs under load** (CE3).
- [ ] Verification is split into **rings** (fast every-push / slow at-readiness / post-merge), feeding a fail-closed required check whose contract is unchanged (CE4–CE5).
- [ ] The full gate does **not** run twice for one change (CE6); any path-filter was proven not to create a false green (CE7).
- [ ] The runner **multiplier** was considered and the cheapest sufficient runner chosen; any platform lock was treated as removable, not constant (CE8–CE10).
- [ ] The solution is **built once** and artifacts shared; caches added only where measured to pay; WIP runs collapsed (CE11–CE13).
- [ ] Expensive fixtures are **shared**, per-class setup is **clone-not-rebuild**, contention removed rather than out-cored, retry-storms against missing resources killed, and every durable test resource has **teardown + a janitor** (CE14–CE18).
- [ ] No gate step is **muted**; each control **reports its own status**; the gate runs the **derived aggregate** suite and can report its own **absence** as red (CE19–CE21).
- [ ] Any coverage-for-speed trade is a **recorded deviation** with residual risk and where-else-covered (CE22).
- [ ] New/moved controls are proven on a **fresh runner**; runner/infra changes are **spiked on a throwaway job** and landed behind an **inert seam** before the protected gate flips (CE23–CE25).
- [ ] A **cost/duration budget control** trends the aggregate and warns before the wall (CE26).

---

## 11. References

- **Evidence base:** where a consuming repo has run the investigation, its CI-cost report and its defect-class register carry the measured source of every directive here — the classes **OPS-CI**, **GATE-MUTED**, **GATE-SHELL**, **CTRL-D**, **CI-A** (control-passes-only-where-authored *and* gate-cannot-report-its-own-absence), **PARALLEL-BLIND**, **RES-LEAK**, **TEST-CLOCK**. Cite that repo's recorded numbers rather than re-deriving them.
- **`testing-strategy.md`** — governs *what* to test and *what counts as proof* (coverage-is-a-floor, mutation resistance, D0 determinism — the home of the TEST-CLOCK rule, D4 real-infra integration, D7 mock fidelity). This document is its **execution-economics** companion: same coverage, fewer minutes and dollars.
- **`end-to-end-integrity.md`** — **E11** prove the rendered surface (CE24), **E13** a gate's green ≠ its contents passed (CE19–CE21), **E14** an exit code is not a result.
- **`continuous-improvement.md`** — CI1–CI6 the class→sweep→derive→prevent mechanism and the control ladder (CE26 is a rung-2 control); CI9 the `simplify:` marker harvest (CE15). The seed register carries the CI-efficiency classes referenced above.
- **`solution-selection-ladder.md`** — smallest-correct applied to CI: the cheapest minute is the one never billed (CE8, CE11).
- **`agent-body-of-knowledge.md`** Part VII.8 (measure, don't guess) and Part VIII (the Hunch Optimization anti-pattern) — the reasoning floor under CE1.
- **Personas:** the **SRE & Systems Diagnostician** owns design-time performance budgets & profiling; the **Release / Deployment Engineer** owns the CI/CD path; the **Test Architect** holds the veto that cheaper is never weaker (`persona-audit.md` §5, `persona-cards.md`).
- **GitHub Actions billing** — per-minute multipliers (Linux 1×, Windows 2×, macOS 10×), `concurrency`/`cancel-in-progress`, merge queues, `actions/upload-artifact`/`download-artifact`. Verify current multipliers on use; they change.
