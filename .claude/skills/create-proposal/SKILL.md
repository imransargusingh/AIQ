---
name: create-proposal
description: Brainstorm an idea into reviewable Markdown and HTML proposals, with optional mockups.
runs_as: either
---

# Skill: /create-proposal

Explore an idea before specification. The human and AI can compare framings, alternatives,
trade-offs, and rough mockups without prematurely turning the idea into a governed specification.

**Spine:** the Rigor Protocol, weighted toward OPEN and INTERROGATE. **Authority:** the
Decision Interrogation pattern (`knowledge/decision-interrogation.md`) and the UI standards when a
mockup is created. **Mode:** collaborative brainstorming; the proposal is exploratory, not accepted
requirements.

## Grounding (first action)
CO-S0 applies first: compile the idea without adding scope, then use that goal state throughout.
Load related proposals, research, specs, and mockups. Reuse established facts and clearly separate
them from new possibilities.

## Input
An idea, problem, opportunity, or rough concept. One sentence is enough.

## Cast
- **Peers:** Product Strategist, UX Researcher/IA when user-facing, and the relevant domain lens.
- **Reviewers:** Simplifier checks that options are distinct and the proposal does not masquerade as
  a specification. UX & Accessibility reviews any mockup.

## Flow (Rigor Protocol, specialized)
**Stage 0 — Interdict the rush.** Do not present an exploratory proposal as a decision.

**Stage 1 — OPEN.** Frame the problem, intended users, possible value, constraints, and at least two
genuinely different approaches.

**Stage 2 — INTERROGATE.** Use `knowledge/decision-interrogation.md` for consequential preferences
that shape the proposal. Show Question / Needed for / Recommendation, then ask one at a time.

**Stage 3 — EVIDENCE.** Add only enough comparable or repository evidence to make the alternatives
concrete. Label assumptions.

**Stage 4 — DISCONFIRM.** The Simplifier removes speculative scope and false precision. If a mockup
exists, review its hard states and accessibility.

**Stage 5 — CONVERGE.** Write `docs/proposals/<slug>.md`, then render
`docs/proposals/<slug>.html` with `render-markdown.py`. Optional interactive or visual experiments
go in `docs/mockups/` and are linked from both proposal files. End with explicit questions the later
`/specify` run must settle.

## Output artifact
- `docs/proposals/<slug>.md`
- `docs/proposals/<slug>.html`
- optional `docs/mockups/<slug>-<experiment>.html`

The proposal contains: problem/opportunity, audience, evidence and assumptions, alternative
approaches, recommendation, trade-offs, optional mockups, open questions, and a `/specify` handoff.

## Definition of done (exit gate)
- [ ] Markdown and self-contained HTML proposal both exist under `docs/proposals/`.
- [ ] At least two real alternatives are compared.
- [ ] Facts and assumptions are distinguishable.
- [ ] Consequential open questions are resolved or listed for `/specify`.
- [ ] Optional mockups live under `docs/mockups/` and are linked.
- [ ] The proposal states that it is exploratory and not an accepted specification.

## Documentation & discoverability (last action)
Write frontmatter on the Markdown node, render the HTML companion with
`python docs/ai-forward-pack/scripts/render-markdown.py docs/proposals/<slug>.md`, then run
`docs-graph.py derive`. On Windows use `python` or `py -3`.

**Handoff:** `/specify` after the human chooses to turn the explored idea into requirements.
