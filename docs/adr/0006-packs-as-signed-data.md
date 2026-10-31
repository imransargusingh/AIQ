---
id: adr-0006
title: "ADR-0006: Coaching packs are signed data; code only in a WASM sandbox"
type: adr
status: proposed
owner: "Imran"
phase: "0-1"
tags: [architecture]
links:
  - { to: aiq-architecture, rel: refines }
review-by: 2027-04-29
summary: >-
  Detectors, tips, thresholds, caps and job rubrics ship as signed pack data; any pack code runs in a WASM sandbox with no network, file or store access.
---

# ADR-0006: Coaching packs are signed data; code only in a WASM sandbox

- **Status:** Proposed
- **Date:** 2026-10-31
- **Deciders:** Imran (owner); drafted with Claude via /define-architecture

## Context
D5 thresholds and the D3 cap must be tunable without a release; D7 organises content into job
packs; third-party packs come later (Platform phase). Pack content runs next to the person's
private data.

## Decision
- A pack is a signed bundle: manifest (id, version, job, min core version), detector rules as
  declarative data, tips, Job-classifier rules, rubric examples, golden set, and default thresholds
  (cooldowns, D5 parameters). The person's own settings (e.g. daily cap 1/3/5, default 5) override
  pack defaults.
- Detectors that need code compile to WebAssembly and run with no imports for network, file system
  or store; they receive an event slice and return Signals. CPU and memory limits per call.
- The core verifies an Ed25519 signature against keys pinned in the binary before loading; a pack
  that fails keeps the previous version.

## Alternatives considered
- **Detectors as native code in the core:** rejected; every tweak becomes a release.
- **Lua/JS scripting:** rejected; harder to sandbox than WASM.

## Consequences
- **Positive:** tuning without releases; safe path to third-party packs.
- **Negative:** a rule DSL and WASM host to build and maintain.
- **Follow-ups:** signing key custody and rotation (threat model T-B2).
