---
load: glob
applyTo: "**/*.tsx,**/*.jsx,**/*.vue,**/*.svelte,**/*.css,**/*.scss,**/*.html,**/DESIGN.md"
---
# Technical, Scientific & Quantitative UI Design

*Base knowledge the UX lenses (UX & Accessibility, UX Researcher/IA) maintain for **expert, data-dense, computational** interfaces — CAD & engineering, scientific visualization (CFD, FEA), computational notebooks, financial/spreadsheet modeling, simulation (Monte Carlo for risk, sports, forecasting), and mathematical design systems. It sits beside the **UI & Interaction Design Standard** (`ui-interaction-design.md`, U1–U20, the token/state/excellence authority) and feeds the **UI Archetype Catalog** Section G (the technical/scientific/quantitative archetypes). The general standard governs whether *any* UI is excellent; this governs whether a UI for **experts working with quantities, models, and uncertainty** is correct and legible.*

Normative keywords (**MUST**, **SHOULD**, **MAY**, **MUST NOT**) follow RFC 2119.

The governing idea: **these interfaces serve experts manipulating quantities, models, and uncertainty — so the bar is not "looks clean" but "is numerically honest, legible at density, and truthful about what is known."** A beautiful dashboard that misreads a colormap, hides uncertainty behind a point estimate, or rounds away significant digits is *wrong*, not merely unpolished. Density is welcomed (these are expert tools), but only with **stronger** hierarchy — never weaker.

---

## Rule index — read this first; open a section only when a rule applies

*This doc loads on demand (`load: glob`). The index is the contract in one screen: cite a rule by id, and open its section below only when a finding needs the rule's full text. A profiled session read four of these docs whole (~200 KB) to author one mockup — the index is the fix (class CTX-E).*

| Rule | In one line |
|---|---|
| **TQ1** | Density with hierarchy, never density as disorder |
| **TQ2** | Numerical legibility is a hard requirement |
| **TQ3** | Perceptually-uniform, colorblind-safe colormaps; never rainbow/jet |
| **TQ4** | Maximize data-ink (Tufte) |
| **TQ5** | Uncertainty is first-class; never a point estimate for a stochastic result |
| **TQ6** | Direct manipulation and precision input — both, never only one |
| **TQ7** | Units & dimensional integrity |
| **TQ8** | Reproducibility & provenance |
| **TQ9** | Reactive recomputation with visible staleness |
| **TQ10** | Performance for large data |
| **TQ11** | Accessibility still applies (and is harder) |
| **TQ12** | Specify, design, build to these |

## 0. When this applies

Any UI whose primary user is an expert working with **models, measurements, simulations, or quantitative analysis**: parametric CAD and engineering tools; scientific visualization and post-processing (CFD/FEA contour plots, streamlines, volume rendering); computational notebooks (Jupyter/Observable/Mathematica); financial and spreadsheet modeling; probabilistic simulation and risk/forecasting; and interactive mathematical systems (graphing, symbolic). When a change produces such a surface, these directives are **triggered** (Testing-Strategy sense): a triggered-but-unmet directive is a gap the gate catches, owned by the UX & Accessibility lens (with the Test Architect for verification).

---

## 1. The principles (every technical UI)

**TQ1 — Density with hierarchy, never density as disorder.** Expert/data-dense UIs **SHOULD** present more information per screen than consumer UIs — reduced margins, compact controls, tabular layouts. But density demands a *stronger* visual hierarchy (size, weight, position, rule lines), not a weaker one. Make metadata **parsed and actionable** — searchable, filterable, sortable — not a wall of text. (Extends `ui-interaction-design.md` §0 "technical and dense domains".)

**TQ2 — Numerical legibility is a hard requirement.** Numbers **MUST** be **tabular/lining figures**, **right-aligned** in columns, with **consistent significant digits** and **explicit units**. No ambiguous precision (don't show `3.14159` next to `3.1`), no unit-less quantities, no proportional figures in a numeric column (they misalign the decimal). Thousands separators and sign/▲▼ deltas where they aid scanning. A number a user cannot trust the precision of is a defect.

**TQ3 — Perceptually-uniform, colorblind-safe colormaps; never rainbow/jet.** Any continuous scalar field (heatmap, contour, choropleth, density) **MUST** use a **perceptually-uniform** colormap — **viridis / cividis / plasma / inferno / magma** — so equal data steps read as equal color steps. The **rainbow / jet** colormap is **forbidden** for scientific encoding: it is perceptually non-uniform (invents false boundaries, hides real gradients), fails for color-vision-deficient users (red/green confusion), and collapses in grayscale/print. Choose the colormap *class* by data type (ColorBrewer): **sequential** for ordered magnitude, **diverging** for a meaningful midpoint (anomaly, ±), **qualitative** for categories. A **colormap legend with units is mandatory** on every encoded field. *(Verified — Moreland/Kovesi; matplotlib viridis default; ColorBrewer.)*

**TQ4 — Maximize data-ink (Tufte).** Every visual element **SHOULD** carry data. Remove chart junk — gratuitous gridlines, 3D bevels, drop shadows, redundant legends, decorative gradients. In a domain plot the data is the message; the chrome serves it. (Composes U7 essentialism.)

**TQ5 — Uncertainty is first-class; never a point estimate for a stochastic result.** Any value that is **estimated, simulated, forecast, or measured with error** **MUST** be shown with its uncertainty, not as a single number or line. Use the technique fit to the data: **distribution** (histogram / density / violin) for a result set; **fan chart / percentile bands** for a time series forecast; **tornado / sensitivity** to show which inputs drive the spread; **probability-of-outcome** (P(exceed threshold)) for decisions. For **lay or mixed audiences, prefer Hypothetical Outcome Plots (HOPs) or quantile dotplots** — research shows conventional **error bars and confidence bands are widely misread**, while HOPs/dotplots improve probabilistic reasoning. Always state the **model's assumptions** (distributions, # iterations, correlations). *(Verified — Kay & Hullman, CHI 2016; central-bank fan charts.)*

**TQ6 — Direct manipulation *and* precision input — both, never only one.** The expert **MUST** be able to manipulate the object directly (orbit a 3D model, drag a node, brush a region) **and** enter an exact value or expression (type `42.5 mm`, a formula, an equation). Direct manipulation gives intuition; precision input gives correctness. A CAD tool that only lets you drag, or a plot that only lets you type, fails its expert.

**TQ7 — Units & dimensional integrity.** Every physical quantity **MUST** carry its unit; conversions are explicit and lossless; the system **SHOULD** enforce dimensional consistency (you cannot add a length to a mass). Unit ambiguity is a class of error that has destroyed spacecraft — treat it as a correctness concern, not a formatting one.

**TQ8 — Reproducibility & provenance.** The artifact the expert builds — a parametric **feature history**, a notebook's **cell order**, a visualization **pipeline**, a model's **formula graph** — **MUST** be a recorded, inspectable, replayable record of *how a result was produced*. Show the lineage: which inputs, which steps, which version produced this output. A result with no traceable provenance is not trustworthy in a technical domain.

**TQ9 — Reactive recomputation with visible staleness.** When an input changes, dependents **MUST** recompute (spreadsheet dependency graph; reactive notebook DAG; parametric history regeneration) and the UI **MUST** show **stale / recomputing / error** state per affected element — never silently display an out-of-date result as current. (Composes U9 state completeness, specialized to compute graphs.)

**TQ10 — Performance for large data.** Expert datasets are large (millions of cells/points/elements). The UI **MUST** stay responsive: **virtualize** grids and lists, apply **level-of-detail** to 3D meshes and point clouds, **downsample/bin** plots beyond pixel resolution, and keep interaction (pan/zoom/scrub) at interactive frame rates. The performance budget (U17) is judged against the *expert's* data scale, not a demo's.

**TQ11 — Accessibility still applies (and is harder).** WCAG 2.2 AA is the floor here too (U16), and density makes it harder: ensure focus order through dense grids, keyboard operation of canvases (not mouse-only), screen-reader summaries of charts (a table alternative or sonification where apt), and **never encode by color alone** — pair the colormap with contours/labels/values. Colorblind-safe colormaps (TQ3) are an accessibility control, not just a science one.

---

## 2. Domain → archetype map (UI Archetype Catalog, Section G)

Each domain the expert works in has a determinism archetype in `ui-archetype-catalog.md` (Section G). Select it as the routing/temporal/data selector, then make it concrete against U1–U20 and these principles:

| Domain | Archetype (catalog §G) | The distinct surface |
|---|---|---|
| CAD / mechanical & parametric design | **G1 · Parametric Modeling Workbench** | 3D viewport + feature/history tree + parameters dock + ribbon |
| CFD / FEA / scientific post-processing | **G2 · Scientific Visualization Pipeline** | render view + filter pipeline + viridis colormap legend + time scrubber |
| Computational math / data / simulation authoring | **G3 · Computational Notebook** | prose + code cells + kernel + live rich output (reactive variant = Observable/Marimo) |
| Financial modeling / quantitative analysis | **G4 · Computational Spreadsheet / Grid** | addressable cell grid + formula dependency-graph recompute + scenario/sensitivity tables |
| Monte Carlo / risk / sports / forecasting | **G5 · Probabilistic / Uncertainty Explorer** | distribution inputs → narrated stochastic run → **uncertainty-first** output (fans, tornado, HOPs) |
| Quant terminal / trading / ops command | **G6 · Multi-Panel Data Terminal** | command-code-driven, linked, real-time tiled panels at ultra-density |

Interactive mathematical systems (Desmos, GeoGebra) are the **consumer-facing reactive cousin of G3** — an equation list + a live plot that recomputes on edit; build them as a reactive `NotebookCells`/`GraphingCalculator` variant with TQ6 direct-manipulation-plus-precision at the center.

## 3. The mandate (how the lenses carry this)

**TQ12 — Specify, design, build to these.** When `/specify`, `/design-slice`, or `/implement` produces a technical/scientific/quantitative surface: `/specify` records the domain, the expert's density/precision profile, and the **uncertainty and units requirements** as testable criteria; `/design-slice` selects the §G archetype, names the **colormap (and bans jet)**, the **uncertainty representation**, the **precision-input + direct-manipulation** pair, and the **reactive/provenance** model, with the complete state set incl. stale/recomputing/error (TQ9); `/implement` builds them and proves them red-first — a colormap-is-perceptually-uniform check, a no-PII-style **no-jet** check, a numbers-are-tabular-and-unit-bearing check, an uncertainty-is-shown check, and the large-data performance budget (TQ10). A triggered-but-unmet directive fails the gate (UX & Accessibility veto for legibility/inclusion; Test Architect for coverage).

---

## 4. Self-verification checklist (technical UI work)

- [ ] Density carries a **stronger** hierarchy; metadata is parsed/actionable (TQ1).
- [ ] Numbers are **tabular, right-aligned, consistent-precision, unit-bearing** (TQ2, TQ7).
- [ ] Continuous fields use a **perceptually-uniform, colorblind-safe** colormap with a legend; **no rainbow/jet**; class matches data type (TQ3).
- [ ] Data-ink maximized; chart junk removed (TQ4).
- [ ] **Uncertainty shown** for every stochastic/estimated/forecast value; technique fits the data; assumptions stated; HOPs/dotplots for lay audiences (TQ5).
- [ ] **Direct manipulation *and* precision input** both present (TQ6).
- [ ] **Provenance** visible (history/cells/pipeline/formula graph); results are reproducible (TQ8).
- [ ] **Reactive recompute** with visible stale/recomputing/error states (TQ9).
- [ ] **Large-data performance**: virtualization / LOD / downsampling at interactive rates (TQ10).
- [ ] **WCAG 2.2 AA** met under density; not color-alone; keyboard-operable canvas/grid (TQ11).
- [ ] The right **§G archetype** is selected and made concrete against U1–U20 (TQ12).

## 5. References

- **Colormaps:** K. Moreland, *"Why We Use Bad Color Maps and What You Can Do About It"*; P. Kovesi, *perceptually uniform colour maps*; matplotlib **viridis/cividis** (the perceptually-uniform default); **ColorBrewer 2.0** (sequential/diverging/qualitative). The rainbow/jet critique is the settled consensus in scientific visualization.
- **Edward Tufte**, *The Visual Display of Quantitative Information* — data-ink ratio, chartjunk.
- **Uncertainty visualization:** M. Kay, J. Hullman et al., *Hypothetical Outcome Plots* (CHI 2016) and *quantile dotplots* — the evidence that error bars/bands are misread and HOPs/dotplots aid probabilistic reasoning; central-bank **fan charts**; **tornado** diagrams for sensitivity.
- **Domain exemplars:** Fusion 360 / SolidWorks / Onshape / Blender (CAD); ParaView / Tecplot / COMSOL (scientific viz); Jupyter / Observable / Mathematica / Marimo (notebooks); Excel / Google Sheets / Causal (modeling); @RISK / Crystal Ball / FiveThirtyEight / Metaculus (probabilistic); Bloomberg / LSEG Eikon / TradingView (terminals); Desmos / GeoGebra (interactive math).
- The pack's **`ui-interaction-design.md`** (U1–U20) and **`ui-archetype-catalog.md`** (Section G) — the standard and the archetypes this base knowledge serves.
