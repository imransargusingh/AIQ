---
name: apply-learnings
description: Push approved, generalised fleet learnings (promoted from /dream into the ai-forward learnings/ store) into one or more target repos, reconciling each against that repo's existing register so nothing is duplicated or contradicted. Produces a reviewable plan per repo — never merges, never executes.
runs_as: Coordinator
---

# Skill: /apply-learnings

**Push** approved, generalised **fleet learnings** — the control-bearing classes promoted from `/dream` into the ai-forward `learnings/` store — into one or more **target repos**, **reconciling** each against that repo's existing directives and defect-class register so nothing is duplicated or silently contradicted. It is the **push** half of federation (the pull half is `/updatepack`) — NASA's *Disseminate → Apply* and CI8, **automatically and safely**.

**The safety posture is the whole point:** `/apply-learnings` produces a **reviewable plan/diff per target repo** — it **never merges** and **never executes** anything in a target (the stop is a message — CO-S2) (spec-dreaming US-5; ADR-0002/0004/0005). A human reviews each plan and applies it by hand or via a PR.

**Spine:** the Rigor Protocol on the *distribution* — Stage 3 establishes each target's existing state, Stage 4 the Simplifier/Security check that nothing leaks or duplicates. **Authority:** `spec-dreaming` (US-5), `architecture-dreaming` (Federation context), ADR-0002 (two federation paths), ADR-0004 (abstraction + the five guards). **Lead:** the maintainer, with **Privacy & Data Governance** (hard veto — nothing personal/specific crosses a repo boundary) and the **Simplifier** (no duplicate classes).

## Grounding (first action)
CO-S0 applies first — the sentence is `reference/co-s0.md`. Load and treat as authoritative: the fleet store `learnings/fleet-classes.jsonl`, `spec-dreaming` US-5, ADR-0002/0004, and — for each target — that repo's `docs/lessons/defect-classes.md`. **Open the target's register; do not assume its contents** (E15). A target without the pack installed is skipped with a note.

## Input
The target repos: `--repos <path,path,…>` or `--repos all` (all sibling repos of ai-forward that have the pack). What is pushed = the approved *general* learnings already in the fleet store (put there by `/dream`'s `apply-decisions`).

**Or a Dream Manifest** — a persisted **learnings×repos targeting/record matrix** (`--manifest <file>`, ADR-0006) when you want *this* learning to go to *these* repos (not a broadcast) and a durable record of what happened. Compose it in a UI, then apply it.

## Flow — broadcast (`--repos`)
1. **Load + de-dup the fleet learnings** (latest wins per class slug).
2. **Per target repo, reconcile each learning** (slug-exact + signature-overlap; **no fuzzy index** — the Simplifier's call, ADR-0004):
   - **add** — no equivalent in the target → the plan contains the exact register entry to add.
   - **merge** — the target already has an equivalent class (slug/id or ≥0.6 signature overlap) → the plan says *append the instance / upgrade the control*, **never add a duplicate** ("one quantity, two homes").
   - **conflict** — the incoming class contradicts an existing directive → **surfaced in the plan for human resolution**, never overridden.
   - **skip** — no control (a lesson without a control is a memoir — CI6), or a taint/scrub hit at the boundary, or no pack in the target.
3. **Re-run strip + scrub at the boundary** (defence in depth — nothing specific/personal crosses, ADR-0004 G5).
4. **Write a plan per repo** to `learnings/plans/<repo>.plan.md`, print a summary (`add N, merge N, conflict N`), and **offer it** as `coord decide request --to <the target's human-seat session>` with the five fields, `--deadline default` and `--fallback` "plan stays unapplied" — the merge is never automatic.

```bash
python3 docs/ai-forward-pack/scripts/apply-learnings.py push --repos "../repo-a,../repo-b"
# or, for every sibling repo that has the pack:
python3 docs/ai-forward-pack/scripts/apply-learnings.py push --repos all
```

## Flow — targeted (`--manifest`, the Dream Manifest — ADR-0006)
When you want per-learning targeting and a durable rollout record instead of a broadcast:
1. **Scaffold** the matrix from the fleet store (+ a self-contained compose HTML):
   ```bash
   python3 docs/ai-forward-pack/scripts/apply-learnings.py manifest-init --repos "../repo-a,../repo-b" [--dream drm-0007]
   ```
   Every fleet class is assigned to every repo by default (`scope:all`, `status:pending`).
2. **Compose** in the HTML matrix (`learnings/manifests/<id>.html`): toggle each *(learning, repo)* cell to target a learning at only the repos it belongs in (a C# class → the .NET repos, not the Python one), then **Export the manifest JSON** over the scaffold.
3. **Apply** — reconcile **per assignment** (a learning only into its `targets`), write a plan per repo, and **record the outcome back** into the manifest's `status` map; the HTML re-renders in read-only **rollout** mode:
   ```bash
   python3 docs/ai-forward-pack/scripts/apply-learnings.py push --manifest learnings/manifests/<id>.json
   ```
Still **never a merge** — the plan is the artifact; the human applies it. **Manifests name repositories → they are local-only** (excluded from the published site; ADR-0006), consistent with keeping raw dreams and the audit log local while only the *abstracted* fleet classes are public.

## The other federation path (pull)
General, control-bearing classes that belong in the pack's own always-loaded discipline are promoted into the pack via `/extendaibundle` (into `continuous-improvement.md` / the seed register). Any repo then **inherits them by running `/updatepack`** — the deployment map ships the class + its control. `/apply-learnings` is **targeted, immediate**; `/updatepack` is **broad, pull-based**.

## Definition of done
- [ ] A **reviewable plan** was produced per target repo; **nothing was merged or executed** in any target.
- [ ] Each learning was classified **add / merge / conflict / skip** by reconciling against the target's **actual** register (opened, not assumed).
- [ ] An equivalent existing class produced a **merge** instruction, never a duplicate; a contradiction produced a **surfaced conflict**, never a silent override.
- [ ] **Strip + scrub ran at the boundary**; no path/name/value/secret/PII crossed; a control-less learning was skipped.
- [ ] Targets without the pack were skipped with a note.
- [ ] If a **manifest** was used: targeting was **per assignment** (a learning only into its `targets`), **status was recorded back** into the manifest, and the manifest stayed **local-only** (not published).

## Documentation & discoverability (last action)
The plans are **data** under `learnings/plans/`. The push appends an audit entry automatically. If distributing a learning settles a cross-repo decision, capture a decision note (V17).

**Handoff:** review each `learnings/plans/<repo>.plan.md` → apply by hand or open a PR in the target → the target's next skill run reads the new class at grounding (CI5).
