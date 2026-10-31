---
id: adr-0001
title: "ADR-0001: Local-first core, thin collectors, stateless backend"
type: adr
status: proposed
owner: "Imran"
phase: "0-1"
tags: [architecture]
links:
  - { to: aiq-architecture, rel: refines }
review-by: 2027-04-29
summary: >-
  One AIQ core per OS user owns all state and logic; collectors are thin pipes; the backend only serves signed packs and updates and never holds user data.
---

# ADR-0001: Local-first core, thin collectors, stateless backend

- **Status:** Proposed
- **Date:** 2026-10-31
- **Deciders:** Imran (owner); drafted with Claude via /define-architecture
- **Context spec/architecture:** docs/architecture.md, docs/specs/coaching-loop.md

## Context
D4 and US-6 require that only the person can see their data and that nothing leaves the device in
normal operation. The surface parity rule requires one interruption budget and one set of Habit
tracks per person across VS Code, Claude Code and Copilot CLI.

## Decision
Run one background **AIQ core** per OS user. It owns the encrypted store, Task grouping, detectors,
Job classification, trigger engine, Habit tracking and the rung model. Collectors and renderers on
each surface are thin: they forward events and display cards. The backend is static and stateless:
a CDN for signed packs and signed app updates. No AIQ account; no user data on any server.

LOA archetype: **F Copilot Aside Hot Path** composed with **G Continuous Sentinel** and an inner
**A Cascade Pipeline** (T0 → T2).

## Alternatives considered
- **Smart collectors, no core:** rejected; state must be shared across surfaces, and logic would be
  written three times.
- **Hosted service with per-user encryption:** rejected; the server sees data in use, breaking D4.
- **Run the logic inside the VS Code extension host:** rejected; CLI agents run without VS Code open.

## Consequences
- **Positive:** privacy by construction; one budget and one history; collectors stay simple.
- **Negative / accepted trade-offs:** a background process to install, update and keep healthy on
  three OSes; no cross-device view until the opt-in sync relay (Platform phase).
- **Follow-ups / new risks:** auto-start and update mechanics per OS (design-slice); core crash must
  never affect the tools (ADR-0005).

## Evidence
Brief, Design > Implementation tab; spec D2, D4, parity rule.
