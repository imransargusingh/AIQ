---
id: adr-0002
title: "ADR-0002: Append-only facts and dimensions in an encrypted local SQLite store"
type: adr
status: proposed
owner: "Imran"
phase: "0-1"
tags: [architecture, data, privacy]
links:
  - { to: aiq-architecture, rel: refines }
review-by: 2027-04-29
summary: >-
  AIQ stores everything in one SQLCipher-encrypted SQLite file per OS user: entities as dimensions,
  everything that happens as append-only facts, current state as rebuildable views. Raw content is
  kept until the person deletes it.
---

# ADR-0002: Append-only facts and dimensions in an encrypted local SQLite store

- **Status:** Proposed (retention decided by Imran 2026-10-31)
- **Date:** 2026-10-31
- **Deciders:** Imran (owner); drafted with Claude via /define-architecture

## Context
Bounded context: **Coaching** (spec domain model). Aggregates and invariants: Coaching session
(budget never exceeded, one card at a time), Habit track (status only from observed behaviour; never
Adopted from one Task), Tip preference ("don't show again" never overridden), Job classification
(exactly one Job or Unclassified per Task). D5 needs 14-day windows over history; rung judgments are
versioned (rung-model spec); US-6 needs export and delete-all.

## Decision
- **One store per OS user:** SQLite with SQLCipher, key from the OS keychain (ADR-0003).
- **Dimensions:** surface, surface session, Task, Job, pack version, tip type, habit.
- **Append-only facts:** `event` (normalised collector events), `signal`, `card_decision`
  (show/hold/drop + rule + pack version), `card_feedback`, `habit_observation`, `rung_judgment`
  (versioned), `setting_change`. History and the audit trail *are* the data.
- **Current state is derived:** budget counters, Habit status, tip ranking and job mix are views or
  labelled, rebuildable caches over the facts. Budget counters are persisted so they survive a restart.
- **Content split:** raw content (prompt text, code excerpts, transcripts) lives in a separate
  `content` table keyed by `event_id`, so it can be purged on a schedule without touching facts.
  **Retention: kept until the person deletes it** (Imran, 2026-10-31). Settings offer delete-one,
  delete-older-than and delete-all; there is no automatic purge.
- **Delete-all** drops the database file and the keychain entry; **export** writes facts and any
  retained content to a user-chosen file.

## Alternatives considered
- **Normalised current-state tables:** rejected; D5 windows and versioned judgments need history,
  which would become a shadow log.
- **Full event sourcing with replay:** rejected for Phase 1; facts + views give the same audit trail
  without a replay engine.
- **Per-surface stores:** rejected; breaks the shared budget.

## Consequences
- **Positive:** history is free; deleting content is a clean purge; rebuildable state.
- **Negative / accepted trade-offs:** current-state reads go through views or caches that must be
  indexed; the file grows (signals are small; raw content grows without bound, so the store must stay fast at multi-GB size and Settings show its size).
- **Security consequence:** with no purge, a stolen unlocked laptop or a leaked key exposes all history; encryption at rest and key handling (ADR-0003) carry more weight.
- **Follow-ups:** schema migration strategy (design-slice); size budget per year of use.
