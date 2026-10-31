---
load: skill
skills: [ui-design, design-slice, implement]
---
# UI Design Craft — direction, prototyping & critique

*Normative guidance for **producing** and **judging** user-interface work at a professional standard. The UI & Interaction Design Standard (`ui-interaction-design.md`, U1–U20) defines the floor a UI must meet; the UI Archetype Grammar (`ui-archetype-grammar.md`) fixes the *kind*; the Specification Standards (`specification-standards.md`) fix the layers. **This document is the craft between them** — how creative direction is established before pixels, how a mockup is built to be reviewable, what separates "meets the floor" from "obviously designed by someone who cares", and how an existing interface is critiqued with a rubric instead of an opinion. It is the knowledge the `/ui-design` skill runs on.*

Normative keywords (**MUST**, **SHOULD**, **MAY**, **MUST NOT**) follow RFC 2119.

The governing idea: **generic is the default output, and only process defeats it.** A model asked for "a clean dashboard" samples the mean of everything it has ever seen and returns the mean — the same card grid, the same violet-to-indigo gradient, the same three stat tiles, the same empty-state illustration. That is not a taste failure; it is a *process* failure. Human designers do not work in one pass either: they explore, they establish direction, they build a system, they critique, and only then do they produce. This document is that process, made explicit enough for an agent to run and for a reviewer to check.

---

## Rule index — read this first; open a section only when a rule applies

*This doc loads on demand (`load: skill`). The index is the contract in one screen: cite a rule by id, and open its section below only when a finding needs the rule's full text. A profiled session read four of these docs whole (~200 KB) to author one mockup — the index is the fix (class CTX-E).*

| Rule | In one line |
|---|---|
| **DX1** | Separate creative direction from implementation; never do both in one pass |
| **DX2** | Build the design system before the screens |
| **DX3** | Know the tells, and design against them |
| **DX4** | Anchor to named references, not adjectives |
| **DX4a** | Native client references carry license posture |
| **DX5** | Write a direction brief before any visual artifact exists |
| **DX6** | Decide the personality in three moves, and justify each |
| **DX7** | Climb the fidelity ladder; do not skip to production |
| **DX8** | The high-fidelity mockup is a dependency-free, self-contained HTML file, committed to the repo |
| **DX9** | The mockup renders the hard states, not the happy one |
| **DX9a** | Native clients require runtime proof beyond the mockup |
| **DX10** | Ship the mockup with a review harness |
| **DX11** | Include an in-harness self-audit where it is mechanical |
| **DX12** | Hierarchy through contrast of scale, not just weight |
| **DX13** | Space is the primary grouping mechanism; borders are the last resort |
| **DX14** | Restraint in color; earn every hue |
| **DX15** | Optical over mathematical |
| **DX16** | Real content, at its real extremes |
| **DX17** | Density is calibrated to the audience, and density demands more hierarchy |
| **DX18** | Give the surface one focal point and defend it |
| **DX19** | Keep a motion inventory: few, purposeful, consistent |
| **DX20** | Micro-interactions are where "polished" is actually decided |
| **DX21** | Write the interface copy as part of the design, in the product's voice |
| **DX22** | Critique against a rubric, not a reaction |
| **DX23** | Measure before you diagnose |
| **DX24** | Critique the structure before the surface |
| **DX25** | A review ends with a ranked plan, not a list |

## 0. When this applies

Whenever the work **produces, changes, or reviews a user-facing surface** — a screen, component, flow, page, email, CLI output, or voice turn — and the goal is a professional result rather than a functional one. It composes *downward* from the archetype (`ui-archetype-grammar.md` selects the kind) and *upward* into the floors (`ui-interaction-design.md` U1–U20 for excellence, `technical-ui-design.md` TQ1–TQ12 for expert/quantitative surfaces, WCAG 2.2 AA for inclusion). It does not restate them; it tells you how to *reach* them.

---

## 1. The anti-generic mandate

**DX1 — Separate creative direction from implementation; never do both in one pass.** One prompt that must simultaneously decide what it should feel like, what the system is, and what the markup says will average all three. The phases are distinct and **MUST** be run in order: **direction** (words, references, constraints — no code) → **system** (tokens, the design language) → **screens** (composition) → **build** (implementation) → **critique** (adversarial review) → **refine**. Each phase's output is an artifact the next phase consumes.

**DX2 — Build the design system before the screens.** Generating screens first and extracting a system afterwards produces a system that is a description of accidents. The token system (`ui-interaction-design.md` U3–U3a, `DESIGN.md`) comes first, so every screen is composed *from* decisions rather than making new ones. This is also what makes the output consistent across sessions and across agents: a screen generated from a committed `DESIGN.md` is reproducible; a screen generated from vibes is not.

**DX3 — Know the tells, and design against them.** The recognisable "AI look" is a finite list, and each item has a specific antidote. An agent producing UI **MUST** self-check against it:

| The tell | Why it happens | The antidote |
|---|---|---|
| Violet/indigo gradient, or a single saturated blue on white | The statistical centre of training data | Derive the palette from the product's own direction (§2); state the accent's *reason* |
| Every element in a card with the same radius and shadow | Cards are the safest default container | Vary containment deliberately: rules, spacing, background shifts, and *nothing at all* are also containers |
| Three equal stat tiles across the top | The canonical dashboard opening | Rank the metrics; give the important one more size/position weight (U6 — one focal point) |
| Uniform spacing everywhere | Applying one scale value repeatedly reads as safe | Use spacing to express grouping: tight within a group, generous between groups. Rhythm, not repetition |
| Type that is all one or two sizes with weight doing all the work | Ratio choices weren't made | Commit to a type scale with real contrast between levels (§5) |
| Lorem, "John Doe", "Lorem ipsum dolor", `$1,234.56` | Content wasn't decided | Real, domain-accurate copy and realistic data — including the long name, the zero, and the negative (U11) |
| Emoji as iconography; a rocket for "launch" | Cheap semantic filler | A real icon set, used consistently, or text |
| Perfectly full, perfectly happy screens | Fixtures are populated | Design the empty, loading, error and overflow states *first*, not last (U9) |
| Centred everything, symmetrical everything | Symmetry is the safe layout | Asymmetry with intent — an editorial column, an off-centre focal point — where the content earns it |
| Motion on everything, or none at all | Both are defaults | A motion inventory (§6): a small number of purposeful moments |

**DX4 — Anchor to named references, not adjectives.** "Modern and clean" constrains nothing. Direction **MUST** name concrete references — products, design languages, or committed exemplars — and say *what specifically* is being taken from each ("Linear's information density and keyboard-first posture; **not** its color"). The pack ships attributed exemplars (`examples/design-languages/`, `ui-archetype-catalog.md` §J) for exactly this. **Adapt, never clone** (U12): take the principle, not the pixels, and never reproduce a protected brand's distinctive visual identity.

**DX4a — Native client references carry license posture.** For WPF, WinUI, Avalonia and Blazor Hybrid work, use public native-client exemplars as **pattern references**, not visual assets to clone. The current reference set:

| Exemplar | License posture | Use |
|---|---|---|
| `microsoft/WinUI-Gallery` | MIT | WinUI control/style samples and adaptive UI references |
| `microsoft/PowerToys` | MIT | Windows utility/settings shell and command-palette patterns |
| `files-community/Files` | MIT | Native file manager workbench patterns |
| `lepoco/wpfui` | MIT | WPF Fluent controls and navigation/dialog patterns |
| `MaterialDesignInXAML/MaterialDesignInXamlToolkit` | MIT | WPF Material theming and ResourceDictionary patterns |
| `AvaloniaUI/Avalonia` | MIT | Cross-platform XAML styling, focus and accessibility patterns |
| `microsoft/WPF-Samples` | MIT | WPF DPI/control samples |
| `File-New-Project/EarTrumpet` | Flagged/non-standard | Reference-only tray/audio UX; do not reuse without legal review |
| `rocksdanister/lively` | GPL-3.0 | Reference-only; do not copy code/assets into permissive pack output |

Borrow patterns and cite license posture. Do not clone brand, screenshots, icons, product identity, or any reference-only code/assets.

---

## 2. Creative direction (phase 1 — words before pixels)

**DX5 — Write a direction brief before any visual artifact exists.** One page, and it **MUST** contain:

1. **Who and what for** — the user, the job-to-be-done, the emotional state they arrive in (rushed? anxious? exploratory? expert-in-flow?). A tool used at 2 a.m. under pressure is a different design problem from one browsed on a sofa.
2. **The archetype** — the Archetype Signature from `ui-archetype-grammar.md`, chosen from the JTBD. **The most consequential single decision in the brief**: a pack repo applied a dashboard/bento archetype to a data-entry task and shipped 138 controls mounted at once across 16 cards, with three view modes doing the same job. The owner's verdict was *"cluttered and cumbersome"*. *Reading is parallel; entering is serial* — the archetype must match the shape of the task, not the shape of the data.
3. **Three defining adjectives — and their opposites.** "Calm, not sterile. Dense, not cramped. Authoritative, not corporate." The opposite is the load-bearing half: it is what makes the adjective falsifiable and gives critique something to test.
4. **Named references**, with what is taken from each (DX4).
5. **The anti-goals** — what this must never look or feel like. (A finance product that must not read as a sportsbook; a health product that must not read as a game.)
6. **The constraints** — medium(s) and platform guidelines, density profile, brand assets that exist, technical limits, and the accessibility obligation level.

**DX6 — Decide the personality in three moves, and justify each.** *Type* (what the voice sounds like: editorial serif, neutral grotesque, technical mono — and the pairing), *color* (the dominant/secondary/accent balance and what the accent is *for*), and *space* (the density calibration for this audience). These three carry ~80% of perceived personality. Each **MUST** have a one-line justification tied to the brief — an unjustified font choice is a coin flip, and it will be re-flipped next session.

---

## 3. The fidelity ladder and the mockup as primary artifact

**DX7 — Climb the fidelity ladder; do not skip to production.** Each rung answers a different question, and answering them out of order is how expensive rework happens.

| Rung | Artifact | Answers | Do not yet decide |
|---|---|---|---|
| 1 | **Flow / wireframe** (Mermaid flowchart + boxes) | Does the *structure* work? Is the sequence right? Is anything unreachable? | Any visual choice |
| 2 | **Design language** (`DESIGN.md` + preview) | What is the system? | Any screen composition |
| 3 | **High-fidelity mockup** (self-contained HTML) | Does it *look and feel* right, in every state? | Production architecture |
| 4 | **Interactive prototype** (the mockup + the review harness) | Does it work under review — every persona, viewport, state? | — |
| 5 | **Production build** | Does it work for real? | — |

**DX8 — The high-fidelity mockup is a dependency-free, self-contained HTML file, committed to the repo.** One file, no build step, no CDN, no framework — it opens over `file://` on any machine, survives having no network, and can be reviewed by a human who will not run `npm install`. It is the pack's house pattern for the same reasons the Docs Explorer and design-language preview are (`knowledge-visualization.md` V9): reviewability must not depend on a toolchain. It lives at `docs/mockups/<name>.html` with a graph-linked `docs/mockups/<name>.md` hub node carrying the frontmatter.

**DX9 — The mockup renders the *hard* states, not the happy one.** It **MUST** show, for the surfaces it covers: **empty / first-run**, **loading** (skeleton, not spinner, where content has shape), **error** (with the real recovery affordance), **partial/degraded**, and **overflow** (the longest realistic name, the biggest realistic number, the 40-item list). *"It looked fine with data"* is how a real production defect shipped in a pack repo — a read path that could not distinguish a failed read from an empty account. The mockup is where that is cheap to discover.

**DX9a — Native clients require runtime proof beyond the mockup.** For WPF, WinUI, Avalonia and Blazor Hybrid, the dependency-free HTML mockup remains valuable direction evidence, but it is **not native PASS**. Native PASS requires the proof rows in `templates/native-ui-proof-pack.template.md`: keyboard traversal, accessibility tree, theme/high-contrast, DPI/windowing, large-list performance, OS integration and distribution trust. A screenshot or static craft detector cannot prove UI Automation names, focus restoration, mixed-DPI behavior, native windowing, package signing or WebView/native-shell focus handoff.

---

## 4. The review harness

**DX10 — Ship the mockup with a review harness.** A high-fidelity mockup that shows one persona, one viewport and one state is a screenshot with extra steps. The harness is a thin control bar, in the same file, that switches:

- **Persona / role** (what an administrator sees vs. a member vs. a signed-out visitor)
- **Viewport** (mobile / tablet / desktop / wide — as a container, so no device is needed)
- **Component state** (default → hover → focus → loading → **empty** → **error** → success → overflow) applied across the surface at once
- **Theme** (light / dark / high-contrast) and **density** if the system has modes
- **Capability flags** (AI available/unavailable; feature on/off; offline) where the design branches on them
- **Reduced motion** (to prove the reduced-motion path is real, not aspirational)

This turns a static artifact into something a reviewer can *interrogate* in a minute, and it makes the state-completeness floor (U9) visibly true or visibly false rather than a claim in a document. The harness is chrome for review only — it is not part of the design being reviewed, and it never ships to production.

**DX11 — Include an in-harness self-audit where it is mechanical.** Contrast ratios computed and displayed for each text/surface pairing, target sizes flagged below the minimum, and the token used by each element visible on inspection. A mechanical check that runs in the artifact beats an assertion in a document, and it makes `design-lint.py`'s findings visible to a non-technical reviewer. **Classify each pairing so the audit is honest**: `text` (4.5:1, required), `large` (3:1, required), `ui` (3:1, required — a boundary or indicator *needed to identify a control*, and focus indicators, per WCAG 1.4.11/2.4.11), and `decorative` (**measured and reported, but not counted** — purely decorative dividers and separators are explicitly exempt from 1.4.11). Counting a decorative divider as a failure teaches heavy, ugly borders in the name of accessibility, which is the wrong lesson; *hiding* its ratio teaches nothing. Report it, do not fail on it — and reclassify honestly, because a divider that is the only thing separating two interactive rows is `ui`, not decoration. **Re-state semantic colours in every theme:** a high-contrast theme that inherits the dark theme's `--danger` against a pure-black canvas will silently drop below AA — this is a real defect the pack's own template shipped until its own in-artifact audit caught it, which is the argument for the audit.

---

## 5. Visual craft — what separates "meets the floor" from "excellent"

The floors in U1–U20 are necessary and not sufficient. These are the moves that read as *designed*.

**DX12 — Hierarchy through contrast of scale, not just weight.** A design where everything is 14–16px with bold doing the work reads as flat. Real hierarchy needs a **type scale with genuine ratio** (a modular scale — 1.2 minor third for dense/technical, 1.25–1.333 for balanced, 1.5+ for editorial) and the confidence to make the primary thing *significantly* larger and the secondary thing *significantly* quieter.

**DX13 — Space is the primary grouping mechanism; borders are the last resort.** Proximity groups; distance separates. Reach for spacing first, then a background shift, then a rule, and only then a bordered card. A UI made entirely of bordered boxes has outsourced its grouping to lines and reads as a form even when it isn't one. **Within-group spacing must be visibly tighter than between-group spacing** — if they are equal, there are no groups.

**DX14 — Restraint in color; earn every hue.** A palette with one accent used *only* for the primary action and the single most important status reads as confident. A palette where six things are colored reads as decorated. Semantic colors (success/warning/danger/info) are reserved for semantics — never for decoration, and never as the *only* channel carrying meaning (WCAG).

**DX15 — Optical over mathematical.** Centring by pixel is not centring by eye: icons beside text need optical alignment; a circular badge next to a square one needs a size adjustment; punctuation hangs; a button's label sits optically centred, not arithmetically. Where the two disagree, **the eye wins** — and the deviation is worth a comment so the next person does not "fix" it.

**DX16 — Real content, at its real extremes.** Design with the actual longest product name, the actual 7-digit number, the empty string, the negative value, the 4-word label in German, the user with no avatar. Content-shaped design survives contact with data; lorem-shaped design does not.

**DX17 — Density is calibrated to the audience, and density demands *more* hierarchy.** An expert tool should be denser than a consumer app — that is respect for the user's time, not a compromise. But compression without hierarchy is noise: as density rises, size/weight/position contrast must rise with it (`technical-ui-design.md` TQ1).

**DX18 — Give the surface one focal point and defend it.** Every screen answers "where do I look first?" with exactly one answer. When a second element competes, one of them must yield size, weight, color, or position. A screen with three equally-loud regions has no hierarchy, only arrangement.

---

## 6. Motion & micro-interaction

**DX19 — Keep a motion inventory: few, purposeful, consistent.** List every animated moment, its purpose (causality / continuity / feedback / delight), its duration, and its easing — and cut the ones with no purpose. The house defaults that read as considered: **120–200ms** for state feedback (hover/press), **200–300ms** for entrances and transitions, **300–500ms** only for a genuinely large spatial change; ease-out for entering, ease-in for exiting, spring only where a physical metaphor is real. Motion **MUST NOT** block input, and **MUST** collapse to instant under `prefers-reduced-motion` (U10) — with the reduced path exercised in the harness (DX10).

**DX20 — Micro-interactions are where "polished" is actually decided.** The press state that responds within a frame; the focus ring that is beautiful rather than tolerated; the number that counts up on change; the row that settles after a save; the skeleton whose shape matches the content that replaces it (so nothing shifts). These are small, cheap, and they are the entire difference between "works" and "feels good". Layout shift on load or on state change is the opposite signal — treat CLS as a craft defect, not only a performance metric.

---

## 7. Words are design

**DX21 — Write the interface copy as part of the design, in the product's voice.** Buttons say what they do (`Save changes`, not `Submit`). Empty states teach the first action rather than announcing absence. Errors say what happened, why, and what to do next — never a code alone, never blame. Confirmations name the consequence, especially the irreversible one. Numbers carry their units and their precision (`technical-ui-design.md` TQ2). **Copy is drafted in the mockup, not deferred to implementation** — deferred copy becomes placeholder copy that ships.

---

## 8. The critique method

**DX22 — Critique against a rubric, not a reaction.** A review that produces "feels a bit cluttered" is unactionable. Every finding **MUST** carry: **location** (screen/component/state) · **dimension violated** (below) · **severity 0–4** · **evidence** (what you observed, or the measurement) · **recommended fix** · **confidence** (Verified / Inferred / Flagged, per `persona-audit.md` §8.3).

**The dimensions** — Nielsen's ten usability heuristics, plus the pack's own floors, plus this document's craft dimensions:

| # | Dimension | Source |
|---|---|---|
| 1–10 | Visibility of system status · Match to the real world · User control & freedom · Consistency & standards · Error prevention · Recognition over recall · Flexibility & efficiency · Aesthetic & minimalist design · Error recovery · Help & documentation | Nielsen's heuristics |
| 11 | **Archetype fit** — does the interface's kind match the job? | `ui-archetype-grammar.md` |
| 12 | **State completeness** — empty / loading / error / overflow designed and present | U9 |
| 13 | **Token discipline** — no arbitrary values; everything resolves to the system | U3, `design-lint.py` |
| 14 | **Accessibility** — WCAG 2.2 AA: contrast, keyboard path, focus visibility, semantics, targets, not-by-color-alone | U16 (**hard veto**) |
| 15 | **Performance & stability** — budget met; no layout shift | U17 |
| 16 | **Content & copy** — real, in-voice, actionable errors, units and precision | DX21, TQ2 |
| 17 | **Craft** — hierarchy, spacing rhythm, optical alignment, restraint, focal point | DX12–DX18 |
| 18 | **AI-surface honesty** — disclosure, uncertainty, correction, oversight before consequential action | U13–U15, HAX |

**Severity scale** (0 = not a problem · 1 = cosmetic · 2 = minor, easy workaround · 3 = major, hard workaround · 4 = catastrophe, must fix). Map to the pack's own severities so a review composes with every other gate: **4 → Blocker · 3 → Major · 2 → Minor · 1 → Nit**. An accessibility finding at severity ≥3 is a **Blocker** under U16 regardless of its usability impact.

**DX23 — Measure before you diagnose.** "Cluttered" is a symptom; the diagnosis is a count. Count the interactive controls on the screen, the simultaneous sections, the network calls on load, the competing focal points, the distinct type sizes, the distinct colors, the modes that do the same job. A pack repo's data-entry crisis was diagnosed exactly this way — *138 input controls, 16 cards, 8 sections, 14 parallel API calls, 3 view modes that all do the same job* — and the measured version is what made the fix obvious and the argument unarguable. *"Three modes that do the same job is not flexibility, it is an unmade decision handed to the user."*

**DX24 — Critique the structure before the surface.** Run the dimensions in order: **archetype fit** and **flow/IA** first (is this the right *kind* of thing, can the user get to their goal), then **state completeness**, then **accessibility**, then **craft**. A beautiful surface over a wrong archetype is a rewrite, and finding that last wastes the whole review. This mirrors the spec's bottom-up gating (`specification-standards.md` S2): structure gates surface.

**DX25 — A review ends with a ranked plan, not a list.** Findings are grouped into: **must fix before ship** (Blockers), **should fix next** (Majors, ranked by user impact × effort), and **worth doing** (Minors/Nits). Include the **one change with the highest ratio of improvement to effort** called out explicitly — reviews that return 40 undifferentiated findings get ignored, and the reviewer owns that outcome.

---

## 9. Self-verification checklist

- [ ] **Direction brief** written before any visual artifact: user + emotional state, archetype (from the JTBD), three adjectives *and their opposites*, named references with what's taken, anti-goals, constraints (DX5).
- [ ] **Archetype verified against the job**, including on changes to *existing* screens (DX5.2 — the reading-vs-entering test).
- [ ] Direction → system → screens → build → critique run **in order**; system built before screens (DX1–DX2).
- [ ] Self-checked against the **generic-tells table**; each tell either absent or a deliberate, justified choice (DX3).
- [ ] Type / color / space personality decisions each carry a **one-line justification** tied to the brief (DX6).
- [ ] **Fidelity ladder** climbed; a self-contained, dependency-free **mockup** committed at `docs/mockups/` with a graph-linked hub node (DX7–DX8).
- [ ] The mockup renders **empty, loading, error, partial and overflow** states, with realistic extreme content (DX9, DX16).
- [ ] A **review harness** switches persona, viewport, state, theme, capability flags and reduced motion; mechanical contrast/target checks run in-artifact (DX10–DX11).
- [ ] **Craft**: real scale contrast, spacing-as-grouping with tighter within than between, restrained color, optical alignment, one defended focal point, density calibrated with matching hierarchy (DX12–DX18).
- [ ] **Motion inventory** written; durations/easings consistent; reduced-motion path exercised; no layout shift (DX19–DX20).
- [ ] **Real copy** drafted in the mockup, in voice, with actionable errors and correct units/precision (DX21).
- [ ] Any review produced **rubric findings** (location · dimension · severity · evidence · fix · confidence), **measured** before diagnosing, ran **structure before surface**, and closed with a **ranked plan and the highest-leverage change named** (DX22–DX25).

---

## 10. References

- **`ui-interaction-design.md`** (U1–U20) — the floor: medium, tokens/`DESIGN.md`, hierarchy, complete states, motion, copy, familiar-then-novel, HAX + Shape-of-AI, WCAG 2.2 AA, performance budget. This document tells you how to *reach* it.
- **`ui-archetype-grammar.md`** (G1–G16) + **`ui-archetype-catalog.md`** — the determinism-of-kind selector, the codegen descriptors, and the surface design-language exemplars (§J).
- **`technical-ui-design.md`** (TQ1–TQ12) — expert/quantitative surfaces: density-with-hierarchy, numeric legibility, perceptually-uniform colormaps, uncertainty-first.
- **`specification-standards.md`** (S1–S10) — the three spec layers and the bottom-up gating that DX24 mirrors.
- **Nielsen Norman Group** — the ten usability heuristics and severity rating scale; heuristic evaluation as an expert method (3–5 evaluators find ~75% of problems).
- **Practitioner research on generative UI (2025–2026)** — the "AI look" as a *process* failure: split creative planning from implementation, build the design system first, use reference-backed direction, iterate in a sandbox before production, add motion and micro-interaction for personality, encode tokens as code.
- **In-repo precedents** — a pack repo's measured data-entry diagnosis (archetype mismatch, 138 controls, three redundant modes) and another's self-contained mockup with a persona/viewport/state/theme review bar: the two worked examples behind DX23 and DX10.
- **Personas** — **UX Researcher / IA** (structure, flows, findability — UX-specification veto), **UX & Accessibility** (surface, states, WCAG — accessibility hard veto), **The Simplifier** (every element earns its place), **Product Strategist** (does this serve the job).
