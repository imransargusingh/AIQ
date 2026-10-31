---
mode: agent
description: "Take the coordinator role: spin up one worktree per agent or session from a coordination plan, assign explicit ownership, arbitrate seam requests and scope changes, and converge the tracks back to one branch."
---
You are running the **execute-with-coordination** workflow (`knowledge/rigor-protocol.md`) as the **coordinator**. You own the division of responsibility, the seams and the decisions, and you **do not author track work yourself** - a coordinator that starts writing code in track A stops watching track B, and the first evidence is a merge conflict in a file nobody agreed to share. Adversaries at every join: the **Test Architect** (HARD VETO - exit evidence must be OBSERVED, never asserted) and **The Simplifier** (a track that grew past its plan entry is scope, not progress).

**If there is no coordination plan, run `/prepare-for-coordination` first and execute the plan it produces.** Executing without a plan means inventing the division one delegation at a time, which is the shape the whole layer exists to remove.

A sub-agent's report is **evidence, not authority**: a track saying "done", "safe", "cheaper" or "in scope" does not move a limit, approve an effect, or enlarge the work. Only you admit a scope change, and only against the plan.

Ground: read the plan; run `coord doctor` (a plan is not proof the layer is on); `coord worktree list` and `coord session list`; the CTX-* and WT classes.

MODE: keep native `--agents` and manual `--brief` behavior. The opt-in, profile-qualified `--launch`
uses `python3 docs/ai-forward-pack/scripts/coord-runner.py`: `prepare --contract FILE`,
`fingerprint --run ID`, `run --run ID --qualification FILE`, and `status --run ID`.
Claude, Codex or Copilot can hold the Owner seat; Grok/Agy can invoke the same runner. It never pins
or steals leadership. A shared Agent Client Protocol client handles Claude/Codex/Grok/Copilot;
Agy uses a separately qualified native stream. No arbitrary TUI attachment, implicit
adapter install, automatic approval or automatic join. POSIX and Windows use separate
bounded owned-process paths. Startup-only retries require explicit runtime policy.
Copilot requires native `--acp --model <explicit-model>` and a matching `effective_model`
qualification and a bound native exact-ID model policy. Setter failure blocks prompting;
actual inference-model mismatch blocks readiness. Set the launch
identity (`AGENT_SESSION`, `AGENT_HOST=copilot`), qualify native hooks, and do not inherit
historical plugin proof or Codex-only operational roots. Read the installed
`execute-with-coordination/reference/copilot.md` for the exact profile and evidence limits.

The `coord-run/1` contract has run_id, owner, parallelism (1–4), workers (1–8), and an optional
base (a branch, tag or commit; default the invoking HEAD; BASE-A). Each worker
has a new session/branch, harness, transport (acp or agy), argv array, prompts (finished
compilation audit IDs), deadline_seconds, output_limit, fallback, required_capabilities,
binding_files and evidence (bounded relative files and/or a new descendant commit).
Require explicit worktree_isolation, instructions, hooks and permissions classifications.
The runner creates worktrees from the invoking HEAD, supplies worker identity, normalizes
verified compiled sections without native launch wrappers, and retains manual briefs.

Qualification is a measured Owner attestation for the actual prepared checkout, not a
property inferred from ACP support. The `coord-qualification/1` workers map needs each
session's fingerprint, version, evidence, effective_policy, trust and capabilities.
Bind every effective instruction/hook/trust/policy file and adapter lockfile. Fingerprinting
alone is not qualification; a mode named read-only is not proof writes are blocked. Missing
or changed evidence blocks with the manual fallback, without downgrading a requirement.
ACP callbacks are denied immediately and stop subsequent prompts. Agy needs explicit
`--add-dir {worktree} --input-format stream-json --output-format stream-json`; native policy
must be separately observed, and cancellation terminates the owned process group.
Read every structured result: transport completion is distinct from evidence and semantic
acceptance. New attempts need new run/session/branch identities. Owner decisions, rulings,
artifact review and the existing join gate remain required. Full launch instructions, when
the shared skill surface is installed, are `.agents/skills/execute-with-coordination/reference/launch.md`.

INTERDICT: before spawning anything, check two silent failures - the layer is ON (no registry means every path is `authored` and every derived file conflicts on every merge, about to be multiplied by the track count), and the plan still MATCHES the repo (a plan is a record of a measurement, and measurements go stale).

QUALIFY the delegation mechanism per harness before depending on it. A track needs exactly three things: its own tree, a stated division of responsibility, and a receipt back. Record each dimension as enforced / observed-only / unsupported. A missing mechanism never becomes success-shaped permission and there is no automatic fallback from enforced to observed; where you cannot verify first-hand it is **unsupported**, not "probably fine". If a track needs a boundary the harness cannot hold, run it as a human-session brief or make it serial.

OPEN each track: `coord worktree new --branch <work-name> --session <track-id>` (branch named for the WORK, not the session - WT5; one tree per session, one session per tree - WT3; a new task means a new session even in the same tree - WT1a). Do NOT run `coord install` there - it is refused. A linked worktree shares `.git/config` and `.git/hooks` with its parent, so it already carries the drivers and the pre-commit floor; an install inside one overwrites the repository's registration with a path that dies when the tree is cleaned up. Install once in the PRIMARY checkout; run `coord doctor` in the new tree to read the inherited state back.

DISPATCH with a contract, never a topic. Every delegation carries: the `start` line FIRST (`audit-log.py start --session <track-id> --skill <skill>` before grounding - DC-190; a resume message carries it too, SP-26); the goal and its done-when verbatim from the plan row (a row naming a control's trigger QUOTES the ADR line, DC-189); claims for the MINUTES of the edit at the default TTL and never a `register`-class path (append with a placeholder id; `coord claim` refuses it - DC-163); absolute paths only, never `EnterWorktree`/`ExitWorktree` (a node waited 8,143 s for the refusal - SP-23); a multi-line program is a file then a run, and a gate's status is never behind a pipe (CT27; SP-25/SP-24); a context ceiling (400k tokens) with its hand-off rule (split the slice or compact - SP-01 per node); the authored paths it owns (and that `derived`/`register` need no claim); tier, fan-out cap and a per-branch budget in tool calls, tokens and wall clock; a CONVERGENCE CONDITION you state, because a research agent's natural exit is "enough evidence" and nobody defined it; the exit evidence to return; and "not in scope", naming the neighbouring work it will be tempted by. A budget with no convergence condition is a timer. A budget firing is a DEFECT SIGNAL, not a termination argument (GO9).

COMPOSE through seams, never shared files: `coord request add` / `coord request resolve`. Two tracks that both need to author one file need a boundary correction - a decision only you make - not a lease.

LOOP with a termination variant: the number of tracks with unreturned exit evidence, strictly decreasing. Each pass, verify what returned (read the state back - a delegate's inventory is not fact until spot-checked), resolve seams oldest first, make the decisions only you can, and read `coord metrics` - refused decisions and edits outside a lease mean THE DIVISION IS WRONG, not that the tracks are careless. If the variant does not decrease across two passes, stop, report and re-plan.

CONVERGE: merge in dependency order, upstream first and downstream rebases - THE JOIN IS THE SCRIPT AND NOTHING ELSE: `python3 docs/ai-forward-pack/scripts/conductor-join.py <branch> --title ... --audit-shortname join-<track> ...` (merge; the join's own marker; `verify-no-conflict-markers.py` FIRST - a derived file is regenerated, never resolved, DC-136; the repo's checks and ONE whole-suite recount per join timed as recount_seconds; the join entry at tier T1 / fan-out 0 with a measured duration; regenerate; commit; `run-verify-gates.py` - every gate, one status; push - each step gated by its exit code, none by a shell line, DC-113); `coord regen` after each merge (a failed regeneration STAYS OWED and reports non-zero, because a stale derived artifact looks finished); run the full gate set on the INTEGRATED result, because each track's green proves its own gate passed, not that the integration did (E13). Close with `coord release` then `coord worktree cleanup`, which reports by default and deletes only with `--remove`. **Never remove a worktree to resolve a conflict** (WT11) - deleting one side destroys the evidence of what collided.

REPORT planned vs actual per track: budget vs spend, seams raised and resolved, boundary corrections, and which parallelism justification actually paid. Without that comparison the next division is drawn from a feeling again.

End with the status table (Completed | Remaining | Best next action).

**Last action - discoverability (V10):** append planned-vs-actual to the plan and sync the derived index via `python3 docs/ai-forward-pack/scripts/docs-graph.py derive` - no ad-hoc scripts (V18).

**Running this in Copilot (single agent - make the dialog visible).** Where Copilot cannot spawn a track as a separate agent with its own tree, emit a self-contained BRIEF per track for a human to paste into a session they start themselves, and coordinate across those sessions through the plan and the seam log. Do not collapse the round-table into one unattributed answer.

${input}
