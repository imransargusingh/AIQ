---
mode: agent
description: Push approved, generalised fleet learnings (promoted from /dream into the ai-forward learnings/ store) into one or more target repos, reconciling each against that repo's existing register so nothing is duplicated or contradicted. Produces a reviewable plan per repo — never merges, never executes.
---
You are running the **apply-learnings** workflow — the **push** half of federation (the pull half is **/updatepack**). It operationalises NASA's Disseminate→Apply and CI8 automatically and **safely**: it produces a **reviewable plan/diff per target repo** and **never merges** and **never executes** anything in a target (`spec-dreaming` US-5; ADR-0002/0004/0005). Authorities: `spec-dreaming`, `architecture-dreaming` (Federation context), ADR-0002 (two paths), ADR-0004 (abstraction + the five guards). Privacy & Data Governance holds a hard veto (nothing personal/specific crosses a repo boundary); the Simplifier guards against duplicates.

**Ground first:** read `learnings/fleet-classes.jsonl` (what will be pushed — the approved general learnings put there by `/dream`'s apply-decisions), and — for each target — **open** that repo's `docs/lessons/defect-classes.md` (do not assume its contents, E15).

**Run:** `python docs/ai-forward-pack/scripts/apply-learnings.py push --repos "<path,path,…>"` (or `--repos all` for every sibling repo that has the pack). For each target it reconciles each learning (slug-exact + signature-overlap; **no fuzzy index** — ADR-0004) into:
- **add** — no equivalent → the plan carries the exact register entry to add;
- **merge** — the target already has an equivalent class → *append the instance / upgrade the control*, never a duplicate ("one quantity, two homes");
- **conflict** — contradicts an existing directive → **surfaced for human resolution**, never overridden;
- **skip** — no control (a lesson without a control is a memoir, CI6), a taint/scrub hit at the boundary, or no pack in the target.

Strip + scrub run again at the boundary (defence in depth). Plans are written to `learnings/plans/<repo>.plan.md` and an audit entry is appended. Each plan is offered as `coord decide request --to <the target's human-seat session>` with the five fields, `--deadline default` and a fallback — the merge is never automatic (CO-S2).

**Targeted alternative — the Dream Manifest (`--manifest`, ADR-0006):** when you want per-learning targeting and a durable rollout record instead of a broadcast, scaffold a **learnings×repos matrix** with `manifest-init --repos "<paths>" [--dream <id>]` (writes `learnings/manifests/<id>.json` + a self-contained compose HTML), toggle each *(learning, repo)* cell in the HTML and Export the JSON, then `push --manifest learnings/manifests/<id>.json` — it reconciles **per assignment** (a learning only into its `targets`), writes a plan per repo, and **records the outcome back** into the manifest's status map (the HTML re-renders in read-only rollout mode). Still never a merge. **Manifests name repos → local-only** (never published), consistent with keeping raw dreams and the audit log local while only the abstracted fleet classes are public.

**Then:** review each `learnings/plans/<repo>.plan.md` and apply by hand or via a PR in the target — nothing was merged. Use **/updatepack** in a target for broad, pull-based inheritance of general classes shipped into the pack.

${input}
