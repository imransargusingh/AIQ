---
load: skill
skills: [ui-design, design-slice, implement]
---
# UI Craft Detection — the deterministic control beneath the craft

*Normative guidance for the **mechanical enforcement** layer under the pack's UI doctrine. `ui-interaction-design.md` (U1–U20) sets the floor, `ui-design-craft.md` (DX1–DX25) sets the craft and the critique rubric, `ui-archetype-grammar.md` fixes the kind — and all three are **prose**. This document governs the tool that turns the mechanizable subset of them into a **runnable, LLM-free, CI-gateable control**: the **Impeccable** detector (`impeccable.style`, Apache-2.0). It is to the UI standards what a lint rule is to a style guide.*

Normative keywords (**MUST**, **SHOULD**, **MAY**, **MUST NOT**) follow RFC 2119.

The governing idea comes straight from this pack's own continuous-improvement doctrine: **a lesson recorded as prose is a memoir** (`continuous-improvement.md` CI6). The control ladder ranks *automated control* (rung 2 — a test, a lint rule, a CI gate) above *always-loaded instruction* (rung 3) and *knowledge doc* (rung 4), because only a control **fires at the moment of the mistake**. Before this document, the pack's entire UI craft doctrine sat at rungs 3–4: `ui-design-craft.md` DX3 *describes* the generic-AI-look tells in a table a reviewer must remember to consult, and `design-lint.py` checks only that `DESIGN.md` is internally consistent — **nothing in the pack checked the built interface against it**.

The proof is uncomfortable and therefore worth stating first: run against **this pack's own `mockup-harness.template.html`**, the detector found three real defects — two `side-tab` accent borders (*"the most recognizable tell of AI-generated UIs"*, which DX3 names in prose) and one `layout-transition` animating `max-width` (which U17/DX20 forbid in prose). The pack had documented all three rules and shipped a template that violated them. That is exactly the failure mode CI6 exists to prevent, and it is the whole argument for adopting a control.

---

## Rule index — read this first; open a section only when a rule applies

*This doc loads on demand (`load: skill`). The index is the contract in one screen: cite a rule by id, and open its section below only when a finding needs the rule's full text. A profiled session read four of these docs whole (~200 KB) to author one mockup — the index is the fix (class CTX-E).*

| Rule | In one line |
|---|---|
| **CD1** | Know what was adopted, and on what evidence |
| **CD2** | Flagged: the version surfaces disagree |
| **CD3** | Adopt the detector; the rest is optional |
| **CD4** | The pack's design language is the detector's token source |
| **CD5** | The two linters are complementary and both run |
| **CD6** | Every rule has an owning pack directive; the detector is enforcement, not new doctrine |
| **CD7** | A rule the pack does not already hold is a candidate doctrine change, not a silent import |
| **CD8** | Four placements, each with a different job |
| **CD9** | Verify the gate executes; a green pipeline is not evidence |
| **CD10** | The hook is optional and is not the gate |
| **CD11** | Translate, don't append |
| **CD12** | Severity floors: accessibility outranks the tool's own opinion |
| **CD13** | A detector finding is Verified; a detector silence is not |
| **CD14** | A clean run is a floor, never a verdict |
| **CD15** | Never optimize for the detector |
| **CD16** | Suppression is a deviation and is recorded as one |
| **CD17** | The dependency was justified before it was adopted |
| **CD18** | Attribution is a licence obligation, not a courtesy |
| **CD19** | Scanning a file stays local; scanning a URL does not |
| **CD20** | A client-rendered surface is invisible to static scanning, and that silence reads as a pass |

## 0. When this applies

Any work that **produces or changes a rendered user interface** — HTML/CSS/JSX/TSX/Vue/Svelte source, a template, a mockup, or a deployed URL. It is triggered by `/ui-design` (Stage 3 measure, Stage 4 gate), `/implement` (build-time control), `/design-slice` (naming the gate in the test plan), and any CI pipeline that ships a UI. It is **not** triggered by backend work, and it is not a substitute for the human and adversarial critique it feeds.

**Owner:** the **UX & Accessibility** lens (which holds the U16 hard veto and consumes the contrast/legibility findings) with the **Test Architect** (who owns whether the control is real, red-first, and actually runs — `end-to-end-integrity.md` E13: *a gate's green result is evidence the gate passed, not that its contents passed*).

---

## 1. What the tool is — established, not assumed

**CD1 — Know what was adopted, and on what evidence.** The following were established by fetching the source and **executing** the tool, not inferred from the name (`no-guessing-protocol.md` NG1):

| Fact | Value | Confidence |
|---|---|---|
| Project | **Impeccable** — `github.com/pbakaus/impeccable`, `impeccable.style` | **Verified** |
| What it is | An AI-coding-agent skill set **plus** a standalone deterministic detector CLI. Not a CSS library, not a component library, not an MCP server. | **Verified** |
| Licence | **Apache-2.0**, © 2025 Paul Bakaus; one MIT transitive dep (`ehmo/platform-design-skills`, iOS/Android reference only) | **Verified** |
| npm package | `impeccable`, bin `impeccable` → `cli/bin/cli.js` | **Verified** (installed and run) |
| Detector rules | **59**, enumerated from the installed package | **Verified** (counted) |
| Runs without an LLM | Yes — no API key, no network for local files | **Verified** (executed offline) |
| Exit code | **non-zero on findings** (observed `2`) — CI-gateable | **Verified** |
| Output | `--json` emitting `antipattern · name · description · severity · category · file · line · snippet` | **Verified** |
| Maintenance | Actively released (multiple releases per week at adoption); ~48k stars | **Verified** (registry + changelog) |

**CD2 — Flagged: the version surfaces disagree.** The site's changelog advertised **v4.0.4** while the npm registry served **3.5.0** at adoption. Pin the version you install, record it, and **re-establish the rule set on upgrade** — the rule inventory is a contract, and a contract asserted from last month's memory is a guess (NG3). Treat a rule-count change as a breaking change to this document.

**CD3 — Adopt the detector; the rest is optional.** Impeccable also ships a multi-harness *skill* (23 commands), a design **hook**, a browser extension and a **Live Mode**. Those overlap substantially with this pack's own `/ui-design`, `ui-design-craft.md`, and persona/veto machinery, and adopting them wholesale would install a **second, competing methodology** — the Convention Importer anti-pattern at the scale of an entire doctrine (BoK Part VIII; Part V.1). The pack therefore adopts the part it genuinely lacks — **the deterministic detector** — and keeps its own direction, archetype, spec-layer, persona and critique machinery. A team **MAY** additionally install the skill; if they do, `/ui-design` remains authoritative on process and its vetoes are not overridden by a second skill's opinion. Record a dual install as a deviation (Rules of the Road §4).

---

## 2. The seam — `DESIGN.md` is already the input contract

**CD4 — The pack's design language *is* the detector's token source. No glue.** The detector resolves a design source by looking for `DESIGN.md` (or `Design.md`/`design.md`) at the project root, falling back to `.agents/context/` and **`docs/`** — exactly where this pack's design-language artifact lives (U3a, `templates/design-language.template.md`, the Stitch format extended with the pack's floors). Verified by execution: the pack's own `examples/design-languages/linear.design.md`, placed as `DESIGN.md`, caused the detector to flag an off-palette colour, an undeclared font and an off-ramp font-size **against the pack's own tokens**:

```
[design-system-font]      div uses comic sans ms; not declared in DESIGN.md typography
[design-system-color]     text color rgb(255,0,170) is outside DESIGN.md colors
[design-system-font-size] font-size: 9px is off the DESIGN.md type ramp
[low-contrast]            3.6:1 (need 4.5:1) — text #ff00aa on #ffffff
[undersized-ui-text]      9px functional text (below 11px floor)
```

This is the enforcement of **U3** ("every value references a token; a raw hex or magic pixel in component code is a finding") and **U20** ("build against the tokens, never arbitrary values") that the pack has mandated since revision 15 and could not previously check. It is also the cheapest available satisfaction of `end-to-end-integrity.md` **E11** — *prove the rendered surface, not just the units* — because `detect <url>` runs against the actually-rendered page.

**CD5 — The two linters are complementary and both run.** They check opposite directions of the same contract:

| Tool | Direction | Question it answers |
|---|---|---|
| `design-lint.py` (in-pack, stdlib) | **inward** — DESIGN.md against itself | Does every `{group.token}` reference resolve? Are `colors:`/`typography:` declared? Is there raw hex in a component spec? |
| `impeccable detect` (adopted) | **outward** — implementation against DESIGN.md | Does the built source use a colour/font/size/radius the token system never declared? Plus 55 further craft rules. |

A DESIGN.md that is `design-lint.py`-clean but whose implementation is riddled with off-token literals is a token system in name only. **Both MUST run** on UI work at T1/T2.

---

## 3. The rule inventory, mapped onto the pack's own standards

**CD6 — Every rule has an owning pack directive; the detector is enforcement, not new doctrine.** This mapping is what keeps the detector *subordinate* to the pack rather than a second source of truth. The 59 rules cluster as:

| Cluster | Representative rules | Pack directive it enforces |
|---|---|---|
| **Token discipline** | `design-system-color`, `design-system-font`, `design-system-font-size`, `design-system-radius` | **U3, U20** — no arbitrary values |
| **Accessibility & legibility** | `low-contrast`, `gray-on-color`, `tiny-text`, `undersized-ui-text`, `skipped-heading`, `justified-text`, `all-caps-body` | **U16** (WCAG 2.2 AA), **TQ2/TQ11** |
| **The generic-AI-look tells** | `ai-color-palette`, `cream-palette`, `gradient-text`, `side-tab`, `nested-cards`, `icon-tile-stack`, `kicker-above-heading`, `hero-eyebrow-chip`, `numbered-section-labels`, `overused-font`, `radial-halo`, `radial-spotlight-glow`, `dark-glow`, `codex-grid-background`, `repeating-stripes-gradient`, `gpt-thin-border-wide-shadow`, `border-accent-on-rounded`, `italic-serif-display`, `shape-assembled-illustration` | **DX3** — the tells table, mechanized |
| **Hierarchy, rhythm & space** | `flat-type-hierarchy`, `heading-rhythm`, `monotonous-spacing`, `cramped-padding`, `oversized-h1`, `line-length`, `tight-leading`, `wide-tracking`, `extreme-negative-tracking`, `edge-flush-cards` | **DX12, DX13, DX15, DX17** — scale contrast, spacing-as-grouping, density-with-hierarchy |
| **Motion & stability** | `layout-transition`, `bounce-easing`, `marquee`, `pulsing-dot`, `blinking-cursor`, `image-hover-transform` | **U10, U17, DX19–DX20** — purposeful motion, no layout thrash |
| **Copy** | `marketing-buzzword`, `em-dash-overuse`, `aphoristic-cadence`, `theater-slop-phrase`, `repeated-container-text` | **U11, DX21** — real, in-voice copy |
| **State & overflow integrity** | `broken-image`, `text-overflow`, `text-occlusion`, `clipped-overflow-container`, `content-hidden-at-rest`, `first-viewport-column-overflow`, `body-text-viewport-edge`, `script-error` | **U9, DX9, DX16** — the hard states and realistic extreme content |

**CD7 — A rule the pack does not already hold is a candidate doctrine change, not a silent import.** Where the detector flags something the pack has never articulated (its copy-cadence rules are the clearest case), the finding is still actionable — but promoting it to a *pack* rule goes through `/extendaibundle` like any other change. Do not let a tool quietly become the standard.

---

## 4. Where it runs

**CD8 — Four placements, each with a different job.**

1. **`/ui-design` Stage 3 — measure before you diagnose (DX23).** Run `detect --json` over the surface and **count** findings by rule and cluster. "Cluttered" is a symptom; `17 × nested-cards` is a diagnosis.
2. **`/ui-design` Stage 4 — the gate.** Findings enter the rubric critique (§5) as evidence the UX & Accessibility lens consumes. The detector does not itself hold the veto.
3. **`/implement` — build-time control.** Runs against the built source; a **Blocker-mapped** finding fails the build, exactly as an accessibility or security test does — and, like those, it is written **red-first**: confirm the rule fires on the un-fixed code before claiming the control works (CI6; `testing-strategy.md` D1).
4. **CI — the durable control.** `impeccable detect --json <dir>` in the pipeline, non-zero exit gating the merge. This is the rung-2 control; the other three are how a human meets it earlier.

**CD9 — Verify the gate executes; a green pipeline is not evidence.** `end-to-end-integrity.md` **E13** exists because a repo ran 19 control suites in one shell block and propagated only the last exit code — 18 were advisory and one had never run. The detector step **MUST** report its own status independently, and the run **MUST** be confirmed to have scanned a **non-empty corpus**: a detector that matched no files exits clean and proves nothing (E14, a success-shaped failure).

**CD10 — The hook is optional and is not the gate.** Impeccable's design hook (fires on UI file edits) is fast feedback. It **MUST NOT** be the control of record — a hook is per-developer, per-harness and silently disable-able. CI is the control; the hook is ergonomics.

---

## 5. Mapping findings onto the pack's rubric

**CD11 — Translate, don't append.** A detector finding is **not** a review finding until it carries the pack's shape (DX22): **location · dimension · severity · evidence · fix · confidence**. The translation is mechanical and is what `ui-craft-gate.py` performs:

| Detector field | Rubric field |
|---|---|
| `file` + `line` | **location** |
| the rule's cluster (§3) | **dimension** (one of DX22's 18) |
| `severity` + the cluster's floor (CD12) | **severity** (0–4 → Nit / Minor / Major / Blocker) |
| `snippet` + `name` | **evidence** |
| `description` | **fix** |
| — | **confidence: Verified** (CD13) |

**CD12 — Severity floors: accessibility outranks the tool's own opinion.** The detector's own `severity` is advisory. The pack overrides it **upward** in two cases. Any **accessibility-cluster** finding (`low-contrast`, `gray-on-color`, `tiny-text`, `undersized-ui-text`, `skipped-heading`) is **Major minimum, and Blocker under an accessibility obligation** — U16 is a hard veto and does not negotiate with a linter's default. Any **token-discipline** finding is **Major minimum** at T1/T2, because U3/U20 is the contract the whole design language rests on. Everything else maps from the tool's own severity.

**CD13 — A detector finding is Verified; a detector *silence* is not.** The tool executed and observed the pattern, so a finding is **Verified** in the confidence ledger (`rigor-protocol.md`). But absence of findings is **not** evidence of quality — it is evidence that 59 specific patterns are absent. Recording "detector clean" as "the design is good" is exactly the laundering NG7 forbids, and it is the most likely way this control gets misused.

---

## 6. The boundary — what the detector cannot do

**CD14 — A clean run is a floor, never a verdict.** The detector cannot see whether the **archetype fits the job** (DX5, the reading-is-parallel/entering-is-serial test — the failure that turned one repo's data-entry screen into a rewrite); whether the **information architecture** lets a user reach their goal (S6, the UX-specification veto); whether the **empty/loading/error states exist at all** (it catches overflow and hidden content, not *absence of design*); whether the **copy is true**; whether the **focal point is defended** (DX18); or whether the thing solves the user's problem. Those are the human and adversarial layers — and the detector's job is to stop them wasting attention on `border-left: 3px`.

**CD15 — Never optimize for the detector.** Passing 59 rules is not the goal; a good interface is, and the rules are a proxy. Reshaping a design to silence a rule *without improving it* is Coverage Theater (BoK Part VIII) in a new costume — and the tool's own `undersized-ui-text` text names the trap precisely: *"adding 8px to the ramp launders the token but not the legibility problem."* Widening `DESIGN.md` to legalise drift is the same move. If a token must be added, that is a **design-system decision with a rationale**, not a lint fix.

**CD16 — Suppression is a deviation and is recorded as one.** An inline ignore (`<!-- impeccable-disable <rule>: <reason> -->`) or an ignores-file entry **MUST** carry a real reason, and a **standing** suppression follows the Deviation Protocol (Rules of the Road §4): name the rule, state the consequence, justify it, record it. A reason-free suppression is indistinguishable from a defect, and a growing ignore file is the measurable signal that the design language and the implementation have diverged — review it like any other debt ledger (`solution-selection-ladder.md` L6).

---

## 7. Adoption posture & risk

**CD17 — The dependency was justified before it was adopted.** Per BoK Part III (adopt-or-not) and the Solution-Selection Ladder (L3), the honest ledger. **For:** it supplies a capability the pack demonstrably lacked (proven by finding three defects in the pack's own template), Apache-2.0, no copyleft, actively maintained, no API key, no data egress for local scanning, and it reads a file format the pack already produces. **Against:** a **solo maintainer** (bus-factor 1), a fast release cadence that can move the rule set under you (CD2), and Node/npm added to the toolchain. Mitigation: pin the version, record the rule inventory, and keep the pack's doctrine authoritative — so losing the tool degrades the control to prose rather than losing the standard.

**CD18 — Attribution is a licence obligation, not a courtesy.** Apache-2.0 requires preserving the copyright and NOTICE. Any vendored rule text, description or list reproduced in a repository carries its attribution (the pack's existing pattern is `examples/design-languages/ATTRIBUTION.md`).

**CD19 — Scanning a file stays local; scanning a URL does not.** `detect <file|dir>` is offline. `detect <url>` drives a headless browser against that URL. Pointing it at a **private or authenticated** environment is a trust-boundary decision (Security & Identity lens); pointing it at a page rendering **real personal data** is a Privacy decision (hard veto) — never do it as a convenience.

**CD20 — A client-rendered surface is invisible to static scanning, and that silence reads as a pass.** When a page's HTML is a shell and the interface is built at runtime, the detector scans the shell: it reports few findings because there is little to see, and the report is honest about what it scanned while being useless about the surface. Measure it before trusting a clean run — **strip the `<script>` blocks and count what is left**. If the remaining body is a small fraction of the whole, static scanning is advisory only and the real control is `detect <url>` against the rendered page. Two operational consequences worth knowing before you rely on it: URL scanning **requires `puppeteer`**, which is *not* part of the base install and must be added deliberately (a dependency decision in its own right, CD17); and a surface that can only be scanned when it is running is a surface whose control is weaker than one that can be scanned from source — which is one more reason the pack's own HTML artifacts are dependency-free and server-rendered by construction (V9, DX8).

---

## 8. Self-verification checklist

- [ ] The tool's **version and rule count** were established by execution and recorded, not recalled (CD1–CD2).
- [ ] Only the **detector** was adopted; any additional install of the Impeccable skill is a recorded deviation and does not override the pack's process or vetoes (CD3).
- [ ] The project's **`DESIGN.md`** sits where the detector resolves it (root or `docs/`), so token-discipline rules actually fire (CD4).
- [ ] **Both** linters ran — `design-lint.py` inward, `impeccable detect` outward (CD5).
- [ ] Findings were **counted before being diagnosed** (CD8, DX23).
- [ ] The CI step **reports its own status**, scanned a **non-empty corpus**, and was seen to **fail red-first** (CD8–CD9, E13–E14).
- [ ] Findings were **translated into rubric shape** with the accessibility and token **severity floors** applied (CD11–CD12).
- [ ] "Detector clean" was **not** reported as "the design is good"; archetype, IA, states and copy were judged by the human/adversarial layers (CD13–CD14).
- [ ] No rule was silenced by **laundering** a token or reshaping without improving (CD15).
- [ ] Every **suppression** carries a reason; standing ones follow the Deviation Protocol (CD16).
- [ ] Any **new rule** worth keeping was promoted through `/extendaibundle`, not silently absorbed (CD7).
- [ ] URL scanning of private or personal-data-bearing pages was a **deliberate, reviewed** decision (CD19).
- [ ] For a **client-rendered** surface, the shell was measured (strip `<script>`, count what remains) before a clean static run was trusted; where the body is mostly runtime-built, static scanning was treated as advisory and `detect <url>` (which needs `puppeteer`) is the real control (CD20).

---

## 9. References

- **Impeccable** — `impeccable.style`, `github.com/pbakaus/impeccable` (Apache-2.0, © 2025 Paul Bakaus); npm `impeccable`; `detect [file|dir|url] [--json]`. Rule inventory verified by execution at adoption.
- **`continuous-improvement.md`** — **CI6** the control ladder (rung 2 beats rungs 3–4) is the entire argument for this document; CI1–CI5 for registering what the detector finds as classes.
- **`ui-design-craft.md`** — **DX3** the tells table this mechanizes, **DX22** the rubric findings are translated into, **DX23** measure-before-diagnose, **DX24** structure-before-surface (which the detector cannot do).
- **`ui-interaction-design.md`** — **U3/U3a/U20** token discipline, **U9** state completeness, **U10/U17** motion and performance, **U16** the accessibility hard veto.
- **`end-to-end-integrity.md`** — **E11** prove the rendered surface, **E13** a gate's green ≠ its contents', **E14** read the state.
- **`no-guessing-protocol.md`** — NG1/NG3/NG7: the contract was executed, not assumed; a tool's output does not promote a claim beyond what it observed.
- **`ui-visual-assets.md`** — the generation half of the same loop: the visual world this control then verifies.
- **`scripts/design-lint.py`** (inward linter) and **`scripts/ui-craft-gate.py`** (the CD11 translation) — the pack's own halves of this control.
