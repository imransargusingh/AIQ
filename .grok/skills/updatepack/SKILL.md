---
name: updatepack
description: Update an installed AI-Forward Pack to the latest revision — reads the pack source INSTALL.md from a local ai-forward clone, diffs the installed vs source revision, applies the deployment map mechanically with pack-apply.py (managed-block re-paste, stale-copy removal, the CLAUDE.md import conversion, parity-control retirement, three-way merges over repo-local deviations), and produces a tabular action summary before offering to commit and push. Run this from the repo that already has the pack installed.
runs_as: Coordinator
---

# /updatepack — pull the latest AI-Forward Pack into an installed repo

The pack evolves, and sometimes the *shape* of the install changes — a doc's load scope moves, `CLAUDE.md` becomes an import, a control that encoded the old invariant must retire. This skill is the **single safe path for refreshing the pack** in a repo that has already had it installed. Since revision 61 the mechanical half is a program, `docs/ai-forward-pack/scripts/pack-apply.py` (deployed with the pack; the source clone carries it as `pack/scripts/pack-apply.py`), and the skill's job is what a program cannot decide: reading the changelog for the *why*, reconciling the conflicts the program reports, and confirming the commit. **Nothing in the deployment map is remembered by a person any more** — a step that was forgotten in one repo becomes a row the program prints in every repo.

**Spine:** the Rigor Protocol (`knowledge/rigor-protocol.md`) applied to the update delta — every change is named by the changelog, applied by the program, and verified by its report before it is called done. **Cast:** the **Release Engineer** leads (owns the installation path and the "no silent overwrites" rule — the program backs up and reports, never destroys); the **Documentation Steward** reviews every artifact landing (correct destination, managed-block integrity, V10 protection); the **Tech Lead** guards against over-application (the program applies the deployment map, and only that). **Tooling:** the **source clone's** `pack/scripts/pack-apply.py plan|apply --target .` (never the target's own copy — a program that deploys itself cannot be trusted to deploy its own fix; `STALE-APPLIER` is the backstop when someone does), `pack-doctor.py`, `git diff --stat`.

## Grounding (first action)

CO-S0 applies first — the sentence is `reference/co-s0.md`. `python3 docs/ai-forward-pack/scripts/audit-log.py start --session <id>` (IO1). The source of truth for *what changes and why* is `INSTALL.md`'s **`changes` frontmatter in the pack source**; the *baseline* is the target repo's **installed revision** (`docs/ai-forward-pack/INSTALL.md`). Read both before running anything. If the target lacks `docs/ai-forward-pack/INSTALL.md`, this is a fresh install — redirect to `/addpacktorepo` (which runs the same program with `--install`). Read the target's **deviation register** if it has one (`scripts/test-pack-deviations.py` or similar): it names the repo-local additions the program three-way merges and you must see land.

## Input

The path to the local AI-Forward repository (from `$ARGUMENTS` or the user's message). Locate it in this order: (1) an explicit path; (2) `AI_FORWARD_PACK`; (3) a sibling `ai-forward` / `AI-Forward` directory. If none holds `pack/adapters/INSTALL.md`, ask.

## Modes — dry-run & idempotency
- **Dry-run (preview).** `dry run`, `--dry-run`, `preview` or `what would change` → run the **source's** `pack-apply.py plan` and stop after the table. `plan` writes nothing.
- **Idempotent by construction.** `apply` on a current target reports `already at revision N` and changes nothing; every action is a wholesale copy, a wholesale managed-block re-paste between markers, or a three-way merge — never an append — so re-running never duplicates a block, never re-converts `CLAUDE.md`, and never advances past the source revision. An interrupted update is re-run to completion.

## Stages

**Stage 0 — interdict the rush.** Do not copy files by hand and do not diff directory trees: the changelog says *what changed*, the program applies the *map*. It prevents overwriting accumulated artifacts (`docs/docs-index.js`, V10 — the program never touches it) and a missed managed-block re-paste (the program always re-pastes) — plus the three that used to be remembered: the stale wrapped copy of a re-scoped doc, the `CLAUDE.md` import conversion, and a parity control that asserts the old invariant.

**Stage 1 — OPEN (read both revisions, then plan).**
- Read the source `revision`, `bundle_version` and `changes`; read the installed `revision`; `delta = source − target`. `delta = 0` → report current and stop. `delta < 0` → anomaly; surface and ask.
- **Run the SOURCE's copy, not the target's** — the whole of the bootstrap discipline. The deployment map **is** this program, so a revision whose point is a change to `pack-apply.py` cannot apply itself: the target's copy would compute the plan and the new copy would be merely one of the files it copies. Measured — a repo at revision 63, planning against revision-64 source, proposed re-appending two `.gitignore` lines it had explicitly declined. Running the source's copy makes that **impossible rather than detected**, and `--source` defaults to the clone shipping the script so the two can never be mismatched.
- Run the preview: `python3 <pack-source>/pack/scripts/pack-apply.py plan --target . --quiet`. Read the table. Every row is one action the program will take: `ADD` · `UPDATE` (the repo had the pack's old text verbatim) · `MERGE` (a repo-local deviation carried over by three-way merge against the pack text at the installed revision) · `KEEP` (a deviation over an unchanged pack file) · `REMOVE` (a stale copy whose load scope moved) · `CONVERT` (`CLAUDE.md` → `@AGENTS.md` + addendum, previous file backed up under `docs/ai-forward-pack/retired/`, unique paragraphs retained) · `REWRITE` (a parity control rewritten into the new-invariant shim, original backed up) · `CONFLICT` (a deviation the merge could not carry; the new pack text is parked under `docs/ai-forward-pack/conflicts/`) · `REVIEW` (a non-PowerShell control that asserts the old invariant) · `SKIP` (protected).

**Stage 2 — INTERROGATE (the rows that need a person).**
For each `CONFLICT` and `REVIEW` row, and for each `changes` entry whose `deploy` names a non-file directive the program does not implement, decide *before* applying: which repo-local addition must survive (the deviation register is the list), and where it now belongs (a skill that was split into `SKILL.md` + `reference/` carries its stage text in `reference/flow.md`, so an addition to a stage moves there). For a `CONVERT` row, read the retained paragraphs the program lists: they belong in `AGENTS.md` above its managed block (both hosts read it), and the program leaves them in `CLAUDE.md` only so nothing is lost. The Release Engineer holds a hard veto on resolving a `CONFLICT` by discarding the repo's text.

**Stage 3 — EVIDENCE (apply, then reconcile).**
1. `python3 <pack-source>/pack/scripts/pack-apply.py apply --target .` — it applies the map, backs up before every conversion, merges deviations, removes stale copies, re-pastes both managed blocks, merges `.claude/settings.json`, writes `.github/hooks/ai-forward.json`, writes `.grok/{skills,agents,hooks,rules}`, writes `.agents/{skills,hooks,rules}` and `.agents/skills.json`, adds the `.gitignore` lines (`!.agents/log/` — ledgers tracked, D10; `.agents/mail/` ignored), places the hook adapters (`reread-guard`, `session-start`, `mail-doorbell`; `heartbeat` and `owner-review-gate` are named by the merged host entries — confirm they landed, else copy them from `<pack-source>/pack/adapters/hooks/`), advances the installed revision, and records the three `context-budget` baselines (`gate`, `prefix`, `skills`) for this repo.
2. Reconcile every `CONFLICT` row by hand: merge the repo-local addition into the new pack text (from `docs/ai-forward-pack/conflicts/<path>`), write the result to its destination, delete the parked file. Update the deviation register entry if the addition moved file. **Then, before anything else reads the tree, `python3 docs/ai-forward-pack/scripts/verify-no-conflict-markers.py`** on its own line — it is the first check of every resolution, because a marker left in a hand-resolved file is legal text in almost every format and survived a skimmed diff once (DC-136); a derived file is regenerated, never resolved.
3. Move any paragraphs `CONVERT` retained from `CLAUDE.md` into `AGENTS.md` and remove them from `CLAUDE.md`; delete the `retired/` backups once the diff is reviewed (they exist for the review, not the commit).
4. Apply any remaining non-file `deploy` directive from the changelog literally, and log it like any other action.
5. Run the deployed `context-budget.py gate --update-baseline` again only if step 2 changed an always-on doc.

**Stage 4 — DISCONFIRM (the gate).**
- `python3 docs/ai-forward-pack/scripts/pack-doctor.py` — every check PASS, or WARN with a reason you can state (`copilot settings` is a user-level choice, not an install defect); its `doorbells` line names each harness `verified · observed-only · unsupported`; existing ledgers: `coord log portable <files>`.
- `python3 docs/ai-forward-pack/scripts/verify-no-conflict-markers.py` — clean, on its own line (never behind a pipe, CT27).
- `git diff --stat` — `docs/docs-index.js` absent from the diff; exactly one `AI-FORWARD-PACK:BEGIN` in `AGENTS.md` and one in `CLAUDE.md`; `CLAUDE.md` begins with `@AGENTS.md`; no `.instructions.md` remains for a doc now under `.github/knowledge/`; no file left under `docs/ai-forward-pack/conflicts/`.
- Run the repo's own controls that touch the front doors (the rewritten parity shim, the deviation register): green, or the reason is in the report.
- The Release Engineer vetos reporting "done" while a `CONFLICT` or `REVIEW` row is unresolved.

**Stage 5 — CONVERGE (summary + commit offer).**
Paste the program's final table (`pack-apply.py apply … --quiet`, so `UNCHANGED` rows are hidden) as the action summary, then the line **Pack updated: revision `<from>` → `<to>` (`<bundle_version>`).** Name any new skills or knowledge docs so the user can orient. Then ask:
> "Would you like me to stage, commit, and push these changes?
> Proposed message: `chore: update AI-Forward Pack to revision <N> (<bundle_version>)`"

On confirmation stage the *pack surfaces* (`.claude/`, `.github/`, `.grok/`, `.agents/`, `docs/ai-forward-pack/`, `docs/index.html` if new, `AGENTS.md`, `CLAUDE.md`, `.gitignore`, the rewritten control and the reconciled files) — not `-A` in a repo that runs several worktrees off one checkout — commit, and push.

## Documentation & discoverability (note)
A pack-lifecycle skill: it operates on the installation, not the product, so it writes no frontmatter and never syncs `docs/docs-index.js` (V10 does not apply). Its durable record is the advanced `revision`, the `retired/` backups reviewed and removed, and the commit.

## Definition of done
- [ ] Both revisions read; delta computed; anomalies surfaced before any write.
- [ ] `plan` run and read before `apply`; every `CONFLICT` / `REVIEW` row decided by a person, none resolved by discarding repo text.
- [ ] `apply` run; the table shows the managed blocks re-pasted, stale copies removed, `CLAUDE.md` in the import form, parity controls rewritten, baselines recorded.
- [ ] `docs/docs-index.js` untouched; no duplicate block; nothing left under `conflicts/`; `retired/` reviewed and cleaned.
- [ ] `pack-doctor.py` green (or WARNs explained); the repo's front-door controls green.
- [ ] Summary table pasted from the program; commit offered with the exact message; the user's decision honoured.
- [ ] Dry-run requests produced the table with zero writes; a normal run is idempotent.

**Audit (last action).** `python3 docs/ai-forward-pack/scripts/audit-log.py append --shortname "updatepack-r<to>" --session "<id>" --skill updatepack --kind command --prompt "<verbatim>" --summary "<from → to; N rows; conflicts reconciled>" --goal "<goal>" --done-when "<done when>" --tier T1 --fan-out 0`.

**Handoff:** if the update added skills or knowledge docs, name them; if it converted `CLAUDE.md`, say where the retained paragraphs went; if `pack-doctor` warned on `copilot settings`, say it is the user's per-phase choice (WT1a).
