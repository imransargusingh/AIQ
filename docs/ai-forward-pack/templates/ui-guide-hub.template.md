---
id: ui-capability-guide
title: "UI & UX Capability Guide"
type: doc
status: accepted
owner: "@maintainers"
tags: [ui, ux, guide, design, accessibility]
links:
  - { to: audit-log, rel: relates-to }
review-by: __REVIEW_BY__
review-suggested: []
summary: >-
  Hub node for the browsable UI & UX Capability Guide (ui-guide.html): the how-to layer
  over the pack's UI standards - the layer stack, a job-to-path picker, the /ui-design
  stages, the archetype picker, and the veto table.
---

# UI & UX Capability Guide

The guide itself is the self-contained page [`ui-guide.html`](ui-guide.html); open it in a
browser. This node exists so the guide is a first-class artifact in the knowledge graph
(V10) rather than an unreachable file.

> **Add a link as the repo grows.** The single `relates-to: audit-log` edge exists because a
> fresh install has no other artifact to point at, and both a dangling link and an orphan
> fail `docs-graph.py validate`. Once this repo has an architecture or design artifact,
> link this node to it and the guide joins the real graph.

## What it covers
- The UI standard stack and which standard governs which decision.
- A job-to-path picker: what to run for *design a new surface* vs *review an existing one*.
- The `/ui-design` stages, the archetype picker, and the accessibility veto table.
