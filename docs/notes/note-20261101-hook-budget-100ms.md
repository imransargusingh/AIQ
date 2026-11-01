---
id: note-20261101-hook-budget-100ms
title: "The Claude Code Stop hook budget is 100 ms p95 for the whole process"
type: decision-note
status: accepted
owner: "Imran"
phase: "1"
tags: [decision-note, performance, collector, claude-code, windows]
links:
  - { to: design-claude-code-skeleton, rel: relates-to }
  - { to: adr-0005, rel: relates-to }
  - { to: proof-claude-code-skeleton, rel: relates-to }
review-by: 2027-01-31
review-suggested: []
summary: >-
  Imran raised the whole-process p95 budget for AIQ's synchronous Claude Code Stop hook from 50 ms to
  100 ms after his Windows laptop measured 65-70 ms p95, almost all of it process start. The hook's own
  work stays at about 3 ms p95.
---

# The Claude Code Stop hook budget is 100 ms p95 for the whole process

- **Kind:** decision
- **Confidence:** Verified (Imran's choice on the decision card in the project thread, 2026-11-01 04:12 UTC:
  "Raise to 100 ms at p95")
- **Made during:** the Windows laptop check of `/implement` for `design-claude-code-skeleton` (PR #9)

## The call
AIQ's Stop hook runs synchronously at the end of every Claude Code turn. Its budget is now **100 ms p95 for
the whole process** (start to exit), up from 50 ms. The e2e test
`e2e_core_down_stop_exits_zero_within_budget_p95_and_spools` defaults to 100 ms; the shared Windows CI
runner keeps its wider 200 ms. `aiq install`'s self-test reports against 100 ms.

## Why
On Imran's laptop (Windows, release build, direct exec, 50 runs each) the whole process measured p50
27.5 ms and p95 66.8-70.5 ms; the hook's in-process work measured p95 3.1 ms. The gap is Windows process
start (Inferred: likely security scanning; not measured). AIQ cannot shrink that part, and a 70 ms pause
after a reply that took seconds is not noticeable.

## Options weighed
- Typical (p50) under 50 ms plus in-process p95 under 10 ms. Recommended by Claude; not chosen.
- Keep 50 ms p95 and investigate process start before merging. Not chosen.
- **100 ms p95. Chosen.** Trade-off: less sensitive to a real slowdown in AIQ's own code than an
  in-process bar would be.

## What it changes
- The design's Stop budget and the Proof Pack's C3 oracle.
- ADR-0005, the architecture and the coaching-loop spec state "50 ms p95 added to any hook" as a
  collector-wide NFR. They are flagged review-suggested, not rewritten: this decision covers the Claude
  Code Stop hook measured here; other collectors keep 50 ms until measured.
