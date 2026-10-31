---
name: visualize
description: Generate, curate and commit the visual assets a site shows — hero and section imagery, consistent personas, and cinematic motion — from a configured generation backend (Higgsfield, Google Gemini/Veo), under the ui-visual-assets guardrails. Use to make a surface look genuinely produced rather than templated, or to build a direction board that makes a brief concrete.
runs_as: either
---

# Skill: /visualize

Produce the **imagery, personas and motion a surface shows** — and commit them with the provenance, alt text, budget and licence record that makes them safe to ship. This is the *production* half of visual work: `/ui-design` decides what the surface should be, `/visualize` makes the pictures it contains. It runs standalone against a site you already have, and `/ui-design` hands off to it at Stage 1 (the direction board) and Stage 5 (the final assets).

The one thing to internalise before running it: **it generates what the interface shows, never the interface.** Ask an image model for "a better version of this page" and you get malformed text and invented controls — an image that looks finished and encodes nothing anyone can build. Generate the photograph inside the card; hand-author the card.

**Spine:** the Rigor Protocol, weighted toward **Stage 1 OPEN** (the register comes from words, before any generation) and **Stage 4 DISCONFIRM** (the imagery tells, the budget, the licence). **Authority:** **`ui-visual-assets.md`** (VA1–VA22) governs this skill absolutely; `ui-design-craft.md` DX4/DX5/DX16/DX19 supplies the direction and motion discipline; `ui-interaction-design.md` U11/U13–U17 supplies the copy, disclosure, accessibility and performance floors; `ai-commercial-models.md` AC2–AC3 supplies the cost posture. **Mode:** Peer Mode to generate, Adversary Mode to cull — and most of the value is in the culling.

## Grounding (first action)
Consume the compiled prompt when one is in hand (CO-S0, `knowledge/agent-coordination.md`; `/compile`): its goal state is the turn's goal state and its Not-in-scope is the interdiction — derive nothing from raw prose that a compiled prompt already fixed. Read, before generating: the project's **design language** (`DESIGN.md` — the token system and any existing `assets:` manifest), the **direction brief** if one exists (`docs/mockups/`, `docs/reviews/`, the spec's Part C), the surface that will render the asset (**open it — do not describe it from memory**, E15), and the **defect-class register** for the VA-* classes. Then run **`python3 docs/ai-forward-pack/scripts/visual-assets-setup.py --check`** to establish which backend this environment can actually reach. If none is confirmed, stop and say so rather than describing images you cannot produce.

> **Establish the entitlement before designing around it (VA19).** *"I have a Google subscription"* does **not** mean an agent can generate. A consumer Google AI Pro/Ultra plan grants the Gemini app, Flow and Whisk and grants **no API access at all**; the API is a separate billing relationship (an AI Studio key on a Cloud project with active billing), and **image and video generation are not on the free tier**. The same question applies to any backend: which account holds it, does it expose an **API** rather than only a web app, and is the capability on the tier being paid for. Check it; do not infer it from the fact that a subscription exists.

## Input
A surface plus what it needs. Examples: *"hero imagery for the landing page"*; *"a direction board for the onboarding flow"*; *"three consistent personas for the review harness"*; *"a short cinematic clip for the product section"*; *"beautify the marketing site"*.

**If the input is "beautify" or similarly open, stop and narrow it first.** "Beautify" usually means one of three different jobs and only one of them is this skill: *make the visual design better* (that is `/ui-design`, and imagery will not fix hierarchy, spacing or type), *add imagery the page displays* (that is this skill), or *make the direction concrete* (that is the direction-board mode below). Name which one before generating anything.

**Native app inputs.** `/visualize` may produce images, fictional personas, direction boards and motion that a native app **shows** — for example a WPF onboarding illustration, a fictional persona portrait for a native review harness, or a product-turntable clip for a native marketing surface. It **must reject generated native interfaces**: no "WPF settings window screenshot", WinUI control panel, native chart/table, icon set, menu, toolbar, file-manager pane, or operable control image. Route those requests to `/ui-design` or implementation. Generated assets are content fixtures; native XAML/WinUI/WPF/Avalonia controls remain hand-authored and token/proof checked.

## Modes

| Mode | When | Produces |
|---|---|---|
| **board** | The direction brief exists in words and needs to be made concrete | 2–4 candidate direction boards in `_scratch/`, one chosen and recorded in `DESIGN.md` |
| **asset** | The direction is settled; the surface needs specific imagery | Committed, optimized, manifested images for named slots |
| **persona** | The review harness or the surface needs consistent people | A small fictional cast with stable references, treated as fixtures |
| **motion** | A marketing or demonstration surface needs a clip | A short video with a poster frame, a reduced-motion fallback, and a motion-inventory entry |

**Default when unclear:** **board**. A concrete visual reference is cheap, is the highest-leverage thing generation offers, and prevents the expensive mistake of generating a dozen finished assets in the wrong register.

## Cast
- **Peers:** **UX & Accessibility** (lead — the register, alt text, the contrast of anything rendered over imagery), **Product Strategist** (does this serve the job, or is it decoration?), **Domain Researcher** (establish the backend contract and the licence terms rather than recall them).
- **Adversaries:** **The Simplifier** (**soft veto** — does this asset earn its weight at all? the honest answer is often no), **AI Systems Engineer** (**hard veto** — non-determinism in a committed artifact, and the inference spend), **Privacy & Data Governance** (**hard veto** — likeness, personal data, egress), **UX & Accessibility** (**hard veto** — missing alt text, contrast over imagery, motion without a reduced path), **SRE** (the performance budget, which generated assets blow more often than anything else).

## Flow

**Stage 0 — Interdict the rush.** **Do not generate yet.** Generation is the cheapest step to start and the most expensive to redo at scale: forty images in the wrong register cost forty times one image in the wrong register. Nothing is generated until the register exists in words and the budget is stated.

**Stage 1 — OPEN (the register, in words).** From the direction brief (or by writing the missing part of one): the **register** — atmosphere, palette temperature, material, era, light; the **anti-goals** (what this must never look like); the **slot list** — every asset needed, with its rendered dimensions, aspect ratio and whether text will sit over it; and the **budget** (VA10) — a stated ceiling before the first call, because batch exploration is where budgets die. Choose the **preset** from the product-appropriate subset, not the whole catalogue (VA3).

> **Mood, never structure (VA6), and never instead of a real reference (VA7).** A generated board sets atmosphere. It does not choose the archetype, the information architecture or the layout — those come from the grammar and the UX layer, and they are settled first. And a generated image is evidence of nothing: no product shipped it and no user used it, so it *supplements* the named real references DX4 requires, never replaces them.

**Stage 2 — INTERROGATE.** For each slot: what job does this image do that type and space could not? (If there is a good answer, generate; if not, the Simplifier already won.) What sits **over** it, and at what contrast? What is the largest rendered dimension, and therefore the resolution actually needed? Does it need to be **consistent** with another asset, and therefore need a character or style reference? What happens when it fails to load — is the layout still sound?

**Stage 3 — EVIDENCE (generate, into scratch).** Generate candidates into **`docs/assets/<surface>/_scratch/`**, which is git-ignored: exploration is not a commit. Record the **verbatim prompt, backend, model and preset** for every candidate as you go, not afterwards — without the exact prompt nobody can regenerate a consistent sibling. Handle the async reality: poll to a terminal state, and treat `failed` and content-refusal as ordinary outcomes rather than crashes (VA2). Prefer **one better prompt over ten samples**.

**Stage 4 — DISCONFIRM (cull, hard).** This is where the value is.
- **The imagery tells (VA8).** Over-lit teal-and-orange grading, impossible bokeh, glossy plastic skin, symmetrical hero compositions, stock-mimicry, subtly wrong geometry. If it looks like every other AI image, it is doing the same damage a violet gradient does. Reject and re-prompt rather than settling.
- **Against the anti-goals.** Hold each candidate against the brief's opposites, not just its adjectives.
- **The Simplifier's pass.** Which of these slots is better served by nothing at all? Delete before committing; a committed asset is a maintenance obligation.
- **Licence and likeness (VA9, VA11).** No recognisable real person, trademark or brand. Nothing depicting a real individual. Commercial-use terms **checked at time of use**, not recalled — provider terms change.
- **Contrast over imagery (VA18).** Where text sits on the asset, the floor is measured against the actual composite, not the intended overlay colour.

**Stage 5 — CONVERGE (commit the survivors).**
- **Download immediately and commit** (VA4). Provider results expire — Higgsfield retains roughly a week, and a returned URI should be treated as ephemeral everywhere. A committed artifact that links a provider URL is a broken image with a delay timer, and `broken-image` is one of the detector's own rules.
- **Optimize to the performance budget** (VA14, U17): a modern format, sized to the largest rendered dimension, explicit `width`/`height` to prevent layout shift, responsive sources, lazy-loading below the fold.
- **Write the manifest entry** in `DESIGN.md` (VA12): id, file, purpose, backend, model, preset, **verbatim prompt**, date, cost, **alt text**, disclosure, licence-checked.
- **Write the alt text now** (VA13) — descriptive for informative imagery, `alt=""` for genuinely decorative. The prompt is not alt text: one describes what you asked for, the other what a non-sighted user needs.
- **Motion additionally** (VA17): a poster frame, a `prefers-reduced-motion` fallback, captions or a text alternative where it carries information, and an entry in the **motion inventory** with its purpose, duration and easing.
- **Verify the rendering surface**: run `ui-craft-gate.py` over the surface that now displays the assets (`broken-image`, `text-occlusion`, `low-contrast`, `image-hover-transform`), and confirm the budget still holds.
- Delete `_scratch/` contents that were not chosen, and **state the spend**.

**Close with the status table (mandatory).**

| | |
|---|---|
| **Completed** | slots filled, assets committed, spend |
| **Remaining** | slots still empty or deliberately left empty |
| **Best next action** | the single concrete next step |

## Output artifact
- `docs/assets/<surface>/*` — committed, optimized assets referenced by relative path.
- `DESIGN.md` — an `assets:` manifest entry per asset, with verbatim prompt, model, preset, cost, alt text and disclosure.
- The surface itself, wired to the assets with dimensions, responsive sources and alt text.
- For **board** mode: the chosen direction board recorded in `DESIGN.md` with its prompt; the rejected candidates deleted.

## Definition of done (exit gate)
- [ ] **Entitlement established** — which account, API or app only, and is the capability on the paid tier (VA19). `visual-assets-setup.py --check` run and a backend confirmed.
- [ ] **The job was named** — direction board, asset production, personas or motion — and "beautify" was narrowed rather than assumed.
- [ ] **Register and slot list in words, and a stated budget, before the first call** (VA10; Stage 1).
- [ ] **No generated interface** — nothing containing readable UI text or operable controls (VA5).
- [ ] **Native app classification (if applicable)** — generated content is an asset/persona/motion fixture the native UI shows, not a generated WPF/WinUI/Avalonia/Blazor Hybrid screen, window, control panel, chart, table, icon set, menu or toolbar.
- [ ] **Mood not structure** (VA6); **real named references still present** in the brief (VA7).
- [ ] Presets chosen from the **product-appropriate subset** and named in the manifest (VA3).
- [ ] Candidates went to git-ignored **`_scratch/`**; only survivors were committed.
- [ ] **Culled against the imagery tells and the anti-goals** (VA8); the Simplifier's "none at all" was genuinely considered.
- [ ] **No real likeness, personal data or customer content uploaded** (VA9); personas are fictional fixtures (VA16).
- [ ] **Commercial terms and provenance checked at time of use**; disclosure applied (VA11).
- [ ] **Committed, not linked** (VA4); optimized to the **performance budget** with explicit dimensions (VA14, U17).
- [ ] **Manifest entry per asset** with verbatim prompt, backend, model, preset, date, cost (VA12).
- [ ] **Alt text written at generation time**; decorative uses `alt=""` (VA13).
- [ ] **Motion** has a poster, a reduced-motion fallback, a text alternative and a motion-inventory entry (VA17, U10).
- [ ] **The rendering surface was re-verified** — detector clean for `broken-image` / `text-occlusion` / `low-contrast`, contrast measured on the **composite** (VA18).
- [ ] **Spend stated** against the budget.

## Documentation & discoverability (last action)
Per the Discoverability Mandate (`knowledge-visualization.md` V10): the assets are **data**, so the graph node is the surface's hub `.md` (or `docs/assets/<surface>/README.md` where the set warrants its own node) carrying V2 frontmatter with typed links to the design language and the surface it serves. Sync the derived index with `python3 docs/ai-forward-pack/scripts/docs-graph.py derive` (V18). A changed direction board or a replaced hero is **material** — flag the inbound neighbours (`DESIGN.md`, the spec's Part C, the mockup) `review-suggested` (V16). Capture the register decision and the rejected candidates as a **decision note** in `docs/notes/` (V17) — the rejected direction is the part people forget and re-litigate. Register any new failure as a **class** in `docs/lessons/defect-classes.md` (CI1).

**Audit (last action).** `python3 docs/ai-forward-pack/scripts/audit-log.py append --shortname "visualize-<surface>" --session "<id>" --skill visualize --kind skill --prompt "<verbatim>" --summary "<slots filled, spend>" --artifact docs/assets/<surface>/…` (AL5). When the run settles the visual direction, add a change-log entry (CL1).

**Handoff:** → `/ui-design` if the surface's hierarchy, states or archetype are the real problem (imagery will not fix any of them) · → `/implement` to wire the assets into the built surface against `DESIGN.md`.
