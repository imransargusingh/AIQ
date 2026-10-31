---
name: extendaibundle
description: Extend the AI-Forward pack itself from a prose prompt — add a new skill, knowledge doc, template, or script — running collectknowledge → specify → design → implement under the covers, specialized for the reality that pack work is writing Markdown and scripts. Bakes in the consistency discipline (scaffold via tools/new-capability.py, prove via tools/verify-bundle.ps1) so any contributor extends the bundle the same way, with zero drift, fit for both Claude Code and GitHub Copilot. Run it from an AI-Forward clone with a description of the capability you want.
runs_as: Coordinator
---

# Skill: /extendaibundle

Turn "I want the pack to be able to **X**" into a correctly-built, fully-wired, drift-free extension of the AI-Forward pack — without the contributor needing to remember the dozen places a new capability has to touch. This skill runs the macro-loop (**collectknowledge → specify → design → implement**) *compressed for pack work* — where the artifacts are Markdown (skills, knowledge, managed blocks, docs) and scripts, the "tests" are eval cases plus the consistency gate, and "shipped" means **`tools/verify-bundle.ps1` is green**. It exists so that *anyone with repo access*, given only a description, extends the pack **consistently** — never repeating the past mistakes (a skill that exists for one tool but not the other, no eval case, stale counts, an un-re-pasted managed block, a forgotten revision bump).

**Spine:** the Rigor Protocol (`knowledge/rigor-protocol.md`) over the four embedded phases. **Authority:** the deployment map and changelog convention in `adapters/INSTALL.md` (the contract — every source path has one destination per tool); the Discoverability Mandate (`knowledge-visualization.md` V10). **Mode:** Peer Mode to author, Adversary Mode at the gate. **Tooling:** `tools/new-capability.py` scaffolds the correctly-placed files for **both tools** and re-derives counts; `tools/check-consistency.py` is the drift detector; `tools/sync-pack.ps1` regenerates both surfaces and re-pastes the managed blocks; `tools/verify-bundle.ps1` chains all of it into the **one-command consistency proof**. Reach for a script before doing anything by hand.

## Locating the pack source
Pack work edits the canonical `pack/` tree. Resolve it (the same resolution `/addpacktorepo` and `/updatepack` use, so the meta-skills behave identically):
1. **The current repo**, if it contains `pack/adapters/INSTALL.md` (you are in the AI-Forward repo — the common case).
2. An explicit path in the user's message.
3. The `AI_FORWARD_PACK` environment variable.
4. A sibling directory named `ai-forward` or `AI-Forward`.

If none resolves, ask for the path. New capabilities land in the **canonical source**; installed repos receive them later via `/updatepack` — never hand-edit a target repo's installed copy.

## Grounding (first action)
CO-S0 applies first — the sentence is `reference/co-s0.md`. Read, before writing: the deployment map and the **`changes` changelog convention** in `pack/adapters/INSTALL.md`; the **closest existing exemplar** of the kind you are adding (an analogous `pack/commands/<name>/SKILL.md` + its `adapters/copilot/prompts/<name>.prompt.md` + its `pack/evals/cases/<name>-01.json`) and conform to its shape; the governing standard for the capability's area (e.g. the Testing Strategy for a test directive, the UI standards for a UI rule). Run `python3 tools/check-consistency.py` to capture the **green baseline** — you will return it to green at the end. Skip grounding only if the user explicitly says so.

## Input
A prose description of the capability the user wants the pack to gain (e.g. *"a skill that audits a repo's dependency licences"*, *"a knowledge doc that standardises our API-versioning rules"*, *"a template for incident post-mortems"*). One or two sentences is enough; the skill expands it.

## The invariants (what "zero drift" means — the mistakes of the past, made impossible)
Every extension MUST end in this state, and `tools/verify-bundle.ps1` proves it:
- **Every harness surface, plus a seat.** A new skill ships as a Claude `SKILL.md` *and* a Copilot `.prompt.md` (Grok and Antigravity take the skill directory), declares `runs_as:` and cites CO-S0; one that dispatches names the five-part contract with a termination condition, a deadline and a fallback — `python3 pack/scripts/verify-skill-contracts.py` refuses it otherwise, at authoring time. A knowledge doc ships as `.claude/knowledge/` *and* a wrapped `.github/instructions/`. (`new-capability.py` and `sync-pack.ps1` enforce the pairing.)
- **An eval exists** for a new skill, with real assertions (not the placeholder).
- **Counts reconcile** with the filesystem everywhere — INSTALL frontmatter, managed blocks, README, OVERVIEW, the explainer, copilot-instructions — and every skill is named in both managed-block lists. (`check-consistency.py` fails the build otherwise.)
- **Managed blocks re-pasted** into root `CLAUDE.md`/`AGENTS.md` (done by `sync-pack.ps1`).
- **Revision bumped** in `INSTALL.md` with a `changes` entry; the previous delta archived to Prior revisions (the changelog convention).
- **The tree is clean after sync** — running `sync-pack.ps1` produces no further diff (proves source == install).

## Modes — dry-run & idempotency
- **Dry-run (preview).** If the request contains `dry run`, `--dry-run`, `preview`, or `what would change`, run COLLECT–DESIGN and `new-capability.py --dry-run`, present the plan and the files that *would* change, and **write nothing / commit nothing**. The commit step is always confirmed regardless.
- **Idempotent by construction.** `new-capability.py` refuses to clobber existing files without `--force`; `sync-pack.ps1` regenerates wholesale; `verify-bundle.ps1` is read-only except for the sync it wraps. Re-running the skill over a half-finished extension completes it rather than duplicating it.

## Cast
- **Peers (author together):** Orchestrator, **Tech Lead** (lead — the smallest correct capability that earns its place), **Documentation Steward** (placement, both-tool parity, the knowledge graph), the **Domain Researcher** (only if the capability touches an unfamiliar external domain), and the relevant authoring persona for the kind (a skill is reasoning-design; a knowledge doc is a standard).
- **Adversaries (attack at the gate):** **The Simplifier** (does the pack need this capability, or does an existing skill already cover it? — soft veto on speculative bloat), **Test Architect** (is there an eval with a real, falsifiable assertion, and is `verify-bundle` green? — **hard veto**), **Documentation Steward** (consistency: both tools, counts, managed-block lists, deployment-map conformance — escalates any gap), **Release Engineer** (revision bumped, changelog written, no un-synced drift — **the consistency gate**). The author never clears their own hard veto.

## Flow (the macro-loop, compressed for pack work)

**Stage 0 — Interdict the rush.** Do not start writing a skill body before the capability is justified and shaped. Two failure modes to kill: (1) building a capability an existing skill already covers (ask the Simplifier first); (2) "I'll wire the counts up later" — the wiring is not optional and is exactly what drifts.

**Phase 1 — COLLECT (knowledge, only as deep as needed).** If the capability encodes an unfamiliar external domain (a new compliance standard, an unfamiliar protocol), run a focused `/collectknowledge` pass and cite it. For purely pack-internal capabilities the "knowledge" is the pack's own conventions — read the closest exemplar and the governing standard (above) and write a **two-line capability brief**: *what* it does and *which existing artifact it most resembles* (the thing you will conform to).

**Phase 2 — SPECIFY (small, but explicit).** State the capability's **purpose** (the job it does for a pack user), **who runs it and when**, **falsifiable acceptance criteria** (e.g. "a new skill applies in Claude Code by description and as a Copilot `/prompt`"; "its eval asserts a produced artifact"; "`verify-bundle` is green"), and the **non-goals** (what it deliberately will not do). If the capability is itself a *workflow skill with a user-facing surface*, the `/specify` UX rules apply to whatever it generates. Keep it to a few lines — but written, not assumed.

**Phase 3 — DESIGN (decide the shape and the wiring).** Decide:
- **Kind:** skill · knowledge doc · template · script · persona/agent. (Most extensions are a skill or a knowledge doc.)
- **Files and destinations** from the deployment map (`INSTALL.md` §1) — every source path and where it deploys per tool.
- **The eval:** what objective post-condition proves the capability works (a produced file, a grep, a `cmd-exit` running a check).
- **Scripts to reuse:** `new-capability.py` to scaffold; `verify-bundle.ps1` to prove. Name any *new* repetitive mechanic this capability introduces and **generalize it into a script** rather than leaving it as prose steps.
- **The consistency obligations** for this kind (which counts move, which prose docs and managed-block lists must name it). Name the **closest exemplar** the new artifact must match.

**Phase 4 — IMPLEMENT (scaffold → write → prove, red-first).**
1. **Scaffold with the script** (never by hand): `python3 tools/new-capability.py --kind <skill|knowledge> --name <name> --summary "<desc>"`. It creates the correctly-placed skeletons for both tools, the eval stub, and bumps the numeric counts. Use `--dry-run` first to preview.
2. **Write the real content** — the skill body / knowledge rules / template / script — conforming to the exemplar. Replace every `<placeholder>`.
3. **Curate the eval red-first:** write the assertion so it would have *failed* before the capability existed, then confirm the capability satisfies it.
4. **Complete the wiring the script flagged:** add the capability to the managed-block lists; update the prose counts/lists (run `python3 tools/check-consistency.py` — its output is the exact `file:line` to-do list); bump the **revision** and add a `changes` entry in `INSTALL.md`, archiving the previous delta to Prior revisions.
5. **Prove it:** run `pwsh tools/verify-bundle.ps1`. It syncs both tool surfaces, runs every consistency check, validates the evals, and confirms the tree is clean after sync. Iterate until it prints **BUNDLE CONSISTENT**. That output is the proof you present.

**Stage 4 — DISCONFIRM (the gate).** Adversary Mode: the Simplifier challenges the capability's right to exist; the Test Architect refuses to pass without a real eval assertion and a green `verify-bundle`; the Documentation Steward verifies both-tool parity and that every count/list names the new capability; the Release Engineer confirms the revision bump and the clean-after-sync tree. Authors do not self-clear.

**Stage 5 — CONVERGE.** Present the **summary**: the capability added, the files created/changed (grouped by tool), the eval, and the **`verify-bundle` proof** (the BUNDLE CONSISTENT output). Offer to commit and push:
> "Stage, commit, and push? Proposed: `feat: add <capability> to the AI-Forward pack (revision <N>)` (y · n · custom message)."
On confirmation, run in order: `git add -A`, `git commit -m "…"`, `git push`.

## Output artifact
A committed, drift-free pack extension: the new capability's files (both tool surfaces), its eval, the updated counts/lists/managed-blocks/changelog, and the regenerated `.claude/` + `.github/` + `docs/` — all proven by `tools/verify-bundle.ps1` (BUNDLE CONSISTENT).

## Definition of done (exit gate)
- [ ] Capability justified (Simplifier cleared) and specified with falsifiable acceptance criteria and non-goals.
- [ ] Scaffolded via `tools/new-capability.py` (not by hand); content written conforming to the closest exemplar; every `<placeholder>` replaced.
- [ ] **Every harness surface present** for the new artifact; `verify-skill-contracts.py` clean.
- [ ] A new skill has a real, **red-first eval assertion**; any new repetitive mechanic was generalized into a script.
- [ ] `tools/check-consistency.py` clean — counts reconcile everywhere and the capability is named in both managed-block lists and the prose docs.
- [ ] **Revision bumped** + `changes` entry written; previous delta archived.
- [ ] `tools/verify-bundle.ps1` prints **BUNDLE CONSISTENT** (sync clean, all checks green, evals valid) — the proof is shown to the user.
- [ ] Adversarial gate passed; the consistency gate (Release Engineer) and the eval gate (Test Architect) cleared; authors did not self-clear.

## Documentation & discoverability (note)
This is a **pack-lifecycle skill**: it authors *pack* content (skills, knowledge, scripts), not a project `docs/` knowledge-graph artifact, so its own discoverability discipline is `tools/verify-bundle.ps1` (the bundle's "is it consistent?" oracle) rather than `docs-graph.py`. **But** if the capability it adds is itself a knowledge doc or template that becomes part of a project's graph, that artifact carries V2 frontmatter and is swept into the index on the next skill run, exactly like any other. The new capability's deployment is recorded as a `changes` entry in `INSTALL.md` (the consumer-facing changelog) — that is how installed repos learn of it via `/updatepack`.

**Audit & change (last action).** Append an audit-log entry for this run — `python3 docs/ai-forward-pack/scripts/audit-log.py append --shortname "extendaibundle-<capability>" --session "<id>" --skill extendaibundle --kind skill --prompt "<the prompt, verbatim>" --summary "<capability added; BUNDLE CONSISTENT>" --artifact pack/<new files>` — per the Audit Mandate (`knowledge/audit-and-change-log.md`, AL5). Extending the pack is itself a load-bearing change, so **also append a change-log entry** (`audit-log.py change --kind decision --skill extendaibundle … --git-before "<sha at grounding>"`, CL1–CL2) capturing what was added and why.

**Handoff:** `/updatepack` propagates the new capability to every repo that has the pack installed.
