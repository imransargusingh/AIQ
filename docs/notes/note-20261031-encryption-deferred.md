---
id: note-20261031-encryption-deferred
title: "Phase 1 stores data in plain SQLite; encryption at rest is deferred"
type: decision-note
status: accepted
owner: "Imran"
phase: "1"
tags: [decision-note, privacy, store, encryption]
links:
  - { to: design-claude-code-skeleton, rel: relates-to }
  - { to: adr-0002, rel: relates-to }
  - { to: adr-0003, rel: relates-to }
review-by: 2027-01-31
review-suggested: []
summary: >-
  Imran deferred encryption at rest during /implement of the Claude Code slice: the store is a plain
  SQLite file readable only by the OS user, with no OS-keychain key. Whether AIQ needs encryption at all
  will be revisited; ADR-0002/0003 and the threat model carry the change until then.
---

# Phase 1 stores data in plain SQLite; encryption at rest is deferred

- **Kind:** decision
- **Confidence:** Verified (Imran's message in the project thread, 2026-10-31 19:42 UTC: "Let's defer
  encryption - we can revisit later whether we even require it")
- **Made during:** `/implement` of `design-claude-code-skeleton`

## The call
The Phase 1 store is `aiq.db`, a plain SQLite file (rusqlite `bundled`), not SQLCipher. There is no store
key and no OS-keychain entry. Protection at rest is the OS user boundary: the data directory is `0700` and
every file AIQ creates is `0600` on Linux/macOS; on Windows the files live under `%USERPROFILE%\.aiq` (moved from `%LOCALAPPDATA%\AIQ` on 2026-11-01, DC-007) and
inherit its per-user ACL. This also removes spike S3c (SQLCipher's vendored OpenSSL needs a native Perl on
Windows) from the critical path.

## Alternatives dismissed
- **Build SQLCipher anyway** (CI Windows runner has Perl; laptop needs Strawberry Perl). Dismissed by the
  owner: the cost (build friction, key management, keychain prompts) is not yet justified.
- **Keep the keychain seam with a no-op key.** Dismissed as dead code (HYG-A): nothing reads it.

## What it constrains / what breaks if it is wrong
- A stolen disk, an unencrypted backup or another administrator on the machine can read stored prompts.
  This was threat T-B4-1 (mitigated by SQLCipher); it is now an **accepted risk** in
  `docs/security/threat-model.md` until revisited.
- Raw prompts are kept until the person deletes them (`aiq forget --all` in Phase 1), so exposure grows
  with time. `aiq status` shows the store size.
- Re-adding encryption later is an expand-migrate-contract change: open the plain file, `sqlcipher_export`
  into an encrypted copy, swap. No data model change.

## Revisit when
Before any alpha user other than Imran installs AIQ, or when an enterprise / managed-device requirement
names encryption at rest.
