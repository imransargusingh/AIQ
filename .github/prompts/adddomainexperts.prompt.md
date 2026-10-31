---
mode: agent
description: Identify this project's domain and add domain-expert personas (peer + adversary) tailored to it, wiring in existing Claude domain skills and updating every roster artifact locally.
---
You are running the **adddomainexperts** workflow (`knowledge/rigor-protocol.md`, specialized to the *domain*; schema per `knowledge/persona-cards.md` and the Persona Operating Standard in `persona-audit.md` §8). Convene the Orchestrator, Product Strategist, and Domain Researcher as collaborating peers to propose the roster delta; switch to the Simplifier, Patterns Expert, Tech Lead, and the relevant general personas as adversaries at the gate.

Run the stages:
1. **Interdict the rush** — propose no persona before the domain and its high-cost failure modes are established from evidence.
2. **OPEN** — derive the domain(s) from *this repo's own evidence* (README, dependency manifests, namespaces, the ubiquitous language); sketch the domain failure map (where a domain error is most expensive, silent, or irreversible); enumerate candidate expert lenses broadly.
3. **INTERROGATE** — per candidate: the Simplifier test (does it catch a domain-error class no current lens — general or already-added — catches?); peer-useful, adversary-useful, or both; its **seam** (especially vs the Domain Researcher: subject-matter judgment vs research method); and whether domain criticality warrants a hard veto.
4. **EVIDENCE** — ground each lens in the domain's authoritative standards (cite them; a standard recalled from memory is Flagged); and **discover existing Claude domain skills/agents and prefer wiring them in over reinventing** (the persona supplies judgment; the skill supplies capability).
5. **DISCONFIRM (gate)** — attack the proposed roster as potential sprawl; reject what doesn't survive and record why; then **present the surviving set to the developer for confirmation** before writing anything (adding personas changes the repo's standing config).
6. **CONVERGE** — for each confirmed expert, generate a §8-conformant persona file from `templates/domain-expert.template.md` in both `.claude/agents/<expert>.md` and `.github/agents/<expert>.agent.md`; wire in the backing Claude skills; and **update every local roster artifact for consistency** — the casting sheet (`collaborative-personas.md` §5), `persona-cards.md`, the trigger/anti-pattern tables (`persona-audit.md` §8.7–8.8), the README roster count, and `adapters/INSTALL.md`. Write `docs/domain-experts.md` (domain + evidence, experts added with rationale/seam/veto/backing skills, rejected candidates). Record the gate.

Do not write files until the developer has confirmed the roster. A domain expert is *subject-matter judgment*, never the Domain Researcher.


**Last action — discoverability (V10, `knowledge/knowledge-visualization.md`):** write the artifact's **frontmatter** (id, type, status, owner, typed links per the relation registry, review-by per the type's SLA, real summary — V2/V13/V14) and sync the derived index via the **script bundle** `python3 docs/ai-forward-pack/scripts/docs-graph.py derive` (`flag` for V16) — no ad-hoc scripts (V18); ensure `docs/index.html` (the Docs Explorer) exists; verify the artifact is linked into the graph; on **material change**, flag inbound neighbors `review-suggested` (V16); capture sub-ADR decisions/assumptions as linked **decision notes** in `docs/notes/` (V17) — same change as the content; frontmatter wins where the index disagrees.


**Running this in Copilot (single agent — make the dialog visible).** Copilot does not auto-spawn the personas as separate subagents the way Claude Code does, so enact the round-table *inline*: first **ground** in the prior `docs/` artifacts (spec → architecture → design) and treat them as authoritative; then for each peer named above, write a short labeled contribution in that persona's voice (e.g. **Patterns Expert —** …); then run the adversary round, each reviewer giving a labeled critique with a severity **[Blocker|Major|Minor|Nit]** and, for hard-veto roles, an explicit **PASS/BLOCK** plus the veto-clears-when predicate (`persona-cards.md` §8). Do not collapse this into one unattributed answer — the dialog is the deliverable. (Alternatively `@`-mention a specific agent from `.github/agents/` for a focused single-persona pass, or use agent **handoffs** to chain the stages.)

**Audit (last action):** append an audit-log entry for this run via `python3 docs/ai-forward-pack/scripts/audit-log.py append --shortname "adddomainexperts-<domain>" --session "<id>" --skill adddomainexperts --kind skill --prompt "<verbatim>" --summary "<experts added>" --artifact docs/domain-experts.md` (the Audit Mandate, `knowledge/audit-and-change-log.md`, AL5).

${input}
