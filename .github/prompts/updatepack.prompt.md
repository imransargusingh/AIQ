---
mode: agent
description: Update an installed AI-Forward Pack to the latest revision from a local ai-forward clone — read the changelog, run pack-apply.py plan then apply (managed blocks re-pasted, stale copies removed, CLAUDE.md converted to the @AGENTS.md import, parity controls rewritten, repo-local deviations three-way merged), reconcile what the program reports, and summarise every action in a table before offering to commit and push.
---
You are running the **updatepack** workflow (`knowledge/rigor-protocol.md` applied to the update delta). The **Release Engineer** leads (owns the installation path; the program backs up and reports, it never destroys — resolving a conflict by discarding repo text is a BLOCK); the **Documentation Steward** reviews every artifact landing (correct destination, managed-block integrity, V10); the **Tech Lead** guards against over-application (the program applies the deployment map and only that). **Tier: T1 · Fan-out cap: 0.**

**Ground:** `python docs/ai-forward-pack/scripts/audit-log.py start --session <id>`. Locate the pack source (explicit path, `AI_FORWARD_PACK`, or a sibling `ai-forward`/`AI-Forward`); read its `revision`, `bundle_version` and `changes` from `pack/adapters/INSTALL.md`; read the installed `revision` from `docs/ai-forward-pack/INSTALL.md` (absent → redirect to /addpacktorepo). Read the repo's deviation register if it has one. Delta 0 → report current and stop; negative → surface the anomaly.

**OPEN (plan):** `python3 <pack-source>/pack/scripts/pack-apply.py plan --target . --quiet` (the SOURCE's copy, never the target's: the deployment map IS this program, so a revision that changes it cannot apply itself - measured, a rev-63 copy re-proposed two .gitignore lines its repo had declined; --source defaults to the clone shipping the script). Read every row: ADD · UPDATE · MERGE (deviation carried over) · KEEP · REMOVE (stale copy after a load-scope move) · CONVERT (CLAUDE.md → `@AGENTS.md` + addendum; backup under `docs/ai-forward-pack/retired/`; unique paragraphs retained) · REWRITE (parity control → new-invariant shim; original backed up) · CONFLICT (new text parked under `docs/ai-forward-pack/conflicts/`) · REVIEW · SKIP. A dry-run request ends here.

**INTERROGATE:** decide every CONFLICT and REVIEW row before applying — which repo-local addition survives and where it now belongs (a split skill carries its stage text in `reference/flow.md`); for CONVERT, note that retained paragraphs belong in `AGENTS.md` above its block.

**EVIDENCE:** `python3 <pack-source>/pack/scripts/pack-apply.py apply --target .`; reconcile each CONFLICT by merging the repo-local text into the parked new text and deleting the parked file; move retained CLAUDE.md paragraphs into AGENTS.md; apply any remaining non-file `deploy` directive from the changelog literally; delete the `retired/` backups once reviewed.

**DISCONFIRM:** `pack-doctor.py` green (WARNs explained — `copilot settings` is a user-level choice); `git diff --stat` shows no `docs/docs-index.js`, one managed block per front door, `CLAUDE.md` starting with `@AGENTS.md`, no `.instructions.md` for a doc now in `.github/knowledge/`, nothing under `conflicts/`; the repo's front-door controls green. Enact the adversary round inline (labelled critiques, **[Blocker|Major|Minor|Nit]**, Release Engineer PASS/BLOCK with the veto-clears-when predicate).

**CONVERGE:** paste the program's table, state `revision <from> → <to> (<bundle_version>)`, name new skills/docs, then ask: *"Would you like me to stage, commit, and push? Proposed: `chore: update AI-Forward Pack to revision <N> (<bundle_version>)`"*. On confirmation stage the pack surfaces explicitly (never `-A` in a repo that runs several worktrees off one checkout), commit, push.

**Audit (last action):** `audit-log.py append --shortname "updatepack-r<to>" --session "<id>" --skill updatepack --kind command --prompt "<verbatim>" --summary "<from → to; rows; conflicts reconciled>" --goal "<…>" --done-when "<…>" --tier T1 --fan-out 0`.

After every hand-reconciled CONFLICT, run `python3 docs/ai-forward-pack/scripts/verify-no-conflict-markers.py` on its own line before anything else reads the tree (DC-136; a derived file is regenerated, never resolved), and again at the gate.

${input}
