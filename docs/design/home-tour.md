---
id: design-home-tour
title: "Design: AIQ home tour for new users"
type: design
status: accepted
owner: "Imran"
phase: "3"
tags: [design, aiq-home, onboarding, tour, accessibility]
links:
  - { to: design-aiq-home-app, rel: depends-on }
  - { to: design-aiq-home-deferred-pages, rel: depends-on }
review-by: 2027-05-05
summary: >-
  An eight-step tour of AIQ home's views, shown on a fixed example snapshot so every view is full on a new
  install. Home offers it once after setup in a corner card; Ctrl K, Settings and the getting-started list start
  it again. A tour-seen flag file next to setup-done stops the offer.
---

# Design: AIQ home tour for new users

Imran asked for a built-in tour on 2026-11-04. Mockups: `/mnt/project-files/tour/tour-proposal.html`. He chose
option A (tour on example data) on 2026-11-05 00:28Z. Options B (tour on own data, offered after 10 judged
tasks) and C (a "?" hint per heading) were not chosen.

| # | Question | Decision | Why |
|---|---|---|---|
| T1 | What does the tour show? | Eight steps: Right now, How you work with agents, Is each kind moving up, Tasks by kind, the Day view, Habits, tips in the person's tool, Settings. One title and one sentence each | One step per view the person will use. Text stays under 25 words a step (unit test) |
| T2 | Whose data? | A fixed example snapshot, `apps/home/tests/fixtures/tour.json`, written by the Rust fixture example from `seed::tour` (progress + coaching + a full example day + an open task). Folder names are neutral; the generator fails if a real name is left. The person's own status, update state and daily cap stay | A new install's Home is empty for days. A fixture from the Rust read model can't drift from the UI types. It loads only when a tour starts (own chunk) |
| T3 | When is it offered, and when never again? | A corner card on Home after setup (not a modal, not on other pages). "Take the tour" and "Not now" both write a `tour-seen` flag file in the data folder (shell command `tour_seen`). Ctrl K, Settings > Tools and the getting-started list start it any time | No manual burden and no undismissable banner. A file, like `setup-done`, works before the core ever ran; delete everything keeps it (app state, not the person's data) |
| T4 | Can the tour change anything? | No. The page under it is `inert` and covered by a blocking layer; the tour writes only its flag; single-key shortcuts are off while it is open | Example data must never reach the store or a judge call. e2e asserts the only write is `tour_seen` |
| T5 | Access | `role=dialog` with `aria-modal`, focus on Next, Tab stays in the coach mark, Esc closes on any step, arrows move, status line announces start and end. axe clean in light, dark and high contrast | UI standard U-floor (WCAG 2.2 AA) |
| T6 | Getting started | Stays the whole of empty Home; gains a "Take the tour" link | The list answers "is it working?"; the tour answers "what will I see?" |

**Measured by:** the shell logs `tour_seen` in `home.log` with its timing, like every command. Not recorded: how
many steps a person reads before closing. Add a step counter to the flag file if that question comes up.
