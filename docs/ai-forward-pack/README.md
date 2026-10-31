# The AI-Forward Pack

*A repository-droppable extension that turns the **Agent Knowledge Pack** into a working swarm: collaborating peers that author, adversarial personas that review, and a staged reasoning discipline that slows the rush to a plausible answer and replaces it with evidence at every step.*

Works with **Claude Code**, **GitHub Copilot**, **Grok Build**, **Antigravity**, **Codex**, or any combination. Install into an existing Git repository or a plain project; GitHub hosting is not required.

---

**Codex users:** invoke `$collectknowledge` or `$specify` (CLI/IDE: `/skills` or `$`).
Skills live in `.agents/skills/`; `AGENTS.md` supplies project instructions.
See `adapters/codex/codex.md` in the source bundle or
`docs/ai-forward-pack/codex.md` in the installed project for Codex setup and troubleshooting.

## Why this pack exists

The Agent Knowledge Pack you already have is excellent at one half of engineering judgment: it defines eleven **adversarial** reviewer personas whose job is to find the flaw in a design before it becomes code. But a flaw-finding council can only *review* a proposal — it cannot *author* one. The strongest multi-agent software teams in the research literature (MetaGPT, ChatDev) run a **cooperative phase** — product, architecture, and engineering reasoning together as peers — and *then* an **adversarial phase** that attacks the result. This pack supplies the missing cooperative half, the rule for switching between the two, and a rigorous reasoning protocol that governs both.

It adds three things and nothing you have to relearn:

1. **The Rigor Protocol** — a staged, broad-to-narrow reasoning discipline (`knowledge/rigor-protocol.md`) that composes on top of your Body of Knowledge's Coning and Iterative Critical Thinking. It exists to defeat one failure mode above all: the model's built-in rush to the most *plausible-sounding* answer, which is not the most *correct* one.
2. **The dual-mode persona model** — collaborating peers plus the rule for moving between collaboration and adversarial review (`knowledge/collaborative-personas.md`), including three new peer-first roles your all-adversary catalog lacks.
3. **The Spike Protocol** — read-the-code and run-a-PoC discipline for unfamiliar APIs, SDKs, and MCP servers (`knowledge/spike-protocol.md`), so designs rest on established contracts rather than guessed semantics.

On top of these sit **thirty skills** that any developer can invoke — **seven delivery workflows** that carry a piece of work from idea exploration to shipped code (including `/create-proposal` and `/ui-design`), **eight supporting skills** (domain knowledge, persona tailoring, execution-graph planning, documentation, brownfield adoption, whole-repo forensic review, characterization-first migration, and code-hygiene review/fix), **three pack-lifecycle skills** that manage the pack itself (install, update, and extend), **three utility skills** — `/auditlog`, the command-line lens over the project's durable **audit & change log**, `/also`, which appends a late addition to the prior prompt without derailing the work in flight, and `/compile`, which turns the operator's prose into the harness-specific starting prompt without adding scope (stage CO-S0) — and **two prompt-log utilities** (`/prompts` and `/searchprompts`) for reusing prior prompts.

---

## The dual-mode model in one breath

> **A persona is a lens. A lens can be worn two ways.** In **Peer Mode** the lens *builds the best possible thing*; in **Adversary Mode** the same lens *attacks the proposed thing*. Ideation runs in Peer Mode, review runs in Adversary Mode, and the integrity rule survives intact: **the author never clears its own hard veto** — even when one model plays both roles in sequence, the critique is generated from a structurally separate seat.

Every adversary in your catalog already implies its peer counterpart (the C# Developer who *attacks* non-idiomatic code is the lens that *writes* idiomatic code in a pairing). Three roles have no adversarial twin and are defined fresh: the **Orchestrator** (runs the protocol and the gates), the **Product Strategist** (defines the right thing, testably), and the **Domain Researcher** (grounds every claim in evidence and runs the spikes).

---

## The Rigor Protocol in one line

> Don't answer the question. First **map** it, then **interrogate** it, then **ground** it in evidence, then try to **break** your own conclusion — only then answer, with your confidence and your residual risk attached.

Six stages: **0 Rush Interdiction** (no conclusion without a confidence label) → **1 OPEN** (map the whole before touching a part) → **2 INTERROGATE** (precise questions, one at a time) → **3 EVIDENCE** (establish contracts, verify by execution) → **4 DISCONFIRM** (try to falsify; convene the adversary) → **5 CONVERGE** (commit to exactly what the evidence supports). It scales with the tier: T0 runs a quick self-check; T2 runs all five with full evidence and an external adversary.

---

## Start here

You need Git, [uv](https://docs.astral.sh/uv/getting-started/installation/), network
access and a supported coding app with its own account/model access.
Open a terminal in the project you want to work on and copy this one line:

```text
uv run --no-config --no-project --script https://raw.githubusercontent.com/timianmalloo/ai-forward/v2026.10.19.4/bootstrap.py
```

Setup defaults to the upstream repository and `main`; no `--repo` or `--ref` is
needed for normal installation. An unmerged contribution is not yet in `main`:
reviewers use a committed local source or explicitly select its repository/ref.
That is review guidance, not another onboarding step.

The line works in Windows PowerShell/Command Prompt and macOS/Linux terminals,
with Git and uv on PATH. Setup reports `AI-Forward installed`, `AI-Forward updated`
or `AI-Forward already current`, with the revision and exact source commit.
Open that project in your coding app and start a fresh chat.

- **Claude Code / apps with the installed slash entry point:** `/deliver <your task>`.
- **Codex:** `$deliver <your task>`.
- **Copilot CLI:** find `deliver` with `/skills`, then use `/deliver <your task>`.
  You can also ask `Use the /deliver skill to <your task>`. If installed during a
  chat, use `/skills reload`, then `/skills info deliver`.
- **VS Code Copilot:** use `/deliver` when listed, or select/request the installed
  `deliver` skill. Agent Host sessions use skills rather than the older prompt files.
- **Grok Build / Antigravity:** select or request the installed `deliver` skill by name.

These are chat requests, not shell commands. For example:

```text
/deliver Add CSV export for the active project and filter. Export every matching task, not only the visible page. Do not add scheduling or new roles.
```

Replace the example with a change in your project. Say what correct behavior looks
like and what must stay unchanged. You do not need to choose each stage yourself.

`deliver` selects only applicable existing workflows; it does not run the whole
catalog or start a swarm by default. Required review, permission and release gates
still apply. Individual skills remain available for stage-by-stage work.

## When work pauses

A pause keeps the task open. The agent explains what is blocked, asks a specific
question and gives you a task id. Reply in the same chat. Approval applies only to
the decision you answered; it does not grant unrelated tool access or permission to
release. A blocking review needs the finding addressed and independently
re-reviewed; the author cannot clear its own veto.

In a fresh chat in the same project, use `/deliver resume <task-id>` (Codex:
`$deliver resume <task-id>`). In Copilot CLI, ask it to use the `/deliver` skill to
resume that id. The agent checks the request, project and evidence before reusing
valid work. Changed or missing inputs require an explanation and revalidation, not
an invented approval or a silent reset. Checkpoints are local, not automatically
shared with another clone or computer.

The local helper records and checks progress. It does not run the coding workflows,
authenticate people or judge the meaning of a result. Your coding agent and you
still need to inspect the actual evidence.

## Setup and updates without surprises

The one-line setup above uses the existing deployment map and checks installed
files. Append `--dry-run` to preview without writing to the project. Rerun it to
check or update the installed pack. To pin a version, use the same full commit id
in the wrapper URL and `--ref`; a branch can move.

Setup does not initialize Git, install project dependencies, commit, push, deploy
or change model/trust permissions. If instructions, hooks, Git settings or checks
conflict, it names the item and stops for review. Keep the existing file and
reconcile the specific conflict rather than deleting project policy or granting
blanket permissions. Use a credential-free repository URL with Git's credential
helper, not a token in a URL.

Manual reconciliation is an expert alternative, not a required extra step. Its
file-by-file map and revision changelog are in `adapters/INSTALL.md` in this bundle
and `docs/ai-forward-pack/INSTALL.md` after installation. The base knowledge pack
is included; local project rules still need to be preserved and reconciled.

## The workflow skills

| Skill | Turns… | …into | Peers (author) | Adversaries (review) |
|---|---|---|---|---|
| **/specify** | a prompt or idea | a testable spec with acceptance criteria | Product Strategist, Domain Researcher (+ Privacy if data) | Simplifier, Test Architect, Security (+ Privacy / AI Systems if data or model) |
| **/define-architecture** | a spec | the top-level architecture + ADRs | Enterprise + Distributed + Security architects, Tech Lead, Domain Researcher, AI Systems, Data & Persistence | full architect council, Patterns Expert, SRE, Privacy, Release |
| **/design-slice** | a spec/component | a detailed component design | Patterns Expert, Simplifier, language Dev, Domain Researcher, AI Systems (prompt/eval), Data (schema) | + Security, Distributed, Test Architect |
| **/implement** | a design | tested code + a Proof Pack | language Developer ⇄ Test Architect (pair), AI Systems (eval), Data (migration) | Test Architect, SRE, architects, Release Engineer |
| **/investigate** | a defect | a verified root cause, the failure class generalized + a phased repair plan (stops for your review) | SRE + Distributed Systems, Domain Researcher, Data | + Security, Test Architect, AI Systems, Release |

Each skill is the Rigor Protocol specialized to its phase — the same six stages, a different cast, technique, and artifact. Each produces a committed document from a template in `templates/`. Conditional members join by their triggers; the full twenty-three-lens casting per workflow is in `knowledge/collaborative-personas.md` §5 and the per-persona trigger predicates are in `knowledge/persona-audit.md` §8.7 and §9.3.

**Four supporting skills** bracket or assess the delivery five:

- **`/collectknowledge`** runs *before* design: from a domain and problem you state, it does deep, **sourced** research — industry state of the art, comparable solutions and problem framings, reference standards and data, a glossary — and saves a confidence-labeled knowledge base to `docs/knowledge/` that the team and the personas reason from. It bootstraps domain expertise instead of assuming it.
- **`/adddomainexperts`** tailors the *roster itself* to your project: it derives the domain from the repo's own evidence, proposes the subject-matter lenses that domain needs in peer and adversary modes (finance → accounting/controls; CFD → a fluid dynamicist; clinical → a clinical-safety lens), wires in any existing Claude domain skills that supply the capability, and — once you confirm the set — adds each as a §8-conformant persona and updates every roster artifact locally.
- **`/document`** produces and maintains the **documentation bundle**: a JavaDoc-style API reference plus sequence, class, layered-architecture, and component diagrams, in both committed markdown and a self-contained browsable HTML view — and installs an after-commit freshness check (owned by the **Documentation Steward**) so the docs never drift from the code.
- **`/forensicreview`** reconstructs an existing repo's architecture and full documentation from code, runs the complete architecture/design-slice/implementation council against it, and emits a separate evidence-linked review plus a prioritized backlog of risks, verified issues, and todos. It changes documentation, never production code, and stops for human triage.

The natural order: `/collectknowledge` → `/adddomainexperts` → `/specify` → `/define-architecture` → `/design-slice` → `/implement` → `/document`, with `/investigate` whenever a defect appears. See `commands/`.

---

## The persona roster (twenty-three lenses)

The swarm is **twenty-three lenses**: the eleven adversaries of your catalog, the three peer-first roles this pack added (Orchestrator, Product Strategist, Domain Researcher), **four more adversaries** added after an audit found uncovered failure modes — **AI Systems Engineer** (prompt/model/eval/tier), **Data & Persistence Architect** (schema/migration/integrity), **Privacy & Data Governance Counsel** (data lifecycle, distinct from security-as-threat), and **Release / Deployment Engineer** (the path to production and back) — and **four added in a UI/app + documentation expansion**: a **Mobile App Developer** (iOS + Android) and a **Native Desktop Developer** (macOS + Windows) as conditional-convene platform lenses, a cross-platform **UX & Accessibility** lens (a conditional hard veto on a11y), and the **Documentation Steward**. The audit proves each gap against your own governance lenses and anti-patterns, reasons over the requested platform/UX personas (reframing "iPhone/Android · Mac UX · PC UX" into two platform lenses + one cross-platform UX lens — §9), and rejects the seats not worth adding — see `knowledge/persona-audit.md`.

Every lens is rendered as a uniform, actionable **persona card** in `knowledge/persona-cards.md`: each carries a machine-routable *convene-when* trigger, a Peer-Mode deliverable, a finding **severity scale** (Blocker/Major/Minor/Nit), a **falsifiable veto-clears-when** predicate, the anti-patterns it owns, and one shared output contract. A deterministic veto-conflict rule (with true hard-vs-hard ties escalated to you) is in the audit's Persona Operating Standard. Subject-matter experts *beyond* these twenty-three — finance, CFD, clinical, legal — are added per project by **`/adddomainexperts`**, conforming to the same card schema so they slot into the casting and the gates exactly as the general lenses do.

---

## What's in the pack

```
ai-forward-pack/
├─ README.md                         ← you are here
├─ OVERVIEW.md                       ← start here: how to install, what's inside, how to use the skills
├─ research-synthesis.md             ← the research: industry/OSS comparison, reasoning
│                                       disciplines, and the gap analysis vs your pack
├─ adapters/managed-blocks/          ← ready-to-paste CLAUDE.md / AGENTS.md blocks (the wiring)
├─ scripts/                          ← docs-graph.py (graph mechanics + grounding packets, V18) + docs-explorer-core.js (deterministic browser state/layout) + bounded_process.py (subprocess limits/deadlines) + audit-log.py + prompt-log.py + foundation-check.py + pack-doctor.py + scrub.py + design-lint.py + obsidian-setup.py (Obsidian lens + dependency-free graph analysis) + graphify-setup.py (code knowledge graph + code/doc join) + coord-core.py (the coordination layer) + session-profile.py (the profiler; reads the sub-agent store) + run-verify-gates.py (every gate, one status) + conductor-join.py (the join as a script) + verify-no-conflict-markers.py + verify-no-new-console-launches.py
├─ evals/                            ← the pack's own regression suite: golden tasks + objective trajectory assertions per skill
├─ ci/docs-health.yml                ← reference GitHub Actions workflow: graph validate + freshness gate + foundation drift
├─ knowledge/
│  ├─ rigor-protocol.md              ← the staged reasoning discipline (the spine)
│  ├─ collaborative-personas.md      ← dual-mode model + the 3 new peer roles
│  ├─ spike-protocol.md              ← read-the-code + run-a-PoC for unfamiliar contracts
│  ├─ persona-audit.md               ← roster gap analysis + the Persona Operating Standard
│  ├─ persona-cards.md               ← all 23 lenses as uniform, actionable cards
│  ├─ observability-and-instrumentation.md  ← logging/tracing/error standard (OTel data model, error codes, RFC 9457)
│  ├─ knowledge-visualization.md     ← the knowledge graph + Docs Explorer standard (V1–V18): frontmatter-as-record, derived index, three projections, ownership/freshness SLAs, glossary + relation registry, graph-aware grounding, change-impact propagation, decision notes, the docs script bundle
│  ├─ audit-and-change-log.md        ← the Audit & Change Log Standard: a durable, committed history (docs/audit/) every skill writes to — the audit log (every prompt/skill/script) + the change log (design decisions, with git before/after); the session-history counterpart so work compounds across sessions
│  ├─ specification-standards.md     ← spec standards (S1–S10): the three layers — Functional (ISO 29148 + Gherkin + ISO 25010) / UX (Garrett Structure+Skeleton: IA, flows, wireframes) / UI — their bottom-up dependency and separate owners
│  ├─ ui-archetype-grammar.md        ← UI Archetype Grammar (G1–G16): a hardened two-layer grammar that selects a UI's archetype (routing/temporal/data) as a determinism control for code generation; composes with U1–U20
│  ├─ ui-archetype-catalog.md        ← 16 UI/UX archetypes with exemplars, canonical Archetype Signatures, and model-agnostic codegen descriptors
│  ├─ ui-interaction-design.md       ← UI & interaction design standard (U1–U20): token systems, the Elegance-Formula craft, HAX 18 + Shape-of-AI patterns for AI UIs, WCAG 2.2 AA, per-medium excellence
│  ├─ FOUNDATION.md                  ← provenance manifest for the 7 vendored base-pack docs (normalized hashes, known divergences)
│  └─ + 7 vendored Agent-Knowledge-Pack foundation docs (so the bundle is self-contained):
│       body-of-knowledge · rules-of-the-road · persona-catalog · layered-optimized-architecture ·
│       engineering-governance · testing-strategy · csharp-style-guide
├─ commands/                         ← the thirty skills (SKILL.md + reference/ each)
│  ├─ specify/  define-architecture/  design/  implement/  investigate/
│  ├─ collectknowledge/              ← deep domain research before design → docs/knowledge/
│  ├─ adddomainexperts/              ← tailors the roster to your project's domain
│  ├─ document/  forensicreview/     ← truth-to-code docs + whole-repo review/backlog
│  ├─ auditlog/                      ← the CLI lens over the audit & change log (last-N, search, recall-and-redo)
│  └─ addpacktorepo/  updatepack/  extendaibundle/  ← pack-lifecycle: install · update · extend the pack itself
├─ templates/                        ← the committed artifacts each skill produces
│  ├─ spec · architecture · design · adr · investigation · proof-pack · domain-expert
│  ├─ knowledge-base (for /collectknowledge)
│  ├─ audit-explorer.html (for /auditlog — the searchable timeline viewer)
│  └─ documentation-bundle + doc-viewer.html (for /document)
├─ adapters/
│  ├─ INSTALL.md                     ← manual wiring for Claude Code, Copilot, Grok Build, Antigravity, or any combination
│  ├─ grok/grok-surface.md           ← Grok Build path map (deployed to .grok/rules/)
│  ├─ antigravity/agy-surface.md     ← Antigravity path map (deployed to .agents/rules/)
│  ├─ claude-code/agents/            ← 11 agents: 3 peers + 4 governance adversaries + 4 UI/app & docs lenses
│  └─ copilot/
│     ├─ agents/                     ← the 11 upgraded adversaries (emit the §8 verdict shape)
│     └─ prompts/                    ← 17 prompt wrappers, one per skill
└─ examples/
   └─ finance-repo/                  ← a worked /adddomainexperts result (3 domain experts)
```

> Deploy **all 23 agents to every host** (the 12 claude-code-sourced lenses plus the 11 upgraded adversaries), every skill as a Claude skill, a Copilot prompt, a Grok project skill (`.grok/skills/`), and an Antigravity skill (`.agents/skills/`), and the knowledge docs as Claude knowledge *and* Copilot `applyTo:"**"` instructions (Grok and Antigravity read the shared `.claude/knowledge/` copies) — the deployment map is `adapters/INSTALL.md`.

---

## Install (summary — full guide in `adapters/INSTALL.md`)

**Install with the one-line setup above, or reconcile manually.** For the manual alternative, copy each source to its mapped destination per **`adapters/INSTALL.md`** (knowledge, skills, the 23 agents, templates, the Docs Explorer template (available for the first content-creating skill to instantiate at `docs/index.html`, not copied by install)), and paste the managed blocks from `adapters/managed-blocks/` into `CLAUDE.md` / `AGENTS.md`. Updates follow the **`changes` changelog in `adapters/INSTALL.md`'s frontmatter** — "what changed since the last version" — so a refresh re-copies exactly the changed sources and re-pastes the marked blocks, rather than diffing the whole tree.

Both tools share one model: **knowledge** = always-on reference, **skills** = workflow logic, **agents** = personas, **commands/prompts** = thin entry points. Only locations differ.

- **Claude Code:** `CLAUDE.md` + `.claude/{knowledge,skills,agents,commands}/`. Skills auto-apply by description; spikes run under `spikes/`.
- **GitHub Copilot:** `AGENTS.md` + `.github/{instructions,prompts,agents}/`, using `applyTo` globs to scope instructions (C# style on `**/*.cs`, the Rigor Protocol on `**`). Composes with the GitHub Spec Kit if you use it.
- **Grok Build:** `AGENTS.md` (native project rules) + `.grok/{skills,agents,hooks,rules}/`. Personas spawn with `spawn_subagent` (`subagent_type` = persona `name`). Knowledge is the shared `.claude/knowledge/` copy — not wrapped into `.grok/rules/` (CTX-B). Project hooks need folder trust (`/hooks-trust`).
- **Antigravity (agy):** `AGENTS.md` (native project rules) + `.agents/{skills,hooks.json,rules,skills.json}`. Auto-discovers skills from `.agents/skills/`. Hooks enforce re-read guard and session start. Knowledge is the shared `.claude/knowledge/` copy.
- **Several hosts:** keep knowledge and templates as one source of truth in `docs/`; each host's skills reference the same files, so the protocol and personas stay identical.

**Smallest viable install:** the Rules of the Road, the Rigor Protocol and Collaborating Personas, the `/specify` and `/implement` skills, and the orchestrator + product-strategist + domain-researcher + test-architect agents. That alone gives you the rush-interdicting spine, the peer/adversary switch, and a spec-to-tested-code loop.

---

## How it fits the Agent Knowledge Pack

This pack is an **extension, not a replacement**. It speaks your pack's vocabulary throughout — the three Prime Directives (D1 correctness over completion, D2 no guessing at contracts, D3 verification never self-certified), Coning and Iterative Critical Thinking, the Proof Pack and the phase gates, the capability tiers and the LOA principles P1–P11 and conformance criteria C1–C11, the persona names and the veto matrix, the Testing Strategy triggers, and the Deviation Protocol. The eleven adversaries ship with your existing pack; this one adds their peer mode, three new authoring personas, four further adversaries that close audited coverage gaps, four more for the UI/app and documentation surface, a Persona Operating Standard that makes every lens uniform and machine-routable, the reasoning protocol they all run, and the thirty skills that put them to work.

New here? **`OVERVIEW.md`** is the practical start — how to install, what's inside, and how to use the skills. Then `research-synthesis.md` for the *why* behind every choice, `knowledge/rigor-protocol.md` for the *how*, and `adapters/INSTALL.md` to wire it in by hand.
