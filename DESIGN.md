---
id: design-language
title: "AIQ — Design Language (DESIGN.md)"
type: design-language
status: draft
owner: "Imran"
tags: [design-language, ui, tokens, aiq-home]
links:
  - { to: spec-aiq-home, rel: implements }
  - { to: spec-coaching-loop, rel: relates-to }
  - { to: ui-interaction-design, rel: refines }
review-by: 2027-04-30
summary: >-
  AIQ's Surface token system for AIQ home (the Tauri desktop app, Windows first): calm, specific,
  private. Graphite neutrals with a teal accent and a single-hue teal rung ramp, Segoe UI Variable,
  4px spacing grid, Fluent-like utility shell with Linear-style keyboard navigation. In-tool cards use
  host tokens and are out of this file's scope.
designmd_version: alpha
archetype: "WindowsFluentUtility { Type:OLTP; Arch:SPA; Layout:MasterDetail; Density:Comfortable|Compact; Nav:Sidebar+CommandPalette; Viewport:DesktopBound; Input:KeyboardFirst+PrecisionPointer; Color:LightDark+HighContrast; Type:Utilitarian; Depth:Flat; Sync:LocalFirst; Persistence:LocalOnly; Feedback:Calm; Motion:Micro; Pacing:Freeform; Transition:HardCut; A11y:WCAG_2.2_AA; x-platform:windows; x-framework:tauri-webview2-react; }"
modes: [light, dark, contrast]
colors:
  primary: "#0b6b6b"
  on-primary: "#ffffff"
  canvas: "#f3f4f4"
  canvas-soft: "#eaeeee"
  surface: "#ffffff"
  ink: "#1a1c1d"
  ink-mute: "#596164"
  hairline: "#d6dcdd"
  success: "#1e7a45"
  warning: "#8a5600"
  danger: "#b42318"
  focus-ring: "#0b6b6b"
  scrim: "#0a1414"
  rung-1: "#c4e6e3"
  rung-2: "#86c9c4"
  rung-3: "#3f9f99"
  rung-4: "#17706b"
  rung-5: "#0b3f3d"
typography:
  display:
    fontFamily: "Segoe UI Variable Display, Segoe UI, system-ui, sans-serif"
    fontSize: 44px
    fontWeight: 600
    lineHeight: 1.05
    letterSpacing: -0.5px
  title:
    fontFamily: "Segoe UI Variable Display, Segoe UI, system-ui, sans-serif"
    fontSize: 24px
    fontWeight: 600
    lineHeight: 1.25
    letterSpacing: -0.2px
  heading:
    fontFamily: "Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: 16px
    fontWeight: 600
    lineHeight: 1.35
    letterSpacing: 0
  body:
    fontFamily: "Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: 14px
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: 0
  caption:
    fontFamily: "Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: 12px
    fontWeight: 400
    lineHeight: 1.4
    letterSpacing: 0.1px
  mono:
    fontFamily: "Cascadia Mono, Consolas, ui-monospace, monospace"
    fontSize: 13px
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: 0
rounded:
  none: 0px
  sm: 4px
  md: 8px
  pill: 9999px
spacing:
  scale: [2, 4, 8, 12, 16, 24, 32, 48]
elevation:
  flat: "none"
  overlay: "0 8px 28px rgba(10,20,20,0.18)"
motion:
  fast: 100ms
  base: 160ms
  easing: "cubic-bezier(0.2, 0, 0, 1)"
---

# AIQ — Design Language

> The Surface token system for **AIQ home**, in the Stitch DESIGN.md format. Reference tokens as
> `{colors.primary}`, `{typography.body}`, `{rounded.md}`; never raw values. Lint with
> `python3 docs/ai-forward-pack/scripts/design-lint.py DESIGN.md`. Structure comes from
> `docs/specs/aiq-home.md` Part B; this file governs only how it looks.
> **Scope:** AIQ home only. The in-tool cards (Claude Code, VS Code, Copilot CLI) use their host's
> theme and colours by rule (`coaching-loop.md` Part C).

## 1. Overview — theme, atmosphere & archetype

**Direction brief.** A developer or PM opens AIQ home after a tip or on a Monday, relaxed and a little
wary ("is this grading me?"). Their job is to **check whether agents are finishing more of their work,
pick one next step, and go back to their own tool**. The room should feel **calm** (not alarming),
**specific** (not vague), and **private** (not corporate or surveilling). Opposites we refuse: a
leaderboard, a red-amber-green scorecard, a gamified streak screen, an admin console.

**Archetype.** Nearest catalog row **H1 · Windows Fluent Utility Shell** (Windows-first desktop
utility, sidebar plus command search, keyboard-first). Deviations: `x-framework: tauri-webview2-react`
(not WinUI/WPF), `Density: Comfortable|Compact` (Home comfortable, tables compact),
`Persistence: LocalOnly`, `Feedback: Calm`. Checked against the task: the main job is **reading**
(Home, Tasks, Habits), with short entering moments (setup, This doesn't fit, delete confirms), so a
summary-first reading layout with serial steps only in setup fits; a bento dashboard does not.

**Named references (adapt, never clone).**
- *Windows 11 Settings / PowerToys:* the left-nav utility shell, Segoe UI Variable, 4px grid, quiet
  surfaces. Not its blue accent.
- *Linear:* `G` then a letter to jump, Ctrl+K palette, dense tables that stay readable. Not its
  dark-only look or violet.
- *Apple Screen Time weekly summary:* a private, personal number with a short trend and no
  comparison with anyone. Not its bar-chart-first layout.

**Personality in three moves.**
- **Type:** Segoe UI Variable (native on the Windows target, no download) with a real scale jump
  from 14px body to a 44px headline number, so the one number that matters wins.
- **Colour:** graphite neutrals with a slight teal bias and **one teal accent**. Teal reads as
  "growth, steady" without borrowing the success green or the default blue. The rung ramp is the
  same teal, light to dark, so the ladder reads as one scale and never as good/bad.
- **Space:** 4px grid; tight inside a group (4–8), generous between groups (24–32). Rules and
  background shifts do most of the containing; only the headline and the next step get a card.

## 2. Color palette & roles — with contrast audit

Ratios computed from the hex values (WCAG relative luminance), light mode on `{colors.surface}`
unless stated. Dark and high-contrast values are in §8.

| Token | Hex | Role | On surface | Contrast | AA |
|---|---|---|---|---|---|
| `{colors.ink}` | #1a1c1d | Body text | `{colors.surface}` | 17.10:1 | pass |
| `{colors.ink}` | #1a1c1d | Body text | `{colors.canvas}` | 15.52:1 | pass |
| `{colors.ink-mute}` | #596164 | Captions, helper | `{colors.surface}` | 6.33:1 | pass |
| `{colors.ink-mute}` | #596164 | Captions on canvas | `{colors.canvas}` | 5.74:1 | pass |
| `{colors.ink-mute}` | #596164 | Captions on soft | `{colors.canvas-soft}` | 5.41:1 | pass |
| `{colors.on-primary}` | #ffffff | Primary button label | `{colors.primary}` | 6.31:1 | pass |
| `{colors.primary}` | #0b6b6b | Links, selected nav text | `{colors.surface}` | 6.31:1 | pass |
| `{colors.success}` | #1e7a45 | "Adopted" chip text | `{colors.surface}` | 5.35:1 | pass |
| `{colors.warning}` | #8a5600 | "Blocked by the repo", paused | `{colors.surface}` | 6.16:1 | pass |
| `{colors.danger}` | #b42318 | Errors, delete | `{colors.surface}` | 6.57:1 | pass |
| `{colors.focus-ring}` | #0b6b6b | Focus ring (2px) | `{colors.surface}` | 6.31:1 | pass (≥ 3) |
| `{colors.scrim}` | #0a1414 | Dialog and palette backdrop at 30–35% | — | — | not text |
| `{colors.hairline}` | #d6dcdd | Dividers (decorative) | `{colors.surface}` | 1.39:1 | exempt (decorative); never the only control boundary |

Rung ramp `{colors.rung-1}`…`{colors.rung-5}` fills bars only; every bar carries its rung name and
value as text, so no meaning rests on the colour (WCAG 1.4.1).

## 3. Typography

| Token | Size | Weight | Line height | Tracking | Use |
|---|---|---|---|---|---|
| `{typography.display}` | 44px | 600 | 1.05 | -0.5px | The kept rate on Home, nothing else |
| `{typography.title}` | 24px | 600 | 1.25 | -0.2px | Page titles |
| `{typography.heading}` | 16px | 600 | 1.35 | 0 | Section and card headings |
| `{typography.body}` | 14px | 400 | 1.5 | 0 | Default text, table cells |
| `{typography.caption}` | 12px | 400 | 1.4 | 0.1px | Meta, counts, key hints |
| `{typography.mono}` | 13px | 400 | 1.5 | 0 | Prompts, commands, starter prompt |

Open-source substitute off Windows: system-ui (Linux/macOS). Numbers use tabular figures.

## 4. Components & the complete state set

| Component | default | hover | focus | active/selected | disabled | loading | empty | error | success | overflow |
|---|---|---|---|---|---|---|---|---|---|---|
| Nav item | `{colors.ink-mute}` text | `{colors.canvas-soft}` fill | `{colors.focus-ring}` ring | `{colors.surface}` fill + `{colors.primary}` text + 3px left bar | — | — | — | — | — | label truncates with ellipsis |
| Button (primary) | `{colors.primary}` fill, `{colors.on-primary}` text, `{rounded.sm}` | brightness +6% | 2px ring offset 2px | brightness -6% | 45% opacity, not focusable | label stays, inline progress text ("Checking…") | — | — | — | — |
| Button (secondary) | `{colors.surface}`, hairline border | `{colors.canvas-soft}` | ring | pressed fill | 45% | — | — | — | — | — |
| Button (danger) | `{colors.danger}` text, hairline border | danger tint | ring | — | 45% until confirm typed | — | — | — | — | — |
| Kept-rate card | number + count + trend | — | — | — | — | skeleton in the number's shape | "Patterns show after 10 tasks" + progress per job | "AIQ isn't running" + last stored data | — | 3-digit counts fit |
| Opportunity row | name, job, rung chips, times/week, next step | `{colors.canvas-soft}` | ring on row | opens detail | — | 3 skeleton rows | "No opportunities right now" | — | — | names wrap to 2 lines, then ellipsis |
| Rung bars | name + bar + "today → potential" | — | — | — | — | skeleton bars | hidden until 10 tasks | — | — | — |
| Data table | rows, tabular numbers | row hover | row ring; J/K move | selected tint | — | skeleton rows | teaching empty state | inline error + Retry | — | content cell ellipsis, full text on focus |
| Chip | text + hairline | — | — | — | — | — | — | — | — | never wraps |
| Banner | tinted ground + 1px tinted border (never a thick side bar) | — | — | — | — | — | — | danger variant | — | wraps |
| Dialog (confirm) | `{elevation.overlay}`, `{rounded.md}` | — | focus trapped, starts on Cancel | — | confirm disabled until typed | — | — | — | — | — |
| Command palette | input + list | item tint | active item tint | Enter runs | — | — | "No command matches." | — | — | list scrolls |

## 5. Layout — spacing, grid, whitespace

Spacing uses `{spacing.scale}` only. Shell: 216px nav, content max 960px for reading pages (Home,
detail), full width for tables. Within a group 4–8px; between groups 24–32px. Home is one column with
a two-up top row (headline left, the one next step right). No grid of equal stat tiles.

## 6. Elevation, shapes & motion

Flat by default (`{elevation.flat}`); `{elevation.overlay}` only for the palette and dialogs. Radius
`{rounded.sm}` on controls and chips' squares, `{rounded.md}` on cards and dialogs, `{rounded.pill}`
on chips.

**Motion inventory (all `{motion.fast}` or `{motion.base}`, `{motion.easing}`):**
| Moment | Purpose | Duration | Reduced motion |
|---|---|---|---|
| Nav, row, button hover tint | Affordance | fast | instant |
| Palette and dialog appear | Show a layer arrived | base, opacity only | instant |
| Skeleton shimmer | Show loading is live | 1.2s loop | static skeleton |
Page changes are hard cuts. Nothing else moves. No layout shift: skeletons match content size.

## 7. UI copy — real strings

- Home headline: "Tasks agents finished without you redoing them" · "{kept} of {judged} tasks in the last 30 days"
- First weeks: "AIQ is watching quietly. Patterns show after about 10 tasks of the same kind."
- Queued: "{n} tasks are waiting to be judged. They're left out of these numbers until then."
- Core down: "AIQ isn't running, so this shows what was stored at {time}. It starts with your next prompt in Claude Code."
- Low confidence: "AIQ isn't sure enough to suggest a next step yet."
- Environment blocker: "Blocked by the repo: {finding}. That's a repo fix, not a habit."
- This doesn't fit: "What doesn't fit?" · "This tunes your tips. It doesn't change how the task is judged." · "Thanks. AIQ won't suggest this step for this task. Undo"
- Setup judge, signed in: "AIQ will judge finished tasks with your Claude Code. Each check uses your Claude Code plan usage, up to {budget} tasks a day. AIQ never reads or stores your sign-in."
- Setup judge, not signed in: "Sign in to Claude Code" · "Use an API key instead" · "Skip for now"
- Work-account caveat: "On a work account, your organisation's policies apply to these calls, including logging and retention."
- Delete all: "Delete everything AIQ stored?" · "This deletes {events} events, {tips} tips and their evidence. It can't be undone. Your tools and your work aren't affected." · "Type forget to confirm"
- Deleted: "AIQ has no stored history. Capture is still on."
- Paused: "AIQ is paused. Nothing is captured and no tips show." · "Resume (P)"
- Sharing off: "Off. Nothing leaves this computer."

## 8. Modes

| Token | Light | Dark | High contrast |
|---|---|---|---|
| canvas | #f3f4f4 | #1b1d1e | #000000 |
| canvas-soft | #eaeeee | #2d3234 | #000000 |
| surface | #ffffff | #24282a | #000000 |
| ink | #1a1c1d | #eceeee | #ffffff |
| ink-mute | #596164 | #a3abae | #e8e8e8 |
| hairline | #d6dcdd | #3a4143 | #ffffff |
| primary / focus-ring | #0b6b6b | #5cc8c2 | #ffd400 |
| on-primary | #ffffff | #0b1d1d | #000000 |
| success | #1e7a45 | #6fd39a | #7ee2a8 |
| warning | #8a5600 | #f0b860 | #ffd166 |
| danger | #b42318 | #ff9a8f | #ffb4ab |
| rung-1…5 | #c4e6e3 → #0b3f3d | #1d4a48 → #c4ece9 | outlines only, white |

Dark-mode audit (on surface): ink 12.77, ink-mute 6.37, primary 7.43, on-primary on primary 8.69,
success 8.12, warning 8.31, danger 7.27. All pass AA. Density: Compact drops row padding from 12 to 8
in tables only.

## 9. Performance budget

Cold start to interactive Home ≤ 1 s with 30 days of data (spec NFR); view switches ≤ 100 ms; all
reads local. No web fonts, no images beyond inline SVG; skeletons for any read over 150 ms.

## 10. AI surface

AIQ home shows a model's judgment (the rung judge) and a model's suggested next step.
- **HAX G1/G2:** the judge's source is named on every detail ("judged by your Claude Code").
- **HAX G11 (why):** the facts the judge used sit beside every next step.
- **HAX G15/G16 (feedback):** "This doesn't fit" with a reason, and Undo.
- **Uncertainty:** below 0.8 confidence no next step is shown; uncertain facts say "uncertain".
- **Wrong answer:** the person never sees a rung as a verdict on them; they can withdraw any guidance.
- **Governor:** nothing is sent or run for the person; the starter prompt is copied, not sent.
- **Unavailable judge:** views that need it say why and how to connect, and the rest works.

## 11. Do's & Don'ts

- Do lead with the kept rate; rungs explain underneath.
- Do write rung names first, numbers second ("Directed (2)").
- Don't show scores, grades, streaks, badges or any comparison with other people.
- Don't use `{colors.danger}` for anything except errors and deletion.
- Don't use a thick coloured side bar on banners or cards.

## 12. Responsive behaviour

Desktop-bound. Minimum window 720×520. Below 900px wide the nav collapses to icons plus key hints
and Home's top row stacks. Pointer targets ≥ 32px high in the desktop app (WCAG 2.2 2.5.8 minimum
24px); primary actions 36px.

## 13. Agent prompt & iteration guide

Reference tokens and component names from this file; one component at a time; run `design-lint.py`
after each edit.

## 14. Provenance

Stitch DESIGN.md format. Not seeded from an exemplar; references in §1 are adapted, not cloned.
