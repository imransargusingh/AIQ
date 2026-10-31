---
id: adr-0004
title: "ADR-0004: Build the AIQ core, hook client and desktop shell in Rust"
type: adr
status: accepted
owner: "Imran"
phase: "0-1"
tags: [architecture, tech-stack]
links:
  - { to: aiq-architecture, rel: refines }
review-by: 2027-04-29
summary: >-
  The core service, the tiny `aiq hook` client and the Tauri desktop shell are Rust; the VS Code
  extension and the React UI are TypeScript; event and card contracts are JSON Schema shared by both.
---

# ADR-0004: Build the AIQ core, hook client and desktop shell in Rust

- **Status:** Accepted (Imran, 2026-10-31, decision card)
- **Date:** 2026-10-31
- **Deciders:** Imran

## Context
NFRs: core idle CPU < 1%, RSS < 150 MB; hooks add ≤ 50 ms p95. The Phase 0 probe measured a Python
hook process at 47 ms p95 cold, so the hook client must be compiled (ADR-0005). The desktop app was
proposed as Tauri, which is Rust.

## Decision
- **Rust:** the core service (store, ingest, detectors host incl. WASM runtime, trigger engine,
  habit tracker, judge client), the `aiq hook` client (same binary, subcommand), and the Tauri shell.
- **TypeScript:** the VS Code extension and the React UI (desktop app and VS Code webview).
- **Contracts:** `aiq.event.v1` and `aiq.card.v1` as JSON Schema, with generated Rust and TypeScript
  types; a schema change fails CI on either side.
- One signed binary per OS (Windows, macOS, Linux).

## Alternatives considered
- **TypeScript core (Node/Bun):** rejected; higher memory, still needs a compiled hook helper.
- **Go:** rejected; no reuse with Tauri, so the desktop app needs a second shell.

## Consequences
- **Positive:** meets the resource and hook budgets; one binary to sign and ship.
- **Negative / accepted trade-offs:** two languages; Rust ramp-up cost for the team.
- **Follow-ups:** pick the SQLCipher, keychain and WASM crates in `/design-slice` and spike each (Spike Protocol).
