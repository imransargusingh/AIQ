# FOUNDATION — provenance of the vendored base-pack docs

The seven docs below are **vendored copies** from the base **Agent Knowledge Pack**, frozen into
this bundle so it is self-contained. They *will* diverge from the base over time; this manifest
makes divergence **visible** instead of surprising, in both directions.

**Check:** `python3 scripts/foundation-check.py` verifies the vendored files against the hashes
below (drift = an uncatalogued edit). Add `--base <path-to-base-pack>` to also compare against
the base copies. After an intentional vendored edit, update the known-divergence list below and
run `--update` to refresh the hashes. Hashes are sha256 (first 16 hex) over **normalized**
content (CRLF→LF, trailing whitespace stripped) so line endings never masquerade as drift.

| Vendored doc | Role | Vendored hash (normalized) |
|---|---|---|
| `agent-body-of-knowledge.md` | The reasoning constitution | `17c26b6ffecc0b1a` |
| `agent-rules-of-the-road.md` | Tiers, gates, the loop | `844eaa73e497f5c5` |
| `agent-persona-catalog.md` | The persona roster's source | `89329ed8ccd71d5f` |
| `layered-optimized-architecture.md` | LOA — AI-integrated architecture | `9c4a2f1b423336a2` |
| `engineering-governance.md` | SDLC lenses around the code | `828063b2be3a8524` |
| `testing-strategy.md` | The proof discipline | `c9c968f077be8152` |
| `csharp-style-guide.md` | C# house style | `c08b9cdf29db065a` |

## Known intentional divergences (vendored ≠ base, by design)

| Doc | Pack-side change | Status in base pack |
|---|---|---|
| `agent-body-of-knowledge.md` | Latest-stable-SDK default policy (currently .NET 10 LTS / C# 14; previews excluded; repo pin wins) | **pending back-port** |
| `testing-strategy.md` | Stale version/example references removed; tracks the latest-SDK policy | **pending back-port** |
| `csharp-style-guide.md` | §2.6 constant-on-the-left comparisons added; §2.2 example fixed to match | **pending back-port** |
| `csharp-style-guide.md` | §1.6 "No commented-out or dead code" added (delete-don't-park; unused-code diagnostics IDE0051/IDE0052/CS0219/IDE0059 as build errors), plus a Defaults row and an Enforcement bullet. Pairs with the language-agnostic CT18a / defect class HYG-A. | **pack-local (dead-code policy)** |
| `agent-rules-of-the-road.md`, `agent-body-of-knowledge.md`, `csharp-style-guide.md` | **Deployment-path correction (FR-045).** The base docs' deployment map named `.github/instructions/{knowledge,csharp,loa,tests}.instructions.md` and a `.github/knowledge/` directory. This pack deploys one `<docname>.instructions.md` per knowledge doc, so four of those instruction filenames were wrong. Corrected to the real paths. *(FR-072 note: `.github/knowledge/` now exists again, as the on-demand half of the load-scope tiering — `load: skill` and `load: reference` docs deploy there instead of being attached to every request.)* | Base is unchanged; the base's map reflects a different install layout. |
| all seven vendored docs | **Load-scope frontmatter (FR-072).** Each vendored doc gained a `load:` declaration (`always` / `glob` / `skill` / `reference`) so the deploy step can scope it instead of attaching every doc to every request. Content is otherwise untouched; only frontmatter was added. | **pack-local (context budgeting).** The base pack has no load-scope concept. |
| `layered-optimized-architecture.md` | **Part IV extracted (FR-072).** The 63,831-character Pattern Catalog moved verbatim to `docs/knowledge/layered-optimized-architecture/pattern-catalog.md`, replaced by a pointer. Part IV is a *lookup* surface while the rest of the document is read linearly, so holding both in one file charged every reader of the principles for the whole catalog. No pattern text was changed, added or removed. | **pack-local (context budgeting).** |
| `engineering-governance.md` | **Responsible-AI row in the governance checklist (2026-09-19).** One row added to the "Governance checklist" table naming the committed policy (`responsible-ai-policy.md`) and the PII/secret scrub (`scrub.py`) as a lens to mark applies / does-not-apply / flagged — the pack-evolution knowledge review found the policy shipped without the checklist referencing it. No other line changed. | **pending back-port** (the base has no RAI policy doc to point at). |

Everything else matches the base at vendoring time (engineering-governance otherwise differs only in
line endings, which normalization ignores). When the base pack absorbs a back-port, re-vendor the doc
here, clear its row above, and run `foundation-check.py --update`.
