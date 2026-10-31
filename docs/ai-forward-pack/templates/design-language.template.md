---
# ── Graph record (V2 — knowledge-visualization.md) ──────────────────────────
id: design-language
title: "<Product> — Design Language (DESIGN.md)"
type: design-language
status: draft            # draft | in-review | accepted | superseded
owner: "@handle"
tags: [design-language, ui, tokens]
links:
  - { to: <spec-id>, rel: implements }        # the spec whose UI layer this realizes
  - { to: ui-interaction-design, rel: refines }
review-by: 2026-12-31    # 180-day SLA (Surface artifact)
summary: >-
  The project's Surface token system in the Stitch DESIGN.md format, extended with
  the pack's floors (contrast-audited palette, complete component states, theme/
  density modes, the paired UI Archetype Signature, motion, perf budget, AI-UX).
# ── Design-language record (Stitch DESIGN.md format — agents read this) ──────
designmd_version: alpha
archetype: "<Signature from ui-archetype-grammar.md, e.g. KeyboardVelocity { Type:OLTP; Arch:SPA; Layout:MasterDetail; Pacing:Freeform; A11y:WCAG_2.2_AA; }>"
modes: [light, dark]      # U4 — theme/density/platform modes over the semantic layer
colors:
  primary: "#______"
  on-primary: "#ffffff"
  canvas: "#ffffff"
  canvas-soft: "#______"
  surface: "#______"
  ink: "#______"           # body text — never pure black unless the brand demands it
  ink-mute: "#______"      # captions / helper
  hairline: "#______"      # 1px borders
  success: "#______"
  warning: "#______"
  danger: "#______"
  focus-ring: "#______"    # U16 — visible focus, AA against adjacent surfaces
typography:
  display-xl:
    fontFamily: "<family>, system-ui, sans-serif"
    fontSize: 48px
    fontWeight: 600
    lineHeight: 1.1
    letterSpacing: -0.5px
  body-md:
    fontFamily: "<family>, system-ui, sans-serif"
    fontSize: 15px
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: 0
  # ... add the full hierarchy
rounded:
  none: 0px
  sm: 6px
  md: 10px
  pill: 9999px
spacing:           # the scale — every layout value references a rung, no magic px (U3)
  scale: [2, 4, 8, 12, 16, 24, 32, 48, 64]
elevation:
  flat: "none"
  card: "0 1px 2px rgba(0,0,0,0.06)"
motion:            # U10 — durations/easings; reduced-motion handled in the body
  fast: 120ms
  base: 200ms
  easing: "cubic-bezier(0.2, 0, 0, 1)"
---

# <Product> — Design Language

> **What this is.** The Surface (visual) token system for the whole product, in the
> Google Stitch **DESIGN.md** format so any AI agent can read it and generate UI that
> stays visually consistent — *extended* with the pack's floors. It is the concrete
> serialization of the token system **U3–U5** require, and the thing **/implement**
> builds against (U20: tokens, never arbitrary values). Reference a token as
> `{colors.primary}` / `{typography.display-xl}` / `{rounded.pill}` — never a raw hex.
> Lint with `python docs/ai-forward-pack/scripts/design-lint.py <this file>`.
>
> **This governs how it *looks*, not how it *works* or *what kind* it is.** The
> archetype (above, `archetype:`) fixes the *kind* (`ui-archetype-grammar.md`); the
> UX spec (`docs/specs/<feature>.md` Part B) fixes the *flow*; the **UI & Interaction
> Design Standard (U1–U20)** stays the law above this vocabulary.

## 1. Overview — theme, atmosphere & the archetype it pairs with
<Mood, density, design philosophy in 2–4 sentences.> The paired **UI Archetype
Signature** is `archetype:` above (record the nearest `ui-archetype-catalog.md` row +
any facet deviations, G11/G13). *Build to its facet rules* — the routing/temporal
model, the data posture, the feel — so generation is the same *kind* every time.

## 2. Color palette & roles — **with contrast audit (U16/U3)**
Each token: semantic name + hex + role. Every **text-on-surface pair** carries its
contrast ratio and **AA pass/fail** — accessibility is baked in at the token layer,
not bolted on per screen. A failing pair is a finding, not a footnote.

| Token | Hex | Role | On surface | Contrast | AA (4.5 / 3.0) |
|---|---|---|---|---|---|
| `{colors.ink}` | #__ | Body text | `{colors.canvas}` | __:1 | ☐ pass ☐ fail |
| `{colors.ink-mute}` | #__ | Caption | `{colors.canvas}` | __:1 | ☐ pass ☐ fail |
| `{colors.on-primary}` | #__ | CTA label | `{colors.primary}` | __:1 | ☐ pass ☐ fail |
| `{colors.danger}` | #__ | Error text/icon | `{colors.canvas}` | __:1 | ☐ pass ☐ fail |
| `{colors.focus-ring}` | #__ | Focus ring | adjacent surface | __:1 | ☐ ≥3.0 |

## 3. Typography
Font family + the full hierarchy table (token · size · weight · line-height ·
tracking · use). Name the **open-source substitute** for any proprietary face.

| Token | Size | Weight | Line height | Tracking | Use |
|---|---|---|---|---|---|
| `{typography.display-xl}` | 48px | 600 | 1.1 | -0.5px | Hero |
| `{typography.body-md}` | 15px | 400 | 1.5 | 0 | Default body |

## 4. Components & **the complete state set (U9)**
For each component (Button, Input, Card, Nav, …) specify **every** state — not just
the visual ones. The empty / loading / error states are where polish is usually
missing; they are mandatory here.

| Component | default | hover | focus | active | disabled | **loading** | **empty** | **error** | **success** | first-run / overflow |
|---|---|---|---|---|---|---|---|---|---|---|
| Button | `{colors.primary}` pill | — | `{colors.focus-ring}` ring | — | 40% opacity | spinner-in-place | — | — | — | — |
| Input | hairline border | — | ring | — | — | — | placeholder copy (§7) | `{colors.danger}` border + message | — | char-overflow truncates |
| Data list | — | row hover | — | — | — | skeleton rows | zero-data CTA (§7) | retry affordance | — | virtualize > N rows |

## 5. Layout — spacing, grid, whitespace
Spacing references `{spacing.scale}` rungs only (no magic px). Grid/container widths,
density choice (matched to audience), whitespace philosophy.

## 6. Elevation, shapes & motion
Elevation tokens (`{elevation.*}`), radius scale (`{rounded.*}`). **Motion (U10):**
purpose of each transition; **everything respects `prefers-reduced-motion`** — name
the reduced-motion fallback (instant / cross-fade only).

## 7. UI copy (U11) — real strings, in the product's voice
The load-bearing strings, written (no lorem): primary CTA label, the **empty-state**
guidance ("No invoices yet — create your first"), the **error** messages (humane,
actionable), confirmations, and any AI disclosure line. These are part of the design.

## 8. Modes (U4) — theme / density / platform
How the semantic tokens retarget across `modes:`. Light/dark/high-contrast values for
each semantic color; density variants of the spacing scale; platform deltas. One
component authored once retargets — do not fork components per theme.

## 9. Performance budget (U17)
The named budget for the medium: perceived load, interaction latency, frame-rate
during motion/scroll, payload/asset weight (web) or cold-start (native/mobile). The
perceived-performance techniques in play (skeletons, optimistic UI, prefetch).

## 10. AI surface (U13–U15) — *only if the UI fronts AI*
The applicable **HAX** guidelines (G1–G16) and **Shape-of-AI** patterns — Wayfinders,
Tuners, **Governors** (action plan before an irreversible step, stream-of-thought,
verification), Trust builders (disclosure that content is AI, citations, provenance),
Identifiers — and the **wrong-answer/uncertainty** design (effortless correction &
regeneration, honest confidence). Each becomes a testable acceptance criterion. If the
UI fronts no AI, state that in one line.

## 11. Do's & Don'ts
Brand guardrails and anti-patterns (token-referenced).

## 12. Responsive behavior
Breakpoints table; **touch targets ≥ 44×44px**; collapsing strategy; image behavior.

## 13. Agent prompt & iteration guide
- Reference component names and **tokens** directly (`{colors.primary}`, `{rounded.pill}`).
- Focus on ONE component at a time; add variants as new entries.
- Run **`python docs/ai-forward-pack/scripts/design-lint.py <this file>`** after edits
  (every `{token}` resolves; no raw hex in the body — U3 as a forcing function).

## 14. Provenance & the adapt-don't-clone rule (U12)
Format: Google Stitch **DESIGN.md** (stitch.withgoogle.com/docs/design-md). If this
design language was seeded from an exemplar (e.g. the `awesome-design-md` library,
MIT — voltagent/awesome-design-md), say so here and credit it. **Honor the familiar
pattern, but adapt — never clone** a brand wholesale (Jakob's Law balanced with
intentional novelty, U12). The floors above (U9/U16/U17 + the archetype) are *added*
to whatever surface vocabulary you start from; they are not optional.
