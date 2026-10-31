---
applyTo: "**/*.tsx,**/*.jsx,**/*.vue,**/*.svelte,**/*.css,**/*.scss,**/*.html,**/DESIGN.md"
---
# UI & Interaction Design Standard

*Normative guidance for producing **highly polished, industry-leading user interfaces** whenever a UI is part of the work — on any medium (web, native desktop, mobile, CLI, voice, embedded, XR). It governs how `/specify` frames UI requirements, how `/design-slice` specifies the interface, and how `/implement` builds it. The Testing Strategy governs proof; the Observability Standard governs telemetry; the Accessibility lens governs inclusion; **this document governs whether the interface is excellent.** A UI that merely functions is unfinished — the bar is *crafted*.*

Normative keywords (**MUST**, **SHOULD**, **MAY**, **MUST NOT**) follow RFC 2119.

*Related: the **UI Archetype Grammar** (`ui-archetype-grammar.md`, G1–G16) and its **catalog** (`ui-archetype-catalog.md`) sit one level above this standard — they select *which kind* of UI is being built (the routing/temporal/data archetype) as a determinism control for code generation, then compose **down** into the tokens, component states, and patterns specified here. Pick the archetype there; make it concrete here.*

The governing idea, synthesized from the research below: **great UI is not decoration applied at the end — it is a chain of deliberate decisions, each traceable to the user, the medium, and the brand, executed against a token-based system and (where AI is involved) a vocabulary of established AI-UX patterns.** Polish is the *absence of arbitrary choices*: every spacing value, every state, every word comes from the system, not from default.

---

## Rule index — read this first; open a section only when a rule applies

*This doc loads on demand (`load: glob`). The index is the contract in one screen: cite a rule by id, and open its section below only when a finding needs the rule's full text. A profiled session read four of these docs whole (~200 KB) to author one mockup — the index is the fix (class CTX-E).*

| Rule | In one line |
|---|---|
| **U1** | UI trigger |
| **U2** | Medium declaration |
| **U3** | Token-based, no arbitrary values |
| **U3a** | Serialize the token system as a DESIGN.md (the design-language doc) |
| **U4** | Context as modes |
| **U5** | A real type and color system |
| **U6** | Hierarchy and focal point |
| **U7** | Space and essentialism |
| **U8** | Guidance and flow |
| **U9** | Feedback and state completeness |
| **U10** | Motion with intent |
| **U11** | Content-first and real words |
| **U12** | Familiar, then novel (Jakob's Law) |
| **U13** | HAX baseline |
| **U14** | Pattern-language fluency (Shape of AI) |
| **U15** | Probabilistic by design |
| **U15a** | Cost & commercial transparency (when the AI has a per-use cost) |
| **U16** | Accessibility is a floor, not a feature |
| **U17** | Performance is part of the design |
| **U18** | Specify: name the UI bar |
| **U19** | Design: specify the interface, not just the system |
| **U20** | Implement: build it to the system, prove the states |

## 0. Research synthesis (why these rules)

**The craft baseline (the "Elegance Formula" and the canon of UI rules).** Durable, repeatedly-cited principles: establish a **clear visual hierarchy** through size, weight, color, and spacing so the eye lands on the most important thing first; use **negative space** deliberately — clutter obscures the message; apply a **grid** and a typographic **scale** rather than ad-hoc values; the **60-30-10** color balance (dominant / secondary / accent) and *sufficient contrast* as a hard floor; **typography carries personality** — pair a display and a body face deliberately, limit variations, tune line-height and measure; **progressive disclosure** and *breaking large tasks into steps* to manage cognitive load ("don't make users think"); **immediate feedback** for every action; **consistency** via a design system and standardized templates so behavior is predictable; **micro-interactions and motion** that enhance rather than distract (over-animation reads as un-crafted, often as AI-generated); content-first — *let the content lead, not the chrome*. Novelty is valuable but **balanced with familiarity** (Jakob's Law: users spend most of their time on *other* products, so honor established patterns unless there is a strong reason to deviate).

**The system beneath the surface (design tokens).** The modern consensus for consistency at scale and across platforms is a **design-token architecture** in three layers: **primitive** tokens (the raw palette — a finite set of colors, a spacing scale, type ramp, radii, shadows, durations — named by what they *are*), **semantic** tokens (meaning and role — `color.background.primary`, `color.text.danger`, `space.inset.md` — named by what they're *for*), and **component** tokens (per-component specifics — `button.radius`, `card.shadow`). Add **context dimensions** as modes over the semantic layer — **theme** (light/dark/high-contrast), **density**, **platform**, **brand** — so one system retargets to many surfaces without rewriting components. Tokens are the reason a change propagates everywhere and the reason an interface has no arbitrary values; they are typically serialized as JSON and transformed per platform. Accessibility (contrast ratios, mode readiness) is baked in **at the token level**, not bolted on per screen.

**Designing for AI (the AI-UX pattern languages).** When the interface fronts AI, two complementary references govern. **Microsoft's 18 Guidelines for Human-AI Interaction (HAX)** — the validated spine, organized by *when* they apply: **Initially** (G1 make clear what the system can do; G2 make clear how well it does it), **During interaction** (G3 time services on context; G4 show contextually relevant info; G5 match social norms; G6 mitigate social biases), **When wrong** (G7 efficient invocation; G8 efficient dismissal; G9 efficient correction; G10 scope services when in doubt; G11 make clear *why* it did what it did), and **Over time** (G12 remember recent interactions; G13 learn from behavior; G14 update and adapt cautiously; G15 encourage granular feedback; G16 convey consequences of actions; G17 provide global controls; G18 notify about changes). And **The Shape of AI** pattern taxonomy — six families of concrete, reusable solutions: **Wayfinders** (help users start: suggestions, templates, sample galleries, nudges, an initial open CTA — solving the blank-canvas problem), **Inputs** (the actions AI can take: open input, inline action, summarize, transform, regenerate, expand), **Tuners** (refine the request: attachments, filters, parameters, presets, modes, voice-and-tone), **Governors** (human-in-the-loop oversight: action plans before execution, a stream-of-thought view, controls to pause/steer mid-stream, variations, verification before consequential action, draft mode, citations), **Trust builders** (confidence in results: disclosure that content is AI, caveats about limitations, footprints/provenance, sources/references, consent, data ownership), and **Identifiers** (recognizable, on-brand AI: avatar, name, color, iconography, personality). AI interfaces are also **probabilistic** — design for the wrong answer as a first-class state (easy correction, regeneration, graceful uncertainty), surface **confidence** honestly, and *never* present a generation as settled fact without a path to verify it.

**Designing for technical and dense domains.** For developer-, data-, and operations-facing UIs: **dense information is welcomed** when it carries a strong hierarchy (reduce margins, but never hierarchy); make **metadata parsed and actionable** — searchable, filterable, sortable; tables, side drawers, and tagging are the load-bearing components; respect established leaders' patterns (Jakob's Law again). Speed *is* a feature ("it's not shipped until it's fast"); "anything added dilutes everything else." *For the deeper, expert end of this spectrum — CAD/engineering, scientific visualization (CFD/FEA), computational notebooks, financial/spreadsheet modeling, probabilistic simulation, and mathematical systems — the base knowledge is **`technical-ui-design.md`** (TQ1–TQ12: numerical legibility, perceptually-uniform colormaps — **never jet** — uncertainty-first representation, direct-manipulation-plus-precision, provenance, reactive recompute), and the matching determinism archetypes are **`ui-archetype-catalog.md` Section G** (G1–G6).*

**Medium-specific truth.** Excellence is medium-relative: a great CLI is not a small GUI. **Web** rewards a strong hero thesis, responsive fluidity, and performance budgets. **Native desktop** rewards platform conventions (the OS's HIG), keyboard-first operation, window/state management, and density. **Mobile** rewards thumb-reach ergonomics, touch-target minimums, system gestures, and offline/interrupt resilience. **CLI** rewards *visibility of system status* above all (clear progress, exit codes), composability, sensible color schemes, and good help. **Voice** rewards short turns, confirmation of consequential actions, and graceful re-prompts. The **platform's own human-interface guidelines are authoritative** for that platform; this standard sits above them, not against them.

---

## 1. When this standard applies

**U1 — UI trigger.** Any work that produces or changes a **user-facing interface** — a screen, a component, a flow, a page, a CLI command's output, a voice turn, a notification — is in scope. If `/specify`, `/design-slice`, or `/implement` touches an interface, these directives are **triggered directives** (Testing-Strategy sense): a triggered-but-unmet UI directive is a gap the gate must catch, not a stylistic nicety to skip.

**U2 — Medium declaration.** The first UI decision is naming the **medium(s)** and, for each, the **authoritative platform guidelines** (Apple HIG, Material Design, Microsoft Fluent, GNOME HIG, the relevant CLI conventions, etc.). Excellence is judged *relative to the medium* — the rest of this standard is applied through that lens. Cross-medium products state how the experience adapts per medium rather than forcing one medium's shape onto another.

---

## 2. The system (tokens before screens)

**U3 — Token-based, no arbitrary values.** A UI of any size MUST rest on a **design-token system**: **primitive** tokens (palette, spacing scale, type ramp, radii, shadows, motion durations/easings), **semantic** tokens (role-named: background/text/border/feedback, inset/stack spacing), and **component** tokens where a component needs specifics. Every value in the implementation references a token; a raw hex, a magic pixel count, or a one-off duration in component code is a finding. New products define the token set (proportional to scope — a small tool needs a small set); products with an existing or platform design system **adopt it** rather than inventing a parallel one.

**U3a — Serialize the token system as a DESIGN.md (the design-language doc).** A product's token system (U3–U5) **SHOULD** be recorded once, for the whole product, as a **design-language document** in the Google Stitch **DESIGN.md** format (`templates/design-language.template.md`) — YAML token frontmatter (`colors:`, `typography:`, `rounded:`, `spacing:`, `elevation:`, `motion:`) plus prose sections — so any agent reads one file and generates visually consistent UI, and so `/implement` builds against it (U20: reference a `{token}`, never a raw value). The pack's DESIGN.md is the Stitch format **extended with this standard's floors**, which the raw format omits: a **contrast audit at the token layer** (U16 — every text/surface pair carries its ratio + AA pass/fail), the **complete component-state set** incl. empty/loading/error (U9), **theme/density/platform modes** over the semantic layer (U4), the **paired UI Archetype Signature** (the determinism-of-kind selector, `ui-archetype-grammar.md` G11), **motion + reduced-motion** (U10), **real UI copy** (U11), the **performance budget** (U17), and — for AI UIs — the **HAX/Shape-of-AI** patterns (U13–U15). The `{token}` references are **mechanically checkable**: run `scripts/design-lint.py` (every reference resolves; no arbitrary hex in component specs — U3 as a forcing function), and render `templates/design-language-preview.template.html` as the visual catalog (swatches with computed contrast, type scale, every component state, light/dark). Seed from an exemplar if useful — the **`awesome-design-md`** library (MIT) maps to the archetype catalog (`ui-archetype-catalog.md` §J) — but **adapt, never clone** (U12); the floors above are *added*, not optional.

**U4 — Context as modes.** Theming and adaptation (light/dark/high-contrast, density, platform, brand) are expressed as **modes over the semantic layer**, so a component is authored once and retargets. Dark mode and a high-contrast mode are baseline expectations for screen media unless explicitly out of scope.

**U5 — A real type and color system.** Typography uses a deliberate **scale** (not arbitrary sizes), a considered display/body **pairing**, limited weights/styles, and tuned line-height and measure. Color follows a coherent palette with a **dominant/secondary/accent** balance and **semantic feedback colors** (success/warning/danger/info) defined as tokens. Both encode the product's personality; neither is left to defaults.

---

## 3. The craft (every screen)

**U6 — Hierarchy and focal point.** Each view has **one** clear focal point and a deliberate hierarchy built from size, weight, color, and spacing. The user's eye should land where the task begins. Layout uses a grid and honors natural scan patterns (F for text-dense, Z for sparse landing views).

**U7 — Space and essentialism.** Negative space is used deliberately; every element earns its place ("anything added dilutes everything else"). Density is a *choice matched to audience* — generous for consumer/marketing, high for expert/data/ops — but density never means disorder: dense UIs need *stronger* hierarchy, not weaker.

**U8 — Guidance and flow.** The interface leads: meaningful onboarding for first use, an intuitive step-by-step flow, **progressive disclosure** of complexity, contextual hints where needed, and prominent, unambiguous calls to action. Large tasks are **broken into steps** with visible progress.

**U9 — Feedback and state completeness.** Every user action gets **immediate feedback**. Every component is specified and built for its **full set of states** — default, hover, focus, active, disabled, **loading**, **empty**, **error**, and **success** — plus first-run/zero-data and overflow/truncation. The empty state and the error state are designed, not afterthoughts; the loading state is real (skeletons/optimistic UI where apt), not a spinner placeholder. *This is the most common place polish is missing and the gate checks it explicitly.*

**U10 — Motion with intent.** Micro-interactions and transitions clarify causality, continuity, and state change. Motion is **purposeful and restrained** — it respects `prefers-reduced-motion`, never blocks input, and never decorates for its own sake (gratuitous animation reads as un-crafted). An orchestrated key moment beats scattered effects.

**U11 — Content-first and real words.** Content leads; chrome serves it. UI text — labels, buttons, empty states, errors, confirmations — is **part of the design** and written deliberately (clear, concise, in the product's voice; errors are humane and actionable). Placeholder lorem and generic copy are findings. Imagery and iconography are purposeful and on-system, not generic stock.

**U12 — Familiar, then novel (Jakob's Law).** Honor the established pattern for a given job unless there is a *stated, defensible* reason to deviate; users carry expectations from the products they already use. Novelty is welcome where it **enhances** the experience and is balanced with familiarity — never novelty that taxes the user to relearn the obvious.

---

## 4. The AI layer (when the UI fronts AI)

**U13 — HAX baseline.** AI-facing UIs MUST satisfy the applicable **HAX 18** guidelines across the lifecycle: *initially* set capability and accuracy expectations (G1–G2); *during* time and scope contextually (G3–G4), match norms, mitigate bias (G5–G6); *when wrong* support efficient invocation/dismissal/**correction**, scope under uncertainty, and explain **why** (G7–G11); *over time* remember, learn cautiously, take granular feedback, convey consequences, provide global controls, and notify of change (G12–G18). The design names which guidelines apply and how each is met.

**U14 — Pattern-language fluency (Shape of AI).** Reach for the **established AI-UX pattern** rather than inventing one: **Wayfinders** to defeat the blank canvas (suggestions, templates, sample gallery, an inviting initial CTA); **Tuners** to let users shape requests (attachments, parameters, presets, modes); **Governors** for oversight on anything consequential (an **action plan** shown before execution, a **stream-of-thought** view, **controls** to steer mid-stream, **variations**, and **verification before irreversible action**); **Trust builders** for confidence (**disclosure** that content is AI-generated, **caveats**, **citations/sources**, **footprints/provenance**); **Identifiers** for a recognizable, on-brand AI. Name the patterns used; justify any bespoke one.

**U15 — Probabilistic by design.** The AI **will be wrong** — treat that as a first-class state, not an edge case: make correction and regeneration effortless (G9), surface **confidence/uncertainty honestly** (G2), never render a generation as settled fact without a verify path, and degrade gracefully when the system is unsure (G10). Streaming output shows progress and remains interruptible. (This dovetails with the Observability Standard: AI actions are auditable, and with the Privacy Review: prompts/outputs touching personal data follow the data rules.)

**U15a — Cost & commercial transparency (when the AI has a per-use cost).** The product's **commercial model** (`ai-commercial-models.md`: BYO subscription/key · metered pass-through · absorbed subscription) is a UX surface, not just an architecture choice. Build what the model requires: a secure **"connect your AI account"** flow for BYO (masked key/OAuth, F2 settings archetype); a **usage meter + soft/hard quota states + cost breakdown** for metered/credit models (a Trust builder — disclose what a run costs; a *regenerate spends again*, so say so); and, even for an absorbed model where per-call cost is hidden, a **fair-use allowance indicator and a graceful quota wall with an upgrade path** (a silent hard stop is a UX failure). Pair the numbers with the numeric-legibility rules in `technical-ui-design.md` (TQ2).

---

## 5. Inclusion and performance (non-negotiable floors)

**U16 — Accessibility is a floor, not a feature.** The interface meets **WCAG 2.2 AA** (or the platform's equivalent accessibility standard): sufficient color contrast (enforced at the token level), full keyboard operability and visible focus, correct semantics/roles and screen-reader labeling, target sizes, motion-reduction support, and no information conveyed by color alone. The **Accessibility lens holds a hard veto** on UI work — an inaccessible interface is not done. (Accessibility checks are negative tests in the plan, mirroring security tests.)

**U17 — Performance is part of the design.** "It's not shipped until it's fast." The design names the **performance budget** for the medium — perceived load, interaction latency, frame-rate during motion/scroll, payload/asset weight on web, cold-start on native/mobile — and the build meets it. Perceived performance techniques (skeletons, optimistic UI, prefetch) are design decisions, not afterthoughts.

---

## 6. The mandate (how the skills carry this)

**U18 — Specify: name the UI bar.** When the work has a UI, `/specify` records the **medium(s)** and authoritative platform guidelines (U2), the **target users and their density/expertise profile**, the **experience qualities** the UI must hit (the adjectives — and their opposites — that define done), the **accessibility and performance floors** (U16–U17), and, for AI UIs, the **AI-UX expectations** (which HAX guidelines apply; whether oversight/trust patterns are required). These become **testable acceptance criteria** — "the empty state guides the user to first action", "all interactive targets meet contrast AA", "the AI shows an action plan before any irreversible step" — not vague wishes.

**U19 — Design: specify the interface, not just the system.** When the work has a UI, `/design-slice` produces, beside the component/data design: the **token references** the UI uses (or the new tokens it defines, U3–U5); the **key screens/flows** with their hierarchy and the **complete state set per component** (U9); the **motion** intent (U10); the **UI copy** for the load-bearing strings (U11); the **medium adaptation** (how it changes across the declared media, U2); and for AI UIs, the **named HAX guidelines and Shape-of-AI patterns** in play (U13–U15). The **UX & Accessibility lens co-owns the design** and holds the U16 veto; bespoke deviations from established patterns (U12) are recorded as deviations with rationale.

**U20 — Implement: build it to the system, prove the states.** When the work has a UI, `/implement` builds against the tokens (no arbitrary values, U3), implements **every specified state** including empty/loading/error (U9), honors the platform HIG and responsive/adaptive behavior (U2), wires motion with reduced-motion support (U10), uses the specified real copy (U11), and meets the **accessibility (U16)** and **performance (U17)** floors — with these proven as tests: accessibility checks (keyboard path, contrast, labeling) and performance-budget assertions are **negative/threshold tests written red-first**, exactly like security tests, and a triggered-but-unmet UI directive fails the gate (the UX & Accessibility lens's veto for inclusion, the Test Architect's for coverage).

---

## 7. Self-verification checklist (UI work)

- [ ] Medium(s) and authoritative platform guidelines named; excellence judged relative to medium (U2).
- [ ] UI rests on a **token system** — primitive/semantic/component, modes for theme/density/platform; no arbitrary values in code (U3–U5).
- [ ] Clear hierarchy and single focal point per view; deliberate space; content-first (U6–U7, U11).
- [ ] **Every component state** specified and built: default/hover/focus/active/disabled/loading/empty/error/success + first-run/overflow (U9).
- [ ] Real UI copy in the product's voice; no lorem, no generic stock (U11).
- [ ] Motion purposeful, restrained, reduced-motion-aware (U10).
- [ ] Familiar pattern honored unless deviation is stated and justified (U12).
- [ ] **AI UIs:** applicable HAX guidelines met; Shape-of-AI patterns named; wrong-answer/uncertainty designed as first-class; oversight before consequential actions (U13–U15).
- [ ] **Accessibility AA** met and tested; Accessibility lens veto cleared (U16).
- [ ] **Performance budget** named and met; perceived-performance techniques applied (U17).
- [ ] Acceptance criteria for the above are falsifiable and tested red-first (U18, U20).

---

## 8. References

- **Microsoft HAX Toolkit** — *18 Guidelines for Human-AI Interaction* and the HAX Design Library (patterns + examples): microsoft.com/en-us/haxtoolkit. The validated spine for AI-facing UX.
- **The Shape of AI** (Emily Campbell) — AIUX pattern library, six families (Wayfinders, Inputs, Tuners, Governors, Trust builders, Identifiers): shapeof.ai.
- **"58 rules for beautiful UI design"** (Taras Bakusevych) — the Elegance Formula (Empathy, Layout, Essentialism, Guidance, Aesthetics, Novelty, Consistency, Engagement).
- **Design tokens** — primitive/semantic/component architecture with theme/mode/density/platform context; JSON serialization, cross-platform transform (Contentful, GitLab Pajamas, Material, IBM Carbon practice).
- **"Designing for AI Engineers"** (uxdesign.cc) — density-with-hierarchy, parsed/actionable metadata, Jakob's Law, JTBD-driven IA for technical audiences.
- **Platform HIGs** — Apple Human Interface Guidelines, Material Design, Microsoft Fluent, GNOME HIG; CLI conventions — authoritative *per medium*.
- **WCAG 2.2 AA** — the accessibility floor (W3C); pairs with the pack's Accessibility lens.
- The pack's **`frontend-design`** skill — the hands-on craft companion when the medium is web/component UI (design tokens, type, motion, anti-template guidance).
