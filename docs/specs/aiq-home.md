---
id: spec-aiq-home
title: "Spec: AIQ home (desktop app views)"
type: spec
status: draft
owner: "Imran"
phase: specify
tags: [aiq-home, views, rung-distribution, opportunities, habits, data-controls, settings, setup]
links:
  - { to: spec-coaching-loop, rel: relates-to }
  - { to: spec-rung-model, rel: depends-on }
review-by: 2026-11-30
summary: >-
  What AIQ home, the private desktop app, must let a person do: set AIQ up, see how much of their
  work agents now finish, find the recurring tasks where agents could do more and the one next step
  for each, follow their habits, and see, pause and delete everything AIQ has captured. Draft: Part A
  grounded in decisions already made; Part B and C wait on the open questions below.
---

# Spec: AIQ home (desktop app views)

Produced with AI-Forward `/specify` (v2026.10.19.4) as the UX layer `/ui-design` needs before it can
design AIQ home. **Status: draft, Stage 1–2.** Starting point: round-1 mockups
(`docs/mockups/aiq-ui-round1.html`, https://claude.ai/artifact/SAhC7cYJCHDqmVvGmWKwwH).

This spec fills the gap both sibling specs leave open: `rung-model.md` puts "the rung distribution
view and the opportunity view in AIQ home" out of scope ("own spec"), and `coaching-loop.md` puts
"AIQ home views" out of scope.

## Open questions (decision interrogation, asked one at a time)

| # | Question | Needed for | Recommendation |
|---|---|---|---|
| Q1 | What counts as a task "finished without you redoing it" (the Home headline)? | Home hero, the outcome-first framing, any trend | **Decided (Imran, 2026-11-01): kept with small edits and no re-prompt.** The person wrote at most about 20% of the final change (rung-model F2) and sent no re-prompt that corrected the agent in that task (F5). Threshold is tunable |
| Q2 | Which screens ship in the alpha app? | Scope, IA, effort | **Decided (Imran, 2026-11-01): all six.** Setup, Home, Task detail, Habits and tips, Your data, Settings. Sharing shown as off with a preview only |
| Q3 | Default time window for Home? | Every number on Home | **Decided (Imran, 2026-11-01): last 30 days, with a weekly trend line** |
| Q4 | Show an estimated time saved per opportunity? | Opportunity rows, trust | No, until it can be measured. Show times per week instead |
| Q5 | Ask for the API key during setup, or when the first rung view is ready? | Setup flow, first-run drop-off | Offer it in setup but allow skip; tips work without it; ask again when the first rung view has enough tasks |
| Q6 | Is there a full task list, or only the top opportunities? | IA, navigation | A Tasks page listing recurring task groups, sortable by how often and by gap |

## Part A — Functional specification

### Problem
People who work with AI agents cannot see, in one private place, how much of their work agents now
finish, which recurring tasks agents could take further, and what single change would get them there.
They also need to trust AIQ: see what it captured, pause it, and delete it.

### Target users & personas
Developers and PMs first (shared surfaces); data scientists and other knowledge workers later.
Coaching follows the job being done (Code a change; Write a spec or doc), never the job title.
Roles are presets only (coaching-loop D7).

### Core scenario
Once a week, or after a tip, a person opens AIQ home. In under a minute they see how much of their
work agents finished without rework, the top recurring task where agents could do more, and one
next step with a starter prompt. They try it on their next task in their own tool.

### In scope / Out of scope
In scope: setup; Home; task detail (rung judgment, evidence facts, next step, "This doesn't fit");
habits and tip history; your data (capture log, pause, delete one / older than / all); settings
(tip limit, quiet hours, tip types, work mix and preset, judge, sharing).

Out of scope: any team or manager view (never); the in-tool cards (`coaching-loop.md`); how the
rung is judged (`rung-model.md`); sync across devices; weekly summary emails.

### Decisions already made (inherited, not re-opened)
| Ref | Decision | Source |
|---|---|---|
| H1 | Only the person sees their data; no manager or team view; sharing is opt-in and must be easy to reason about | Imran, 2026-10-31 |
| H2 | Lead with the user outcome (agents do more with less rework); rungs explain underneath | Imran, 2026-10-31 |
| H3 | Task-level, not person-level, framing; success is a recurring task moving up a rung | Imran, 2026-10-31 |
| H4 | Rung ladder: 1 Assisted, 2 Directed, 3 Delegated, 4 Orchestrated, 5 Factory; names first, numbers second | Brief; rung-model Part C |
| H5 | People never reclassify a rung; "This doesn't fit" gives feedback on the guidance with a reason | rung-model Q3 |
| H6 | No next step coached below 0.8 confidence | rung-model |
| H7 | Raw prompts stay on device until the person deletes them (one item, older than a date, all) | ADR-0002 |
| H8 | Lost machine means lost history; stated at setup | Imran, 2026-10-31 |
| H9 | Daily tip limit 1, 3 or 5, default 5, set at setup and in Settings | coaching-loop D3 |
| H10 | Habit states Not yet / Tried / Adopted (rule D5) | coaching-loop D5 |
| H11 | Judge uses the person's own Anthropic API key (keychain) or VS Code's language model API; work-account caveat shown at setup | ADR-0007 |
| H12 | Keyboard-first UI style | coaching-loop |
| H13 | AIQ home is a Tauri desktop app with a React UI; no server holds user data | ADR-0004 |

### User stories & acceptance criteria
To be written after Q1–Q6.

## Part B — UX specification
Pending Part A questions.

## Part C — UI specification
Pending Part B. Produced by `/ui-design` together with `DESIGN.md`.

## Flagged risks & residual unknowns
- The Home headline metric (Q1) is not yet defined or measured anywhere. *Flagged.*
- The rung-distribution and opportunity views depend on the judge's accuracy bar (rung-model) being met. *Flagged.*

## Gate record
Not yet run.
