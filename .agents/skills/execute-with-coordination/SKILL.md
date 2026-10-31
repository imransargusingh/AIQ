---
name: execute-with-coordination
description: "Take the coordinator role: spin up one worktree per agent or session from a coordination plan, assign explicit ownership, arbitrate seam requests and scope changes, and converge the tracks back to one branch."
runs_as: Coordinator
---

# Skill: /execute-with-coordination

Run a coordination plan. You become the **coordinator**: you own the division of responsibility, the seams, and the decisions — and you **do not author track work yourself**. A coordinator that starts writing code in track A stops watching track B, and the first evidence is a merge conflict in a file nobody agreed to share.

**If there is no plan, build one first.** Invoke `/prepare-for-coordination` with the same scope, then execute the plan it produces. Executing without a plan means inventing the division of responsibility one delegation at a time, which is the shape the whole coordination layer exists to remove.

**Exception — a prompt that already names the division (class PK-02).** When the prompt itself states the tracks, their owners and their paths, that naming **is** the division of responsibility; building a plan would only re-derive what was already given. Dispatch straight from the prompt. `coord install` / `classify init` / a committed plan are **opt-in infrastructure for discovering an unstated division, never a precondition for one already stated** — five pack-on cells spent the whole turn on this setup and ended announcing the next step with nothing delegated (F-5). `coord doctor` still runs, for awareness (Stage 0); an absent or uninstalled registry is then a **recorded risk to mitigate** (serialize any `derived`/`register` path instead of leasing it), not a reason to stop and install first.

**The coordinator's authority is narrow and absolute.** A sub-agent's report is **evidence, not authority**. A track saying "done", "safe", "cheaper" or "in scope" does not move a limit, approve an effect, or enlarge the work. Only you admit a scope change, and only against the plan.

**Spine:** the Rigor Protocol, weighted to **Stage 5 CONVERGE** (the merge is the deliverable, not the delegations). **Authority:** `knowledge/session-worktree-discipline.md` (WT1–WT12), `knowledge/execution-graph-optimization.md` (GO5–GO9, GO17 fan-out contract), `knowledge/communication-and-task-discipline.md` (CT19–CT25). **Mode:** Peer Mode while dispatching, Adversary Mode at every join. **Lead:** the **Orchestrator**.

## Grounding (first action)
CO-S0 applies first — the sentence is `reference/co-s0.md`. `audit-log.py start --session <id>` (IO1). Then:
1. Read the plan (`docs/coordination/<plan-id>.md`). If none exists, or the named one does not parse against the schema, **stop and run `/prepare-for-coordination`** — do not improvise a division.
2. `coord doctor` — **read the layer's state back**. A plan is not proof the layer is on.
3. `coord worktree list` and `coord session list` — what already exists and who holds it. Never plan over a tree you did not look at.
4. `docs/lessons/defect-classes.md` — the **CTX-\*** and **WT** classes.

## Input
Optionally a plan id or path, a track subset, and a mode. No input: the newest plan in `docs/coordination/`. Three execution modes, and **the plan does not change between them** — only who reads it:
- **`--agents`** (default) — you spawn one sub-agent per track, each in its own worktree.
- **`--brief`** — you emit one self-contained brief per track for a human to paste into a session they start themselves, possibly on a different harness. You then act as coordinator across those sessions through the plan and the seam log rather than through delegation.
- **`--launch`** (opt-in, profile-qualified) — `coord-runner.py` prepares isolated tracks and launches qualified ACP (Claude/Codex/Grok/Copilot) or Agy native-stream workers. Any designated Owner can operate it. POSIX/Windows containment is platform-specific. Read `reference/launch.md` and, for Copilot, `reference/copilot.md`. Unknown instructions, hooks, trust or permissions block with a retained brief; ACP support is not qualification.

## Cast
- **Peers:** Orchestrator (coordinator). Track agents are the personas the plan names — they are *delegates*, not council members.
- **Adversaries at each join:** **Test Architect** (hard veto — a track's exit evidence is present and was *observed*, not asserted), **Simplifier** (soft veto — a track that grew past its plan entry is scope, not progress).

## Flow

**Stage 0 — Interdict the rush.** Do not spawn anything yet. Two checks first, because both failures are silent:
- **The layer is on.** `coord doctor` clean. If the registry is absent every path is `authored`, every derived file will conflict on every merge, and you are about to multiply that by the number of tracks — this blocks dispatch **unless** the prompt-named-division exception above applies, in which case it is a recorded risk, not a gate.
- **The plan matches the repo.** Every path the plan (or, under the exception, the prompt) assigns still exists and is still the class it says. A plan is a record of a measurement, and measurements go stale.

**Stage 1 — Qualify the delegation mechanism (per harness, before you rely on it).** The plan records what each harness is qualified to do. Re-check it here, because you are about to depend on it. What a track needs is exactly three things: **its own tree**, **a stated division of responsibility**, and **a receipt back**. Record each harness dimension as `enforced` (the mechanism cannot be bypassed), `observed-only` (you can see a violation, not prevent it) or `unsupported`. **A missing mechanism never becomes success-shaped permission**, and there is no automatic fallback from enforced to observed: if a track needs a boundary the harness cannot hold, either run that track in `--brief` mode, or make it serial. Where you cannot verify a mechanism first-hand, it is `unsupported` — not "probably fine".

**Stage 2 — Open each track.** One tree per session, one session per tree (WT3); a new task means a new session even in the same tree (WT1a — a worktree isolates the tree, nothing isolates the context).
```
coord worktree new --branch <work-name> --session <track-id>   # named for the WORK, not the session (WT5)
# then, INSIDE the new tree:
coord doctor                                                   # read the inherited state back
```
**Never install from inside a worktree** — `coord install` refuses there, and the refusal is the point. A linked worktree shares `.git/config` *and* `.git/hooks` with its parent, so it already carries the drivers and the pre-commit floor; an install there does not add a registration, it **overwrites the repository's** with a path inside a tree that WT8 cleanup will delete. The install belongs in the **primary checkout, once per clone**. `coord doctor` in the new tree confirms the inherited registration, and reports `COORD-DRIVER-PATH-FOREIGN` if some earlier session already repointed it.

**Stage 3 — Dispatch with a contract, never a topic.** Before dispatch, refuse a compiled prompt whose `dispatchable` is false or whose text still carries an unanswered `DR-n` line — stop with `decision request unanswered: DR-n` (CO-S0). Every delegation carries, explicitly (GO7, class CTX-F):
- **the `start` line first** — `audit-log.py start --session <track-id> --skill <skill>` is the brief's first command, before grounding, so the node's duration is measured from the right instant (DC-190; the host's `SessionStart`/`SubagentStart` hook marks the seam as a backstop);
- **the exact goal and its done-when** — the track's plan row, verbatim; a row that names a control's trigger **quotes the ADR line**, never a paraphrase (DC-189);
- **the authored paths it owns**, and the statement that `derived`/`register` paths need no claim — **a `register`-class artifact is never claimed**: append with a placeholder id where the allocator is the join's, and commit (`coord claim` refuses it, DC-163);
- **claim for the minutes of the edit** — the default TTL, released at once; never `--ttl 3600` for the node's lifetime (DC-163); a genuinely long edit passes `--long-edit <reason>`;
- **absolute paths only, no `EnterWorktree`/`ExitWorktree`** — the tool refuses when the session cwd is the primary and a background node waited 8,143 s for the refusal (F-25, SP-23);
- **a multi-line program is a file, then a run**, and **a gate's status is never behind a pipe** (CT27; SP-25 / SP-24);
- **tier, fan-out cap, a per-branch budget** — tool calls, tokens, wall clock — **and a context ceiling** (400k tokens unless the plan says otherwise) with its hand-off rule: at the ceiling the node splits the slice or `/compact`s rather than continuing (F-14 ext.; SP-01 per node is the measurement);
- **a convergence condition** — what "enough" is, stated by you, because a research agent's natural exit is "enough evidence" and nobody defined it;
- **the exit evidence** it must return;
- **"not in scope"**, naming the neighbouring work it will be tempted by.

A budget with no convergence condition is a timer, not a contract. **A budget firing is a defect signal, not a termination argument** (GO9): when one fires, ask why the estimate was wrong before you raise it.

**A resume message carries the `start` line.** A node resumed by message is a second run nobody marked: six resumed nodes' audit durations stopped at the first run (up to 60,524 s unmeasured, AC-09). The resume brief's first line is the same `audit-log.py start --session <track-id> --skill <skill>`; the profiler's SP-26 flags a resumed node whose span outran its measurement.

**Stage 4 — Compose through seams, never through shared files.** When track B needs something from track A, it records a **seam request** (`coord request add`); A resolves it on its own cadence (`coord request resolve`). Neither blocks, and the seam is recorded rather than negotiated inside a merge. Two tracks that need to edit one authored file do not need a lease — they need a boundary correction, and that is a decision only you make.

**Stage 5 — Coordinate: the loop, with its termination variant.** Until every track has returned its exit evidence or been stopped:
1. Collect what returned. **Verify the exit evidence — do not accept the claim** (E14/E16: read the state back; a delegate's inventory is not fact until spot-checked).
2. Resolve open seam requests, oldest first.
3. Decide the things only you can: a scope change, a boundary correction, a conflicting recommendation between two tracks, a track that wants to enlarge its authority.
4. `coord metrics` — refused decisions and edits outside a lease are the signal that **the division is wrong**, not that the tracks are careless.
5. **Owner review is a ruling, never an acceptance (D6, CO1).** When a track raises a decision request (`coord decide request` — the five fields, a deadline and a fallback, P1's request plus a `decision-request` mail), the Owner seat answers it with `coord decide rule next --title "…" --text "…" --request <req-id>`: the next numbered heading is appended to `docs/notes/rulings.md`, the request is resolved with `Ruling NN`, and the requester is mailed. The requester never rules on its own request (`COORD-RULING-SELF`). A track's `Stop` is refused by `owner-review-gate.py` while a decision request it sent is open — that refusal is the loop's signal, not an error to work around; `coord decide list` shows what is open and what was ruled (`NOT CHECKED` over an empty corpus).

**Termination variant:** the number of tracks with unreturned exit evidence, which must strictly decrease. If it does not decrease across two passes, the loop is not converging: climb the **kick ladder** (CO17 — `coord kick`, then a decision request at rung 2, the human at rung 3), never the paragraph. A plan that cannot converge is a finding.

**Stage 6 — Converge.** Merge in dependency order — upstream first, downstream rebases. **The join is the script and nothing else:**
```
python3 docs/ai-forward-pack/scripts/conductor-join.py <branch> --title "<merge title>" --audit-shortname join-<track> --audit-summary "<what landed>" --audit-goal "<goal>" --audit-done-when "<done when>"
```
Before the join, `python3 docs/ai-forward-pack/scripts/verify-ruling-citations.py` is green: a `Ruling NN` cited anywhere under `docs/`, `pack/`, `.agents/log/`, `.github/`, `.claude/` with no heading in `docs/notes/rulings.md`, or a number defined twice, blocks the join (class ID-A).

The join's steps and their gates are `reference/join.md` — read at this stage, never by re-invoking the skill (CTX-E).

**Never remove a worktree to resolve a conflict** (WT11). If two tracks collided, deleting one side destroys the evidence of what collided.

**Stage 7 — Report.** Planned vs actual, per track: budget vs spend, seam requests raised and resolved, boundary corrections made, and **which of the plan's parallelism justifications actually paid**. That comparison is the input to the next plan, and without it the next division is drawn from a feeling again.

## Definition of done (exit gate)
- [ ] A plan existed and parsed, **or** the prompt already named the tracks, owners and paths (the stated-division exception) and that naming is recorded as the division used.
- [ ] `coord doctor` was run **before** dispatch; the layer was clean, **or** (under the exception) a registry gap was recorded as a mitigated risk rather than blocking dispatch.
- [ ] Harness delegation capability was qualified per dimension; nothing unverified was used as though enforced.
- [ ] Every track ran in **its own worktree**; the layer was installed **once, in the primary checkout**, and `coord doctor` in each tree confirmed the inherited registration.
- [ ] Every delegation carried goal, done-when, owned paths, tier, fan-out cap, budget, convergence condition, exit evidence and not-in-scope.
- [ ] Every returned exit evidence was **verified**, not accepted.
- [ ] Seam requests were used for cross-track needs; no file was authored by two tracks.
- [ ] The loop's termination variant strictly decreased, or the failure to converge was reported as a finding.
- [ ] Merged in dependency order **by `conductor-join.py`** — the conflict-marker gate first, one recount per join with `recount_seconds` recorded, the join entry at T1 / fan-out 0 with a measured duration; `coord regen` clean; the **integrated** gate set green through `run-verify-gates.py`.
- [ ] Every brief opened with the `start` line, claimed for the minutes of the edit, never claimed a `register`-class path, never called `EnterWorktree`, and carried a context ceiling with its hand-off rule; every resume carried `start`.
- [ ] Trees closed with `coord worktree cleanup`; nothing removed to resolve a conflict; every refusal reported with its reason.
- [ ] Planned vs actual recorded per track.
- [ ] Status table emitted.

## Documentation & discoverability (last action)
Append the planned-vs-actual section to the plan and run `python3 docs/ai-forward-pack/scripts/docs-graph.py derive`. A boundary correction that will outlive this run is a decision note (V17).

**Audit (last action).** `python3 docs/ai-forward-pack/scripts/audit-log.py append --shortname "coordinate-<plan-slug>" --session "<id>" --skill execute-with-coordination --kind skill --prompt "<verbatim>" --summary "<tracks run, seams resolved, planned vs actual>" --artifact docs/coordination/<plan-id>.md --goal "<goal>" --done-when "<done when>" --tier T2 --fan-out <the plan's cap> --agent-run "<track>|<start-iso>|<end-iso>|<calls>/<budget>"` (one per track - the budget half is what makes an over-run a finding without a profiling pass; `audit-log.py selfcheck` reads both).

**Handoff:** → `/session-profiler` (did the division pay?) · → `/dream` (a recurring boundary correction is a class) · → `/prepare-for-coordination` (re-plan when the variant stopped decreasing).
