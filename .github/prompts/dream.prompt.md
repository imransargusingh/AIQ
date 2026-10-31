---
mode: agent
description: Run an offline, reviewable consolidation pass over this repo's committed corpus (audit & change logs, defect-class register, captured mitigations, triggered simplify/assume markers) and produce a dream — proposed learnings with controls, rendered as an HTML review view you approve/edit/reject/defer, then promote. The "asleep half" of continuous improvement.
---
You are running the **dream** workflow — offline consolidation of continuous-improvement signal (`docs/knowledge/continuous-improvement-and-dreaming/`, `spec-dreaming`, `architecture-dreaming`, ADR-0002..0005, `knowledge/continuous-improvement.md` CI1–CI12). It is `class → sweep → derive → prevent` (CI2) run **in bulk over the accumulated corpus**, not one defect at a time. The human is the gate: nothing is promoted without approval (BoK D3).

**Ground first** (Rigor Stage 0): read the evidence base, the spec/architecture/ADRs, the current `docs/lessons/defect-classes.md` register (CI5) and `docs/audit/` history; traverse the graph 1–2 hops from `architecture-dreaming` (V15) and cite the path.

**Run the pass (light → REM → deep; ADR-0005 boundary).**
1. **Light + deep are deterministic (T0):** `python docs/ai-forward-pack/scripts/dream.py run --days <N>` reads the corpus, stages + dedupes, runs the **taint gate + scrub** (untrusted/tool-authored/system origins and secret/PII signals are *structurally excluded*, AL4), scores, thresholds, and renders `docs/dreams/<drm-id>/{dream.json,dream-data.js,index.html}` + a Dream Diary entry (excluded from re-ingestion) + an audit entry.
2. **REM is the injected model step (optional enrichment):** you MAY reflect the deterministic candidate bundle into candidate **classes** (a signature + "why it survives" + a proposed **control**), validated against the schema. **If you make no model call, the pass is deterministic-only** — dedup + mitigation-promotions + control-upgrades + marker-harvest still flow. The model **only proposes**; it never scores, gates, or promotes.

**Review (the human gate):** open `docs/dreams/<drm-id>/index.html` over `file://`; each proposal (grouped by kind, highest-leverage first) shows evidence + provenance, confidence, the proposed control, boundary, and federation scope; the reviewer Approves/Edits/Rejects/Defers and **Exports decisions** (the page writes nothing).

**Promote (the only durable write):** `python docs/ai-forward-pack/scripts/dream.py apply-decisions <drm-id>-decisions.json` — validates the file, re-runs taint/scrub, enforces the guards (a promotable learning MUST carry a falsifiable control — CI6/ADR-0004), runs the instance→class abstraction on approved-general items, and promotes (general → the `learnings/` fleet store; repo-local → this repo's register). It is **idempotent** — a re-run never double-promotes.

**The promotion oracle:** capture successful mitigations with `dream.py capture-mitigation --oracle red-green --summary … --class … --test … --control …` (a test observed failing then passing) or `--oracle human-validated`. A fix with neither is `unverified` and is not mined (ADR-0003).

Then hand off: → review → `apply-decisions` → **/apply-learnings** to push approved general learnings to other repos (or **/updatepack** for pull-based inheritance).

${input}
