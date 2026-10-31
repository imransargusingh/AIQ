---
skill: ui-design
part: definition-of-done
---

> Stage detail for **/ui-design** — read from `SKILL.md` at the stage that needs it (progressive disclosure, class CTX-E). This file is the normative text; `SKILL.md` carries the outline.

## Definition of done (exit gate)
- [ ] **Direction brief written before any visual artifact** — user + emotional state, JTBD, **Archetype Signature** (justified from the JTBD), three adjectives *and their opposites*, named references with what's taken, anti-goals, constraints (DX5).
- [ ] **Archetype verified against the shape of the task** — including on an existing screen; reading-vs-entering checked (DX5, UX-A).
- [ ] **UX layer settled first**: Part B exists and its flows cover alternate/error/recovery paths; no settled Surface over an unsettled Structure (S2, S7).
- [ ] Personality decided in **type / color / space** with a one-line justification each (DX6).
- [ ] **System before screens**: `DESIGN.md` produced/updated with token frontmatter, **token-layer contrast audit**, complete state matrix, modes, paired Archetype Signature, motion, copy, performance budget, AI-UX rules; `design-lint.py` clean; preview rendered (DX2, U3a).
- [ ] **Mockup** is self-contained and dependency-free, committed under `docs/mockups/`, and renders **empty / loading / error / partial / overflow** with realistic extreme content (DX8–DX9, DX16).
- [ ] **Review harness** present and exercised — persona · viewport · state · theme/density · capability flags · **reduced motion** — with in-artifact contrast/target checks (DX10–DX11).
- [ ] **Craft** holds: real scale contrast, spacing-as-grouping (tighter within than between), restrained color with an earned accent, optical alignment, **one defended focal point**, density calibrated with matching hierarchy (DX12–DX18).
- [ ] **Motion inventory** written; durations/easings consistent; reduced-motion path proven in the harness; **no layout shift** (DX19–DX20, U17).
- [ ] **Real in-voice copy** drafted here, not deferred: actionable errors, teaching empty states, consequence-naming confirmations, units + precision (DX21, TQ2).
- [ ] **Generic-tells self-check** passed — each tell absent or a justified deliberate choice (DX3).
- [ ] **Deterministic control run** — `ui-craft-gate.py` executed over the surface, its measurement recorded, findings translated into rubric shape with the CD12 accessibility/token severity floors applied, and any suppression carrying a real reason (CD5, CD8, CD11–CD12, CD16). The gate was confirmed to have **scanned a non-empty corpus** — a detector that matched no files exits clean and proves nothing (CD9).
- [ ] **"Detector clean" was not reported as "the design is good"** — archetype fit, IA, state existence, copy truth and focal point were judged by the human/adversarial layers (CD13–CD14).
- [ ] **Generated assets (if any)** obey `ui-visual-assets.md`: no generated interface (VA5), mood-not-structure (VA6), real named references still present (VA7), committed-not-linked (VA4), manifest + alt text + disclosure (VA12–VA13), budget and perf floors met (VA10, VA14), no personal data or real likeness uploaded (VA9).
- [ ] **Measured before diagnosed** (review/elevate): counts recorded for controls, sections, load calls, focal points, type sizes, colors, redundant modes (DX23).
- [ ] **Rubric critique run structure-before-surface**; every finding carries location · dimension · severity · evidence · fix · confidence (DX22, DX24).
- [ ] **WCAG 2.2 AA** met and evidenced; the **UX & Accessibility hard veto** is cleared **by someone other than the author** (U16).
- [ ] **Ranked plan** delivered with must-fix / should-fix-next / worth-doing and **the highest-leverage change named** (DX25).
- [ ] **Triggered standards mapped at Stage 1 and applied as a union** — every row of the trigger table was walked, the ones that fire were named with their reason, and the ones that did not were explicitly excluded. A triggered-but-unmet directive is a gap, not a nicety (`testing-strategy.md` §3 semantics).
- [ ] For expert/quantitative surfaces (**UI-T1**), **`technical-ui-design.md`** applies: §G archetype, numeric legibility, perceptually-uniform colormaps (never jet), uncertainty-first, direct-manipulation-plus-precision, provenance, reactive recompute (TQ1–TQ12).
- [ ] For AI surfaces (**UI-T3**), the applicable **HAX** guidelines and **Shape-of-AI** patterns are named, and the **wrong answer and uncertainty are designed as first-class states** (U13–U15); metered usage carries its meter, quota states and upgrade path (U15a).
- [ ] For native app surfaces (**UI-T4**), the platform HIG was **established from the source** and the platform developer lens attacked idiom, lifecycle, permissions and the store/signing gates.
