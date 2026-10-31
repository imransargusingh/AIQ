---
name: ui-design
description: Create, review and elevate user interfaces to a professional standard — direction brief, design language, reviewable mockups, and rubric-based critique. Use to design a new surface, audit an existing one, or take a working UI to best-in-class.
runs_as: either
---

# Skill: /ui-design

Take a user-facing surface — new or existing — to a **professional, best-in-class** standard. This is the craft skill for the interface itself: it establishes creative direction in words before pixels, builds the design language, produces a **self-contained, reviewable mockup** that renders the hard states, and critiques against a rubric rather than a reaction. It is deliberately distinct from `/design-slice` (which produces the *component* design — contracts, patterns, failure modes, telemetry, test plan). `/design-slice` decides how the thing works; `/ui-design` decides how it looks and feels, and whether that is actually good.

**Spine:** the Rigor Protocol (`knowledge/rigor-protocol.md`) run on the *interface*, weighted toward **Stage 1 OPEN** (direction, before any visual artifact exists) and **Stage 4 DISCONFIRM** (rubric critique, structure before surface). **Authority:** **`ui-design-craft.md`** (DX1–DX25 — direction, the generic-tells table, the fidelity ladder, the review harness, visual craft, the critique rubric) is this skill's governing standard; the **UI & Interaction Design Standard** (`ui-interaction-design.md`, U1–U20) is the floor; the **UI Archetype Grammar + Catalog** (`ui-archetype-grammar.md`, `ui-archetype-catalog.md`) fixes the *kind*; **`technical-ui-design.md`** (TQ1–TQ12) governs expert/quantitative surfaces; **`ui-craft-detection.md`** (CD1–CD20) supplies the **deterministic control** that mechanizes the craft floor; **`ui-visual-assets.md`** (VA1–VA22) governs any **generated** imagery, persona or motion; the **Specification Standards** (`specification-standards.md`, S1–S10) supply the UX layer this builds on; and for WPF/WinUI/Avalonia/Blazor Hybrid surfaces, the project evidence base **`docs/knowledge/native-client-ui-design/`** plus `templates/native-ui-proof-pack.template.md` define the native proof pack. **Mode:** Peer Mode to create, Adversary Mode to critique — and the author never clears their own accessibility veto.

## Grounding (first action)
CO-S0 applies first — the sentence is `reference/co-s0.md`. Load what already exists and treat it as the **authoritative source of truth** (Rigor Protocol Stage 0; BoK §III.1): the spec's **Part B (UX)** and **Part C (UI)** (`docs/specs/`), the project's **design language** (`DESIGN.md` — the token system, U3a), any existing mockups (`docs/mockups/`), the component designs that render this surface (`docs/design/`), and the real implementation if one exists (**open the components — do not describe them from memory**, `end-to-end-integrity.md` E15). Prefer **graph traversal** (`knowledge-visualization.md` V15): start from the surface's artifact(s) and follow typed edges 1–2 hops (upstream `implements`/`refines`, downstream `documents`/`tested-by`, `uses-term` into the glossary), citing the traversal path; a missing edge, stale node or orphan is a finding. Also read the **defect-class register** (`docs/lessons/defect-classes.md`, `continuous-improvement.md` CI5) for the UX-* classes, so a known failure is designed out rather than rediscovered. **A settled UI over an unsettled UX layer is a block** (S2, Surface-before-Structure) — if Part B does not exist or its flows do not cover the alternate/error/recovery paths, stop and run `/specify` for the UX layer first.

## Input
A surface to create, review, or elevate. Examples: *"design the onboarding flow"*; *"review our settings screens"*; *"the data-entry page feels cluttered and cumbersome — fix it"*; *"take the dashboard to best-in-class"*. One sentence is enough; the skill expands it into a direction brief.

## Modes
State which mode you are in; they share the flow but weight it differently.

| Mode | When | Emphasis |
|---|---|---|
| **create** | No surface exists yet | Stage 1 (direction brief, archetype) and Stage 5 (design language + mockup) |
| **review** | A surface exists; the question is "how good is it?" | Stage 3 (**measure** before diagnosing) and Stage 4 (rubric critique, ranked plan) |
| **elevate** | A surface works but is not good enough | Full loop: review first, then re-direct and rebuild the weakest layer — often the archetype |

**Default when unclear:** if the surface exists, run **review** first and *then* decide with the user whether to elevate. Rebuilding something you have not measured is how a working screen gets replaced by a differently-flawed one.

## Triggered standards (map the surface, apply the union)

Modes are mutually exclusive; the standards are orthogonal and composable. **The trigger table (UI-T1 expert/quantitative · UI-T2 generated assets · UI-T3 fronts a model · UI-T4 native client) and the unconditional floor are in `reference/triggered-standards.md` — read it at Stage 1, walk every row, and state which fire and which do not, with the reason.** A trigger discovered in the definition of done has already cost the archetype decision it was supposed to inform.

## Cast
- **Peers (author together):** **UX & Accessibility** (lead — owns the Surface layer and its excellence), **UX Researcher / IA** (owns the structure beneath it: IA, flows, findability), **Product Strategist** (does this serve the job-to-be-done?), the relevant platform developer (**Mobile App** / **Native Desktop** / the web-facing language Developer) for platform idiom and feasibility, **The Simplifier** (every element earns its place), and **Domain Researcher** when comparables or platform guidelines must be established rather than recalled.
- **Adversaries (attack at the gate):** **UX & Accessibility** (state completeness, token discipline, WCAG 2.2 AA — **hard veto**, and the author does not clear it), **UX Researcher / IA** (archetype fit, flow integrity, findability, unhappy-path coverage — **UX-specification veto**), **The Simplifier** (soft veto on anything that does not earn its place — emits the tagged delete-list, `solution-selection-ladder.md` L9), **Product Strategist** (does this still serve the core scenario?), **Test Architect** (are the UI acceptance criteria falsifiable and covered?), **SRE** (performance budget, layout stability), **AI Systems Engineer** when the surface fronts a model (HAX + Shape-of-AI, wrong-answer states).

## Flow (Rigor Protocol, specialized to the interface)

**Decision interrogation.** Read `knowledge/decision-interrogation.md`. During Stages 1-2, use its
serial dialogue before settling consequential direction, archetype, density, interaction, or visual
trade-offs that evidence cannot decide. Before handoff, run DI6. If questions remain, show the
Question / Needed for / Recommendation table first, then ask one question at a time.

The stages below are the contract; **the full stage text is `reference/flow.md`** — read it once, at Stage 0, and work from it. Do not re-invoke this skill to re-read it (a second invocation re-injects this whole file — class CTX-E), and do not paraphrase a stage from memory when the file is one read away.

- **Stage 0 — Interdict the rush.**
- **Stage 1 — OPEN (direction, in words).**
- **Stage 2 — INTERROGATE.**
- **Stage 3 — EVIDENCE (measure, establish, systematise).**
- **Stage 4 — DISCONFIRM (the gate — critique against the rubric).**
- **Stage 5 — CONVERGE (produce the artifacts).**

## Output artifact
- `docs/mockups/<surface>.html` — self-contained, dependency-free high-fidelity mockup with the review harness and the hard states (+ `docs/mockups/<surface>.md` hub node with frontmatter).
- `DESIGN.md` — the project design language, created or updated, `design-lint.py`-clean, with its rendered preview.
- `docs/reviews/ui-<surface>.md` (review/elevate mode) — measurements, rubric findings with severities, scorecard, and the ranked plan with the highest-leverage change named.

## Definition of done (exit gate)

**22 falsifiable items in `reference/definition-of-done.md`** — read it at Stage 4 (before the gate), tick each item against evidence, and quote the unmet ones in the status table. The gate is the checklist, not this summary: direction before pixels, the UX layer settled, the system before the screens, a self-contained mockup with the harness and the hard states, the deterministic craft gate run and reported as a floor (never a verdict), the rubric critique structure-before-surface, WCAG 2.2 AA cleared by someone other than the author, the ranked plan with the highest-leverage change named, and every triggered standard mapped at Stage 1 and applied as a union.

## Documentation & discoverability (last action)
Per the **Knowledge Visualization & Docs Explorer Standard** (`knowledge/knowledge-visualization.md`, the Discoverability Mandate V10): after producing the artifacts, **write each one's frontmatter** (V2: id, title, type, status, **owner**, phase, tags, **typed links** per the relation registry, **review-by** per the type's SLA, and a real 1–3-sentence summary — the mockup's hub `.md` is the graph node; the `.html` is data) and **sync the derived `docs/docs-index.js`** by running the script bundle — `python3 docs/ai-forward-pack/scripts/docs-graph.py derive` (and `flag --changed <id> --reason …` for V16 propagation) — never ad-hoc scripts (V18); frontmatter wins wherever the two disagree. Ensure `docs/index.html` (the Docs Explorer) exists — instantiate it from `templates/docs-explorer.template.html` if missing — and verify each new entry has at least one typed link into the graph (an orphan is a finding). Index and diagrams land **in the same change** as the content (V11). **Propagate impact (V16):** a changed UI contract, archetype, or token system is material — flag the inbound neighbours (the spec's Part C, the component designs that render this surface) `review-suggested`. **Capture session exhaust (V17):** any direction decision, rejected alternative, or discovered constraint below ADR weight becomes a linked **decision note** (`docs/notes/`) before close. **Register what was learned (CI1):** any UI defect found or created in this run is captured as a **class** in `docs/lessons/defect-classes.md` with its control (`continuous-improvement.md`).

**Audit (last action).** Append an audit-log entry for this run — `python3 docs/ai-forward-pack/scripts/audit-log.py append --shortname "ui-design-<surface>" --session "<id>" --skill ui-design --kind skill --prompt "<the prompt, verbatim>" --summary "<what it produced or found>" --artifact docs/mockups/<surface>.html` — per the Audit Mandate (`knowledge/audit-and-change-log.md`, AL5). When the run settles a load-bearing direction or archetype decision, also add a change-log entry (`audit-log.py change`, CL1).

**Handoff:** → `/specify` if the UX layer beneath is unsettled · → `/design-slice` for the component contracts behind the approved surface · → `/implement` to build it against `DESIGN.md` (referencing tokens, never raw values) · → `/investigate` if the review uncovered a defect in running software.
