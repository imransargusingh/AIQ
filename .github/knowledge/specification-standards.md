---
load: skill
skills: [specify]
---
# Specification Standards — functional, UX, and UI

*Normative guidance for **how a specification is structured, deliberated, and codified** — across its three concerns: the **functional** specification (what the product must do and why), the **UX** specification (how it *works* — structure, flows, information architecture), and the **UI** specification (how it *looks* — the visual surface). It governs the `/specify` skill and the spec template, and hands off to the **UI & Interaction Design Standard** (`ui-interaction-design.md`, U1–U20) for UI-surface excellence. The Testing Strategy governs proof; this document governs whether the spec is **complete, well-formed, and correctly layered** before any design begins.*

Normative keywords (**MUST**, **SHOULD**, **MAY**, **MUST NOT**) follow RFC 2119.

The governing idea: a specification is **one artifact with three separable, individually-ownable layers**, written from abstract to concrete, where each layer **depends on the one beneath it**. You cannot honestly specify how a screen *looks* before how it *works*, and you cannot specify how it *works* before *what* it must do and *why*. Getting the layering right — and the ownership and the gating that go with it — is what prevents the two classic failures: a beautiful surface over a broken flow, and a "single source of truth" that has silently drifted into three out-of-sync documents.

---

## 0. Research synthesis (why these rules)

**Functional specification — the requirements-engineering canon.** The governing international standard is **ISO/IEC/IEEE 29148** ("Systems and software engineering — Life cycle processes — Requirements engineering"), which in 2011 superseded and consolidated **IEEE 830** (SRS), **IEEE 1233** (system requirements), and **IEEE 1362** (concept of operations). It defines a layered hierarchy of requirements artifacts — **StRS** (stakeholder requirements) → **SyRS** (system requirements) → **SRS** (software requirements) — and, decisively, **the construct of a *good* requirement**: each requirement is uniquely identified, necessary, unambiguous, verifiable, and written to a formal syntax — `[Condition] [Subject] [Action] [Object] [Constraint]` ("Upon receiving signal X, the system shall set the 'received' bit within 2 seconds") — using the signaling keywords **shall / should / may / will** (which map cleanly onto RFC 2119 MUST/SHOULD/MAY). The modern product-management expression of the same discipline is the **PRD** (Product Requirements Document): the problem and its evidence, target users and personas, **user stories** ("As a `<user>`, I want `<action>` so that `<benefit>`") with **Gherkin** Given/When/Then acceptance criteria, and **ISO/IEC 25010** product-quality attributes as the non-functional-requirements checklist (performance efficiency, reliability, security, usability, compatibility, maintainability, portability, plus functional suitability). The cross-cutting guidance every source repeats: **separate the problem and requirements (the "what/why") from the solution and build (the "how")** — "the PRD sets the destination, the specification draws the blueprint of the vehicle" — and **prefer measurable thresholds** ("p95 < 500 ms") over adjectives ("fast").

**UX vs UI — a consistent, load-bearing distinction.** Every credible source draws the same line, and it is the line this standard enforces. **UX is how it *works*** — research, jobs-to-be-done, personas, **information architecture** (sitemaps, taxonomy, navigation, labeling), **user flows** and journey maps, and **wireframes** (lo-fi, grayscale, "where does this go?"). **UI is how it *looks and feels*** — visual design, **high-fidelity mockups**, the **design system / tokens**, component states, motion, and the **redlined handoff spec** ("what does this look like?"). The canonical discriminator: *"the export button is buried three levels deep in settings"* is a **UX / information-architecture** failure; *"the export button is easy to find but looks visually disabled"* is a **UI** failure. The deliverable taxonomy is equally consistent — UX deliverables: sitemaps, user/task flows, journey maps, low-fidelity wireframes (a **wireflow** combines a wireframe with its flow); UI deliverables: high-fidelity mockups, style guides, design systems, redlines. Two warnings recur: UX → UI is **not a one-way throw over the wall** (keep the feedback loop — a Surface constraint can send you back to Structure), and at scale **separate ownership of UX and UI outperforms** asking one person to own both.

**The formal framework that unifies all three — Garrett's Five Planes.** Jesse James Garrett's *The Elements of User Experience* models a product as five planes, from abstract to concrete, **each dependent on the plane below it**: **Strategy** (user needs + business/product goals — *why*) → **Scope** (functional and content requirements — *what*) → **Structure** (interaction design + information architecture — how the pieces fit and how the user moves) → **Skeleton** (information design, interface design, navigation design — wireframe-level *arrangement*) → **Surface** (the sensory/visual design — *how it looks*). This maps the entire specification stack onto one coherent model, and **UX is the whole stack while UI is principally the Surface**:

| Spec layer | Garrett planes | Concern | Codified as | Owner |
|---|---|---|---|---|
| **Functional** | Strategy + Scope | What & why | 29148/PRD: problem, personas, user stories + Gherkin, ISO 25010 NFRs | Product Strategist |
| **UX specification** | Structure + Skeleton | How it *works* | Information architecture, user flows (happy + alternate + error + recovery), wireframe-level structure | UX Researcher / IA |
| **UI specification** | Surface | How it *looks* | Tokens, key screens, component states, motion, copy — per `ui-interaction-design.md` (U1–U20) | UX & Accessibility |

The dependency direction *is* the gating rule: **lower planes constrain upper ones**, so the spec is written and reviewed bottom-up, and an upper layer is never settled before the layer beneath it.

**One document or three? — one artifact, three layers.** The convergent modern practice is a **single source of truth** that carries the functional requirements *and* the UX/UI design context together — because the dominant failure mode of split specs is **drift**: "engineering references one version while design works from another," and "decisions drift as the PRD loses its value as a reliable source of truth." This standard therefore makes the spec **one document with three explicitly separable, individually-ownable sections**, not three files — which also lets the knowledge graph track it as one frontmatter record with one `review-by` and one set of typed links. The escape hatch (S6) handles the genuine case where a layer grows large.

---

## 1. The three layers (always reasoned, sometimes N/A)

**S1 — One spec, three layers.** A specification produced by `/specify` MUST contain the **Functional** layer, and MUST contain the **UX** and **UI** layers **or an explicit "N/A — `<reason>`"** for each. Silence is not allowed: a backend-only change states "UX: N/A — no user-facing surface" and "UI: N/A — no visual surface"; a CLI change states its UX layer in CLI terms and may mark the UI layer N/A. An unstated layer is a gap the gate must catch (the UX Researcher/IA's veto for the UX layer; the UX & Accessibility lens's for the UI layer).

**S2 — Bottom-up dependency (UX precedes UI; function precedes both).** The layers are written and reviewed **from the plane beneath upward** (Garrett): Functional (Strategy+Scope) is settled before the UX layer (Structure+Skeleton), which is settled before the UI layer (Surface). **The UI layer MUST NOT be specified before the UX layer it rests on is coherent.** This is a hard ordering, not a stylistic preference — Surface-before-Structure is an owned anti-pattern.

**S3 — Separate ownership.** Each layer has a distinct accountable lens: **Functional → Product Strategist**; **UX → UX Researcher / Information Architect**; **UI → UX & Accessibility**. One person *may* fill several roles on a small team, but the **review** of each layer is done by its owning lens in Adversary Mode, and no layer's author clears their own hard veto (D3).

---

## 2. The functional layer (Strategy + Scope)

**S4 — Well-formed requirements.** The functional layer states the **problem independently of any solution** (if the request arrived as a solution, the underlying problem is extracted and specified), the **target users and the job-to-be-done**, the **core scenario** (the one path that, if it works end-to-end, justifies the build), and **explicit in/out scope** (non-goals are as load-bearing as goals). Requirements are written as **user stories with Gherkin acceptance criteria** — each criterion **falsifiable, traceable to a future test, and acceptable by the Test Architect** — covering the happy path *and* the error/denial cases, with **measurable thresholds** rather than adjectives. The formal 29148 requirement syntax (`shall`/`should`/`may`, condition-subject-action-object-constraint) is the fallback form for any requirement that does not fit the story shape.

**S5 — Non-functional requirements as a checklist.** The functional layer walks the **ISO/IEC 25010** quality attributes as a checklist — performance efficiency, reliability, security, usability, compatibility, maintainability, portability (and functional suitability) — recording a measurable requirement per applicable attribute and an explicit N/A for those that do not apply. These tie to the Engineering Governance lenses and become downstream thresholds and tests.

---

## 3. The UX layer (Structure + Skeleton)

**S6 — UX specification content.** When the change has a user-facing surface (any medium), the UX layer specifies: the **deepened personas and jobs-to-be-done** (the progress a real person is trying to make, evidenced — not assumed); the **information architecture** (categorization, hierarchy, navigation, labeling — labels feed the glossary, V14); the **user flows** from entry to task completion covering the **happy path *and* the alternate, empty, error, permission-denied, interrupted, and recovery paths** — drawn as flowcharts (a **Mermaid flowchart** is the codified form), not described in prose; and the **wireframe-level structure** (the Skeleton — *what* goes where and *why*, deliberately without visual styling). The owned discriminator: findability and flow integrity ("can the target user reach the goal without guessing? where are the dead ends?") live here. *If a layer's content is large enough to dominate the spec, promote it to a linked companion artifact* (`docs/specs/<feature>-ux.md`, type `spec`, `documents` link back) — the same promotion path the pack uses for decision-notes → ADRs — but the default is an in-document section.

**S7 — The UX layer gates the UI layer.** A coherent UX layer — evidenced need, coherent IA, and user flows that cover the real paths — is the **precondition** for the UI layer. The UX Researcher/IA holds the **UX-specification veto**: a user-facing change that ships without it is a BLOCK. This is the structural sibling of the UX & Accessibility lens's state-completeness check: the *flow* must contain the unhappy paths before the *screens* render them.

---

## 4. The UI layer (Surface)

**S8 — UI specification by reference to U1–U20.** When there is a visual UI, the UI layer is specified to the **UI & Interaction Design Standard** (`ui-interaction-design.md`): the **medium(s)** and authoritative platform guidelines; the **token references** (no arbitrary values); the **key screens** with hierarchy; the **complete per-component state set** (default/hover/focus/active/disabled/**loading/empty/error**/success); **motion** with reduced-motion; **real, in-voice UI copy**; and the **accessibility (WCAG 2.2 AA)** and **performance-budget** floors — and, for AI-facing UIs, the applicable **HAX** guidelines and **Shape-of-AI** patterns. This standard does not duplicate U1–U20; it requires the UI layer to satisfy them, owned by the UX & Accessibility lens (which holds the U16 accessibility veto). The UI layer **also records the chosen UI Archetype Signature** (`ui-archetype-grammar.md`; pick the nearest row in `ui-archetype-catalog.md`) — the routing/temporal/data archetype that makes generated UI deterministic in *kind* — with any facet deviations noted; the signature is the selector, U1–U20 the concrete fill. The spec carries the UI *intent and acceptance criteria*; the full visual design is produced in `/design-slice` and built in `/implement`.

---

## 5. The mandate (how `/specify` carries this)

**S9 — Layer the spec, gate bottom-up, mark N/A explicitly.** `/specify` writes the three layers in dependency order, convening the **owning lens per layer** (Product Strategist; UX Researcher/IA when there is a user-facing surface; UX & Accessibility when there is a visual UI), marking any absent layer **N/A with a reason**, and making each layer's claims **falsifiable acceptance criteria** rather than prose. The gate reviews bottom-up: the Test Architect attacks the functional criteria, the UX Researcher/IA attacks the UX layer (flow integrity, IA, findability, unhappy-path coverage), and the UX & Accessibility lens attacks the UI layer (state completeness, tokens, WCAG AA) — each holding its veto, none clearing its own.

**S10 — Traceability through the layers.** Every layer is traceable to the one beneath it: each UX flow realizes a functional user story; each UI screen realizes a UX flow. Downstream, `/design-slice` consumes the UX layer as the authoritative Structure the Surface is built on, and the test plan derives from the functional acceptance criteria *and* the UX flow-completion paths. Labels defined in the UX layer's IA become glossary entries (V14).

---

## 6. Self-verification checklist (specification)

- [ ] All three layers present, or each absent one marked **N/A — `<reason>`** (S1).
- [ ] Written and reviewed **bottom-up**; UI layer not settled before the UX layer beneath it (S2).
- [ ] Each layer owned and adversarially reviewed by its lens; no self-cleared vetoes (S3).
- [ ] **Functional:** problem (solution-independent), personas/JTBD, core scenario, explicit non-goals; user stories with **falsifiable Gherkin** criteria covering happy + error paths; measurable thresholds (S4); **ISO 25010** NFR checklist walked (S5).
- [ ] **UX (if user-facing):** evidenced personas, information architecture, **user flows covering happy + alternate + error + recovery** (drawn, not prose), wireframe-level structure (S6); UX-specification veto cleared (S7).
- [ ] **UI (if visual):** specified to `ui-interaction-design.md` — medium, tokens, complete component states, motion, real copy, WCAG 2.2 AA, performance budget, AI-UX patterns where relevant (S8).
- [ ] Traceability holds: UX flows realize functional stories; UI screens realize UX flows; IA labels seed the glossary (S10).

---

## 7. References

- **ISO/IEC/IEEE 29148** — *Requirements engineering* (supersedes IEEE 830 / 1233 / 1362): the StRS→SyRS→SRS hierarchy, the construct of a good requirement, the `shall/should/may` syntax.
- **ISO/IEC 25010** — the product-quality model used as the NFR checklist.
- **PRD practice** (Atlassian, Figma, Productboard, Perforce) — problem-first, user stories + Gherkin, the PRD-vs-technical-spec separation, and the **single-source-of-truth / drift** lesson.
- **Jesse James Garrett, *The Elements of User Experience*** — the **Five Planes** (Strategy → Scope → Structure → Skeleton → Surface), each dependent on the one below; Skeleton = information + interface + navigation design; UI ≈ Surface.
- **UX vs UI deliverables** (IxDF, Toptal, Slickplan, Nielsen Norman lineage) — IA/flows/wireframes (UX) vs mockups/design-slice-systems/redlines (UI); the buried-vs-disabled-button discriminator; wireflows; the not-a-one-way-handoff and separate-ownership warnings.
- The pack's **UI & Interaction Design Standard** (`ui-interaction-design.md`, U1–U20) — the authority the UI layer is specified against.
- The pack's **Knowledge Visualization Standard** (V14 glossary/relation registry; V13 SLAs) — labels and the spec's place in the graph.
