---
name: addpacktorepo
description: Add the AI-Forward Pack to a local repository from a given path — reason over the target repo's language, tooling, and existing docs before writing anything, apply the full pack deployment map, produce a tabular summary of every artifact installed and what it does, point to the pack explainer and docs, and offer to commit and push. Run this from the AI-Forward repo.
runs_as: Coordinator
---

# /addpacktorepo — install the AI-Forward Pack into a local repository

The target's language, existing CLAUDE.md or AGENTS.md, and existing `docs/` shape what lands where, what needs wrapping, and what must not be overwritten. This skill is the **guided first install** — it reads the repo before writing to it, plans the deployment shape, and produces a discoverable record of everything it placed.

**Spine:** the Rigor Protocol (`knowledge/rigor-protocol.md`), applied to the installation plan. **Cast:** the **Enterprise Architect** (fit and longevity — no speculative extras, proportional to the repo's evident tier); the **Release Engineer** (owns the file operations — correct destinations, no silent overwrites of existing content); the **Documentation Steward** (every installed artifact is discoverable, frontmatter-valid, and correctly placed); the **Tech Lead** (smallest correct install that is still complete). **Tooling:** filesystem reads of the target repo (language detection, existing-file inventory), standard file copies, git for the commit offer.

## Grounding (first action)

CO-S0 applies first — the sentence is `reference/co-s0.md`. Read the target repository before writing to it. Treat existing CLAUDE.md, AGENTS.md, `.claude/`, `.github/`, `.grok/`, `.agents/`, and `docs/` as **evidence of prior decisions** — the install adapts to them, not over them. `<pack-source>/pack/adapters/INSTALL.md` is the authoritative deployment map. If `docs/ai-forward-pack/INSTALL.md` already exists in the target, this is an **update scenario** — redirect to `/updatepack` rather than re-running a full install. Skip grounding only if the user explicitly says so.

## Locating the pack source

The pack source is the `pack/` tree of an AI-Forward repository. Locate it in this order (the same resolution `/updatepack` uses):
1. **The current repo**, if it contains `pack/adapters/INSTALL.md` (you are in the AI-Forward repo — the common case).
2. An explicit path provided in the user's message.
3. The `AI_FORWARD_PACK` environment variable.
4. A sibling directory named `ai-forward` or `AI-Forward` relative to the current repo root.

If none yields a valid path containing `pack/adapters/INSTALL.md`, ask the user for the path before proceeding. (This is why the skill is deployed to every repo yet never assumes it *is* the source — it resolves the source explicitly.)

## Input

Path to the **target** repository (`$ARGUMENTS` or as part of the user's message). The path must be a local, accessible directory containing a Git repository (has a `.git/` directory). If the path is missing, invalid, or points to a non-Git directory, ask the user before proceeding. The target must be different from the resolved pack source.

## Modes — dry-run & idempotency
- **Dry-run (preview).** If the request contains `dry run`, `--dry-run`, `preview`, or `what would change`, run Stages 0–2 and produce the full **install summary table** (Stage 5) **without writing, staging, committing, or pushing anything** — then state it was a preview and how to apply for real. The commit/push step is *always* confirmed regardless of mode; dry-run additionally suppresses every file write.
- **Idempotent by construction.** Re-running is safe. An already-installed target (it has `docs/ai-forward-pack/INSTALL.md`) routes to `/updatepack` rather than re-stamping. Otherwise every action is a **wholesale** copy or a wholesale managed-block re-paste between markers — never an append — so re-running over a partial or interrupted install completes it without duplicating a managed block or clobbering an accumulated `docs/docs-index.js` or a pre-existing `docs/index.html`.

## Stages

**Stage 0 — interdict the rush.** Do not copy files before reading the target. Two failure modes: (1) overwriting repo-specific customizations in CLAUDE.md, AGENTS.md, or an existing `.claude/` setup; (2) a technically-correct but contextually-wrong install (e.g., installing the full C# knowledge set into a Python repo). The recon stage prevents both. The Release Engineer holds a hard veto on any file operation that would destroy existing content without explicit user consent.

**Stage 1 — OPEN (reconnaissance of the target repo).**
Build an install profile by inspecting the target:
- **Primary language(s):** what source extensions dominate? (`.cs`, `.py`, `.ts`, `.js`, `.go`, `.rs`, `.java`, `.rb`, etc.)
- **Existing AI tooling:** does `CLAUDE.md` exist? `AGENTS.md`? `.claude/`? `.github/instructions/`? `.github/prompts/`? `.github/agents/`? `.grok/skills/`? `.grok/agents/`? `.agents/skills/`?
- **Existing docs surface:** does `docs/` exist? `docs/ai-forward-pack/`? `docs/docs-index.js` (never seed or overwrite — V10)? `docs/index.html` — install never creates this either (PK-03): it is instantiated only by a content-creating skill, as its own last action, once it has real content to show.
- **Pack already installed?** Check for `docs/ai-forward-pack/INSTALL.md` — if present, redirect to `/updatepack`.
- **Tier signal:** repo size, number of modules, presence of tests, CI workflows — shapes whether to recommend a minimal or full install. State the tier assessment aloud.

**Stage 2 — INTERROGATE (plan the install).**
From the recon, determine for each artifact class:
- Which knowledge docs to install? All foundation docs always; language-specific guides only if relevant (e.g., `csharp-style-guide.md` only if `.cs` files are present — deployed with `applyTo: "**/*.cs,**/*.csx"`, not `"**"`).
- Does `CLAUDE.md` exist? If yes: **append** the managed block — never replace the file. If no: create it with a short preamble and the block.
- Does `AGENTS.md` exist? Same.
- `docs/index.html`: never copied by install (PK-03) — leave it to the first content-creating skill.
- Is `docs/docs-index.js` present? Never seed or overwrite (V10).
- Which optional artifacts to install? The CI workflow (`docs-health.yml`) is recommended — ask the user.

Produce a brief "here is what I will install" preview and ask for confirmation before executing if any existing file will be modified.

**Stage 3 — EVIDENCE (apply the full deployment map).**
**Run the program first, then verify the list below against its table:** `python3 <pack-source>/pack/scripts/pack-apply.py apply --install --target <target> --project <repo-name>` performs steps 1–13 mechanically and idempotently (knowledge routed by load scope, whole skill directories to `.claude/skills/`, `.grok/skills/`, and `.agents/skills/`, agents renamed and `tools:`-stripped for Copilot and Grok, templates/scripts/hooks/context-budget.json, `.claude/settings.json` merged, `.grok/hooks/ai-forward.json` and `.grok/rules/grok-surface.md`, `.agents/hooks.json`, `.agents/skills.json`, and `.agents/rules/agy-surface.md`, `.gitignore` lines, `docs/index.html` and `docs/docs-index.js` NEVER created or overwritten by install (PK-03, V10 — a content-creating skill instantiates the Explorer once it has real content), both managed blocks, `CLAUDE.md` in the `@AGENTS.md` import form) and prints one row per action. The numbered list is the contract the program implements — read it to check the table, not to copy files by hand. Steps 14–16 remain judgement calls. Since revisions 71/72 the program also deploys the Grok Build and Antigravity surfaces.

Execute the deployment map from INSTALL.md §1, in this order. All source paths below are relative to the resolved `<pack-source>` (e.g. `<pack-source>/pack/knowledge/*.md`); all destinations are relative to the target repo.

1. **Knowledge → `.claude/knowledge/`:** copy all `pack/knowledge/*.md`. Create the directory if absent.

2. **Knowledge → Copilot instructions (`.github/instructions/`):** for each `pack/knowledge/<name>.md`, create `<target>/.github/instructions/<name>.instructions.md` by prepending:
   ```
   ---
   applyTo: "**"
   ---
   ```
   Exceptions: `csharp-style-guide` uses `applyTo: "**/*.cs,**/*.csx"`; `FOUNDATION.md` is copied verbatim (it is a provenance manifest, **not** wrapped as an instruction).

3. **Skills → `.claude/skills/<name>/SKILL.md`:** copy each `pack/commands/<name>/SKILL.md` into a matching subdirectory.

4. **Skills → Copilot prompts (`.github/prompts/<name>.prompt.md`):** copy each `pack/adapters/copilot/prompts/<name>.prompt.md`.

5. **Agents → `.claude/agents/`:** copy all `pack/adapters/claude-code/agents/*.md` and `pack/adapters/copilot/agents/*_agent.md`.

6. **Agents → Copilot agents (`.github/agents/`):** copy all `pack/adapters/copilot/agents/*_agent.md` (these carry no `tools:` line per INSTALL §1.2 — do not add one).

7. **Templates → `docs/ai-forward-pack/templates/`:** copy all `pack/templates/*` recursively.

8. **Scripts → `docs/ai-forward-pack/scripts/`:** copy all `pack/scripts/*`.

9. **Pack docs → `docs/ai-forward-pack/`:** copy `pack/README.md`, `pack/OVERVIEW.md`, `pack/research-synthesis.md`, `pack/adapters/INSTALL.md`.

10. **Docs Explorer → `docs/index.html`:** do NOT create. Instantiating the shell at bare install — with zero content under `docs/` — would satisfy `docs-graph.py derive`'s own opt-in check (`_opted_into_docs_explorer`) for every repo the installer ever touches, so the next bare `derive` (every shipped skill calls it bare) would seed `docs-index.js` unasked (PK-03, measured in harness-bench grid-4). Leave it absent; the first content-creating skill instantiates it from `pack/templates/docs-explorer.template.html`, substituting `__PROJECT__`, as its own last action once it has real content to show (V10 step 3).

11. **`CLAUDE.md` managed block:** locate or create `<target>/CLAUDE.md`. Append `pack/adapters/managed-blocks/CLAUDE.block.md` (markers included) between `AI-FORWARD-PACK:BEGIN` / `AI-FORWARD-PACK:END`. If the file already exists and has no markers, append to the end without altering existing content. If the file does not exist, create it with:
    ```markdown
    # <repo-name>

    Project conventions live here. The AI-Forward Pack's reasoning stack is wired in below.
    ```
    followed by the managed block.

12. **`AGENTS.md` managed block:** same process with `pack/adapters/managed-blocks/AGENTS.block.md` targeting `<target>/AGENTS.md`.

13. **`.gitignore` hygiene (INSTALL.md §2):** ensure the target repo's `.gitignore` contains `*.jsonl.lock` (the graph tool's local lock files, never committed), `spikes/` (throwaway Spike Protocol probes), `.agents/*` with `!.agents/artifacts.yml` and `!.agents/log/` (the coord ledgers are tracked — D10), and `.agents/mail/` (machine-local inboxes; `mkdir -p .agents/mail`). Append whichever lines are missing; create `.gitignore` if absent. Never remove existing entries — append-only.

14. **CI workflow (optional — ask first):** copy `pack/ci/docs-health.yml` to `<target>/.github/workflows/docs-health.yml`. This gates PRs on graph health and is recommended but not mandatory. Ask the user before copying.

15. **`docs/docs-index.js`:** do NOT create or seed. Leave absent; the first skill run creates it (V10).

**Stage 4 — DISCONFIRM (the gate).**
Before reporting success:
- Confirm `docs/docs-index.js` was not touched.
- Confirm no existing `CLAUDE.md` / `AGENTS.md` content was overwritten (only appended or managed-block region replaced).
- Confirm all file copies are non-zero bytes.
- Confirm the C# knowledge wrap uses the `.cs`-scoped `applyTo`, not `"**"`.
- Confirm `FOUNDATION.md` was deployed but NOT wrapped as a Copilot instruction.
- Confirm `.grok/skills/`, `.grok/agents/`, `.grok/hooks/ai-forward.json`, and `.grok/rules/grok-surface.md` landed; confirm `.grok/rules/` contains no knowledge docs.
- Confirm `.agents/skills/`, `.agents/rules/agy-surface.md`, `.agents/hooks.json`, and `.agents/skills.json` landed; confirm `.agents/rules/` contains no knowledge docs.
- Confirm the six hook adapters are under `docs/ai-forward-pack/hooks/` — `reread-guard.py`, `session-start.py`, `mail-doorbell.py`, `heartbeat.py`, `owner-review-gate.py`, `git-identity-guard.py` (the program copies all six) — with their entries merged per host, and that `pack-doctor.py`'s `doorbells` line names each harness `verified · observed-only · unsupported`; existing ledgers are made portable with `coord log portable <files>`.
- The Release Engineer vetos any ❌ action from being included in the summary as "done."

**Stage 5 — CONVERGE (summary + docs pointers + commit offer).**
Produce a **tabular summary** of everything installed:

| Area | Artifact | Destination | What it does | Status |
|------|----------|-------------|--------------|--------|
| knowledge | `rigor-protocol.md` | `.claude/knowledge/` + `.github/instructions/` | Reasoning spine — map → interrogate → evidence → disconfirm → converge on every non-trivial task | ✅ |
| knowledge | `persona-cards.md` | `.claude/knowledge/` + `.github/instructions/` | 23 persona lenses as §8 cards with severity, veto-clears-when, and owned anti-patterns | ✅ |
| skill | `specify/SKILL.md` | `.claude/skills/specify/` + `.github/prompts/specify.prompt.md` + `.grok/skills/specify/` + `.agents/skills/specify/` | Turn a prompt into a testable spec (Functional / UX / UI layers) | ✅ |
| skill | `implement/SKILL.md` | `.claude/skills/implement/` + `.github/prompts/implement.prompt.md` + `.grok/skills/implement/` + `.agents/skills/implement/` | TDD feature implementation with adversarial pre-merge review (overrides Grok's bundled `/implement` in this repo) | ✅ |
| agent | `orchestrator.md` | `.claude/agents/` + `.github/agents/` + `.grok/agents/` | Sequences the persona swarm, enforces phase gates, maintains the evidence trail | ✅ |
| managed-block | AI-FORWARD-PACK block | `CLAUDE.md` | Wires Claude Code to the full reasoning stack (personas, skills, docs explorer) | ✅ |
| managed-block | AI-FORWARD-PACK block | `AGENTS.md` | Same wiring for GitHub Copilot, Grok Build, and Antigravity (loaded natively) | ✅ |
| grok | skills, agents, hooks, rules | `.grok/` | Native Grok Build surface (INSTALL 1.7); path map only in `.grok/rules/` | ✅ |
| agy | skills, hooks, rules, manifest | `.agents/` | Native Antigravity (agy) surface (INSTALL 1.8); path map only in `.agents/rules/` | ✅ |
| docs | `docs/ai-forward-pack/` | templates · scripts · INSTALL · README · OVERVIEW | Reference artifacts, graph scripts, installation guide | ✅ |
| explorer | `docs/index.html` | `docs/` | Docs Explorer — hierarchy · graph · mind map · health views (instantiated by the first content-creating skill, not by install — PK-03) | not yet — created on first use |

*(Rows truncated to template; the actual table lists every file.)*

After the table, output:

> **Pack installed at revision `<N>` (`<bundle_version>`, released `<date>`).**
>
> **Learn more:**
> - **Interactive explainer** (skill map, persona roster, architecture overview): **https://timianmalloo.github.io/ai-forward/** (published to GitHub Pages from the AI-Forward repo). Offline fallback: open `<pack-source>/web/ai-forward-pack-explainer.html` from the clone (the explainer is not copied into target repos).
> - **OVERVIEW** (one-page pack summary): `docs/ai-forward-pack/OVERVIEW.md` in the target repo.
> - **Installation guide & deployment map**: `docs/ai-forward-pack/INSTALL.md` in the target repo.
> - **Docs Explorer** (knowledge graph browser): `docs/index.html` — not created by this install; instantiated and populated by the first content-creating skill run (PK-03, V10).
>
> **Recommended next steps:**
> 1. Run `/adopt` to recover the existing architecture and bring docs under the knowledge graph.
> 2. Run `/collectknowledge` to build a domain knowledge base before the next design cycle.
> 3. Run `/specify` on your next feature — the pack's reasoning stack is now live.

Then ask:
> "Would you like me to stage, commit, and push these changes to the target repo?
> Proposed message: `chore: install AI-Forward Pack revision <N> (<bundle_version>)`
> (y to proceed · n to leave staged · or type a custom commit message)"

If confirmed: run, in order, `git -C <target-repo> add -A`, `git -C <target-repo> commit -m "chore: install AI-Forward Pack revision <N> (<bundle_version>)"`, `git -C <target-repo> push` (use `-C` to target the correct directory without changing the working directory).

## Documentation & discoverability (note)
Unlike the workflow skills, this is a **pack-lifecycle skill**: it installs the pack *into* a repo rather than producing a product artifact, so it writes no knowledge-graph frontmatter and does **not** seed or sync `docs/docs-index.js` — the first *workflow* skill run in the target creates the index (V10). Its durable record is the installed `docs/ai-forward-pack/INSTALL.md` (carrying the `revision`) plus the commit. The recommended `/adopt` handoff is what brings the target's existing artifacts into the graph.

## Definition of done
- [ ] Target repo reconnaissance complete; install profile built; conflicts surfaced and confirmed before any write.
- [ ] Full deployment map applied per INSTALL.md §1 (adjusted for language profile and existing artifacts).
- [ ] `CLAUDE.md` and `AGENTS.md` managed blocks in place — no pre-existing content overwritten.
- [ ] `docs/docs-index.js` not seeded or touched (V10).
- [ ] `.gitignore` contains `*.jsonl.lock` and `spikes/` (appended if missing, existing entries untouched).
- [ ] `docs/index.html` not created by install, and not overwritten if it pre-existed (PK-03).
- [ ] `FOUNDATION.md` deployed but NOT wrapped as a Copilot instruction.
- [ ] C# knowledge wrap scoped to `**/*.cs,**/*.csx` only when C# files are present.
- [ ] Grok surface present (`.grok/{skills,agents,hooks,rules}`); `.grok/rules/` is the path map only.
- [ ] Antigravity surface present (`.agents/{skills,hooks,rules}` and `.agents/skills.json`); `.agents/rules/` is the path map only.
- [ ] Summary table complete; docs and explainer links provided.
- [ ] Commit offered with exact proposed message; user decision honoured.
- [ ] Dry-run requests produced the install table with zero writes; a normal run is idempotent (re-runnable over a partial install without duplicate blocks; an already-installed target routes to `/updatepack`).

**Audit (last action).** Append an audit-log entry to the **target repo's** log recording the install — `python3 docs/ai-forward-pack/scripts/audit-log.py append --shortname "addpacktorepo-<target>" --session "<id>" --skill addpacktorepo --kind command --prompt "<the prompt, verbatim>" --summary "<pack revision installed + what landed>"` — per the Audit Mandate (`knowledge/audit-and-change-log.md`, AL5), so the target repo records that the pack was installed and when.

**Handoff:** direct the user to `/adopt` as the immediate next step — it recovers the existing architecture into the knowledge graph and sets the Docs Explorer baseline.
