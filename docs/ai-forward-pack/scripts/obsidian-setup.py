#!/usr/bin/env python3
"""obsidian-setup.py - stand up (and analyze) the Obsidian lens over an AI-Forward docs graph.

WHAT THIS IS
  The pack's knowledge graph lives in per-artifact YAML frontmatter under docs/ (V2), which
  makes docs/ *already* a valid Obsidian vault. This script makes that lens real and shared:
  it writes a committed .obsidian/ configuration (graph colour groups keyed to the pack's own
  artifact types, the enabled plugin list, sensible defaults), seeds non-canonical dashboard
  "lenses", and keeps per-user workspace/cache files out of git.

  It ALSO ships a dependency-free graph analyzer (--analyze) that computes the same class of
  structural insight the Obsidian graph-analysis plugins provide - degree, betweenness
  centrality (Brandes), components, orphans, structural gaps - directly from docs-index.js.
  That matters: the insight must not be locked behind a GUI plugin, because the pack promises
  tool-neutrality (project-memory-and-obsidian.md M8: Obsidian is a reader, never the writer
  of record).

DESIGN RULES THIS HONORS
  * Frontmatter stays the record; docs-graph.py stays the only writer of the graph. This
    script never edits an artifact's frontmatter and never writes docs-index.js.
  * Obsidian is never required. Every mode works, and --analyze is useful, with Obsidian
    absent.
  * Third-party plugin CODE is not downloaded by default. `--init` writes only the *enabled
    list*, so Obsidian's own UI performs the install with the user's consent. `--fetch-plugins`
    is an explicit, pinned opt-in.

USAGE
  obsidian-setup.py --check                  # report state, write nothing (default)
  obsidian-setup.py --init                   # write .obsidian/ config + lenses + .gitignore
  obsidian-setup.py --analyze                # structural insight report to stdout
  obsidian-setup.py --analyze --write        # ...and save it to docs/lenses/graph-insight.md
  obsidian-setup.py --fetch-plugins          # opt-in: download pinned plugin releases
  obsidian-setup.py --install-app            # print the OS install command (--yes to run it)
  ... --root <repo> --vault docs --dry-run --json

Stdlib only. Python 3.8+. Exit 0 on success, 1 on error, 2 on --check findings.
"""
from __future__ import annotations

import argparse
import io
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import urllib.error
import urllib.request
from collections import defaultdict, deque

# Windows consoles default to cp1252, which cannot encode the glyphs this tool prints
# (DC-211/PLAT-A). Without this the script dies with UnicodeEncodeError on output alone.
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass

TIMEOUT = 30
REGISTRY = "https://raw.githubusercontent.com/obsidianmd/obsidian-releases/master/community-plugins.json"

# ---------------------------------------------------------------------------
# The recommended plugin set. Each entry must justify its seat - this is the
# Simplifier's rule applied to someone else's ecosystem. `why` is shown by
# --check so a maintainer can decline any of them with an informed reason.
#
# NOTE ON "Graphify": Graphify (graphify.com) is a SEPARATE PRODUCT, not an
# Obsidian plugin - an on-device code knowledge graph for AI coding assistants
# (Apache 2.0, PyPI `graphifyy`). It composes with this lens rather than
# competing with it; see knowledge/code-knowledge-graph.md and
# scripts/graphify-setup.py. The Obsidian plugin that provides IN-VAULT graph
# metrics is `knowledge-graph-analysis` by luolanaatud.
# ---------------------------------------------------------------------------
RECOMMENDED = [
    {
        "id": "dataview",
        "name": "Dataview",
        "why": "Turns the V2 frontmatter into queryable data. This is what makes the lenses "
               "(stale, orphan, unowned, review-suggested) live rather than hand-maintained.",
        "tier": "core",
    },
    {
        "id": "knowledge-graph-analysis",
        "name": "Knowledge Graph Analysis",
        "why": "Local graph-theory metrics (degree/betweenness/closeness/eigenvector) plus "
               "optional AI structural analysis. The 'Graphify' capability: finds hubs, "
               "bridges and under-linked regions. Metrics compute locally.",
        "tier": "core",
    },
    {
        "id": "breadcrumbs",
        "name": "Breadcrumbs",
        "why": "Renders the pack's TYPED relations (implements/refines/depends-on/...) as "
               "navigable hierarchy. Obsidian's own graph is untyped; the pack's edges are "
               "typed, and this is what surfaces that difference.",
        "tier": "core",
    },
    {
        "id": "juggl",
        "name": "Juggl",
        "why": "Stylable graph view that can colour and filter by frontmatter - so the graph "
               "can be read by artifact type and status, not just by proximity.",
        "tier": "optional",
    },
    {
        "id": "excalibrain",
        "name": "ExcaliBrain",
        "why": "Parent/child/sibling 'brain' navigation over the same typed links. Best for "
               "tracing spec -> architecture -> design -> proof chains visually.",
        "tier": "optional",
    },
    {
        "id": "smart-connections",
        "name": "Smart Connections",
        "why": "Semantic (embedding) similarity, which finds artifacts that SHOULD be linked "
               "but are not - the gap the typed graph cannot see by construction.",
        "tier": "optional",
    },
]

# Obsidian core plugins worth enabling for this vault.
CORE_PLUGINS = [
    "file-explorer", "global-search", "switcher", "graph", "backlink",
    "outgoing-link", "tag-pane", "properties", "page-preview", "templates",
    "note-composer", "command-palette", "outline", "word-count", "file-recovery",
]

# Per-user Obsidian state that must NEVER be committed (workspace layout, caches,
# plugin data + third-party plugin CODE).
#
# NOT EVERY PLUGIN WRITES INSIDE .obsidian/. Every rule below but the last is scoped to
# {vault}/.obsidian/, and for years that was assumed to cover "per-user state" - because that is
# where Obsidian itself puts it. Smart Connections does not: it writes its vector store to
# {vault}/.smart-env/, a sibling of .obsidian/, so every pattern here missed it and a consuming repo
# committed 268 files and 8.17 MB of embeddings without a single rule firing (TheTerrace FR-371,
# 2026-08-30). The embeddings carry a `last_embed` timestamp and are rewritten on every re-embed, so
# they also conflict by construction between branches.
#
# When adding a plugin to the vault, the question is not "is its state under .obsidian/" but "does
# this plugin write derived state anywhere in the vault at all" - and the answer is checked with
# `git status --untracked-files=all <vault>` after a session, not assumed from the layout.
GITIGNORE_RULES = [
    "# Obsidian lens - per-user state (the vault CONFIG is committed; this is not)",
    "{vault}/.obsidian/workspace.json",
    "{vault}/.obsidian/workspace-mobile.json",
    "{vault}/.obsidian/cache/",
    "{vault}/.obsidian/plugins/*/data.json",
    "{vault}/.obsidian/plugins/*/main.js",
    "{vault}/.obsidian/plugins/*/styles.css",
    "# Plugin state written OUTSIDE .obsidian/ - Smart Connections' vector store (OB4)",
    "{vault}/.smart-env/",
]


# --------------------------------------------------------------------------- io helpers
def out(msg: str = "") -> None:
    """Print without dying on a legacy Windows console codepage."""
    try:
        print(msg)
    except UnicodeEncodeError:
        enc = getattr(sys.stdout, "encoding", None) or "ascii"
        print(msg.encode(enc, "replace").decode(enc, "replace"))


def write_json(path: str, obj, dry: bool) -> str:
    text = json.dumps(obj, indent=2) + "\n"
    return write_text_lf(path, text, dry)


def write_text_lf(path: str, text: str, dry: bool) -> str:
    if os.path.exists(path):
        try:
            with open(path, encoding="utf-8") as f:
                if f.read() == text:
                    return "unchanged"
        except OSError:
            pass
        action = "would update" if dry else "updated"
    else:
        action = "would create" if dry else "created"
    if not dry:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with io.open(path, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(text)
    return action


# --------------------------------------------------------------------------- graph model
def load_index(root: str):
    """Parse docs/docs-index.js (a JS assignment wrapping a JSON object)."""
    path = os.path.join(root, "docs", "docs-index.js")
    if not os.path.exists(path):
        return None, f"docs/docs-index.js not found - run docs-graph.py derive first ({path})"
    with open(path, encoding="utf-8") as f:
        raw = f.read()
    try:
        start = raw.index("{")
        end = raw.rindex("}") + 1
        return json.loads(raw[start:end]), None
    except (ValueError, json.JSONDecodeError) as exc:
        return None, f"could not parse docs-index.js: {exc}"


def build_graph(index):
    """Return (nodes, undirected adjacency, directed out/in) keyed by artifact id."""
    nodes = {a["id"]: a for a in index.get("artifacts", [])}
    adj = defaultdict(set)
    out_e = defaultdict(set)
    in_e = defaultdict(set)
    for a in index.get("artifacts", []):
        src = a["id"]
        adj.setdefault(src, set())
        for link in a.get("links", []) or []:
            dst = link.get("to")
            if dst in nodes and dst != src:
                adj[src].add(dst)
                adj[dst].add(src)
                out_e[src].add(dst)
                in_e[dst].add(src)
    return nodes, adj, out_e, in_e


def betweenness(nodes, adj):
    """Brandes' betweenness centrality, unweighted, on the undirected projection.

    O(V*E). Exact - no sampling - which is affordable at documentation scale and
    means the ranking is reproducible rather than approximate.
    """
    cb = {n: 0.0 for n in nodes}
    for s in nodes:
        stack, preds, sigma, dist = [], {n: [] for n in nodes}, {n: 0 for n in nodes}, {n: -1 for n in nodes}
        sigma[s], dist[s] = 1, 0
        queue = deque([s])
        while queue:
            v = queue.popleft()
            stack.append(v)
            for w in sorted(adj.get(v, ())):
                if dist[w] < 0:
                    dist[w] = dist[v] + 1
                    queue.append(w)
                if dist[w] == dist[v] + 1:
                    sigma[w] += sigma[v]
                    preds[w].append(v)
        delta = {n: 0.0 for n in nodes}
        while stack:
            w = stack.pop()
            for v in preds[w]:
                delta[v] += (sigma[v] / sigma[w]) * (1 + delta[w]) if sigma[w] else 0.0
            if w != s:
                cb[w] += delta[w]
    # undirected: each pair counted twice
    return {n: v / 2.0 for n, v in cb.items()}


def components(nodes, adj):
    seen, comps = set(), []
    for n in sorted(nodes):
        if n in seen:
            continue
        comp, queue = set(), deque([n])
        seen.add(n)
        while queue:
            v = queue.popleft()
            comp.add(v)
            for w in adj.get(v, ()):
                if w not in seen:
                    seen.add(w)
                    queue.append(w)
        comps.append(comp)
    return sorted(comps, key=len, reverse=True)


# Structural expectations drawn from the relation registry (V14): what a given
# artifact type SHOULD normally connect to. A miss is a prompt, never a failure -
# the analyzer reports gaps, it does not gate.
EXPECTED = {
    "spec":         [("tested-by|implements", "no design or proof implements this spec")],
    "design":       [("tested-by", "no proof-pack proves this design's claims")],
    "architecture": [("implements|refines", "nothing refines or implements the architecture")],
    "adr":          [("depends-on|relates-to|supersedes", "ADR is not referenced by anything")],
}


def analyze(index, root):
    nodes, adj, out_e, in_e = build_graph(index)
    if not nodes:
        return {"error": "no artifacts in index"}

    cb = betweenness(nodes, adj)
    comps = components(nodes, adj)
    degree = {n: len(adj.get(n, ())) for n in nodes}

    def rank(scores, k=8):
        return [(n, scores[n]) for n in sorted(scores, key=lambda x: (-scores[x], x))[:k]]

    orphans = sorted(n for n in nodes if degree[n] == 0)
    leaves = sorted(n for n in nodes if degree[n] == 1)
    unowned = sorted(n for n, a in nodes.items() if not (a.get("owner") or "").strip())
    no_review = sorted(n for n, a in nodes.items() if not (a.get("reviewBy") or "").strip())
    flagged = sorted(n for n, a in nodes.items() if a.get("reviewSuggested"))

    owners = defaultdict(list)
    for n, a in nodes.items():
        owners[(a.get("owner") or "(none)").strip()].append(n)

    by_type = defaultdict(list)
    for n, a in nodes.items():
        by_type[a.get("type", "?")].append(n)

    rel_counts = defaultdict(int)
    for a in index.get("artifacts", []):
        for link in a.get("links", []) or []:
            if link.get("to") in nodes:
                rel_counts[link.get("rel", "?")] += 1

    gaps = []
    for n, a in nodes.items():
        rules = EXPECTED.get(a.get("type", ""), [])
        for pattern, message in rules:
            rels = {l.get("rel") for l in (a.get("links") or [])}
            rels |= {l.get("rel") for src in in_e.get(n, ())
                     for l in (nodes[src].get("links") or []) if l.get("to") == n}
            if not any(re.fullmatch(pattern, r or "") for r in rels):
                gaps.append({"id": n, "type": a.get("type"), "gap": message})

    edges = sum(len(v) for v in out_e.values())
    possible = len(nodes) * (len(nodes) - 1) / 2 or 1
    return {
        "project": index.get("project"),
        "generated": index.get("generated"),
        "counts": {
            "artifacts": len(nodes),
            "edges": edges,
            "density": round(len(adj and [1]) and (sum(len(v) for v in adj.values()) / 2) / possible, 4),
            "components": len(comps),
            "largest_component": len(comps[0]) if comps else 0,
        },
        "hubs_by_degree": rank(degree),
        "bridges_by_betweenness": [(n, round(v, 2)) for n, v in rank(cb) if v > 0],
        "authorities_by_inbound": rank({n: len(in_e.get(n, ())) for n in nodes}),
        "orphans": orphans,
        "leaves": leaves,
        "fragments": [sorted(c) for c in comps[1:]],
        "structural_gaps": gaps,
        "unowned": unowned,
        "missing_review_by": no_review,
        "review_suggested": flagged,
        "owners": {k: len(v) for k, v in sorted(owners.items(), key=lambda kv: -len(kv[1]))},
        "by_type": {k: len(v) for k, v in sorted(by_type.items(), key=lambda kv: -len(kv[1]))},
        "relations": dict(sorted(rel_counts.items(), key=lambda kv: -kv[1])),
    }


def render_report(a) -> str:
    L = []
    add = L.append
    c = a["counts"]
    add(f"# Graph insight - {a.get('project')}\n")
    add(f"*Computed from `docs/docs-index.js` (generated {a.get('generated')}) by "
        f"`obsidian-setup.py --analyze`. Dependency-free: no Obsidian or plugin required.*\n")
    add("## Shape\n")
    add(f"- **{c['artifacts']} artifacts**, **{c['edges']} typed links**, density {c['density']}")
    add(f"- **{c['components']} connected component(s)**; largest holds {c['largest_component']}"
        f" artifact(s)\n")
    add("| type | n |\n|---|---|")
    for k, v in a["by_type"].items():
        add(f"| {k} | {v} |")
    add("\n| relation | n |\n|---|---|")
    for k, v in a["relations"].items():
        add(f"| `{k}` | {v} |")
    add("\n## Hubs - the most connected artifacts\n")
    add("*A hub carries the most context. If one is wrong or stale, the error propagates widest.*\n")
    add("| artifact | degree |\n|---|---|")
    for n, v in a["hubs_by_degree"]:
        add(f"| `{n}` | {v} |")
    if a["bridges_by_betweenness"]:
        add("\n## Bridges - highest betweenness\n")
        add("*A bridge is the only path between regions. Losing it fragments the graph; "
            "these are the artifacts most worth keeping accurate.*\n")
        add("| artifact | betweenness |\n|---|---|")
        for n, v in a["bridges_by_betweenness"]:
            add(f"| `{n}` | {v} |")
    add("\n## Attention\n")
    for label, key, note in [
        ("Orphans (no links either way)", "orphans", "an orphan is a finding, not a result (V10)"),
        ("Fragments (disconnected from the main graph)", "fragments", "reachable only in isolation"),
        ("Leaves (single link)", "leaves", "weakly integrated - often correct, sometimes forgotten"),
        ("Unowned", "unowned", "V13 requires an accountable owner"),
        ("Missing review-by", "missing_review_by", "no freshness SLA"),
        ("Flagged review-suggested", "review_suggested", "an upstream change wants a look"),
    ]:
        items = a.get(key) or []
        if key == "fragments":
            items = [", ".join(f) for f in items]
        add(f"\n**{label}** - {len(items)} *({note})*")
        add("\n".join(f"- `{i}`" for i in items) if items else "- none")
    if a["structural_gaps"]:
        add("\n## Structural gaps\n")
        add("*Expected relations that are absent. A prompt, not a failure - "
            "close the link or record why it does not apply.*\n")
        add("| artifact | type | gap |\n|---|---|---|")
        for g in a["structural_gaps"]:
            add(f"| `{g['id']}` | {g['type']} | {g['gap']} |")
    add("\n## Ownership\n")
    add("| owner | artifacts |\n|---|---|")
    for k, v in a["owners"].items():
        add(f"| {k} | {v} |")
    return "\n".join(L) + "\n"


# --------------------------------------------------------------------------- vault config
def type_colors():
    """Graph colour groups keyed to the PACK's artifact types.

    This is the whole point of a committed graph.json: Obsidian's default graph is an
    undifferentiated hairball, and the pack's types are exactly the differentiation that
    makes it readable. Queries use Obsidian's search syntax over frontmatter.
    """
    palette = [
        ("knowledge",     0x7DD3FC), ("design",        0x86EFAC),
        ("architecture",  0xFDBA74), ("adr",           0xFCA5A5),
        ("spec",          0xC4B5FD), ("decision-note", 0xF9A8D4),
        ("proof-pack",    0xFDE047), ("threat-model",  0xF87171),
        ("privacy-review", 0xA5B4FC), ("glossary",     0x5EEAD4),
        ("design-language", 0xD8B4FE), ("doc",         0x94A3B8),
    ]
    groups = [{"query": f'["type":"{t}"]', "color": {"a": 1, "rgb": rgb}} for t, rgb in palette]
    # Status overlays - these deliberately sit last so they win the colour race.
    groups.append({"query": '["status":"superseded"]', "color": {"a": 1, "rgb": 0x64748B}})
    groups.append({"query": '["status":"draft"]', "color": {"a": 1, "rgb": 0xFB923C}})
    return groups


def init_vault(root, vault, dry, enable_optional):
    vault_dir = os.path.join(root, vault)
    cfg = os.path.join(vault_dir, ".obsidian")
    results = []
    if not os.path.isdir(vault_dir):
        return [("error", f"vault directory not found: {vault_dir}")]

    results.append(("app.json", write_json(os.path.join(cfg, "app.json"), {
        "alwaysUpdateLinks": True,
        "newLinkFormat": "relative",
        "useMarkdownLinks": True,
        "attachmentFolderPath": "./assets",
        "showFrontmatter": True,
        "promptDelete": True,
    }, dry)))

    results.append(("appearance.json", write_json(os.path.join(cfg, "appearance.json"), {
        "accentColor": "", "theme": "obsidian", "showInlineTitle": True,
    }, dry)))

    results.append(("core-plugins.json", write_json(
        os.path.join(cfg, "core-plugins.json"), CORE_PLUGINS, dry)))

    enabled = [p["id"] for p in RECOMMENDED if enable_optional or p["tier"] == "core"]
    results.append(("community-plugins.json", write_json(
        os.path.join(cfg, "community-plugins.json"), enabled, dry)))

    results.append(("graph.json", write_json(os.path.join(cfg, "graph.json"), {
        "collapse-filter": False, "search": "", "showTags": True,
        "showAttachments": False, "hideUnresolved": True, "showOrphans": True,
        "collapse-color-groups": False, "colorGroups": type_colors(),
        "collapse-display": False, "showArrow": True, "textFadeMultiplier": 0,
        "nodeSizeMultiplier": 1.2, "lineSizeMultiplier": 1,
        "collapse-forces": False, "centerStrength": 0.5, "repelStrength": 12,
        "linkStrength": 0.8, "linkDistance": 220, "scale": 1,
    }, dry)))
    return results


def lens_notes(project: str):
    """Non-canonical dashboards. Each carries V2 frontmatter so it is a first-class
    graph node rather than an un-indexed finding, and each states plainly that it is a
    LENS - a projection - never a source of truth (M5/M8)."""
    banner = (
        "> **This is a lens, not a record.** Every number below is *derived* from artifact\n"
        "> frontmatter at read time. The frontmatter is the truth (V2); if they disagree, the\n"
        "> frontmatter wins and this page is wrong. Nothing here is load-bearing, and no\n"
        "> canonical document may depend on a query (M8).\n"
        ">\n"
        "> Queries need the **Dataview** plugin. Without it you will see the query source\n"
        "> instead of a table - which is the honest degradation, not a failure.\n"
    )
    return {
        "graph-health.md": f"""---
id: lens-graph-health
title: "Lens - graph health"
type: doc
status: accepted
owner: "@maintainers"
tags: [lens, obsidian, dataview, graph-health]
links:
  - {{ to: lens-graph-structure, rel: relates-to }}
review-by: ""
summary: >-
  A read-time Dataview lens over the knowledge graph's health - stale artifacts, missing
  owners, missing freshness SLAs, and review-suggested flags. Derived, never authoritative.
---

# Lens - graph health

{banner}

## Stale - past their `review-by`

```dataview
TABLE type, owner, review-by AS "due"
FROM "."
WHERE review-by AND date(review-by) < date(today)
SORT review-by ASC
```

## Flagged `review-suggested` (V16 propagation)

```dataview
TABLE type, owner, review-suggested AS "flags"
FROM "."
WHERE review-suggested AND length(review-suggested) > 0
SORT file.name ASC
```

## Missing an owner (V13)

```dataview
TABLE type, status
FROM "."
WHERE !owner
SORT type ASC
```

## Missing a freshness SLA

```dataview
TABLE type, owner
FROM "."
WHERE !review-by OR review-by = ""
SORT type ASC
```

## Draft or superseded

```dataview
TABLE type, status, owner
FROM "."
WHERE status = "draft" OR status = "superseded"
SORT status ASC
```
""",
        "graph-structure.md": f"""---
id: lens-graph-structure
title: "Lens - graph structure"
type: doc
status: accepted
owner: "@maintainers"
tags: [lens, obsidian, dataview, structure]
links:
  - {{ to: lens-graph-health, rel: relates-to }}
review-by: ""
summary: >-
  A read-time lens over the shape of the knowledge graph - artifacts by type and status, and
  the traceability chains (spec to design to proof). Derived, never authoritative.
---

# Lens - graph structure

{banner}

## Everything, by type

```dataview
TABLE rows.file.link AS "artifacts"
FROM "."
WHERE type
GROUP BY type
```

## Traceability - designs and what proves them

```dataview
TABLE status, owner, links AS "typed links"
FROM "."
WHERE type = "design"
SORT file.name ASC
```

## Decisions - ADRs and decision notes

```dataview
TABLE type, status, owner
FROM "."
WHERE type = "adr" OR type = "decision-note"
SORT type ASC, file.name ASC
```

## Recently touched

```dataview
TABLE type, status, file.mtime AS "modified"
FROM "."
WHERE type
SORT file.mtime DESC
LIMIT 20
```

## Deeper structural insight

Degree, betweenness, components, orphans and structural gaps are computed
dependency-free by:

```
python3 docs/ai-forward-pack/scripts/obsidian-setup.py --analyze
```

For the interactive version inside Obsidian, use the **Knowledge Graph Analysis**
plugin (local metrics; AI features are opt-in and require your own API key).
""",
    }


def update_gitignore(root, vault, dry):
    path = os.path.join(root, ".gitignore")
    existing = ""
    if os.path.exists(path):
        with open(path, encoding="utf-8") as f:
            existing = f.read()
    rules = [r.format(vault=vault) for r in GITIGNORE_RULES]
    missing = [r for r in rules if r not in existing and not r.startswith("#")]
    if not missing:
        return "unchanged"
    block = "\n" + "\n".join(r.format(vault=vault) for r in GITIGNORE_RULES) + "\n"
    if dry:
        return f"would append {len(missing)} rule(s)"
    with io.open(path, "a", encoding="utf-8", newline="\n") as fh:
        fh.write(block)
    return f"appended {len(missing)} rule(s)"


# --------------------------------------------------------------------------- plugins / app
def fetch_registry():
    try:
        req = urllib.request.Request(REGISTRY, headers={"User-Agent": "ai-forward-obsidian-setup"})
        with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
            return json.loads(resp.read().decode("utf-8")), None
    except (urllib.error.URLError, OSError, ValueError, json.JSONDecodeError) as exc:
        return None, f"registry unreachable: {exc}"


def fetch_plugins(root, vault, dry, enable_optional):
    """Explicit opt-in: download plugin code from each plugin's GitHub release.

    This executes third-party JavaScript inside Obsidian, so it is NOT the default.
    The safer path - and the one --init sets up - is to let Obsidian's own plugin
    browser install them with the user's consent.
    """
    registry, err = fetch_registry()
    if err:
        return [("error", err)]
    by_id = {p["id"]: p for p in registry}
    results = []
    for spec in RECOMMENDED:
        if not enable_optional and spec["tier"] != "core":
            continue
        entry = by_id.get(spec["id"])
        if not entry:
            results.append((spec["id"], "NOT IN REGISTRY - skipped"))
            continue
        repo = entry.get("repo")
        dest = os.path.join(root, vault, ".obsidian", "plugins", spec["id"])
        if dry:
            results.append((spec["id"], f"would fetch from github.com/{repo}"))
            continue
        try:
            api = f"https://api.github.com/repos/{repo}/releases/latest"
            req = urllib.request.Request(api, headers={"User-Agent": "ai-forward-obsidian-setup"})
            with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
                release = json.loads(resp.read().decode("utf-8"))
            tag = release.get("tag_name", "?")
            os.makedirs(dest, exist_ok=True)
            got = []
            for asset in release.get("assets", []):
                if asset.get("name") in ("main.js", "manifest.json", "styles.css"):
                    areq = urllib.request.Request(
                        asset["browser_download_url"],
                        headers={"User-Agent": "ai-forward-obsidian-setup"})
                    with urllib.request.urlopen(areq, timeout=TIMEOUT) as ar:
                        data = ar.read()
                    with open(os.path.join(dest, asset["name"]), "wb") as fh:
                        fh.write(data)
                    got.append(asset["name"])
            results.append((spec["id"], f"{tag}: {', '.join(got) if got else 'no standard assets'}"))
        except (urllib.error.URLError, OSError, ValueError, json.JSONDecodeError) as exc:
            results.append((spec["id"], f"FAILED: {exc}"))
    return results


def app_install_command():
    system = platform.system()
    if system == "Windows":
        return ["winget", "install", "--id", "Obsidian.Obsidian", "--exact",
                "--accept-package-agreements", "--accept-source-agreements"]
    if system == "Darwin":
        return ["brew", "install", "--cask", "obsidian"]
    return ["flatpak", "install", "-y", "flathub", "md.obsidian.Obsidian"]


def app_installed():
    if shutil.which("obsidian"):
        return True
    if platform.system() == "Windows":
        for base in (os.environ.get("LOCALAPPDATA", ""), os.environ.get("PROGRAMFILES", "")):
            if base and os.path.isdir(os.path.join(base, "Obsidian")):
                return True
        try:
            res = subprocess.run(["winget", "list", "--id", "Obsidian.Obsidian", "--exact"],
                                 capture_output=True, text=True, encoding="utf-8",
                                 errors="replace", timeout=TIMEOUT)
            return "Obsidian" in (res.stdout or "")
        except (OSError, subprocess.SubprocessError):
            return False
    if platform.system() == "Darwin":
        return os.path.isdir("/Applications/Obsidian.app")
    return False


# --------------------------------------------------------------------------- main
def main() -> int:
    ap = argparse.ArgumentParser(description="Set up and analyze the Obsidian lens over an AI-Forward docs graph.")
    ap.add_argument("--root", default=".", help="repository root (default: .)")
    ap.add_argument("--vault", default="docs", help="vault directory relative to root (default: docs)")
    ap.add_argument("--check", action="store_true", help="report state, write nothing (default)")
    ap.add_argument("--init", action="store_true", help="write .obsidian/ config, lenses, .gitignore")
    ap.add_argument("--analyze", action="store_true", help="structural insight from docs-index.js")
    ap.add_argument("--write", action="store_true", help="with --analyze: save to <vault>/lenses/graph-insight.md")
    ap.add_argument("--fetch-plugins", action="store_true", help="opt-in: download plugin code from GitHub releases")
    ap.add_argument("--install-app", action="store_true", help="install the Obsidian desktop app")
    ap.add_argument("--yes", action="store_true", help="with --install-app: actually run it")
    ap.add_argument("--all-plugins", action="store_true", help="include the optional-tier plugins")
    ap.add_argument("--dry-run", action="store_true", help="print the plan, write nothing")
    ap.add_argument("--json", action="store_true", help="machine-readable output (with --analyze)")
    args = ap.parse_args()

    root = os.path.abspath(args.root)
    dry = args.dry_run
    did_something = False

    if args.install_app:
        did_something = True
        cmd = app_install_command()
        if app_installed():
            out("Obsidian: already installed")
        elif args.yes and not dry:
            out("running: " + " ".join(cmd))
            try:
                subprocess.run(cmd, check=False, timeout=900)
            except (OSError, subprocess.SubprocessError) as exc:
                out(f"  install failed: {exc}")
                return 1
        else:
            out("Obsidian is not installed. Run:\n  " + " ".join(cmd) +
                "\n(or re-run this script with --install-app --yes)")

    if args.analyze:
        did_something = True
        index, err = load_index(root)
        if err:
            out(f"error: {err}")
            return 1
        result = analyze(index, root)
        if args.json:
            out(json.dumps(result, indent=2))
        else:
            report = render_report(result)
            out(report)
            if args.write:
                dest = os.path.join(root, args.vault, "lenses", "graph-insight.md")
                fm = (
                    "---\n"
                    "id: lens-graph-insight\n"
                    'title: "Lens - graph insight (computed)"\n'
                    "type: doc\nstatus: accepted\nowner: \"@maintainers\"\n"
                    "tags: [lens, graph-analysis, computed]\n"
                    "links:\n  - { to: lens-graph-structure, rel: relates-to }\n"
                    'review-by: ""\n'
                    "summary: >-\n"
                    "  Computed structural analysis of the knowledge graph - hubs, bridges,\n"
                    "  components, orphans and structural gaps. Regenerate with\n"
                    "  obsidian-setup.py --analyze --write. Derived, never authoritative.\n"
                    "---\n\n"
                )
                out(f"  {write_text_lf(dest, fm + report, dry)}: {os.path.relpath(dest, root)}")

    if args.init:
        did_something = True
        out(f"Initializing the Obsidian lens in {os.path.join(args.vault, '.obsidian')}"
            + (" (dry run)" if dry else ""))
        for name, status in init_vault(root, args.vault, dry, args.all_plugins):
            out(f"  {status:>16}  {name}")
            if name == "error":
                return 1
        for name, text in lens_notes(os.path.basename(root)).items():
            dest = os.path.join(root, args.vault, "lenses", name)
            out(f"  {write_text_lf(dest, text, dry):>16}  lenses/{name}")
        out(f"  {update_gitignore(root, args.vault, dry):>16}  .gitignore")
        out("\nNext: open Obsidian -> 'Open folder as vault' -> select "
            f"{os.path.join(root, args.vault)}")
        out("Then Settings -> Community plugins -> Browse, and install the enabled list "
            "(or re-run with --fetch-plugins to download them directly).")
        out("Finally, re-run docs-graph.py derive so the new lens notes enter the index.")

    if args.fetch_plugins:
        did_something = True
        out("Fetching plugin releases (third-party code - explicit opt-in)"
            + (" (dry run)" if dry else ""))
        for name, status in fetch_plugins(root, args.vault, dry, args.all_plugins):
            out(f"  {name:>28}  {status}")

    if args.check or not did_something:
        cfg = os.path.join(root, args.vault, ".obsidian")
        vault_dir = os.path.join(root, args.vault)
        index, err = load_index(root)
        findings = []
        out("AI-Forward Obsidian lens - status\n")
        # The single most-asked question is "where do I point Obsidian?" - answer it up front,
        # every run, not only on --init (whose output has usually scrolled away by then).
        out(f"  {'vault path':<26} {vault_dir}")
        out(f"  {'':<26} Obsidian -> 'Open folder as vault' -> select that folder")
        out(f"  {'':<26} (the vault is <repo>/{args.vault}, never the repo root - OB5)\n")
        out(f"  {'app installed':<26} {'yes' if app_installed() else 'NO - run --install-app'}")
        if not app_installed():
            findings.append("Obsidian not installed")
        out(f"  {'vault config':<26} "
            f"{'present' if os.path.isdir(cfg) else 'MISSING - run --init'}")
        if not os.path.isdir(cfg):
            findings.append("vault config missing")
        out(f"  {'docs-index.js':<26} {'ok' if index else 'MISSING - run docs-graph.py derive'}")
        if err:
            findings.append(err)
        lenses = os.path.join(root, args.vault, "lenses")
        out(f"  {'lens notes':<26} "
            f"{len(os.listdir(lenses)) if os.path.isdir(lenses) else 0} present")
        out("\n  recommended plugins (why each earns its seat):")
        for p in RECOMMENDED:
            out(f"    [{p['tier']:^8}] {p['name']} ({p['id']})")
            out(f"               {p['why']}")
        out("\n  NOTE: Graphify (graphify.com) is a separate on-device CODE knowledge graph,")
        out("        not an Obsidian plugin - see code-knowledge-graph.md + graphify-setup.py.")
        if findings:
            out(f"\n{len(findings)} finding(s): " + "; ".join(findings))
            return 2
        out("\nno findings")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        sys.exit(130)
