---
load: skill
skills: [adopt, investigate, migrate]
---
# The Code Knowledge Graph — composing Graphify with the pack

*Normative guidance for composing **Graphify** — an on-device code knowledge graph for AI coding assistants — with the pack's own documentation graph and the Obsidian lens. `knowledge-visualization.md` (V1–V18) governs the **docs** graph; `obsidian-lens.md` (OB1–OB14) governs the **human** lens over it; **this document governs the code graph and, more importantly, the join between them.** Its central claim: the pack already demands that no agent assert the shape of our own code from memory (`end-to-end-integrity.md` E15), and a cited graph path is the first mechanism that makes that demand *cheap* to satisfy.*

Normative keywords (**MUST**, **SHOULD**, **MAY**, **MUST NOT**) follow RFC 2119.

The governing idea: **a repository has two knowledge graphs, and the expensive defects live in the gap between them.** The pack's graph holds *intent* — specs, architecture, designs, ADRs, typed links, owners, freshness. Graphify's graph holds *reality* — functions, calls, imports, schemas, with `file:line` citations. Documentation drifts from code precisely because nothing traverses both. Composing them turns "does the code actually do what the design says?" from a reading exercise into a traversal.

---

## 0. What Graphify is (established, not assumed)

- An **open-source (Apache 2.0)** knowledge-graph *skill* for AI coding assistants. Source: `github.com/Graphify-Labs/graphify`. Site: `graphify.com`.
- Parses **36 code languages plus Markdown, PDFs, Office documents, SQL schemas, live PostgreSQL and Terraform** into one graph agents query instead of grepping.
- **Runs entirely on-device**: no account, no API keys, no telemetry, nothing leaves the machine — which is why it clears the pack's dependency and privacy posture without a special case.
- Every edge carries a **provenance tag**: `EXTRACTED` (parsed straight from the tree-sitter AST), `INFERRED` (a model connected the dots), or `AMBIGUOUS` (evidence that could not be fully resolved).
- Answers come back as **explicit graph paths with real `file:line` citations**.
- Exposes an **MCP server** (`graphify-mcp`, or `python -m graphify.serve graphify-out/graph.json`) with graph tools, plus a CLI: `query`, `path`, `explain`, `affected`, `god-nodes`, `update`, `watch`, `reflect`, `merge-graphs`.

> **Package disambiguation (the vendor flags this themselves).** The PyPI package is **`graphifyy`** — *double y*. Other `graphify*` packages are unrelated. Install with `uv tool install graphifyy`. Installing the wrong one is a supply-chain event, not a typo.

**GK1 — Establish the product before you compose with it.** "Graphify" is an overloaded name. It is **not** an Obsidian plugin, and searching the Obsidian registry for it returns nothing — a *correct* answer to the *wrong question*, which is the most convincing way to be wrong. This document exists partly because that mistake was made here (`docs/lessons/defect-classes.md`, class **PACK-E**). Establish the artefact from its own canonical source before building on it.

---

## 1. Three graphs, one knowledge base

**GK2 — Know which graph is authoritative for what.** They are not redundant and they are not interchangeable:

| Graph | Holds | Built by | Authority |
|---|---|---|---|
| **Docs graph** (V2 frontmatter → `docs-index.js`) | intent, decisions, ownership, freshness, typed links between artifacts | `docs-graph.py` | **The record.** Hand-authored; frontmatter wins over everything derived. |
| **Code graph** (`graphify-out/graph.json`) | symbols, calls, imports, schemas, `file:line`, communities, provenance | `graphify update` | **Derived from source.** Regenerable; never edited; never a place to record a decision. |
| **Obsidian** | the human view of the docs graph | the vault config | **A lens.** Reads; never writes. |

**GK3 — The code graph is generated; treat it as a build output.** `graphify-out/` **MUST** be git-ignored. It is reproducible from source at any commit, it is large (megabytes of JSON and HTML), and committing it creates a second thing that can be stale. The *inputs* — `.graphifyignore`, the setup script, this standard — are committed; the output is not.

**GK4 — De-duplicate, and detect which copy is canonical. Do not blanket-ignore.** The methodology layer is deployed to two tool surfaces, so it *is* genuinely duplicated — but **which copy is authoritative inverts between repo kinds**, and getting it backwards guts the graph:

| Repo kind | Detect by | Canonical | Ignore |
|---|---|---|---|
| **Pack source** (this repo builds the pack) | `pack/adapters/INSTALL.md` exists | `pack/` | `.claude/`, `.github/{instructions,prompts,agents}`, `docs/ai-forward-pack/` — all byte-copies of `pack/` |
| **Pack consumer** (a project that adopted the pack as a starter) | no `pack/`, but `docs/ai-forward-pack/` or `.claude/knowledge/` exists | `.claude/` **and** `docs/ai-forward-pack/` | only the `.github/{instructions,prompts,agents}` mirror |
| **Plain repo** | neither | n/a | nothing methodology-related |

**Why this matters more than it looks.** In a consuming repo there is **no `pack/`** — `.claude/` and `docs/ai-forward-pack/` are the *only* copies of the standards, skills, personas, templates and deployable scripts. Applying the pack-source rules there removes them from the graph entirely. Measured on a real consuming repo: a blanket ignore drops **153 files**, of which **97 exist nowhere else** — 28 knowledge docs, 17 skills, 23 personas, 19 templates and 10 scripts. The de-dup rule drops **56**: the `.github/` mirror, and nothing else. `.claude/` is preferred as canonical because it is the fuller surface (23 personas versus 11, and raw knowledge rather than `applyTo`-wrapped copies).

The consequence of getting it wrong is not a smaller graph — it is a graph that **cannot answer the questions the pack exists to make answerable**: *"what governs how we do migrations?"*, *"which standard does this design implement?"*, *"what does `docs-graph.py` actually do?"* all return nothing, because the standards, the skills and the scripts were never parsed.

Same intent — graph each thing exactly once — **opposite rules**. `graphify-setup.py --init` detects the repo kind and emits the right file; `--check` warns when an existing `.graphifyignore` excludes `.claude/` or `docs/ai-forward-pack/` in a repo where those are the only copy.

**GK4.1 — What the de-duplication buys, and why it is still worth doing.** Left entirely un-deduplicated, every synced knowledge doc is parsed two or three times, which inflates the node count and — the part that actually misleads — **puts the same artifact in the god-node ranking several times, so a copy looks like a hub.** *(Observed on this pack's own first build: one vendored doc occupied ranks 3, 4 and 5; after de-duplication it appears once, at rank 3.)* On this repo the correct rules took the graph from 3,898 nodes to 2,033 — a 48% reduction that removed only copies and derived data.

**GK5 — Changing the ignore rules requires a full re-extraction.** `graphify update` is incremental and **fail-closed**: when files leave the scan corpus but still exist on disk it *keeps* their nodes and says so, rather than silently deleting them. That is the right default and it means a narrowed `.graphifyignore` does **not** take effect until `graphify-out/` is removed and the graph rebuilt. Verify the node count actually fell; a rebuild that changed nothing did nothing.

---

## 2. Provenance — where the two disciplines meet

**GK6 — Map Graphify's edge tags onto the pack's confidence labels, and honour the mapping.** This is the highest-value part of the composition, because the two systems independently arrived at the same epistemics:

| Graphify edge tag | Pack confidence label | What an agent may do with it |
|---|---|---|
| `EXTRACTED` | **Verified** | Cite it. It came from the AST — it is what the code says. |
| `INFERRED` | **Inferred** | State it as inference and **check it** before depending on it. A model connected these dots. |
| `AMBIGUOUS` | **Flagged** | Surface it as an open question. Never build a design on it. |

An agent that reports an `INFERRED` edge as established has committed the **Confident Guess** (BoK Part VIII) with extra steps — the citation makes it *more* persuasive, not more true. Read the tag before you quote the path.

**GK7 — E15 becomes cheap, not optional.** `end-to-end-integrity.md` E15 forbids asserting that a type has a member, a helper does what its name suggests, or a caller exists, from memory — *"read it, or label the claim Inferred"*. Graphify supplies the third option E15 always implied: **cite a traversal with `file:line`**. So the rule sharpens rather than relaxes: a claim about our own code is **Verified** when it rests on an `EXTRACTED` path or a file you opened, **Inferred** when it rests on an `INFERRED` edge or recall, and never Verified merely because a tool printed it.

---

## 3. Where it plugs into the workflow

**GK8 — Grounding gains a code-side traversal.** Rigor Protocol Stage 0 already traverses the *docs* subgraph 1–2 hops (V15). Where a code graph exists, grounding **SHOULD** also traverse the *code* side for the symbols in scope — `graphify explain <X>` for what a thing is and touches, `graphify query "..."` for how two areas connect — and cite the path. Docs traversal tells you what was *intended*; code traversal tells you what is *there*. Disagreement between them is a finding, and it is exactly the finding nobody currently catches.

**GK9 — `affected` is the code-side blast radius; V16 is the docs-side.** `knowledge-visualization.md` V16 propagates `review-suggested` to inbound neighbours when an artifact materially changes. `graphify affected "<symbol>"` does the same reverse traversal over code. Before changing a load-bearing symbol, run it — and treat the result as the **surface list** `end-to-end-integrity.md` E7 requires you to write down *before* you start, rather than discovering the missed projection in production.

**GK10 — `god-nodes` is the code-side hub watch.** `obsidian-lens.md` OB14 says to watch the dominant hub in the docs graph, because an error there propagates furthest and an outlier suggests missing intermediate structure. The same reading applies to `graphify god-nodes`: a symbol with a disproportionate edge count is where change carries the most risk, and it warrants the tightest review, the best tests, and — usually — a governing design artifact.

**GK11 — Close the loop between the graphs (the join).** The pack's traceability lens (`forensicreview` §3.4) keeps finding the same two gaps, and the join is what makes them mechanical rather than manual:
- **Documentation with no implementation** — a design or spec whose referenced paths do not exist in the code graph. Either the code was never written, or it moved and the design now lies.
- **Risk with no governance** — a god node or high-fan-in module that **no** docs artifact references. The riskiest code in the repo, undocumented.
`scripts/graphify-setup.py --join` computes both from `graph.json` + `docs-index.js` and writes `docs/lenses/code-doc-join.md`. Like every lens it is **derived, never authoritative** (OB6), and like `--analyze` it is a **prompt, not a gate** (OB13).

**GK12 — Register the classes it surfaces.** A join finding that recurs is a defect *class*, not an instance: "a design references a path that no longer exists" and "a god node has no governing design" both belong in `docs/lessons/defect-classes.md` with a control, per `continuous-improvement.md` CI1–CI6. Graphify's own `graphify reflect` aggregates query outcomes into a lessons document; that is a *tool-local* memory and **does not** substitute for the committed register — the register is the durable, reviewed one.

---

## 4. Security and boundaries

**GK13 — On-device is the reason this clears the bar; keep it that way.** No account, no API keys, no telemetry, Apache 2.0, source public. That is what lets a code graph be adopted without the egress question that governs AI plugin features elsewhere (`obsidian-lens.md` OB11). **Community naming and some inference are optional LLM steps** — the moment one is enabled with a hosted backend, repository content leaves the machine, and it becomes a **Privacy & Data Governance** decision like any other. Prefer the no-LLM path (`graphify update`, `--no-label`) unless the naming genuinely earns the egress.

**GK14 — The MCP server is a tool surface, therefore a trust boundary.** Running `graphify-mcp` exposes graph tools to an assistant. In shared/HTTP mode (`--transport http`) it is a **network service over your source code** and inherits the pack's standing rules: least privilege, bind locally unless there is a reason not to, and never expose a graph of a private repository on a shared interface. The Security & Identity lens reviews the decision to run it in shared mode.

**GK15 — Verify the package, not the name.** Install **`graphifyy`** from PyPI or build from the Apache-2.0 source; confirm you are on `graphify.com` / `github.com/Graphify-Labs/graphify`. The vendor explicitly warns that similarly-named packages and a `graphify.net` domain are unaffiliated. Name-adjacency is the oldest supply-chain trick there is.

**GK16 — The code graph never becomes the record.** It is derived, regenerable and lossy about intent — it can tell you that `A` calls `B`, never *why that was the right design*. Decisions live in ADRs and decision notes; contracts live in designs; ownership and freshness live in frontmatter. A repository that starts recording intent in its code graph has lost the thing the docs graph exists to hold.

---

## 5. Standing it up

```bash
# 1. install (package is graphifyy — double y) and register the /graphify skill
uv tool install graphifyy
graphify install --platform claude          # and/or: --platform copilot
python3 docs/ai-forward-pack/scripts/graphify-setup.py --check

# 2. exclude the generated trees BEFORE the first build (GK4)
python3 docs/ai-forward-pack/scripts/graphify-setup.py --init      # writes .graphifyignore + gitignore

# 3. build (no LLM required)
graphify update .

# 4. read the graph
graphify god-nodes --top 10                 # where change is riskiest
graphify affected "MySymbol"                # code-side blast radius
graphify explain "MySymbol"                 # what it is and touches
graphify query "what connects auth to the database?"

# 5. join it to the docs graph and index the lens
python3 docs/ai-forward-pack/scripts/graphify-setup.py --join
python3 docs/ai-forward-pack/scripts/docs-graph.py derive
python3 docs/ai-forward-pack/scripts/docs-graph.py validate
```

Both graphs, plus the Obsidian lens, are set up across repositories in one command by `tools/setup-knowledge-graphs.ps1`.

---

## 6. Self-verification checklist

- [ ] The product was **established from its canonical source**, not inferred from a name (GK1).
- [ ] It is clear which graph is **authoritative** for intent vs reality; the code graph records no decisions (GK2, GK16).
- [ ] `graphify-out/` is **git-ignored**; only inputs are committed (GK3).
- [ ] `.graphifyignore` **de-duplicates** rather than blanket-ignoring: the repo kind was **detected**, the canonical copy is right for that kind, and in a consuming repo `.claude/` and `docs/ai-forward-pack/` are **kept** (GK4).
- [ ] The god-node ranking contains no duplicate copies of one artifact (GK4.1).
- [ ] After changing ignore rules, a **full re-extraction** was run and the node count actually moved (GK5).
- [ ] Every quoted graph path carries its **provenance**, mapped to Verified / Inferred / Flagged (GK6–GK7).
- [ ] Grounding traversed the **code** side as well as the docs side, and any disagreement was surfaced (GK8).
- [ ] `affected` was run before changing a load-bearing symbol, and fed the **change-surface list** (GK9).
- [ ] `god-nodes` reviewed; the riskiest symbols have tests and a governing design (GK10).
- [ ] The **join** was run; documentation-without-implementation and risk-without-governance were triaged (GK11) and recurring shapes registered as classes (GK12).
- [ ] LLM steps are off, or their **egress** was reviewed by the Privacy lens (GK13).
- [ ] The **MCP server**'s exposure was a deliberate decision, not a default (GK14).
- [ ] The installed package is **`graphifyy`** from the official source (GK15).

---

## 7. References

- **Graphify** — `graphify.com` (canonical), `graphify.com/llms-full.txt` (agent index), `github.com/Graphify-Labs/graphify` (Apache 2.0), PyPI **`graphifyy`**. Concepts used here: confidence tags, communities, god nodes, blast radius, traversal.
- **`knowledge-visualization.md`** (V1–V18) — the docs graph this joins to; V15 grounding traversal, V16 change-impact propagation, V18 the script bundle.
- **`obsidian-lens.md`** (OB1–OB14) — the human lens; OB6 lenses are derived-never-authoritative, OB13 prompt-not-gate, OB14 hub watch, OB11 the egress rule GK13 mirrors.
- **`end-to-end-integrity.md`** — **E15** (never assert own-code shape from memory) is what GK7 makes cheap; **E7** the change-surface list GK9 feeds.
- **`continuous-improvement.md`** CI1–CI6 — where join findings become registered classes (GK12).
- **`rigor-protocol.md`** — the Verified / Inferred / Flagged ledger GK6 maps onto.
- **`scripts/graphify-setup.py`** · **`tools/setup-knowledge-graphs.ps1`** — the mechanics.
