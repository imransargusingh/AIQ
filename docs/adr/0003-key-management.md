---
id: adr-0003
title: "ADR-0003: Store key lives only in the OS keychain; no recovery in the alpha"
type: adr
status: accepted
owner: "Imran"
phase: "0-1"
tags: [architecture, security, privacy]
links:
  - { to: aiq-architecture, rel: refines }
review-suggested:
  - { by: note-20261031-encryption-deferred, on: 2026-10-31, reason: "Imran deferred encryption at rest; Phase 1 has no store key and no keychain entry. Decide whether this ADR is deferred or superseded." }
review-by: 2027-04-29
summary: >-
  The store's encryption key is generated on first run and kept only in the OS keychain under the
  person's account. Losing or wiping the machine loses the history; a recovery phrase arrives with
  the encrypted sync relay.
---

# ADR-0003: Store key lives only in the OS keychain; no recovery in the alpha

- **Status:** Accepted (Imran, 2026-10-31, decision card)
- **Note (2026-10-31):** not exercised in Phase 1: encryption at rest is deferred (Imran), so there is no key to store. See [note-20261031-encryption-deferred](../notes/note-20261031-encryption-deferred.md).
- **Date:** 2026-10-31
- **Deciders:** Imran

## Context
ADR-0002 keeps raw prompts and code until the person deletes them, so the store holds sensitive,
growing history. D4 says only the person can read it. Options were keychain-only, a recovery phrase,
or a passphrase-protected backup file.

## Decision
- On first run the core generates a random 256-bit key and stores it in the OS keychain (Windows
  Credential Manager / DPAPI, macOS Keychain, Linux Secret Service), scoped to the OS user and to
  the AIQ binary where the OS supports it.
- The key never leaves the keychain except into core memory; it is never logged, exported or synced.
- **No recovery in the alpha:** a lost, wiped or re-imaged machine loses the AIQ history. Setup says
  so in plain words.
- The person's Anthropic API key (ADR-0007) is a separate keychain entry under the same rules.
- Export (ADR-0002) writes plaintext only to a location the person chooses, with a warning.

## Alternatives considered
- **Recovery phrase from day one:** deferred to the sync relay (Platform phase); one more secret to lose.
- **Passphrase-protected backup file:** deferred; a backup in a synced folder adds an offline-attack
  target.

## Consequences
- **Positive:** nothing new for people to manage; no key ever leaves the device.
- **Negative / accepted trade-offs:** history is lost with the machine; moving to a new laptop starts
  fresh (export/import is the manual path).
- **Follow-ups:** Linux machines without a Secret Service need a fallback (spike); keychain prompts on
  macOS must not appear on every hook (the core holds the key, hooks never touch it).
