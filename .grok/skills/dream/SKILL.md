---
name: dream
description: Run an offline, reviewable consolidation pass over this repo's committed corpus (audit & change logs, defect-class register, captured mitigations, triggered simplify/assume markers) and produce a dream — proposed learnings with controls, rendered as an HTML review view you approve/edit/reject/defer, then promote. The "asleep half" of continuous improvement.
runs_as: Coordinator
---

# Skill: /dream

Run an offline, reviewable **consolidation pass** over this repo's committed corpus — the audit &amp; change logs, the defect-class register, captured **mitigations** (the promotion oracle), and triggered `simplify:`/`assume:` markers — and produce a **dream**: a set of proposed learnings (new/updated defect classes with controls, register de-duplications, control upgrades, confirmed-mitigation promotions, doc updates) rendered as an **HTML review view** you approve, edit, reject, or defer. This is `class → sweep → derive → prevent` (CI2) run **in bulk over the corpus** — the "asleep half" of continuous improvement (`docs/knowledge/continuous-improvement-and-dreaming/`).

**Spine:** the Rigor Protocol on the *corpus*, weighted to **Stage 3 EVIDENCE** (what actually recurs, with provenance) and **Stage 4 DISCONFIRM** (the Simplifier strikes spurious classes; a proposal is not promotable without a falsifiable control). **Authority:** `spec-dreaming`, `architecture-dreaming`, ADR-0002..0005, and `continuous-improvement.md` (CI1–CI12). **Mode:** deterministic harness in Peer Mode; the human is the gate (Adversary Mode) at the review view — nothing is promoted without approval (BoK D3). **Lead:** a consolidation role composing the **Domain Researcher** (evidence), **the Simplifier** (strike non-load-bearing noise — soft veto), and the **Test Architect** (a lesson is not done until it is a control — CI6).

> **Where it sits.** `/dream` reads what every skill's Audit Mandate (`audit-and-change-log.md` AL5) captured, plus the promotion oracle (ADR-0003), and turns it into reviewable, control-bearing learnings. Approved *general* learnings feed the **fleet store**; push them to other repos with **`/apply-learnings`**, or let repos inherit them via `/updatepack`.

## Grounding (first action)
CO-S0 applies first — the sentence is `reference/co-s0.md`. Load and treat as authoritative (Rigor Stage 0; BoK §III.1): the evidence base `docs/knowledge/continuous-improvement-and-dreaming/`, `spec-dreaming`, `architecture-dreaming`, the ADRs, and the current `defect-classes.md` register + `docs/audit/` history. Traverse the graph 1–2 hops from `architecture-dreaming` (V15) and cite the path. Read the register (CI5) so a known class is recognised, not re-proposed.

## Input
Usually none — `/dream` runs over the whole corpus. Optional: a window (`--days N`, default 30) and a focus instruction ("focus on failed outcomes"; "ignore doc-only churn" — the steer field, cf. Claude Dreams `instructions`).

## The pass (light → REM → deep — ADR-0005 boundary)
1. **Light (deterministic, T0).** `python3 docs/ai-forward-pack/scripts/dream.py run --days N` reads the corpus, **stages + dedupes** signals, and runs the **taint gate + scrub** (`scrub.py`; untrusted/tool-authored/`system` origins and secret/PII-bearing signals are *structurally excluded*, not down-scored — AL4). It mines the compile fields (CO-S0) beside PACK-O: `compiled: false` above T0, unanswered `DR-n`, gate refusals by code — ids and codes, never text.
2. **REM (the injected model step — optional enrichment).** For the best abstractions, the running agent may take the deterministic candidate bundle the harness emits and **reflect** repeated instances into a candidate **class** (a signature + "why it survives" + a proposed control), validated against the schema. **If no model call is available, the pass is deterministic-only** (dedup + mitigation-promotions + control-upgrades + marker-harvest still flow — ADR-0005). The model **only proposes**; it never scores, gates, or promotes.
3. **Deep (deterministic, T0).** Score candidates (frequency · recency · distinct-day diversity · has-control), apply the threshold gate, and **render**: `docs/dreams/<drm-id>/dream.json` (canonical), `dream-data.js` (`window.DREAM_DATA`), `index.html` (the review view), and append a **Dream Diary** entry (`docs/dreams/DREAMS.md`, excluded from re-ingestion) + an audit entry.

## Review (the human gate)
Open `docs/dreams/<drm-id>/index.html` over `file://`. For each proposal — grouped by kind, **highest-leverage first** — inspect its **evidence + provenance**, confidence, proposed **control** (with ladder rung), boundary statement, and **federation scope** (repo-local ↔ general), then **Approve / Edit / Reject / Defer**. **Export decisions** emits a JSON + the exact command; the page **writes nothing** (a `file://` page cannot and must not silently save).

## Promote (the only durable write)
`python3 docs/ai-forward-pack/scripts/dream.py apply-decisions <drm-id>-decisions.json` — validates the (possibly hand-edited) decisions file, **re-runs the taint/scrub pass**, enforces the guards (a promotable learning **MUST** carry a falsifiable control — CI6/ADR-0004; **G1 evidence threshold**, **G3 boundary statement**), runs the **instance→class abstraction** (ADR-0004) on approved-*general* items, and promotes: general → the **fleet store** (`learnings/`), repo-local → this repo's register. **Idempotent** — a promoted proposal id is skipped on re-run.

## The promotion oracle (capture successful mitigations)
The oracle is a captured **MitigationRecord** (ADR-0003), the durable evidence that a fix *worked*: `dream.py capture-mitigation --oracle red-green --summary "…" --class "…" --test "…" --control "…"` (a test observed failing then passing) **or** `--oracle human-validated` (you approved a change). A fix with **neither** is `unverified` and is **not** mined. `/implement` and `/investigate` emit a capture whenever they observe a red→green transition or receive an explicit human validation.

## Definition of done
- [ ] A `dream.json` + `dream-data.js` + review `index.html` + Dream Diary entry + audit entry were produced (or a **valid empty dream** if the corpus had nothing — never a fabricated proposal).
- [ ] Tainted / secret-bearing / untrusted-origin signals were **structurally excluded** (logged in the diary's `excluded` count).
- [ ] Every proposal carries **evidence + provenance**, a confidence label, and a **proposed control**; proposals are ordered highest-leverage first.
- [ ] Promotion happened **only via `apply-decisions`** on human-approved items; the run itself wrote no durable store; the source logs were not mutated.
- [ ] Any promoted learning carries a **falsifiable control** and (for general) passed the abstraction guards; the promote step was idempotent.

## Documentation & discoverability (last action)
The dream artifacts are **data** (not graph nodes). If this run produces a durable *decision* about the dreaming capability itself, capture it as a decision note (V17). Append an audit entry (the harness does this automatically via `audit-log.py`).

**Handoff:** → review the HTML → `apply-decisions` → **`/apply-learnings`** to push approved general learnings to other repos (or `/updatepack` for pull-based inheritance).
