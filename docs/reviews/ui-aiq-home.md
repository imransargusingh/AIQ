---
id: "ui-review-aiq-home"
title: "UI review — AIQ home"
type: doc
status: draft
owner: "Imran"
phase: design
tags: [ui-review, ux, accessibility, aiq-home]
links:
  - { to: spec-aiq-home, rel: documents }
  - { to: mockup-aiq-home, rel: relates-to }
  - { to: design-language, rel: relates-to }
review-by: "2027-01-30"
summary: >-
  Create-mode /ui-design run for AIQ home. An independent UX and accessibility review blocked the first
  mockup on 20 findings (modal focus, unnamed narrow nav, single-key shortcuts, status messages,
  palette semantics, colour-only selection); all were fixed and the re-test passed the WCAG 2.2 AA veto.
  Highest-leverage change was one native modal primitive for dialogs and the palette.
---

# UI review — AIQ home

Mode: **create** (no AIQ home existed; round-1 mockups were direction sketches). Artifacts:
`DESIGN.md`, `docs/mockups/aiq-home.html` (+ `.md` hub), `docs/design-language-preview.html`.
Spec: `docs/specs/aiq-home.md` (Parts A and B settled first).

## 1. Verdict

| Gate | Result | By |
|---|---|---|
| UX & Accessibility hard veto (WCAG 2.2 AA) | **PASS** on re-test (BLOCK on first pass) | Independent reviewer (separate agent), not the author |
| Deterministic craft gate (`ui-craft-gate.py`) | 8 Major, all in the review harness chrome (suppressed, reason below); 0 in the design region | Script |
| `design-lint.py --strict` | 0 failures; exit 1 on 50 warnings, all palette and mode-table hex (allowed by the format) | Script |
| In-artifact audit, 7 pages × 7 states × 3 themes | 0 contrast failures, 0 targets under 24×24 px | Playwright sweep |

A clean detector run is a floor, not a verdict (CD13). Archetype fit, IA, state existence and copy
truth were judged by the reviewer and are recorded below.

## 2. Measurements (Home, default state, 1400 px)

| Metric | Value | Note |
|---|---|---|
| Interactive controls in the window | 12 | nav 5, palette, 2 primary actions, 4 links |
| Sections | 6 | headline, next step, opportunities, rungs, habits, tips |
| Network calls on load | 0 | self-contained |
| Competing focal points | 1 | the 44px kept rate |
| Distinct type sizes | 5 | 12, 14, 16, 24, 44 (all tokens) |
| Distinct text colours | 6 | all tokens |
| Arbitrary values in the design region | 0 | gate found none outside the harness |
| Worst required text pairing | 5.41:1 | `ink-mute` on `canvas-soft` (light) |
| View switch | 26 ms | harness re-render, sample data |

## 3. Findings (first pass → status)

| # | Location | Dimension | Sev | Fix applied | Status |
|---|---|---|---|---|---|
| 1 | Delete dialog | A11y 2.4.3 focus not trapped, focus lost | 4 | Native `<dialog>.showModal()`, focus returns to invoker, post-delete focus on the empty-state heading | Fixed, re-tested |
| 2 | Nav at narrow width | A11y 4.1.2 unnamed links | 4 | Labels kept visible at narrow width; `aria-label` on links; glyph icons removed | Fixed |
| 3 | P, G+letter, J/K | A11y 2.1.4 character key shortcuts | 3 | Settings switch "Single-key shortcuts"; Ctrl+K always works | Fixed |
| 4 | Pause, delete outcomes | A11y 4.1.3 status messages | 3 | All outcomes through the persistent `role=status` toast | Fixed |
| 5 | Command palette | A11y 4.1.2 | 3 | Modal dialog, combobox + listbox, `aria-activedescendant`, focus return | Fixed |
| 6 | Selected table row | A11y 1.4.1 colour only | 3 | `aria-selected`, ✓ marker, 3px primary bar, count on "Delete selected" | Fixed |
| 7 | Delete selected / older than | State completeness (US-H6) | 3 | Both confirms mocked; older-than count updates with the date; evidence-deleted note on detail | Fixed |
| 8 | Paused state | Flow (US-H6) | 3 | Shell renders the paused banner on every page plus a titlebar chip | Fixed |
| 9 | Home | A11y 1.3.1 missing h1 | 2 | Visually hidden h1 | Fixed |
| 10 | Titlebar search | A11y 2.1.1 Space | 2 | Real `<button>` | Fixed |
| 11 | Tasks sort | Spec US-H3 | 2 | "Rungs to gain" sort, `aria-sort` on `th` | Fixed |
| 12 | Home opportunities | IA: blocked row ranked | 2 | "Repo fixes (not habits)" line under the ranked list | Fixed |
| 13 | C, F, X, Del, / hints | Keyboard fidelity | 2 | Wired C, F, X, Del; removed / and Enter hints | Fixed |
| 14 | Settings | US-H7 controls | 2 | Quiet hours, judging limit, API-key choice are controls | Fixed |
| 15 | Tip history | US-H5 | 2 | "Turn back on" row, 90-day label | Fixed |
| 16 | Setup | US-H1 | 2 | "No tools found" step; Back buttons fixed | Fixed |
| 17 | Copy | Copy truth | 2 | "Save" (not "Send"); no invented queue cause; judge-off kept card | Fixed |
| 18 | Nav glyphs | DX3 tell | 1 | Removed | Fixed |
| 19 | Work mix colours; duplicated next step | Craft | 1 | Neutral fills; list starts at the second opportunity | Fixed |
| 20 | Setup steps | A11y | 1 | `<ol aria-label>` | Fixed |
| 21 | Older-than confirm (re-test) | Copy truth | 2 | Count recomputed on date change; `aria-describedby` | Fixed |
| 22 | Home kept-rate trend (Imran, 2026-11-01) | Data viz: no axis labels or values | 2 | Labelled weekly chart: caption, 0/25/50% gridlines, week-start labels, first and latest values direct-labelled, every point focusable with a tooltip and an accessible name; copy says "in the week of 5 Oct" instead of "four weeks ago" | Fixed |

**Suppressed detector findings (CD16):** 8 colour/radius findings on lines 54–60 are the review
harness (dark control bar, verdict colours). The harness is review chrome and never ships, so its
colours are deliberately outside `DESIGN.md`.

## 4. Scorecard by dimension

| # | Dimension | Verdict | Worst remaining |
|---|---|---|---|
| 1 | Visibility of system status | Good | Paused, not running, queued, judge off each have a state |
| 3 | User control & freedom | Good | Undo on "This doesn't fit"; Cancel on every delete |
| 5 | Error prevention | Good | Typed confirm on delete everything |
| 7 | Flexibility & efficiency | Good | Palette + G shortcuts, can be turned off |
| 11 | Archetype fit | Good | Reading-first pages; only setup and confirms are serial |
| 12 | State completeness | Good | Tasks page has no "uncertain" row variant beyond "waiting" |
| 13 | Token discipline | Good | Harness only |
| 14 | Accessibility (WCAG 2.2 AA) | PASS (independent) | Native WebView2 focus behaviour unproven |
| 16 | Content & copy | Good | Sample numbers are illustrative |
| 17 | Craft | Good | No icon set yet (text-only nav by choice) |
| 18 | AI-surface honesty | Good | Judge source named; below-0.8 shows no step |

## 5. Generic-tells self-check (DX3)

Single saturated blue: avoided (teal with a stated reason). Same card everywhere: only two cards on
Home; rules and spacing contain the rest. Three equal stat tiles: none; one ranked headline. Lorem:
none. Emoji or glyph icons: removed. Only happy screens: seven harness states. Centred everything:
left-aligned utility layout. Motion everywhere: three moments, all removed under reduced motion.

## 6. Simplifier's delete-list

- Removed glyph nav icons; removed duplicated next step from the list; removed unwired key hints.
- Kept the rung bars on Home only because the spec puts rungs "underneath" as explanation (H2).
`net: -3`

## 7. Ranked plan

- **Must fix before `/implement`:** prove modal focus and the palette in the real Tauri/WebView2
  window (the browser-chrome Tab stop may behave differently); produce the native proof pack (UI-T4).
- **Should fix next:** an "uncertain" judgment row on Tasks; a real icon set if Imran wants icons.
- **Worth doing:** a weekly summary view (out of spec scope today).
- **Highest-leverage change (done):** one native modal primitive for dialogs and the palette, with
  focus return and all outcomes through one live region.

## 8. Residual risk & not covered

- **UI-T4 native proof not done.** AIQ home ships as Tauri + WebView2. Keyboard, UI Automation tree,
  DPI and signing need proof in the built app; an HTML mockup cannot clear them. *Flagged.*
- UI-T1 (expert quantitative UI): does not fire; numbers are simple percents and counts.
- UI-T2 (generated assets): does not fire; none used.
- UI-T3 (fronts a model): fires; wrong answer and uncertainty are designed (This doesn't fit,
  below-0.8, judge off, waiting to be judged).
- Sample data only. The kept-rate definition depends on judge facts F2/F5 meeting their bar.

## 9. Defect classes registered

None new in AIQ code. One **pack** defect found while running the skill: `render-markdown.py` (V19)
writes `<name>.html` beside `<name>.md`, so running it on the mockup hub `docs/mockups/aiq-home.md`
overwrote the mockup `docs/mockups/aiq-home.html` (recovered from a working copy). Rule here: never
render a mockup hub `.md`; its `.html` sibling is the mockup. This should go upstream to AI-Forward
as a guard (skip when the sibling `.html` is not a render). AIQ has no `docs/lessons/` register yet.
