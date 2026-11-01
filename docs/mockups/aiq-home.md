---
id: mockup-aiq-home
title: "Mockup: AIQ home (desktop app)"
type: mockup
status: in-review
owner: "Imran"
phase: design
tags: [aiq-home, mockup, ui-design, review-harness]
links:
  - { to: spec-aiq-home, rel: implements }
  - { to: design-language, rel: uses }
review-by: 2027-04-30
summary: >-
  Self-contained, dependency-free mockup of AIQ home (Home, Tasks, task type detail, Habits and tips,
  Your data, Settings, Setup) with a review harness for page, state, persona, judge connection, theme,
  density, viewport and reduced motion, plus a live contrast and target-size audit. Built to DESIGN.md
  from docs/specs/aiq-home.md Parts A and B.
---

# Mockup: AIQ home

Open `aiq-home.html` in any browser (no build, no network). Use the dark bar at the top to switch
page, state, persona, judge connection, theme, density, viewport and reduced motion. The bottom of
the page shows the live contrast and target-size audit for the current theme.

| Harness state | What it shows on each page |
|---|---|
| default | Normal 30-day view with sample data |
| loading | Skeletons shaped like the content |
| empty / first-run | Home's first weeks; empty tables; detail below the 0.8 confidence bar |
| error | AIQ not running (stale data banner); Your data: store locked |
| partial | Home: tasks waiting to be judged; detail: "This doesn't fit" form; Settings: Claude Code not signed in |
| overflow | Longest realistic task name, 4-digit counts, long prompt |
| success | Detail: feedback sent; Habits: newly adopted; Your data: after delete everything; Settings: sharing preview |

Setup has its own step selector, including the three judge branches from the rung-model spec
(signed in, test failed, not signed in, API key invalid).

Review: `docs/reviews/ui-aiq-home.md`. Earlier round-1 mockups of all surfaces:
`aiq-ui-round1.html`.
