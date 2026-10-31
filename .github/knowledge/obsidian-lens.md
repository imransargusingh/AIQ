---
load: skill
skills: [document, adopt]
---
# The Obsidian Lens — graph insight over the knowledge base

*Normative guidance for using **Obsidian** and its graph-analysis plugins as a **lens** over the pack's knowledge graph. `knowledge-visualization.md` (V1–V18) defines the graph and the Docs Explorer; `project-memory-and-obsidian.md` (M1–M9) establishes that project memory is committed Markdown and that Obsidian is optional. **This document is how the optional lens is actually stood up** — what is committed, what is not, which plugins earn a seat, how the typed graph is surfaced, and how the insight stays available to people who never install Obsidian.*

Normative keywords (**MUST**, **SHOULD**, **MAY**, **MUST NOT**) follow RFC 2119.

The governing idea: **`docs/` is already a valid Obsidian vault, so the lens costs nothing to adopt and must never become something the repo depends on.** The pack designed per-artifact YAML frontmatter (V2) precisely so Properties, Dataview and the graph view read it natively. Obsidian therefore adds *visualisation and discovery* on top of a graph that already exists and is already authoritative — and the moment the repo cannot be understood without Obsidian, the lens has become a dependency and the pack has lost its tool-neutrality.

---

## 0. When this applies

Any repository running the AI-Forward pack whose maintainers want richer navigation and structural insight over `docs/` than the Docs Explorer provides — hub/bridge analysis, semantic similarity, hierarchical navigation over typed relations, or live queries across frontmatter. It is **always optional**. A repo that never installs Obsidian loses no capability that the pack promises.

---

## 1. The hard boundary (this does not move)

**OB1 — Obsidian is a reader; frontmatter is the record.** Restating `project-memory-and-obsidian.md` M8 because it is the rule most likely to erode: the pack **MUST NOT** require Obsidian or any plugin to read, maintain, or validate canonical knowledge. `docs-graph.py` remains the only writer of the graph (V18); `docs/docs-index.js` remains derived; artifact frontmatter remains the truth. Obsidian never generates a canonical artifact.

**OB2 — No query is load-bearing inside a canonical document.** A Dataview (or any plugin) query **MUST NOT** appear in a spec, architecture, design, ADR, investigation, proof pack, or knowledge doc as a substitute for content. Queries live **only** in artifacts explicitly marked as lenses (OB6). A canonical document that renders as an empty block without a plugin installed is a defect.

**OB3 — The insight must survive the plugin.** Any structural analysis the team comes to rely on **MUST** also be obtainable dependency-free. The pack ships `scripts/obsidian-setup.py --analyze`, which computes degree, **betweenness centrality (exact, Brandes)**, connected components, orphans, leaves, ownership distribution and structural gaps directly from `docs-index.js` with stdlib Python only. Use the plugin for interactive exploration; use the script for anything that enters a report, a review, or a decision — because the script's output is reproducible, reviewable, and available in CI.

---

## 2. What is committed, and what is not

**OB4 — Commit the vault *configuration*; ignore the per-user *state*.** This refines `project-memory-and-obsidian.md` M9, which advised git-ignoring `.obsidian/` wholesale. That advice was right when the lens was a private convenience and wrong once a *team* wants a shared view: an ignored config means every contributor rebuilds the same colour groups by hand and sees a different graph. The split:

| Path | Disposition | Why |
|---|---|---|
| `docs/.obsidian/app.json`, `appearance.json` | **commit** | Shared, harmless defaults |
| `docs/.obsidian/core-plugins.json` | **commit** | Which built-ins the vault expects |
| `docs/.obsidian/community-plugins.json` | **commit** | The *enabled list* — a declaration of intent, not code |
| `docs/.obsidian/graph.json` | **commit** | The colour groups keyed to artifact `type:` — the single highest-value artifact here (OB7) |
| `docs/.obsidian/plugins/*/manifest.json` | **commit** | Records *which version* of each plugin the vault expects — provenance without shipping code |
| `docs/.obsidian/workspace.json`, `workspace-mobile.json` | **ignore** | Per-user pane layout; churns on every session |
| `docs/.obsidian/cache/` | **ignore** | Derived |
| `docs/.obsidian/plugins/*/main.js`, `styles.css` | **ignore** | **Third-party code** — see OB9 |
| `docs/.obsidian/plugins/*/data.json` | **ignore** | Per-user plugin state, and a place API keys land |
| `docs/.smart-env/` | **ignore** | Smart Connections' vector store. **Outside `.obsidian/`** — see below |

**Not every plugin writes inside `.obsidian/`.** Each row above but the last is scoped to
`.obsidian/`, because that is where Obsidian itself keeps state — and that made "per-user state" and
"under `.obsidian/`" feel like the same statement. They are not. Smart Connections writes its vector
store to `docs/.smart-env/`, a *sibling* of `.obsidian/`, so every ignore rule missed it and one
consuming repository committed **268 files and 8.17 MB** of embeddings without a single rule firing.
The embeddings carry a `last_embed` timestamp and are rewritten on every re-embed, so they also
conflict by construction between branches — derived files regenerate, they do not merge.

So the question when adding a plugin is **not** *"is its state under `.obsidian/`"* but *"does this
plugin write derived state anywhere in the vault at all"* — answered by running
`git status --untracked-files=all <vault>` after a real session, never inferred from the layout.

Verify the split rather than assume it — `git status --untracked-files=all docs/.obsidian` should list the config and the manifests and **nothing else**, and `git check-ignore` should return true for every `main.js`, `styles.css`, `data.json` and `workspace.json`.

**OB5 — The vault root is `docs/`, not the repo root.** Pointing Obsidian at the repository root drags `pack/`, `.claude/`, `.github/` and `node_modules/` into the graph and buries the actual knowledge base under generated copies. The vault is the directory the graph describes.

**OB6 — Lenses are artifacts, and they say so.** A dashboard built from queries lives in `docs/lenses/`, carries full V2 frontmatter (so it is a graph node rather than an un-indexed finding), and **MUST** open with a banner stating that it is **derived, never authoritative** — that the frontmatter is the truth, that nothing may depend on it, and that without Dataview it renders as query source, *which is the honest degradation rather than a failure*. Lenses carry an empty `review-by` because a derived view cannot go stale in the way a claim can.

---

## 3. Making the typed graph visible

**OB7 — Colour the graph by the pack's own types.** Obsidian's default graph is an undifferentiated hairball; the pack's `type:` vocabulary is exactly the differentiation that makes it readable. The committed `graph.json` **SHOULD** define a colour group per artifact type (`knowledge`, `design-slice`, `architecture`, `adr`, `spec`, `decision-note`, `proof-pack`, `threat-model`, `privacy-review`, `glossary`, `design-language`, `doc`) using Obsidian's frontmatter query syntax (`["type":"design"]`), with **status overlays last** so `superseded` and `draft` win the colour race and are visible at a glance.

**OB8 — Know what Obsidian's graph cannot show, and cover it.** Obsidian's native graph is **untyped**: a `supersedes` edge and a `relates-to` edge render identically, which erases the distinction the pack's relation registry (V14) exists to make. Two consequences:
- **Breadcrumbs** earns its seat by rendering the typed relations as navigable hierarchy — it is the plugin that makes `implements` / `refines` / `depends-on` mean something in the UI.
- Traceability questions (*"which designs have no proof?"*, *"what does this ADR still govern?"*) are answered by **Dataview queries or `--analyze`**, not by looking at the graph picture. A pretty graph is not an audit.

---

## 4. Plugins: the seat each one has to earn

**OB9 — Adopt plugins deliberately; they are dependencies.** Every plugin is third-party JavaScript executing with vault access, so the Solution-Selection Ladder (L3) and the Security lens's supply-chain rules apply exactly as they would to an npm package. The pack's recommended set, with the justification each must survive:

| Plugin | Tier | Why it earns a seat |
|---|---|---|
| **Dataview** | core | Turns V2 frontmatter into queryable data — the reason the lenses are live rather than hand-maintained. |
| **Knowledge Graph Analysis** | core | Local graph-theory metrics (degree, betweenness, closeness, eigenvector) plus opt-in AI structural analysis. Finds hubs, bridges and under-linked regions interactively. |
| **Breadcrumbs** | core | Renders the *typed* relations as hierarchy — covers OB8's gap. |
| **Juggl** | optional | Stylable graph filtered by frontmatter, for reading the graph by type and status. |
| **ExcaliBrain** | optional | Parent/child/sibling navigation, best for walking spec → architecture → design → proof chains. |
| **Smart Connections** | optional | Embedding similarity — finds artifacts that *should* be linked but are not, the gap a typed graph cannot see by construction. |

> **Naming note — and a correction worth carrying.** **Graphify** is a *separate product*, not an Obsidian plugin: an on-device **code** knowledge graph for AI coding assistants (graphify.com, Apache 2.0, PyPI `graphifyy`). It composes with this lens rather than competing with it — see **`code-knowledge-graph.md`** (GK1–GK16) for the standard and the three-graph model. The Obsidian plugin that provides *in-vault* graph metrics is **`knowledge-graph-analysis`** by *luolanaatud*; there is no plugin named "Graphify" in the community registry, and searching the registry for one is the wrong question. Note also that `graph-analysis` by *SkepticMystic*, widely cited in older write-ups, is **no longer in the registry**. Verify plugin identity against the registry before adopting; write-ups age faster than the ecosystem.

**OB10 — Plugin code is installed with consent, never silently.** `obsidian-setup.py --init` writes only the **enabled list**, so Obsidian's own plugin browser performs the install with the user present. Downloading plugin code directly is a separate, explicit `--fetch-plugins` opt-in. Never wire plugin fetching into an unattended setup path.

**OB11 — AI plugin features are a data-egress decision.** Knowledge Graph Analysis and Smart Connections can send note content to a third-party model with the user's own API key. That is **content leaving the repository's trust boundary**, so it is governed by `ai-commercial-models.md` AC6–AC7 and the **Privacy & Data Governance** lens: enable it only after confirming the repo's content may be sent, and never on a vault containing personal or client-confidential data without a basis. Local metrics require no key and no egress — prefer them.

---

## 5. Keeping the lens honest

**OB12 — Re-derive after adding a lens.** A new file under `docs/` is a graph artifact; run `docs-graph.py derive` so it enters the index, and `validate` so a missing link or absent frontmatter surfaces immediately (V10/V11 — the index lands in the same change as the content).

**OB13 — Read `--analyze` as a prompt, not a verdict.** The analyzer reports **structural gaps** (a design with no `tested-by`, an ADR nothing references) as *questions*. Some are genuine holes; some are correct and simply need the relation recorded or the exception noted. It **MUST NOT** be turned into a blocking gate — that is `docs-graph.py validate`/`freshness`'s job, and conflating discovery with enforcement produces a check people learn to ignore.

**OB14 — Watch the hub.** In practice one artifact — usually `architecture` — accumulates the highest degree *and* the highest betweenness, which means it is the single point through which the graph coheres. Two implications worth acting on: an error there propagates further than anywhere else, so it warrants the tightest `review-by`; and a betweenness score far above every other node is a signal that intermediate structure (a Map of Content, a domain index) is missing, not that the hub is healthy.

---

## 6. Standing it up

```bash
# 0. the graph must exist first — the lens reads it, never the reverse
python3 docs/ai-forward-pack/scripts/docs-graph.py derive

# 1. what is present, what is missing, and why each plugin is recommended
python3 docs/ai-forward-pack/scripts/obsidian-setup.py --check

# 2. install the app (winget on Windows, brew cask on macOS, flatpak on Linux)
python3 docs/ai-forward-pack/scripts/obsidian-setup.py --install-app --yes

# 3. write the committed vault config + the lens notes + the .gitignore split
python3 docs/ai-forward-pack/scripts/obsidian-setup.py --init          # add --all-plugins for the optional tier

# 4. structural insight, dependency-free (add --write to save it as a lens)
python3 docs/ai-forward-pack/scripts/obsidian-setup.py --analyze --write

# 5. index the new lens notes
python3 docs/ai-forward-pack/scripts/docs-graph.py derive
python3 docs/ai-forward-pack/scripts/docs-graph.py validate
```

Then in Obsidian: **Open folder as vault → select `docs/`** → *Settings → Community plugins → Browse* and install the enabled list. (`--fetch-plugins` downloads them directly instead; OB10.)

---

## 7. Self-verification checklist

- [ ] Obsidian is **optional** — the repo is fully readable, maintainable and validatable without it (OB1).
- [ ] **No query is load-bearing** in any canonical artifact; queries appear only in `docs/lenses/` (OB2, OB6).
- [ ] Any relied-upon insight is reproducible **dependency-free** via `--analyze` (OB3).
- [ ] Vault **config is committed**; workspace, caches, plugin code and plugin data are **ignored** (OB4).
- [ ] The vault root is **`docs/`**, not the repo root (OB5).
- [ ] Lens notes carry V2 frontmatter and the **derived-never-authoritative banner** (OB6).
- [ ] `graph.json` colours by **artifact type**, with status overlays last (OB7).
- [ ] Typed-relation navigation is covered (Breadcrumbs / Dataview / `--analyze`), not assumed from the untyped graph picture (OB8).
- [ ] Every enabled plugin has a **stated justification**; identity verified against the registry (OB9).
- [ ] Plugin code installed **with consent**; no unattended fetching (OB10).
- [ ] AI features reviewed as a **data-egress decision** by the Privacy lens before enabling (OB11).
- [ ] `derive` + `validate` run after adding lenses (OB12).
- [ ] `--analyze` output treated as a **prompt**, never wired in as a gate (OB13).
- [ ] The top hub/bridge has a tight `review-by`, and an outlier betweenness is read as **missing intermediate structure** (OB14).

---

## 8. References

- **`knowledge-visualization.md`** (V1–V18) — the graph this is a lens over: V2 frontmatter as the record, V10 discoverability, V11 same-change index, V14 the relation registry, V18 the script bundle as the only graph mechanic.
- **`project-memory-and-obsidian.md`** (M1–M9) — committed Markdown as project memory; **M8** (reader never writer) is preserved verbatim here as OB1, and **M9** is refined by OB4 from *"ignore `.obsidian/`"* to *"commit the config, ignore the state"* now that the lens is shared rather than private.
- **`ai-commercial-models.md`** AC6–AC7 + the **Privacy & Data Governance** lens — the basis for OB11's egress rule.
- **`solution-selection-ladder.md`** L3 — a plugin is a dependency; the Gratuitous-Dependency gate applies (OB9).
- **`scripts/obsidian-setup.py`** — `--check` · `--init` · `--analyze` · `--fetch-plugins` · `--install-app`; stdlib-only, idempotent, `--dry-run` on every mutating mode.
- **Obsidian community registry** — `obsidianmd/obsidian-releases/community-plugins.json`, the authoritative source for plugin identity (OB9).
