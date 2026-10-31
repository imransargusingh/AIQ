---
load: skill
skills: [document, visualize, adopt]
---
# Knowledge Visualization & Docs Explorer Standard

*Normative guidance for how knowledge and engineering artifacts are constructed, connected, indexed, and visualized in this repository — and the standard local HTML/JavaScript toolkit (the **Docs Explorer**) that renders them. It governs any agent that creates knowledge or content (markdown, specs, architecture, designs, ADRs, investigations, proof packs, API docs, source modules). The Testing Strategy governs proof; the Observability Standard governs telemetry; this document governs **discoverability** — work that cannot be found, navigated, and understood by a human is unfinished.*

Normative keywords (**MUST**, **SHOULD**, **MAY**, **MUST NOT**) follow RFC 2119.

The governing idea, distilled from the research below: **all knowledge in the repo is one typed graph, and every visualization is a projection of it.** Atomic artifacts are the nodes; explicit, typed links are the edges; the Browse tree, graph view, and mind-map view are three projections of the same underlying model — not three separate documents to maintain. Maintain the graph (the frontmatter on each artifact; the index derives from it); the views come free.

---

## 0. Research synthesis (why these rules)

**Knowledge construction (Zettelkasten / Obsidian / PKM practice).** The durable findings: keep notes **atomic** (one concept per artifact) and written in your own words; create value through **explicit links**, not folder position — "our brains don't think in folders"; links are bidirectional in spirit (a reference is also a backlink); use **Maps of Content (MoCs)** as curated navigation layers over the link network rather than deep folder trees; tag sparingly; store everything as **plain markdown files** so the knowledge base compounds for years without platform lock-in; link from day one — the network builds itself from habit, not planning. AI agents are effective maintainers of such systems: suggesting links, finding orphans, keeping the graph coherent.

**Visualization paradigms.** Three established forms, with distinct cognitive jobs:
- **Mind map** — a *radial tree*: one central topic, single-parent nodes, unlabeled edges. Best for orientation, overview, and brainstorm around a focus.
- **Concept map** (Novak) — a *multi-parent network with labeled edges* read top-down; nodes can have several parents and every edge carries a relationship phrase ("implements", "depends on"). Best for explaining systems and complex inter-concept relations. UML is the industry-formalized member of this family.
- **Knowledge graph** — a *free typed network* with no privileged root and unlimited typed connections; the semantic-network/triple tradition. Best for the whole corpus and for discovering unplanned connections.

**The transformation insight.** These are not rival formats; they are **projections of one graph**: a mind map is the **BFS/DFS spanning tree of the graph rooted at a chosen focus node** (with the non-tree "cross-links" rendered as secondary/ghost edges, so the projection is honest about what it hides); a hierarchy view is a spanning forest grouped by a chosen facet (type, directory, phase); the concept-map/graph view is the full network with labeled, typed edges. Transformation between paradigms = **choose a root/facet, extract the spanning structure, and overlay the remainder** — so a single index drives every view, and switching views never requires re-authoring content.

**Toolkit findings.** SVG beats canvas for accessibility, CSS styling, and print at documentation scale (canvas wins only for very large graphs); JSON is the de-facto serialization for graph UIs; the most important lesson from production graph UIs: **generic layouts don't reflect meaning** — a layout should encode semantics (cluster by artifact type, lane by phase or layer), not just minimize edge crossings. Rendering can stay native and dependency-free when deterministic state/layout math is isolated in a pure module.

**Diagrams-as-code (UML / C4 / Mermaid).** Text-based diagrams committed to Git are the modern norm: diffable, reviewable, and updated **in the same change** as the code — the proven cure for diagram drift ("living documentation"). The **C4 model** supplies the zoom discipline — Context → Container → Component (Code optional and usually skipped) — and the rule *pick the level for the audience* (Context+Container suffice for most communication). **Mermaid** is the embeddability sweet spot for the UML families this pack uses — sequence (interactions), state, class/object (structure), flowchart-as-activity (flows), C4 (architecture) — while heavier modeling belongs in dedicated tools. UML's value here is its *vocabulary and discipline* (lifelines, swimlanes, multiplicity, level-appropriate abstraction), carried in Mermaid syntax.

---

## 1. The model (the single graph)

**V1 — Atomic, linked artifacts.** Every knowledge/content artifact is **atomic** (one spec, one component design, one ADR, one investigation, one knowledge topic per file), written to be understandable on its own, and stored as plain markdown (or source) in the repo. Value is created by **explicit links** between artifacts — `spec → architecture → design → code/tests → proof-pack` traceability links, `supersedes`/`refines` lineage links, `depends-on` structure links — not by folder placement alone.

**V2 — Frontmatter is the record; the index is the derived projection.** Every artifact carries its own graph metadata in **YAML frontmatter** — the record lives *with the content*, the way a database row lives with its table (the Obsidian Properties/Bases lesson: if data needs to be queried, it must exist as a formal property at the top of the file). The per-artifact frontmatter schema:

```yaml
---
id: design-payment-gateway        # stable, unique, kebab-case
title: "Payment Gateway — Design"
type: design        # knowledge|glossary|spec|architecture|adr|design|investigation|proof-pack|decision-note|threat-model|privacy-review|api|source|doc
status: accepted    # draft|in-review|accepted|superseded|resolved…
owner: "@handle"    # the human accountable for this artifact's truth (V13)
phase: "2"          # delivery phase (vertical slice), if applicable
tags: [payments, resilience]
links:
  - { to: spec-checkout, rel: implements }
  - { to: adr-0007-queue, rel: depends-on }
review-by: 2026-09-12   # freshness SLA per type (V13); empty only for frozen records
review-suggested: []    # change-impact flags pushed by neighbors (V16): [{ by: <id>, on: <date>, reason: "<what changed>" }]
summary: >-
  1–3 sentences; real summary, not the title repeated.
---
```

Because the record is per-file, `docs/` is simultaneously a **valid Obsidian vault** (Properties, Bases tables, and Obsidian's graph view all read this frontmatter natively) and the substrate for the Explorer. The repo still maintains **one machine-readable index**, `docs/docs-index.js` assigning `window.DOCS_INDEX` (a `<script src>`-loadable JS file so the Explorer works over `file://` where `fetch()` of local JSON is blocked) — but the index is **derived from the frontmatter**, never hand-maintained as parallel truth: each skill syncs its own artifact's entry from that artifact's frontmatter (V10), and `/document` performs the full derivation sweep. Where frontmatter and index disagree, the **frontmatter wins** and the index is regenerated. Index schema (the frontmatter fields, plus path and embedded diagram sources):

```js
window.DOCS_INDEX = {
  project: "<name>",
  generated: "<ISO timestamp>",
  generator: "<skill or tool that last regenerated it>",
  artifacts: [{
    id: "design-payment-gateway",          // stable, unique, kebab-case
    path: "docs/design/payment-gateway.md", // repo-relative
    title: "Payment Gateway — Design",
    type: "design",   // knowledge|glossary|spec|architecture|adr|design|investigation|proof-pack|decision-note|threat-model|privacy-review|api|source|doc|index
    phase: "2",       // delivery phase (vertical slice) it belongs to, if applicable
    status: "accepted",                     // draft|in-review|accepted|superseded|resolved…
    owner: "@handle",                       // from frontmatter (V13)
    reviewBy: "2026-09-12",                 // from frontmatter review-by (V13)
    reviewSuggested: [],                    // from frontmatter (V16)
    summary: "1–3 sentences; shown in every view and on file:// where content can't be fetched",
    tags: ["payments", "resilience"],
    links: [{ to: "spec-checkout", rel: "implements" },
            { to: "adr-0007-queue", rel: "depends-on" }],
    diagrams: [{ title: "Charge flow", kind: "sequence", mermaid: "sequenceDiagram\n  ..." }]
  }]
};
```
Links are **typed** (`rel` is mandatory; the governed vocabulary and each relation's semantics are the **relation registry**, V14: `implements`, `refines`, `depends-on`, `supersedes`, `tested-by`, `documents`, `uses-term`, `relates-to`) — the concept-map lesson that labeled edges carry the meaning. Diagram sources MAY be embedded in the index (for view-anywhere rendering) and SHOULD also live in the artifact's markdown as fenced ```mermaid blocks (diagrams-as-code, same-PR updates).

**V3 — MoCs over deep trees.** Navigation layers (the index page, per-domain knowledge-base indexes, the architecture's component map) are **Maps of Content**: curated link lists over the graph. Do not encode navigation solely in folder nesting.

---

## 2. The views (three projections + documents)

**V4 — Browse projection.** An accessible tree grouped by a chosen facet — artifact `type` by default (specs / architecture / designs / ADRs / investigations / knowledge / proofs / docs), with delivery `phase` as the alternate facet. This is the canonical orientation view and the default landing.

**V5 — Graph projection.** The full typed network: nodes grouped by `type`, directed edges identified by `rel`, and a deterministic **semantic-first** layout (stable type/phase lanes and stable ordering — never a randomized hairball). Selecting a node does not silently change the graph's context; an explicit **Explore neighborhood** action changes context, and the traceability chain (`spec → … → proof`) remains followable edge by edge and in an equivalent relationship list.

**V6 — Mind-map projection.** A radial tree computed from the explicit **context node**: BFS spanning tree outward from that context, cross-links drawn as dashed ghost edges. Selection and context are separate: selecting inspects a node; **Explore neighborhood** re-roots the mind map. The transformation is a deterministic function of the same index, never separate content.

**V7 — Document & diagram view.** Selecting an artifact shows its metadata, summary, typed links (outgoing and back-links, computed by inverting `links`), diagram titles/source, and an *open-file* link. When the Explorer is served over HTTP it MAY fetch the markdown body on demand; source or optional diagram-renderer failure MUST preserve metadata, relationships, escaped source, and local navigation rather than produce an error wall. Mermaid rendering is a progressive enhancement over the committed source, not a prerequisite for core use.

**V8 — UML discipline in diagrams.** Use the diagram family fit for the question: **sequence** for interactions over time, **flowchart/activity** for branching flows, **class/object** for structure, **state** for lifecycles, **C4** for architecture zoom (Context → Container → Component; pick the level for the audience; skip Code unless it earns its place). Each artifact type carries its expected families: architecture → C4 Context+Container (+ component map); design → component/class + sequence for the load-bearing interactions; investigation → timeline/sequence of the failure; API docs → sequence per flow + object/class per resource.

---

## 3. The toolkit (the Docs Explorer)

**V9 — One standard Explorer at the docs root.** The repo serves all of this through a **single local HTML shell at `docs/index.html`** (from `templates/docs-explorer.template.html`) plus the local renderer-independent core `scripts/docs-explorer-core.js`: native DOM controls, deterministic SVG layouts (stable lanes; radial BFS), dark/light aware, and zero build step. It loads only local `docs-index.js` and the local core — so regenerating the index updates every view without touching the HTML, and core tasks remain usable with the network blocked. The `/document` skill's deep bundle viewer (`docs/_site/index.html`) remains the close-up reading surface; the Explorer is the map that links to it. Optional diagram/3D enhancements MUST degrade to Browse, relationships, metadata, and source.

---

## 4. The mandate (keeping it alive)

**V10 — The Discoverability Mandate (every content-creating skill).** Any skill or agent action that **creates or materially changes** a knowledge/content artifact (a spec, architecture, design, ADR, investigation, proof pack, knowledge doc, API doc, or a source module worth indexing) MUST, as its **last action**:
1. Ensure the artifact itself is in its right home under `docs/` (or linked from it), with its Mermaid diagrams embedded;
2. **Write the artifact's frontmatter** (the record, V2): id, title, type, status, owner, phase, tags, **typed links**, review-by (per the type's SLA, V13), and a real 1–3-sentence summary — then **sync its derived `docs/docs-index.js` entry** from that frontmatter (plus path and diagram sources); remove entries whose files are gone; bump `generated`/`generator`;
3. Ensure `docs/index.html` (the Explorer) exists — if missing, instantiate it from `templates/docs-explorer.template.html`;
4. Verify the new entry is *connected*: at least one typed link into the existing graph (an orphan node is a finding, not a result).
Skipping the index update is the documentation equivalent of skipping a triggered test directive: the work is not done.

**V11 — Same-change updates.** Index and diagram updates land **in the same change** as the artifact they describe (the docs-as-code anti-drift rule). A PR that changes a design but not its index entry/diagrams is incomplete.

**V12 — Honest projections.** Views never invent structure: a mind map shows its cross-links as ghosts rather than hiding them; Browse discloses its grouping facet; selection is not presented as neighborhood context; summaries in the index are real summaries, not titles repeated. The Explorer is a lens on the graph, and the graph must be trustworthy.


**V13 — Ownership and freshness SLAs.** Every artifact has an **`owner`** (the human accountable for its truth — pair with a `docs/**` section in `CODEOWNERS` so doc changes route to them) and a **`review-by`** date set per its type's freshness SLA. Defaults (a repo may tighten them): **knowledge** 90 days; **spec / architecture / design** 180 days *or* on any change to a linked downstream artifact, whichever first; **ADR** none while accepted (re-dated only when superseded); **investigation** frozen once resolved (immutable record — no SLA); **proof-pack / api** regenerate per release; **doc / glossary** 90 days; **decision-note** 180 days (an assumption is re-validated against its stated condition, then confirmed, retired, or promoted); **threat-model / privacy-review** 180 days or on any change to a linked design — their `documents` edges make V16 flag them automatically, so a pending flag means the model lags reality. Passing `review-by` does not delete or hide anything — it makes the artifact **stale**, a visible health finding in the Explorer and a freshness-gate condition for `/document`, exactly like stale code-docs. Reviewing = re-reading against reality, fixing or confirming, and bumping the date. The decay literature is blunt about why this is mandatory in an agentic repo: humans route around stale docs; **agents confidently act on them** — stale context produces confidently wrong output.

**V14 — The ontology layer: glossary + relation registry.** Shared meaning is defined, not assumed (the GraphRAG lesson: retrieval and reasoning must align with governed definitions, not surface text).
- **Glossary.** `docs/knowledge/glossary.md` (from `templates/glossary.template.md`, type `glossary`) is a first-class artifact: one entry per domain term — the definition in the repo's voice, what it is *not* (the near-miss disambiguation), and typed links to the artifacts that anchor it. Specs, designs, and investigations SHOULD link the load-bearing terms they use (`uses-term` → the glossary), so terminology resolves identically across artifacts and across agent sessions.
- **Relation registry.** The `rel` vocabulary is governed; an edge type not in this table is a recorded deviation, not an improvisation:

| rel | meaning (read source → target) | typical source → target | inverse reading |
|---|---|---|---|
| `implements` | source realizes what target specifies | design→spec, source→design, architecture→spec | "is implemented by" |
| `refines` | source elaborates/derives from target | design→architecture, doc→doc | "is refined by" |
| `depends-on` | source needs target to hold/exist | any→adr, design→design | "is depended on by" |
| `supersedes` | source replaces target (target → status `superseded`) | adr→adr, spec→spec | "is superseded by" |
| `tested-by` | source's claims are proven by target | design→proof-pack, spec→proof-pack | "proves" |
| `documents` | source describes target | api/doc→source, doc→design | "is documented by" |
| `uses-term` | source relies on target's definition | spec/design-slice/investigation→glossary | "defines a term used by" |
| `relates-to` | weak association (use sparingly — prefer a stronger rel) | any→any | symmetric |

**V15 — Graph-aware grounding.** Grounding (Rigor Protocol Stage 0) traverses the graph, it does not re-discover files: starting from the artifact(s) under work, load the **subgraph 1–2 hops out along typed edges** — upstream via `implements`/`refines`/`depends-on` (the spec, the architecture, the ADRs it answers to), downstream via `tested-by`/`documents` (the proofs and docs that constrain change), and `uses-term` (the definitions in play). The traversal path **is the provenance** of the grounding — cite it. A missing expected edge, a stale (`review-by`-past) node, or an orphan discovered during grounding is a **finding to surface**, not something to silently route around.

**V16 — Change-impact propagation.** Maintenance is a dependency problem: when something changes, the system must know what depends on it — and the graph is what answers that. When an artifact **materially changes** (a contract, a requirement, a decision, a public shape, a proven claim — not typos or formatting), the changing skill MUST traverse the **inbound** edges (the artifacts whose `implements` / `refines` / `depends-on` / `tested-by` / `uses-term` links point *at* the changed artifact) and push a **`review-suggested`** flag into each inbound neighbor's frontmatter — `{ by: <changed-id>, on: <date>, reason: "<what changed>" }` — syncing the derived index in the same change (V11). The flag is a *suggestion with provenance*, not a status change: the neighbor's owner reviews it against the change, updates or confirms the neighbor, then **clears the flag and bumps `review-by`**. The Explorer's health view surfaces flagged nodes alongside stale and orphan ones. A superseding change (`supersedes`) always propagates; a glossary-definition change always propagates along `uses-term`. This is the edge direction that matters for maintenance — the same links read downstream for grounding (V15) are read upstream for impact.

**V16a — Gate semantics: a defect fails, a suggestion warns.** Because a flag is a *suggestion* cleared by the **neighbour's owner** on a human timescale, `docs-graph.py validate` **MUST NOT** fail a build on one by default. It separates two kinds of finding: **defects** — invalid frontmatter, dangling link, unregistered `rel`, duplicate id, orphan (V10), index drift (V11) — which the author can fix before pushing and which therefore **always fail**; and **suggestions** — `review-suggested` (V16) and `review-by` decay (V13) — which **warn** on stderr and stay in the JSON, and fail only under an explicit `--gate fail`. The reason is an incentive, not a preference: if propagating a material change turns `main` red for every downstream owner, the rational move becomes **skipping the propagation V16 mandates** — and a skipped propagation is unobservable afterwards. *A control that punishes compliance is worse than no control, because it manufactures invisible non-compliance.* `freshness` has carried the same `--gate warn|fail` choice from the start; `validate` now agrees with it.

**V17 — Session-knowledge capture (decision notes).** ADRs catch the load-bearing decisions; most "why did we…?" knowledge is smaller and evaporates when the session ends. Any **mid-session decision, discovered assumption, or resolved question** that shaped the work but is below ADR weight is captured as a **decision note** (`docs/notes/<id>.md`, from `templates/decision-note.template.md`, type `decision-note`) *before the session closes*: what was decided or assumed, why (one tight paragraph), the alternatives dismissed (one line each), the confidence label (Verified / Inferred / Flagged), and — for assumptions — the **validation condition** under which it holds and must be re-checked. Notes are first-class graph nodes: linked (`relates-to` / `depends-on`) to the artifacts they touched, indexed, and visible in the Explorer. When a note accretes ADR weight, **promote** it: write the ADR, link `supersedes` to the note, set the note `superseded`. Capturing is part of the Discoverability Mandate's last action (V10) — a session whose reasoning left no trace in the graph has leaked knowledge.

**V18 — The docs script bundle (mechanics are tooled, not improvised).** The deterministic mechanics of this standard ship as a single stdlib-only tool — **`docs/ai-forward-pack/scripts/docs-graph.py`** (Python 3.8+, no dependencies; it parses the V2 frontmatter subset natively). Skills MUST invoke it for these operations instead of generating ad-hoc scripts at prompt time:

| operation | command | directive |
|---|---|---|
| scan the graph: invalid frontmatter, dangling links, unregistered rels, orphans, stale, flagged, index drift | `docs-graph.py inventory` (JSON) / `validate` (CI exit code) | V2/V10/V13/V16 |
| full derivation sweep: frontmatter → `docs/docs-index.js` | `docs-graph.py derive` | V2/V10 |
| time-based freshness gate (stale + flagged + orphans; `--gate fail` for CI) | `docs-graph.py freshness` | V13 |
| propagate a material change to inbound neighbors | `docs-graph.py flag --changed <id> --reason "…"` | V16 |
| clear a reviewed flag (and bump `review-by`) | `docs-graph.py clear-flag --id <a> --by <b> --bump-review <days>` | V16 |
| scaffold a schema-correct artifact | `docs-graph.py stub --file … --id … --type … --title … --link to:rel` | V2/V17 |
| append a graph-health record (orphans, stale, flagged, notes — the governance trend) to `docs/health-history.jsonl` | `docs-graph.py snapshot` | V13/V16 |
| aggregate the designs' STRIDE / LINDDUN tables into the threat-model / privacy-review registers (source-attributed, paste-ready) | `docs-graph.py rollup --heading "<section>" --type design` | V8/V16 |

The tool **never invents metadata** (files without frontmatter are findings, not silently indexed) and writes flags by targeted textual edit, preserving the rest of the file. Prompt-time scripting is reserved for judgment work the bundle cannot express — and when a mechanical need recurs, the answer is to **extend the bundle** (a recorded change with tests), not to regenerate the logic per session.

Append-only JSONL operations use a **persistent sibling `.lock` file** so concurrent
writers keep coordinating on one stable lock inode even when the data file is atomically
replaced. Repositories using the tool MUST ignore `*.jsonl.lock`; the lock is local
coordination state, not project history.

**V19 — Human-facing Markdown has an HTML companion.** Whenever a skill creates or materially
updates a human-facing Markdown artifact, it MUST create or refresh a sibling `.html` view in the
same change. Use the stdlib-only renderer:
`python3 docs/ai-forward-pack/scripts/render-markdown.py <artifact.md>` (on Windows use `python` or
`py -3`). The Markdown remains canonical and carries frontmatter; the HTML is a self-contained
consumption view and is not a second source of truth. Purpose-built HTML mockups remain hand-authored.
---

## 5. Self-verification checklist

- [ ] New/changed artifact is atomic, in its right home, with diagrams embedded as Mermaid (V1, V8).
- [ ] `docs/docs-index.js` regenerated: entry present with real summary, typed links, diagrams; stale entries removed; `generated` bumped (V2, V10).
- [ ] At least one typed link connects the artifact into the graph — no orphans (V10.4).
- [ ] `docs/index.html` exists and loads the index (V9, V10.3).
- [ ] Diagrams use the right UML family at the right C4 level for the audience (V8).
- [ ] Frontmatter carries `owner` and a type-correct `review-by`; reviewed artifacts re-dated (V13).
- [ ] Load-bearing terms link to the glossary (`uses-term`); every edge uses a registry `rel` (V14).
- [ ] Grounding traversed the subgraph and cited the path; stale/orphan/missing-edge findings surfaced (V15).
- [ ] Material changes propagated: inbound neighbors flagged `review-suggested` with provenance, in the same change (V16).
- [ ] Session exhaust captured: sub-ADR decisions/assumptions written as linked decision notes before close (V17).
- [ ] Graph mechanics ran through the script bundle (`derive`/`flag`/`freshness`), not ad-hoc scripts (V18).
- [ ] Every human-facing Markdown artifact has a refreshed sibling HTML view (V19).
- [ ] Index/diagrams updated in the same change as the content (V11).

---

## 6. References

- **Zettelkasten method** (Luhmann; atomic notes, linking, MoCs) — e.g. dsebastien.net/2022-05-01-zettelkasten-method; Obsidian community practice (obsidian.md, forum.obsidian.md).
- **NN/g — Cognitive maps, mind maps, concept maps** (the paradigm taxonomy): nngroup.com/articles/cognitive-mind-concept.
- **Novak — concept mapping** (labeled propositions; multi-parent networks).
- **C4 model** (Simon Brown): c4model.com — Context/Container/Component zoom; diagrams-as-code friendliness.
- **Mermaid**: mermaid.js.org — sequence, class, state, flowchart, C4 syntax.
- **Docs-as-code diagram practice** (text diagrams in Git, same-PR updates, drift prevention).
- **Graph UI engineering**: SVG-vs-canvas tradeoffs; semantic layouts over generic ones (production lessons, e.g. Splunk engineering); d3-force/Cytoscape/React-Flow landscape.
