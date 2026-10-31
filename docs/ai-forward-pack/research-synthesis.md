# Research Synthesis — the *why* behind the AI-Forward Pack

*The industry and open-source survey, the reasoning-disciplines digest, and the explicit gap analysis against the Agent Knowledge Pack that produced this pack's design. Version 1.0.*

---

## 0. Scope and method

Two questions drove the research. First: **how do the strongest multi-agent software systems actually divide labor between collaboration and adversarial review**, and what is durable in that division versus what is an artifact of a particular framework? Second: **what disciplines outside software — critical thinking, systems thinking, precision questioning, root-cause analysis, investigative interviewing, and strategic elicitation — can be synthesized into a reasoning protocol that measurably slows an agent's rush to a plausible answer**?

The findings were then held against your existing pack (Body of Knowledge, Rules of the Road, Persona Catalog, Testing Strategy, Engineering Governance, Layered-Optimized Architecture) to find the *gaps* — because the goal was never to rebuild what you have, only to complete it. Everything below is attributed to its originators by name; sources are listed in §6.

---

## Part A — How multi-agent software teams are built

### A.1 The four reference architectures

**MetaGPT** encodes a software company as a fixed pipeline of role-specialized agents — product manager, architect, project manager, engineer, QA — each governed by a standard operating procedure, passing structured documents (requirements, designs, task lists) down the line. Its organizing slogan, *"Code = SOP(Team),"* captures the thesis: quality comes from making the *process* explicit and forcing each role to emit a structured artifact the next role can consume, rather than from a single agent improvising end to end. The lesson taken into this pack: **the human software roles evolved to catch specific failure modes; encode the roles and the artifacts they hand off, and you inherit that error-catching for free.**

**ChatDev** is the most directly relevant. It runs a virtual software company through phased, role-played dialogues — CEO, CPO, CTO, programmer, reviewer, tester, designer — and, critically, it separates a **cooperative designing phase** from an **adversarial testing/reviewing phase**. Agents reason *together* to design, then *attack* the result to test it. This cooperative-then-adversarial structure is the single most important external validation of the model this pack adopts: it is not a Claude-specific invention but a recurring pattern in systems that produce working software.

**CrewAI** organizes agents into a crew with a manager-worker hierarchy and explicit task delegation; **AutoGen** (Microsoft) frames everything as conversation — a `GroupChat` of agents, often with a human in the loop, converging through dialogue. Both contribute the same secondary lesson: **someone must own orchestration**. A swarm without a facilitator either deadlocks or lets the loudest agent's first framing win. This is why the pack defines an explicit **Orchestrator** as a first-class peer rather than leaving coordination implicit.

### A.2 The cost lesson

Serial multi-agent "chatter" — agents talking to agents across many turns, each carrying its own context — is expensive, and the literature and practitioner reports converge on the warning that naïve multi-agent loops can run an order of magnitude more costly per task than a single well-instructed agent. The practical consequence is that **IDE-integrated subagents that share context are cheaper than independent conversational agents**, and that **the number of personas convened must be proportional to the cost of being wrong**. Your Persona Catalog already encodes exactly this with its "how to convene" proportionality table and the tier system (Rules of the Road §0.2); the pack reuses it rather than inventing a parallel mechanism, and the Rigor Protocol explicitly scales its depth with the tier.

### A.3 The tooling surface (current)

Claude Code now exposes the full set this pack targets: **commands** (thin prompt entry points), **skills** (auto-applied workflow logic that takes priority over a same-named command), **subagents** (isolated-context specialists with their own system prompt and tool allowlist), **hooks** (deterministic enforcement), and **CLAUDE.md** (short always-true conventions) — and, as of early 2026, parallel subagents and agent-team features. GitHub Copilot mirrors this with `AGENTS.md`, `.github/agents/*.agent.md`, `.github/prompts/*.prompt.md`, and `.github/instructions/*.instructions.md` with `applyTo` globs, and composes with the GitHub Spec Kit's `/speckit.*` commands. The pack maps one-to-one onto both surfaces (see `adapters/INSTALL.md`), which is what makes it tool-portable.

### A.4 What this means for the swarm design

The durable, framework-independent findings are four: (1) **role specialization with structured handoffs** beats a monolithic generalist; (2) a **cooperative authoring phase followed by an adversarial review phase** is the recurring shape of systems that ship working code; (3) **orchestration must be an explicit role**; and (4) **panel size must track cost-of-error.** Your pack already had (1) and (4) and half of (2) — the adversarial half. This pack adds the cooperative half, the explicit orchestrator, and the protocol that runs inside every role.

---

## Part B — The reasoning disciplines digest

Each discipline below independently raises reasoning quality; the Rigor Protocol is their engineering synthesis, with each stage operationalizing the disciplines that fit it. The protocol's stages (`knowledge/rigor-protocol.md`) are referenced in brackets.

### B.1 Critical thinking — Paul & Elder

The Paul–Elder framework decomposes reasoning into **eight elements of thought** (purpose, question at issue, information, interpretation/inference, concepts, assumptions, implications/consequences, point of view) and judges it against **nine intellectual standards** (clarity, accuracy, precision, relevance, depth, breadth, logic, significance, fairness). The elements are a *checklist* for what any piece of reasoning contains; the standards are the *quality bar* it must clear. The pack uses element-decomposition as the structure of Stage 1 (OPEN) and the nine standards as the rubric the Orchestrator applies when deciding whether a stage's output is good enough to proceed. Its deepest contribution is **fairness** — the discipline of representing the strongest version of a view you intend to reject, which is exactly what an adversarial review must do to be honest rather than performative.

### B.2 Systems thinking — Meadows

Donella Meadows' synthesis: behavior emerges from **structure** — stocks, flows, **feedback loops** (balancing and reinforcing), and **delays** — and the highest-leverage interventions are rarely the obvious parameters. Her ranking of **leverage points** puts paradigms, goals, and rules far above numbers and buffers. The engineering translation the pack takes most seriously: **a correct local fix to a structural problem is still a wrong answer.** This is why Stage 1 requires a *systems sketch* before narrowing, and why `/investigate` distinguishes the proximate cause from the systemic cause and *prefers the systemic fix* over an administrative band-aid.

### B.3 Precision Questioning — Matthies & Worline (Vervago)

Precision Questioning supplies a taxonomy of analytic question types — clarification, assumption, basic-critical (data and its source), cause, effect/implication, evidence, and the go/no-go decision question — worked in a deliberate **one-question/one-answer** rhythm. Its central warning is the one the protocol treats as a load-bearing rule: **"Why?" repeated five times is vague.** Precision of question type beats repetition of a blunt question. Stage 2 (INTERROGATE) is built directly on this taxonomy, and it is why the protocol explicitly rejects the unstructured "5 Whys" in favor of choosing the *right* question type for what you need to learn.

### B.4 Root-cause analysis

The RCA literature offers several methods — **5 Whys**, **Ishikawa/fishbone**, **fault-tree analysis**, **change analysis**, and **events-and-causal-factors charting** — and its key meta-lesson is that **the method is chosen by the shape of the problem**, not applied by habit. But the *universal* step, the one a casual "5 Whys" routinely skips, is this: **every candidate cause must be verified against data as both necessary and sufficient before it is believed** (remove it and the symptom must disappear; introduce it and the symptom must appear). The `/investigate` skill and the investigation template enforce exactly this, and the disconfirmation record — the causes ruled out, with the evidence that ruled them out — is treated as part of the deliverable, not a footnote.

### B.5 Investigative interviewing — the funnel, cognitive interview, and PEACE

Three findings from evidence-based interviewing transfer cleanly to interrogating *problems and designs*:

- **The funnel technique:** start broad and open, narrow deliberately. Premature specificity introduces bias and skips data you didn't know you needed. This is the macro-shape of the whole protocol — the same "cone" your Body of Knowledge already names, here given named techniques for each segment.
- **The cognitive interview:** free recall *before* probing; vary the order; change the perspective; summarize each segment and surface contradictions. Stage 1's free-recall-first rule and the protocol's "re-ground and name any contradiction at each gate" come from here.
- **The PEACE model:** the modern, non-coercive, evidence-based interviewing standard. Its empirical verdict is unambiguous and worth stating plainly: **rapport and structured open-then-narrow questioning outperform pressure and accusation. "Tough tactics" do not produce better information.** This finding directly shaped the pack's stance on what "adversarial" should mean (see §B.7 and Part E).

### B.6 Strategic elicitation — the disconfirmation principle (Scharff/SUE)

From the Scharff technique and the Strategic Use of Evidence (Granhag and colleagues) comes one transferable analytic move: to test a claim, **state it as a confident counter-claim and see whether it survives**, rather than asking an open question that telegraphs what you don't yet know. Applied to a *design or a hypothesis* — "this cache is the bottleneck; prove it isn't" — disconfirmation is a sharper test than an open "is the cache the problem?" Stage 4 (DISCONFIRM) uses this alongside falsification and self-consistency. **Its scope is strictly limited** — see Part E.

### B.7 The AI-native forms — Chain-of-Verification and self-consistency

The above disciplines have direct AI-native expressions. **Chain-of-Verification** (draft → generate verification questions → answer them *independently of the draft* → regenerate keeping only what verified) is your directive D3 ("verification is never self-certified") expressed as a reasoning move. **Self-consistency** (sample multiple independent reasoning paths and look for agreement) is a cheap disconfirmation check. Both carry one known limitation that shaped the whole pack: **a model that cannot find its own error gains nothing from reviewing itself.** That is precisely why high-stakes conclusions are routed through *execution* (the Spike Protocol, verify-by-running) and through a *structurally separate adversary* (a subagent with the critic's system prompt), not through self-review alone. It is also the deepest reason the pack preserves your integrity rule that **the author never clears its own hard veto.**

---

## Part C — Gap analysis against the Agent Knowledge Pack

### C.1 What your pack already does excellently (and the pack leaves untouched)

- **A reasoning constitution** with three Prime Directives (correctness over completion; no guessing at contracts; verification never self-certified) and a named method (Coning + Iterative Critical Thinking). This is rare and strong; the Rigor Protocol *composes on top of it*, never around it.
- **Contract due-diligence** across eleven dimensions with a source-of-truth hierarchy and verify-by-execution. The Spike Protocol and the Domain Researcher are the *operational engine* for this, not a replacement.
- **Eleven world-class adversarial personas** with explicit lenses, interrogation sets, a veto matrix, and convening proportionality. This is the entire adversarial half of the swarm, and it ships unchanged.
- **A test discipline** (triggers → directives, coverage-as-a-floor, mock fidelity, the probabilistic-content vs deterministic-structure split) and a **governance companion** (ten SDLC lenses) and a **deep AI-architecture model** (tiers, eleven principles, archetypes, conformance criteria). The skills *reference* all of these at the relevant stage rather than restating them.

### C.2 The three gaps — and how the pack fills each

**Gap 1 — The catalog is all adversaries; there is no authoring half.** A flaw-finding council can review a proposal but cannot author one, and the strongest multi-agent teams (ChatDev especially) have an explicit cooperative phase. **Filled by** the dual-mode model (`knowledge/collaborative-personas.md`): a persona is a lens worn two ways — Peer Mode builds, Adversary Mode attacks — plus three peer-first roles the catalog structurally lacks (Orchestrator, Product Strategist, Domain Researcher), because orchestration, product definition, and evidence-gathering are authoring activities with no adversarial twin. The integrity rule is preserved: the author never clears its own hard veto.

**Gap 2 — There is no explicit discipline against the rush to a plausible answer.** The Body of Knowledge names the *anti-patterns* (Confident Guess, Plausible Hallucination, Premature Convergence, Unverified Green) but does not supply a staged *procedure* that makes rushing structurally impossible. **Filled by** the Rigor Protocol: Stage 0 forbids stating a conclusion before a frame exists; every claim carries a confidence label; every "why" is paired with "why do I believe that"; and the cone is worked in order with an evidence bar at each transition.

**Gap 3 — Unfamiliar APIs/SDKs/MCP servers are a guessing hazard with no defined countermeasure.** Directive D2 forbids guessing at contracts, but does not say *how* to establish an unfamiliar one. **Filled by** the Spike Protocol: read the source and type signatures, then write and run a minimal disposable PoC (the C# file-based-app idiom) to confirm semantics empirically — *before* an architect or designer commits to the contract. This makes "verify by execution" a concrete, routine step rather than an aspiration.

### C.3 The five skills as the synthesis

The five skills (`/specify`, `/define-architecture`, `/design-slice`, `/implement`, `/investigate`) are where everything meets: each is the Rigor Protocol specialized to one phase, casting the right peers to author and the right adversaries to review, producing a committed artifact, and referencing your existing Testing Strategy, Engineering Governance, and LOA at the exact stage they apply. They map onto the eight-phase session loop your Rules of the Road already defines — they do not introduce a competing lifecycle.

---

## Part D — Design rationale (the decisions worth defending)

- **Extend, don't replace.** Every artifact speaks your pack's vocabulary (D1–D3, Coning, Proof Pack, gates, tiers, P1–P11, C1–C11, the persona names, the Deviation Protocol). A second vocabulary would fracture the system; reuse makes the two packs read as one.
- **A persona is one lens worn two ways**, rather than two separate rosters. This keeps the catalog you invested in authoritative, halves the maintenance surface, and makes the mode-switch — not a different cast — the thing a developer has to understand.
- **Skills carry the logic; commands and prompts stay thin.** This matches how Claude Code and Copilot actually resolve and prioritize these files, and keeps the workflow identical across both tools because both point at the same knowledge and templates.
- **The Orchestrator is a first-class peer, not an implicit loop.** Every framework that omits explicit orchestration pays for it in deadlock or groupthink; the cost lesson (A.2) makes a process-owner who enforces proportional convening worth its seat.
- **Execution and a separate adversary outrank self-review** for anything high-stakes, because the one hard limit of self-verification is a model that cannot see its own error. This is why spikes run real code and why the critic is a structurally separate seat.

---

## Part E — An ethics note on "adversarial interrogation"

Part of the brief was to study how intelligence and law-enforcement organizations interrogate. The honest finding from the modern, evidence-based literature (the PEACE model; cognitive interviewing; the Scharff/SUE research) is that **coercive, deceptive, "tough" interrogation produces worse information, not better** — rapport-based, evidence-based, open-then-narrow questioning outperforms it empirically. That finding is convenient, because it means the techniques worth adopting are also the defensible ones.

This pack therefore adopts these techniques in a deliberately bounded way: **the disconfirmation principle, the funnel, precise questioning, and structured challenge are applied to interrogating problems, designs, hypotheses, and claims — never to deceiving the human in the loop.** Where a person is involved, your Rules of the Road's show-your-work and honesty directives govern absolutely: the agent states its confidence and its residual risk, surfaces rather than hides its uncertainty, and never manufactures false confidence as a tactic. "Adversarial" here means *a rigorous critic of the work*, not *an adversary of the user*. The two modes are both in service of the same person: Peer Mode helps them build the right thing; Adversary Mode helps them trust that it actually works.

---

## 6. Sources (by name)

**Multi-agent software systems:** MetaGPT (meta-programming framework; *"Code = SOP(Team)"*); ChatDev (cooperative-design / adversarial-test virtual software company); CrewAI (role-based agent crews); Microsoft AutoGen (conversational multi-agent, GroupChat). Tooling: Anthropic Claude Code (commands, skills, subagents, hooks, agent teams) and GitHub Copilot custom agents, prompt files, and instructions files; the GitHub Spec Kit.

**Reasoning disciplines:** Richard Paul & Linda Elder, *The Miniature Guide to Critical Thinking* (elements of thought; intellectual standards). Donella Meadows, *Thinking in Systems* (stocks, flows, feedback, leverage points). Dennis Matthies & Monica Worline, *Precision Questioning* (Vervago). The root-cause-analysis canon (5 Whys; Ishikawa; fault-tree analysis; change analysis; events-and-causal-factors). Evidence-based investigative interviewing: the funnel technique, the cognitive interview, and the PEACE model. Strategic elicitation: the Scharff technique and the Strategic Use of Evidence (Pär-Anders Granhag and colleagues). AI-native methods: Chain-of-Verification and self-consistency, as the operational forms of "verification is never self-certified."

*All claims above are paraphrased syntheses; consult the named sources directly for their full treatment.*
