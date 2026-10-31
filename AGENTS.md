# aiq

Project conventions live here. The AI-Forward Pack's reasoning stack is wired in below.

<!-- AI-FORWARD-PACK:BEGIN (managed block — keep this block intact when reconciling; replace it wholesale on pack updates) -->
## AI-Forward Pack + Agent Knowledge Pack

**Codex:** read `docs/ai-forward-pack/codex.md` at grounding. Skills are in
`.agents/skills/<name>/SKILL.md`; invoke `$collectknowledge`, `$specify`, or `$<name>`.
The pack's `/name` notation denotes that skill, not a registered Codex slash command.
Read the foundation and relevant standards from `.claude/knowledge/` as directed by
that guide; Copilot's `applyTo` files are not automatically loaded by Codex.

**Muse:** read `docs/ai-forward-pack/muse-surface.md` at grounding. Muse loads
this `AGENTS.md` (it takes precedence over `CLAUDE.md` here) and discovers skills
from the shared `.agents/skills/<name>/SKILL.md`; invoke by loading the skill
(`read_skill <name>`). The pack's `/name` notation denotes that skill, not a
registered Muse slash command. Read the foundation and relevant standards from
`.claude/knowledge/` as directed by that guide; Copilot's `applyTo` files are
not automatically loaded by Muse, and Muse has no native pack hooks — the
surface doc names the manual steps that replace them.

*Knowledge docs are cited below by their Copilot path (`.github/instructions/<name>.instructions.md`). In Claude Code the same document is `.claude/knowledge/<name>.md` — `CLAUDE.md` is `@AGENTS.md` plus a short addendum that says so (INSTALL 1.1). In Grok Build the same document is `.claude/knowledge/<name>.md` (shared; do not wrap it into `.grok/rules/`). Skills are `.grok/skills/<name>/SKILL.md`; personas are `.grok/agents/<name>.md`, spawned with `spawn_subagent` using the persona `name` as `subagent_type`. The Grok path map is `.grok/rules/grok-surface.md` (INSTALL 1.7). In Antigravity (agy) the same document is `.claude/knowledge/<name>.md` (read on-demand via `view_file`); skills are `.agents/skills/<name>/SKILL.md` (and `.agents/skills.json`); personas are enacted inline, spawned via `invoke_subagent` (`self`), or registered via `define_subagent`. The Antigravity path map is `.agents/rules/agy-surface.md` (INSTALL 1.8).*

This repository uses the **Agent Knowledge Pack** and the **AI-Forward Pack**. Honor them on
every non-trivial task.

- **No guessing (the standing rule):** when you do not know, there are exactly **three** moves —
  **check it** (the default; open the file, run it, read the signature), **mark it** (an inline
  `assume:` carrying the belief, what would confirm it, and what breaks if it is false), or
  **ask** (only when consequential *and* unresolvable by you). Proceeding on an unmarked belief
  is not a fourth option. **An assumption that was not written down before the work is not an
  assumption — it is a guess**, so "I assumed X" is unavailable afterwards. Watch the tells —
  *should be · presumably · typically · I believe · it follows the pattern* — and any API,
  version, path, count or boundary behaviour asserted without opening it. **Verified means you
  observed it, never that it is likely**, and a citation, a tool output, a sub-agent's report or
  an `INFERRED` edge does not promote a claim. Ask of every load-bearing statement as you write
  it: *"if this is wrong, how would I find out, and when?"* — if the answer is "in production",
  check it now. `.github/instructions/no-guessing-protocol.instructions.md` (NG1–NG11).
- **Reasoning spine:** the Rigor Protocol — see `.github/instructions/rigor-protocol.instructions.md`.
  Map, interrogate, ground in evidence, disconfirm, then converge; label every claim with its
  confidence.
- **The standing method (unconditional):** the absence of the words *"use the Rigor Protocol"*,
  *"convene the personas"* or *"run /design-slice first"* is **not permission to skip them** — an
  interactive prompt carries the same standard as a skill run; only the ceremony scales with the
  tier, never the rigor. Never decide in a silo: ground in the **whole intent, end to end**, name
  what the decision constrains, and write down the **surface list** a change must reach before you
  start (store → model → service → projection/wire → client type → UI → compute reader). Prove the
  **rendered surface** and the **consistency across surfaces**, not just the units; a gate's green
  result is evidence the gate passed, not that its contents passed; an exit code is not a result —
  read the state; and never assert the shape of *our own* code from memory — open the file or label
  the claim Inferred. `.github/instructions/end-to-end-integrity.instructions.md` (E1–E18).
- **The data model is the highest-priority decision.** Before any surface, endpoint or table:
  model the domain in **DDD** terms (bounded context, ubiquitous language, entities vs value
  objects, **aggregates bounded by an invariant**, small, referenced by identity). Then choose the
  **durable representation** — by default core entities as **dimensions** and change-over-time as
  **append-only facts**, so history and the audit trail *are* the data rather than a shadow schema,
  and new measures are new rows/columns rather than rewrites. **Declare the grain before the
  columns** ("one row is exactly one ______"), classify every measure additive / semi-additive /
  non-additive, decide the **history rule per attribute** (Type-2 whenever a change would rewrite
  the meaning of a past record), **derive don't store** (two definitions of one quantity is a defect
  signature), and snowflake only when the domain demands the entity. Migrate expand-migrate-contract;
  a backfill never guesses. `.github/instructions/domain-and-data-modelling.instructions.md`
  (DM1–DM18); evidence in `docs/knowledge/domain-and-data-modelling/`; the **Data & Persistence
  Architect** holds the veto.
- **Isolate on concurrent-writer risk, not on session-newness.** Create and enter an own git
  worktree, on its own branch, only when another live agent/session or the pack's coordination
  runner might write the same checkout — two writers sharing one checkout is how a stash in one
  silently reaches into the other's uncommitted work. With **no** such risk — the checkout was
  provisioned for this task, or the prompt names it — work in place; that's the default, no
  exception needed (WT1b). Staying in a shared checkout under a real risk is the recorded exception
  (WT4). **A worktree used for a requested change is not done until it reaches the checkout the
  requester named**, merged or applied — or is handed back explicitly as not delivered (WT1c). More
  trees are fine when the work needs them; each follows the same lifecycle. **Cleanup is the half
  that rots**, so it is fail-safe: a tree is removable only when it is not primary, not your cwd,
  **clean including untracked**, carries no commit that exists nowhere else, and is unheld —
  anything else is *reported, never removed*, and deletion is opt-in (`--remove`).
  `coord worktree new|list|cleanup`;
  `.github/instructions/session-worktree-discipline.instructions.md` (WT1–WT12).
- **Agent coordination (the Owner / Coordinator / Sub-Agent doctrine):** seats and the capability floor,
  the two control relationships, the invariants each traceable to a measurement, the protocol objects
  (session card, delegation contract, seam request, decision request → ruling, leader designation, join
  state, mail, board), the kick ladder, and the three stages every skill cites — **CO-S0 compile first,
  CO-S1 declare the seat, CO-S2 a stop is a message** — plus **CO-L** leadership by designation in a git
  ref. The tree comes before the first spawn (CO15). `.github/instructions/agent-coordination.instructions.md`
  (CO1–CO17, CO-S0–CO-S2, CO-L); `coord leader|mail|board|request`.
- **Continuous improvement (a primary directive):** every bug you create, every mistaken
  assumption, and every correction you receive is captured — as a **class, not an instance** — in
  `docs/lessons/defect-classes.md`, and converted into a **control** that fails when the shape
  recurs. Run **class → sweep → derive → prevent** in writing on every defect fix; a fix that stops
  at the instance is not finished. **A lesson recorded as prose is a memoir** — it only counts once
  it is a test, a gate, a lint rule, or a file that is always loaded. Read the register at
  grounding. `.github/instructions/continuous-improvement.instructions.md` (CI1–CI12).
- **Smallest correct (build discipline):** climb the **Solution-Selection Ladder** before writing —
  YAGNI → reuse-in-codebase → stdlib → native → installed dep → one line → minimum — never cutting
  validation, security, accessibility, or the failure-mode/test floors; mark bounded shortcuts with an
  inline `simplify:` comment (ceiling + upgrade trigger); ceremony scales with the tier (T0 code-first,
  T1/T2 full artifacts). `.github/instructions/solution-selection-ladder.instructions.md`; the
  Simplifier is its adversarial mirror. **No dead code survives the turn:** commenting code out while
  you work is a transient scratch, but a turn never closes with commented-out or dead code in the tree
  — delete it (version control is the archive, not the working tree), and sweep the class, not only the
  line you noticed. `.github/instructions/communication-and-task-discipline.instructions.md` (CT18a);
  defect class **HYG-A**.
- **Start with the goal, then plan the turn — the two-step front matter (universal):** every
  non-trivial turn opens, **before the first substantive tool call**, by writing the **goal state**
  — **Goal · Done when · Not in scope · Tier · Fan-out cap** — the preventive mirror of the *Completed / Remaining /
  Next* close; then it plans the whole turn with **`/optimize-graph` once across all tasks**, whose
  Stage-0 triage means a closed question **skips planning, answers, and stops** (a multi-task turn
  gets the bounded graph). **Autonomy is latitude in the *how*, never the *what*** — a question is
  not a licence to author a change, and a gap found en route is a *finding*, never a new goal. An
  explicit **stop** (*stop · wait · step back · that's not what I asked*) **ends the current track
  and voids any goal the agent authored**; asking how to proceed on the halted track is re-entry,
  not compliance. A harness **"you haven't finished" reminder or autopilot nudge is a cap firing,
  not a termination argument** (GO9): re-read the goal state — if met, conclude. Tells: *"so I can
  wire it in" · "let me look at the structure" · a closed question whose answer is in hand while the
  turn continues · resuming after a stop because a reminder arrived · being unable to say in one line
  what would end this turn · a council, a research agent or a verification run on a turn that declared
  no tier · re-invoking a skill already active in this turn · re-reading a file already in context.* A
  **substantive** turn (one that changed the repo or the plan) records
  its goal-state in the audit log — full prompt, `goal`, `done_when` (AL5b) — the presence signal
  `/dream`'s PACK-O miner reads.
  `.github/instructions/communication-and-task-discipline.instructions.md` (CT19–CT25); defect
  class PACK-O.
- **How you write, and how much you take on:** **compress the expression, never the obligation.**
  Simplified technical English — short sentences, common words, active voice, one idea per sentence,
  **result first**. Every shell call carries a one-line intent — the only reasoning trace a profiler can read
  (CT26). **A gate's exit status is never behind a pipe** (`pipefail`, or the gate on its own line; at a
  join only `run-verify-gates.py` / `conductor-join.py`), **a multi-line program is a file, then a run —
  never a heredoc**, and **a sub-agent never calls `EnterWorktree`** — three line shapes measured at
  168 / 70 / 8,143 s and counted by the profiler (CT27). No monologue, no self-encouragement, no rhetorical transitions, and never announce
  a step and then take it; an interim update earns its place only by carrying a verified result, a real
  blocker, a decision, or the next action. But **concision never drops a confidence label, a citation,
  an assumption, a residual risk, or a correction** — and the response channel is compressed while a
  **committed artifact is written for a reader who was not in the session** (so L8's anti-prose rule is
  scoped to T0 inline prose and must never thin a required artifact). Stay on the requested outcome:
  smallest change, smallest sufficient proof; name the failure any unrequested test/review/control
  prevents or drop it; reviewer findings are advice, not automatic scope; **capture new ideas as next
  steps rather than chasing them**; stop when the result is proven. **Proportionality never reaches the
  floors** — a triggered hard veto, the Testing-Strategy union, the E7 surface list, red-first, and the
  audit entries are not discretionary.
  `.github/instructions/communication-and-task-discipline.instructions.md` (CT1–CT18, CT27).
- **Plan the shape before you execute it:** for work beyond two steps, or containing a loop, a
  fan-out, or a triggered gate, model it as an **execution graph** — real dependencies only (delete
  incidental ordering), **shorten the critical path before widening the graph** (`Tₚ ≥ T∞` always),
  parallelise only what passes the **independence** *and* **coupling** tests and only under a bounded
  fan-out contract (width cap, transient-retry, per-branch exit, join rule, containment), collapse or
  promote nodes to the right granularity, and give **every loop a termination variant** — a cap is a
  circuit breaker whose firing is a *defect signal*, never a termination argument. **The objective is
  lexicographic and speed is last: (1) completeness and rigor · (2) token cost · (3) speed**, each
  optimized only subject to those above it — so a *slower* plan is acceptable **only when completeness
  ↑ AND rigor ↑ AND tokens ↓** (a conjunction), and slower-while-unchanged is pure loss. **Rigor floors
  are immovable nodes: optimization may reorder them, never remove them.** Record planned vs actual so
  the next plan is better.
  `.github/instructions/execution-graph-optimization.instructions.md` (GO1–GO19); evidence in
  `docs/knowledge/graph-and-loop-engineering/`; the workflow is the `optimize-graph` prompt.
- **Instrumentation over inference (a standing bias, and a gate):** when you want to know how
  something behaves, **measure it — do not reason about it**. An uninstrumented system does not become
  unknowable, it becomes one you *reason* about, which is worse: an answer with no error bar and no
  way to be surprised. So **a feature is not done until its behaviour is measurable by default** —
  emitted on the normal path, no flag, no re-run. Name the questions an operator will ask (how long,
  how often, how much, which path, did it fail) **before** building, and give each a named emitting
  source; instrument the **cost axes** (latency, spend/tokens, volume, failure rate), not just
  correctness. **Never infer a deployed system's behaviour from its source** — that is E15 pointed at
  runtime; the tells are *"it should be fast · probably rare · typically about · mostly · obviously
  the bottleneck"*. Where a measurement genuinely does not exist you may model, but label it
  **Inferred**, state the model, and **name the gap that forced it** — then close the gap rather than
  modeling around it. Every measurement path **degrades to "not recorded", never to a plausible wrong
  number**. This applies to the agent's own work too: a skill marks its start at grounding
  (`audit-log.py start --session <id>`) so the closing audit entry records **duration_seconds**
  automatically — measured, not modeled, with no flag to remember. **One marker measures one run**:
  the closing entry consumes it, so each run re-marks at its own grounding rather than a session
  marking once (AL4a); the marker is keyed by session **and** skill, a logged prompt never consumes
  one, and the pack's `SessionStart`/`SubagentStart` hook marks the session seam so an unmarked run
  still measures from its true start (`duration_source: session-start-hook`).
  `.github/instructions/instrumentation-over-inference.instructions.md` (IO1–IO12).
- **Personas (dual-mode):** author in Peer Mode, review in Adversary Mode; the author never
  clears its own hard veto. Agents in `.github/agents/`; the operating standard in the
  `persona-audit` / `persona-cards` instructions.
- **Workflows (30):** the prompts in `.github/prompts/` — twenty-five reasoning workflows
  (`deliver`, `collectknowledge`, `adddomainexperts`, `create-proposal`, `specify`, `define-architecture`, `design-slice`, `ui-design`,
  `visualize`, `implement`, `investigate`, `document`, `adopt`, `forensicreview`, `code-hygiene`, `migrate`,
  `updatepack`, `addpacktorepo`, `extendaibundle`, `optimize-graph`, `dream`, `apply-learnings`, `session-profiler`,
  `prepare-for-coordination`, `execute-with-coordination`),
  the `auditlog` lens over the audit & change log, the `also` turn-control utility, the `compile` prompt compiler (CO-S0), plus two prompt-log utilities, `prompts` and
  `searchprompts`. Templates: `docs/ai-forward-pack/templates/`.
  `/deliver <outcome>` (Codex `$deliver`) is an optional conditional entry point across
  these workflows, preserving human gates and validated pause/resume; individual skills
  remain first-class routes.
- **Human-facing artifact views:** whenever a skill creates or materially updates Markdown for
  people, it also refreshes the sibling HTML view with
  `docs/ai-forward-pack/scripts/render-markdown.py` (V19). Markdown remains canonical.
- **Prompt reuse (utility):** `/prompts` opens the audit log's prompts as an arrow-navigable stack
  (newest on top; → expand, ← collapse, Enter reuse) and `/searchprompts` searches them; reuse
  copies the chosen prompt to the clipboard to paste-and-edit. Engine:
  `docs/ai-forward-pack/scripts/prompt-log.py` (stdlib) — a **reuse lens over the *same* committed
  audit log** (`docs/audit/audit-log.jsonl`), not a second store. When the user gives a
  **substantive** request, log it with `prompt-log.py add "<text>"` (it writes a `kind:prompt`
  audit entry via `audit-log.py`) so it is recallable (no CLI hook auto-captures prompts; stop when asked).
- **Unfamiliar APIs/SDKs/MCP servers:** run the Spike Protocol before depending on a contract.
- **Specification:** `/specify` produces **one spec with three layers** — Functional (what &
  why), UX (how it works: IA, user flows, structure), UI (how it looks) — written bottom-up,
  UX before UI, each absent layer marked N/A — `.github/knowledge/specification-standards.md`; the
  UX Researcher/IA holds the UX-specification veto, UX & Accessibility the UI veto.
- **UI:** whenever the work has a user-facing interface (any medium), the **UI & Interaction
  Design Standard** governs excellence — token systems, complete component states (incl.
  empty/loading/error), HAX + Shape-of-AI patterns for AI UIs, WCAG 2.2 AA, performance budget —
  `.github/instructions/ui-interaction-design.instructions.md` (U1–U20); the UX & Accessibility
  lens holds the veto.
- **UI archetype:** for a user-facing UI, select the **archetype** (routing/temporal/data) as a
  determinism control before generating — `.github/knowledge/ui-archetype-grammar.md` (G1–G16) + the
  archetype catalog; record the Archetype Signature in the spec, build to its facet rules, and
  verify it against the *shape of the task* even on an existing screen (reading is parallel;
  entering is serial).
- **UI craft (`ui-design`):** to create, review or elevate a surface, run the `ui-design` prompt —
  direction in words before pixels, the design system before the screens, a **self-contained
  dependency-free mockup** that renders the hard states with a **review harness** (persona ·
  viewport · state · theme · reduced motion), and a **rubric critique** (location · dimension ·
  severity · evidence · fix · confidence) run structure-before-surface, ending in a ranked plan.
  Measure before you diagnose, and self-check against the generic-AI-look tells.
  `.github/knowledge/ui-design-craft.md` (DX1–DX25).
- **UI craft detection (the control):** the craft floor is not only prose — the **deterministic,
  LLM-free 59-rule detector** (`ui-craft-gate.py`, wrapping Impeccable) reads your `DESIGN.md`
  natively and enforces *outward* against the built source what `design-lint.py` only checks
  inward: every off-token colour, font, size and radius, plus the mechanized generic-AI-look
  tells, hierarchy, motion, copy and overflow rules. Run it at `ui-design` Stage 3 (it **is**
  the measurement), fold its findings into the rubric with the accessibility and token severity
  floors, and gate CI on it — **a lesson recorded as prose is a memoir** (CI6). A clean run is a
  **floor, never a verdict**: it cannot see archetype fit, IA, whether the hard states exist at
  all, or whether the copy is true.
  `.github/knowledge/ui-craft-detection.md` (CD1–CD20).
- **Generated visual assets:** imagery, personas and motion a UI *contains* may be generated;
  the **interface itself may not** (image models render illegible text and invented controls).
  Direction in words first, then optionally a **visual world** that makes the brief concrete —
  mood **never** structure, supplementing **never** replacing real named references. Generate
  once, **download, optimize and commit** (provider results expire and a re-rolling asset is
  non-determinism in a deterministic artifact); every asset carries a manifest entry with its
  verbatim prompt, preset, cost, **alt text** and disclosure; never upload a real person's
  likeness or customer data.
  `.github/knowledge/ui-visual-assets.md` (VA1–VA22).
- **Where to start on any UI job:** `docs/ui-guide.html` (also listed under **Knowledge
  surfaces** in the Docs Explorer) is the how-to layer over all of the above — the layer stack,
  a job-to-path picker, the `ui-design` stages, the command sheet, an archetype picker, the veto
  table, the tells, and where artifacts land. It is **derived** from the standards and never
  authoritative over them.
- **Running the pack's scripts:** commands are written `python3 <script>` (the POSIX name, and
  the shebang on every script). **On Windows that fails — use `python` or `py -3`**: python.org
  ships no `python3.exe`, and the `python3` present there is a Microsoft Store alias that is not
  Python (it prints *"Python was not found"* and exits `9009`). This is a substitution, not a
  missing install. `pack-doctor.py`'s `python interpreter` check names the working form for the
  current machine.
- **Testing:** the Testing Strategy governs what to test and what counts as proof —
  `.github/instructions/testing-strategy.instructions.md`; the Test Architect enforces it.
- **CI & test efficiency:** best coverage at minimum time and cost — profile before optimizing (the
  bottleneck is rarely the suspect), rings of integration (fast every-push / slow at-readiness /
  post-merge), the cheapest minute is the one never billed (runner multiplier, build-once, don't run
  the full gate twice), and cheaper is **never** weaker (no muted steps, fail-closed required check) —
  `.github/instructions/ci-and-test-efficiency.instructions.md` (CE1–CE26); the SRE owns it, the Test
  Architect holds the veto that speed never costs coverage.
- **Instrumentation:** structured, trace-correlated telemetry in the OpenTelemetry data model,
  stable error codes, RFC 9457 error responses —
  `.github/instructions/observability-and-instrumentation.instructions.md`; the SRE enforces it.
- **Docs Explorer:** every knowledge/content artifact carries its graph metadata in YAML
  frontmatter (id, type, owner, typed links, review-by); the derived index `docs/docs-index.js`
  is regenerated from it and browsable at `docs/index.html` (hierarchy · graph · mind map ·
  health). Skills write frontmatter + sync the index as their last action; material changes flag
  inbound neighbors review-suggested; sub-ADR decisions become linked decision notes in
  docs/notes/; grounding traverses the graph; all graph mechanics run through the script bundle
  docs/ai-forward-pack/scripts/docs-graph.py — never ad-hoc scripts (V2/V10/V13–V18).
- **Audit & change log:** the project keeps a durable, committed history so work compounds across
  sessions — every meaningful prompt/skill/script in `docs/audit/audit-log.jsonl` (the Audit
  Mandate: every skill appends an entry as its last action) and every design decision in
  `docs/audit/change-log.jsonl` (collectknowledge/define-architecture/design-slice/migrate capture the
  prompt, result, and git before/after). Browse the searchable timeline at `docs/audit/index.html`
  or via the `auditlog` prompt (last-N, search, recall-and-redo, full-history↔changes toggle); all
  writes go through `docs/ai-forward-pack/scripts/audit-log.py`; the standard is
  `.github/knowledge/audit-and-change-log.md`. A new session reads it to learn what was done and why.
- **Obsidian lens (optional):** `docs/` is already a valid Obsidian vault — the same V2
  frontmatter drives Properties, Dataview and the graph view. Stand it up with
  `docs/ai-forward-pack/scripts/obsidian-setup.py` (`--check` · `--install-app` · `--init` ·
  `--analyze`): it commits the vault **config** (colour groups keyed to artifact `type`) and
  git-ignores the per-user **state** and plugin code. Obsidian stays a **reader** — frontmatter
  is the record, `docs-graph.py` the only writer, and no query is load-bearing in a canonical
  artifact (queries live only in `docs/lenses/`). `--analyze` computes hubs, exact betweenness
  bridges, components, orphans and structural gaps **dependency-free**, so the insight is never
  locked behind a plugin. `.github/knowledge/obsidian-lens.md` (OB1–OB14).
- **Code knowledge graph (optional, composes with the above):** **Graphify** (graphify.com,
  Apache 2.0, PyPI `graphifyy`) builds an **on-device** graph of the *code* — symbols, calls,
  imports, schemas — that an assistant queries instead of grepping, answering with `file:line`
  citations. It is the natural partner to the docs graph: **docs hold intent, code holds
  reality, and the expensive defects live in the gap.** Every edge is tagged `EXTRACTED` /
  `INFERRED` / `AMBIGUOUS`, which maps onto this pack's **Verified / Inferred / Flagged** — so a
  cited traversal is how you satisfy *"never assert the shape of our own code from memory"*
  cheaply, while remembering that **a citation is not a promotion**. Stand it up with
  `docs/ai-forward-pack/scripts/graphify-setup.py` (`--install` · `--init` · `--build` ·
  `--join`); `--init` writes a **repo-kind-aware** `.graphifyignore` (in a consuming repo
  `.claude/` and `docs/ai-forward-pack/` are the *only* copy and are kept). `--join` writes the
  code↔docs lens: documentation with no implementation, and risk with no governance.
  `.github/knowledge/code-knowledge-graph.md` (GK1–GK16).
- **Foundation:** the Body of Knowledge, Rules of the Road, Persona Catalog, LOA, and Engineering
  Governance are in `.github/instructions/` (always applied) — the constitution all of this rests on.
<!-- AI-FORWARD-PACK:END -->
