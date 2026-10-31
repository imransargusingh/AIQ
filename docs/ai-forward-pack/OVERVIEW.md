# AI-Forward Pack — Overview

The practical orientation to the bundle: **how to install it, what it contains, and how to use the skills.** For *why* each piece exists, see `README.md`; for the evidence and design decisions behind it, `research-synthesis.md`.

The pack is a repository-droppable extension to the **Agent Knowledge Pack**. It turns that pack's adversarial reviewer council into a working swarm — collaborating peers that *author*, adversarial personas that *attack*, and a staged reasoning discipline that slows the rush to a plausible answer and replaces it with evidence. It works with **Claude Code**, **GitHub Copilot**, **Grok Build**, **Antigravity**, or any combination.

---

## 1. How to install

The portable setup is the short path. In the project you want to work on, with
Git and [uv](https://docs.astral.sh/uv/getting-started/installation/) on PATH, run:

```text
uv run --no-config --no-project --script https://raw.githubusercontent.com/timianmalloo/ai-forward/v2026.10.19.4/bootstrap.py
```

This works in Windows PowerShell/Command Prompt and macOS/Linux terminals. Setup
uses upstream `main`; no `--repo` or `--ref` is needed. The upstream launcher becomes
available when its contribution merges; reviewers use the matching committed source
before then. Append `--dry-run` to preview, or rerun to check/update an install.
Existing Git repositories and plain projects are supported, without automatic Git
initialization. Setup preserves project policy and stops for specific conflicts;
it does not install project dependencies, grant permissions, commit, push or deploy.
Open the target in your coding app and start a fresh chat after installation.

For expert installation or project-specific reconciliation, the pack can still be installed by **manual reconciliation** — it's all text, and every source path has exactly one destination per host. The deployment map, the ready-to-paste managed blocks (`adapters/managed-blocks/`), the Copilot frontmatter wrap, the Grok path map (`.grok/rules/grok-surface.md`), the Antigravity path map (`.agents/rules/agy-surface.md`), and the update procedure are in **`adapters/INSTALL.md`** (installed copy: `docs/ai-forward-pack/INSTALL.md`) — whose **frontmatter `changes` changelog** is the refresh guide: on a repo refresh, read "what changed since the last version" and apply exactly those re-copies and managed-block re-pastes. In short: knowledge → `.claude/knowledge/` + `.github/instructions/` (wrapped; Grok and Antigravity read the `.claude/knowledge/` copies), skills → `.claude/skills/` + `.github/prompts/` + `.grok/skills/` + `.agents/skills/`, all 23 agents → every host's agent directory, templates + pack docs → `docs/ai-forward-pack/`, the Docs Explorer template is available for the first content-creating skill to instantiate at `docs/index.html` (not copied by install), the managed block in `AGENTS.md`, the Grok path map in `.grok/rules/`, and the Antigravity path map in `.agents/rules/`.

**What lands in your repo** (for `--tool both`):

```
<repo>/
├─ CLAUDE.md / AGENTS.md             # a managed block is added (your own content is preserved)
├─ .claude/  (knowledge, skills, agents)      ← Claude Code
├─ .github/  (instructions, prompts, agents)  ← GitHub Copilot
├─ .grok/    (skills, agents, hooks, rules)   ← Grok Build
├─ .agents/  (skills, hooks, rules)           ← Antigravity (agy)
└─ docs/ai-forward-pack/  (README, research-synthesis, this overview, templates)
```

---

## 2. What it contains

Three foundations, a roster, and the skills that put them to work.

**The reasoning spine.**
- **Rigor Protocol** (`knowledge/rigor-protocol.md`) — a staged, broad-to-narrow discipline in six stages: **0 Rush Interdiction → 1 OPEN → 2 INTERROGATE → 3 EVIDENCE → 4 DISCONFIRM → 5 CONVERGE**. It scales with the work's tier; every conclusion carries a confidence label and a residual risk.
- **Spike Protocol** (`knowledge/spike-protocol.md`) — read-the-code and run-a-PoC before depending on any unfamiliar API, SDK, or MCP server, so designs rest on established contracts, not guessed semantics.
- **UI Archetype Grammar** (`knowledge/ui-archetype-grammar.md`, G1–G16, + the catalog `ui-archetype-catalog.md`) — a hardened, EBNF-valid **two-layer grammar** that **identifies a UI/UX template** by its routing/temporal/data archetype and uses it as a **determinism control** for code generation: a coarse **Archetype Signature** (the selector) composed with the U1–U20 token/state/flow spec (the concrete fill). The catalog carries **16 archetypes** with live exemplars, canonical signatures, and model-agnostic codegen descriptors. `/specify` records the signature in Part C, `/design-slice` resolves each facet, `/implement` builds to it — so an agent in Claude Code or Copilot reaches for the right archetype instead of regressing to a generic dashboard.
- **Specification Standards** (`knowledge/specification-standards.md`, S1–S10) — a spec is **one document with three separable, individually-owned layers**, written abstract-to-concrete and gated **bottom-up** (Garrett's Five Planes): **Functional** (what & why — ISO/IEC/IEEE 29148 + user stories with Gherkin + ISO 25010 NFRs; owner Product Strategist), **UX specification** (how it *works* — information architecture, user flows covering the alternate/error/recovery paths, wireframe-level structure; owner the new **UX Researcher / IA**), and **UI specification** (how it *looks* — specified to U1–U20; owner UX & Accessibility). UX precedes UI; each absent layer is marked **N/A with a reason**, never dropped. `/specify` writes all three; the gate reviews bottom-up.
- **UI & Interaction Design Standard** (`knowledge/ui-interaction-design.md`, U1–U20) — whenever the work has a user-facing interface on *any* medium (web, native, mobile, CLI, voice), this governs whether it's **excellent**: a **design-token system** (primitive/semantic/component with theme/density/platform modes — no arbitrary values), the craft canon (hierarchy, space, type/color systems, purposeful motion, real in-voice copy), **complete component states** (the empty/loading/error states polish usually skips), Jakob's-Law familiarity, and for AI-facing UIs the **HAX 18 guidelines** + **Shape-of-AI** pattern families (Wayfinders, Tuners, Governors, Trust builders, Identifiers) with the wrong answer designed as a first-class state. **WCAG 2.2 AA** and a named **performance budget** are floors. `/specify` names the bar, `/design-slice` specifies the interface, `/implement` builds and proves it — the **UX & Accessibility lens holds the veto**.
- **Knowledge Visualization & Docs Explorer Standard** (`knowledge/knowledge-visualization.md`, V1–V18) — all repo knowledge is **one typed graph** whose record is each artifact's **YAML frontmatter** (id, type, **owner**, typed links per the **relation registry**, **review-by** freshness SLA, summary); `docs/docs-index.js` is the *derived* projection, and `docs/` doubles as a valid **Obsidian vault** for free. The **Docs Explorer** (`docs/index.html`) renders hierarchy, graph, and mind-map projections plus a **health view** (stale, orphan, and review-suggested nodes) and Mermaid/UML diagrams. A first-class **glossary** anchors domain terms (`uses-term` edges), grounding **traverses the graph** with the path as provenance (V15), **material changes propagate** `review-suggested` flags to inbound neighbors (V16), sub-ADR decisions and assumptions are captured as linked **decision notes** (V17), every content-creating skill writes frontmatter + syncs the index as its **last action** (V10), and all graph mechanics run through the stdlib-only **script bundle** (`scripts/docs-graph.py`: inventory · derive · validate · freshness · flag · stub) instead of prompt-time scripting (V18).
- **Audit & Change Log Standard** (`knowledge/audit-and-change-log.md`, AL/CL) — the project keeps a **durable, committed history** so work compounds across sessions: an append-only **audit log** (`docs/audit/audit-log.jsonl`) of every meaningful prompt/skill/script — every skill appends an entry as its last action (the **Audit Mandate**) — and a curated **change log** (`docs/audit/change-log.jsonl`) of design decisions, where `/collectknowledge`, `/define-architecture`, `/design-slice`, and `/migrate` capture the prompt, the result, and the **git context before and after** (the **Change Mandate**). It is the committed counterpart to a session's ephemeral store, registered in the graph as the `docs/audit/audit-log.md` hub node, browsable as a searchable timeline (`docs/audit/index.html`) or via `/auditlog`, and written only through `scripts/audit-log.py`.
- **Observability & Instrumentation Standard** (`knowledge/observability-and-instrumentation.md`) — code emits structured, trace-correlated telemetry in the **OpenTelemetry data model** (regardless of the concrete logging library), with **stable error codes** and **RFC 9457** error responses, so production failures are debuggable and logs correlate to traces. `/design-slice` and `/implement` enforce it; the SRE owns it.

**The dual-mode persona model** (`knowledge/collaborative-personas.md`). A persona is a lens worn two ways: in **Peer Mode** it builds the best possible thing; in **Adversary Mode** the same lens attacks the proposed thing. Ideation runs in Peer Mode, review in Adversary Mode, and one integrity rule holds throughout — **the author never clears its own hard veto.**

**The roster — 23 lenses**, every one rendered as a uniform §8 card (`knowledge/persona-cards.md`) with a convene-when trigger, a Peer-Mode deliverable, a severity scale, a falsifiable veto-clears-when predicate, and the anti-patterns it owns:
- **11 adversaries** from your catalog (Enterprise / Test / Security / Distributed-Systems / Patterns architects, Tech Lead, SRE, the C#/Rust/Python developers, the Simplifier).
- **3 peer-first roles** this pack adds (Orchestrator, Product Strategist, Domain Researcher).
- **4 governance adversaries** added after an audit (AI Systems Engineer, Data & Persistence Architect, Privacy & Data Governance Counsel, Release / Deployment Engineer).
- **4 UI/app & documentation lenses** (Mobile App Developer, Native Desktop Developer, UX & Accessibility, Documentation Steward).
The reasoning behind every seat — and the seats deliberately *not* added — is in `knowledge/persona-audit.md`, which also defines the **Persona Operating Standard**: the severity scale (Blocker/Major/Minor/Nit), the confidence labels (Verified/Inferred/Flagged), the falsifiable veto-clears-when predicates, and the deterministic veto-conflict rule (true hard-vs-hard ties escalate to you).

**Per-project domain experts.** The 23 are domain-*general*. Subject-matter lenses (finance, CFD, clinical, legal…) are added per repo by `/adddomainexperts` and recorded in that repo's `docs/domain-experts.md`.

**The system tests itself.** `evals/` is the pack's own regression suite — golden tasks per skill with objective trajectory assertions (the artifact exists, frontmatter valid, the FMA/STRIDE/phasing fingerprints present, `docs-graph.py validate` clean); skills are prompt-code and are tested like it. `ci/docs-health.yml` is a ready-to-copy GitHub Actions workflow gating PRs on graph health, freshness, and vendored-foundation drift. `knowledge/FOUNDATION.md` + `scripts/foundation-check.py` make divergence between the vendored base docs and your canonical base pack visible (normalized hashes; known intentional divergences cataloged, currently three pending back-port). `docs-graph.py snapshot` appends the governance-health trend every /document run.

**The artifacts.** 29 templates in `templates/` (spec, proposal, architecture, design, ADR, investigation, proof-pack, domain-expert, knowledge-base, documentation-bundle, the **glossary**, the **decision note**, the **project-memory** ledger, the **threat model** and **privacy review** rollups, the **native-UI proof pack**, the **session contract**, the **design-language** doc (Stitch DESIGN.md extended with the pack floors) and its **preview** HTML, the self-contained doc-viewer HTML, the **Docs Explorer** HTML that becomes `docs/index.html`, the **audit & change-log viewer** HTML that becomes `docs/audit/index.html`, and the **UI capability guide** HTML that becomes `docs/ui-guide.html`). The artifact templates all carry the V2 frontmatter header. Worked examples live in `examples/finance-repo/` and `examples/design-languages/`.

**The foundation (vendored, so the bundle is self-contained).** The Agent Knowledge Pack docs this pack builds on ship inside `knowledge/` and install alongside everything else: the **Body of Knowledge**, the **Rules of the Road**, the **Persona Catalog**, the **Layered-Optimized Architecture**, **Engineering Governance**, the **Testing Strategy**, and the **C# Style Guide**. They're heavily referenced throughout the skills and personas; bundling them means the pack works in a repo that doesn't already have the base pack. (They're *copies* — if you maintain the base pack separately, refresh them when it changes.)

```
ai-forward-pack/
├─ README.md · research-synthesis.md · OVERVIEW.md
├─ knowledge/   40 docs (+FOUNDATION manifest) — 32 reasoning + 7 vendored Agent-Knowledge-Pack foundation (BoK, Rules of the Road, Persona Catalog, LOA, Governance, Testing Strategy, C# Style)
├─ commands/    (the 30 skills, SKILL.md + reference/ each)
├─ templates/   (the 29 artifact templates)
├─ adapters/    (INSTALL.md, claude-code/agents, copilot/agents, copilot/prompts)
└─ examples/    (finance-repo — a worked /adddomainexperts result)
```

---

## 3. How to use the skills

**One request across the applicable stages.** Use `/deliver <your task>` in Claude
Code or a host that lists the installed slash entry point; Codex uses `$deliver`.
In Copilot CLI, find it with `/skills`, then `/deliver <your task>`, or ask
`Use the /deliver skill to <your task>`. If installed during a chat, use
`/skills reload`, then `/skills info deliver`. VS Code Copilot Agent Host sessions
use skills rather than the older prompt files; select/request the installed skill
when `/deliver` is not listed. Grok Build and Antigravity can select/request it by
name. These are chat requests, not terminal commands.

```text
/deliver Add CSV export for the current project and filter. Export all matches, not only the visible page. Do not add scheduling or new roles.
```

`deliver` preserves your original outcome, selects only the existing workflows it
needs and continues through approved work. It does not run every skill or start a
swarm by default. A necessary human decision, permission or blocking review pauses
the same task; reply to the specific question in the same chat. A hard veto requires
independent re-review. In a fresh chat in the same project, use
`/deliver resume <task-id>` (Codex `$deliver resume <task-id>`). Checkpoints are local,
not automatically shared across clones, and validate saved state rather than human
authenticity or the meaning of acceptance evidence. The handback still needs checks
of the actual result, including limits and unmet criteria. The individual skills,
reference table and worked example below remain available when you prefer to direct
one stage at a time.

There are **30 skills** — seven that carry an idea or piece of work from exploration to shipped code (`/create-proposal`, `/specify`, `/define-architecture`, `/design-slice`, `/ui-design`, `/implement`, `/investigate`), eight that support them (knowledge collection, persona tailoring, execution-graph planning, documentation, brownfield **adoption**, whole-repo **forensic review**, characterization-first **migration**, and **code-hygiene** review/fix), three **pack-lifecycle** skills that manage the pack installation itself (**/addpacktorepo**, **/updatepack**, and **/extendaibundle**), two **utility** skills (**/auditlog** and **/also**), and two **prompt-log utilities** (**/prompts** and **/searchprompts**). The workflow/support skills form the engineering surface below; lifecycle and utility skills sit outside it.

**Built-in delivery discipline.** `/define-architecture` *defines completely but phases vertically*: the whole architecture is specified, then delivery is partitioned into end-to-end vertical slices (Phase 1 a walking skeleton; mocks at unbuilt edges as contract seams) so serial implementation always yields a deployable, human-validatable increment. `/design-slice` performs a mandatory **failure-mode analysis** (each mode → an explicit disposition: prevent/detect/mitigate/recover/accept) and `/implement` carries every mode into code + a negative test. And `/define-architecture`, `/design-slice`, and `/implement` each **end with a status table** — completed / remaining / best next action — so you always know where the build stands.

**How you invoke them.** In **Claude Code**, the skills install to `.claude/skills/` and apply automatically by their description, or you can call one explicitly (e.g. *"run /specify on this idea: …"*). In **GitHub Copilot**, each skill installs as a prompt in `.github/prompts/`; invoke it in chat as `/specify`, `/design-slice`, etc., with your input. In **Grok Build**, skills install to `.grok/skills/` (slash `/specify`, …) and personas spawn with `spawn_subagent` (`subagent_type` = the persona `name`). Either way the skill convenes the right personas as peers to author and as adversaries to review — you don't summon agents by hand.

**The natural flow** (use only the steps a given piece of work needs):

```
/collectknowledge → /adddomainexperts → /specify → /define-architecture → /design-slice → /implement → /document
                                                                                          ↑
                                                                  /investigate  (whenever a defect appears)
```

**Quick reference**

| Skill | Use it when… | You get | Convened (peers → adversaries) |
|---|---|---|---|
| **/deliver** | one outcome completed without choosing every workflow yourself | applicable existing stages, local checkpoints and original criteria-to-evidence handback; pause at necessary gates | applicable author/reviewer lenses; no default swarm |
| **/create-proposal** | brainstorming an idea before requirements are fixed | `docs/proposals/<idea>.md` + `.html`, with optional `docs/mockups/` experiments | Product Strategist, UX Researcher/IA → Simplifier, UX & Accessibility when mocked up |
| **/collectknowledge** | starting in an unfamiliar or high-stakes domain, before design | `docs/knowledge/<topic>/` — sourced, confidence-labeled domain knowledge | Domain Researcher, Product Strategist → Domain Researcher (adversary), Simplifier |
| **/adddomainexperts** | the project has a real subject-matter domain | domain-expert personas + `docs/domain-experts.md` | Orchestrator, Product Strategist, Domain Researcher → Simplifier, Tech Lead, Data |
| **/specify** | turning an idea or prompt into a testable spec | `docs/specs/<feature>.md` with acceptance criteria | Product Strategist, Domain Researcher → Simplifier, Test Architect, Security (if data/identity), UX (if a UI) |
| **/define-architecture** | a new system or a load-bearing architecture decision | `docs/architecture.md` + ADRs | Enterprise/Distributed/Security architects, AI Systems, Data → full council, SRE, Privacy, Release |
| **/design-slice** | a component or feature inside an existing architecture | `docs/design/<component>.md` + test plan | Patterns Expert, Simplifier, language Dev (+ UX/platform if UI) → Security, Distributed, Test Architect |
| **/implement** | turning a design into tested code | code + tests + a Proof Pack | language Developer ⇄ Test Architect (pair) → Test Architect, SRE, architects, Release |
| **/investigate** | a defect that needs a verified root cause | `docs/investigations/<id>.md`: root cause + specific fix + failure-class generalization + phased repair plan — then stops for your review | SRE, Distributed Systems → Security, Test Architect, Data |
| **/document** | standing up docs, or keeping them current | `docs/` bundle (API ref + 4 diagram families) + the full-sweep Docs Explorer index + `_site` close-up + freshness hook | Documentation Steward, language Dev, Patterns Expert → Steward (adversary), Simplifier, Test Architect |
| **/adopt** | bringing an existing (brownfield) repo into the pack | recovered `docs/architecture.md` (C4, confidence-labeled) + frontmattered existing docs + seeded glossary + the first index/Explorer + a vertically-phased adoption plan | Enterprise Architect + Documentation Steward |
| **/forensicreview** | assessing the true state and risk of an existing repo | rebuilt truth-to-code architecture/docs + `docs/reviews/forensic-review.md` + a P0–P3 backlog separating risks, verified issues, and todos | Enterprise Architect + Documentation Steward → full applicable council, Test Architect evidence gate |
| **/migrate** | dependency/framework upgrades and large refactors | characterization tests green on the old stack first + vertical increments + the equivalence report with the intentional-difference catalog + V16 flags to dependents | language Developer (Test Architect's characterization veto) |
| **/code-hygiene** | holding a codebase to the coding guidelines — dead code, commented-out code, anti-patterns | `docs/hygiene/backlog.md` with an aggregate of violating LOC and % of codebase per class (`review`) + a TDD-guarded, git-labelled, phased remediation plan (`fix`) | The Simplifier, language Dev, Test Architect → Test Architect (hard veto), Security, Tech Lead |
| **/addpacktorepo** | installing the pack into another local repo (run from an AI-Forward clone) | the full pack deployed to the target (knowledge, skills, agents, templates, managed blocks) + a tabular install summary + explainer/docs pointers + a commit offer | Enterprise Architect + Release Engineer + Documentation Steward |
| **/updatepack** | refreshing a repo that already has the pack to the latest revision | only the changelog delta applied (knowledge/skills/agents + managed-block re-pastes) + a tabular action summary + advanced revision + a commit offer | Release Engineer + Documentation Steward |
| **/extendaibundle** | adding a new capability (skill/knowledge/template/script) to the pack itself from a prose prompt | collect→specify→design→implement compressed for pack work; scaffolded via `new-capability.py`, proven by `verify-bundle.ps1` (BUNDLE CONSISTENT) — both tool surfaces, an eval, reconciled counts, bumped revision, zero drift | Tech Lead + Documentation Steward (Test Architect + Release Engineer gate) |
| **/auditlog** | recalling what was done or decided in this repo, or re-running a past prompt | the last N actions / a search of the durable audit & change log / a recalled prompt to re-run / the searchable timeline viewer | — (a reader/dispatcher over `audit-log.py`; logs nothing itself) |

**A worked example — a new feature in an unfamiliar domain.**
1. **`/collectknowledge`** with the domain and problem you're solving → a sourced knowledge base in `docs/knowledge/` the whole team reasons from.
2. **`/adddomainexperts`** → adds the subject-matter lenses that domain needs (they cite the knowledge base) and registers them in the repo's roster.
3. **`/specify`** → a testable spec; the Test Architect rejects any acceptance criterion that can't fail, the Simplifier strips gold-plating.
4. **`/define-architecture`** (new system) or **`/design-slice`** (within an existing one) → the structure, with unfamiliar contracts proven by the Spike Protocol and load-bearing decisions captured as ADRs.
5. **`/implement`** → the language Developer and Test Architect pair in red→green→refactor; you get tested code and a Proof Pack.
6. **`/document`** → the API reference, the four diagram families, and the browsable view — with an after-commit check so they never drift.

**Two things hold across every skill.** Each runs the Rigor Protocol scaled to the work's **tier** (a trivial change runs a quick self-check; a load-bearing one runs all six stages with an external adversary). And the integrity rule never bends: claims carry a **confidence label**, contracts are **verified by execution rather than recalled**, and the **author never clears its own hard veto.**

---

## Where to go deeper

- **`README.md`** — why the pack exists and how it fits the Agent Knowledge Pack.
- **`research-synthesis.md`** — the industry/OSS comparison, the reasoning disciplines, and the gap analysis behind every choice.
- **`knowledge/rigor-protocol.md`** — the reasoning spine in full, with the per-skill mapping.
- **`knowledge/collaborative-personas.md`**, **`persona-cards.md`**, **`persona-audit.md`** — the dual-mode model, the 23 cards, and the operating standard + the reasoning for each seat.
- **`adapters/INSTALL.md`** — the manual reconciliation guide: the deployment map, managed blocks, and the update procedure.
- **`adapters/INSTALL.md`** — the manual file-by-file wiring for each tool.
- **`examples/finance-repo/`** — a worked `/adddomainexperts` result you can read end to end.
