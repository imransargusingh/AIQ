---
id: design-multi-machine-sync
title: "Design: keep your computers in sync (slice 1, Remote Desktop)"
type: design
status: proposed
owner: "Imran"
phase: "5"
tags: [design, sync, privacy, store]
links:
  - { to: privacy-and-terms, rel: documents }
  - { to: design-tip-funnel, rel: relates-to }
review-by: 2027-05-12
summary: >-
  People use AIQ on more than one computer (a laptop and a Devbox), so one computer's AIQ told half the story. A
  computer reached through Remote Desktop now offers "Keep in sync"; the two then swap numbered change files through
  the Remote Desktop drive link, so the data lives only on the person's own computers, with no server, relay, cloud
  folder or open port.
---

# Keep your computers in sync

## Why

Imran (2026-11-12): "many users use multiple machines. As a result, AIQ reporting based on a single machine's data
tells an incomplete/misleading story." Requirements: the person stays in control, the data is never stored on a
device they don't control, and turning it on and off is surprisingly easy.

## What was measured before building (2026-11-12, Imran's laptop and Devbox)

| Check | Result |
|---|---|
| Laptop store | 18.2 MB + 4.1 MB WAL after 11 days; most is one-time imported history. Live growth ≤ ~0.2 MB a day. |
| Direct computer-to-computer QUIC (iroh 1.4.1, relay off, no lookup service), both directions | Failed. The laptop is behind home Wi-Fi; the Devbox has a private cloud address the VPN does not route. |
| Devbox reads the laptop's drive over Remote Desktop (`\\tsclient\c`) | Works. |
| Devbox writes 20.9 MB to the laptop over it | 3.46 s, byte-identical (SHA-256 checked). |

Imran ruled out running a relay server. So slice 1 moves files over the Remote Desktop link the person already has.

## How it works

- **Identity.** Each store gets a random computer id at schema v12 (`sync_self`). A fresh store after "Delete
  everything" is a new computer, so change numbers are never reused.
- **Change files.** Insert triggers queue every row this computer records (`sync_outbox`, AUTOINCREMENT). A pass
  cuts the queue into numbered JSON-lines files (`<aiq home>/sync/<id>/00000001.jsonl`, at most 5,000 lines) and
  marks a file sent only after it is written whole (temp file, then rename).
  A queued row leaves the queue once it is in a written file. While sync has never been on, no queue is kept;
  turning sync on the first time queues every row recorded here, so nothing is missed.
- **The meeting point is the laptop's AIQ home.** The computer inside the Remote Desktop session (the Devbox) copies
  its files into the laptop's `sync/<devbox id>/` and applies the laptop's files from `sync/<laptop id>/`. The
  laptop only reads its own folder. It needs no network, port or relay.
- **Apply exactly once, in order.** `sync_applied (peer, seq)` in the same transaction as the rows. A missing
  earlier file waits; a file cut short or unreadable is an error and is tried again; a file from a newer store
  format is held until this computer updates. Nothing is dropped or guessed.
- **Merge = add what you don't have.** Rows keep their keys (`INSERT OR IGNORE`) and arrive with `computer` set to
  where they came from. Rows that arrived are never sent on. AUTOINCREMENT ids (`card_open`, `tip_feedback`,
  `guidance_flag`) are renumbered locally. An imported session that grew and was re-imported replaces the older
  copy.
- **Deletes follow.** "Delete a prompt" and "forget imported history" write `sync_delete`; the other computer
  repeats them. `aiq forget --all` is this computer only (it says so).

## What merges, what stays

| Merges | Stays on each computer |
|---|---|
| Sessions, prompts, prompt text, turn actions, set-aside sessions, imported history | Settings, connected tools, folder rules (paths are local), update state, logs |
| Analyses, analysis calls, kind labels, type guesses | `import_run`, accuracy-check runs and checks |
| Signals, tip decisions and deliveries, tip opens, tip answers, "This doesn't fit" flags, tip paths | |

## Rules that keep each computer honest

- **Tasks never join prompts from two computers.** Tasks are split per computer, so each computer's task ids are
  exactly what that computer computes, and analyses keyed on them agree.
- **Only the computer that saw a task analyzes it** (`aiq_home::own_tasks`, the judge queue, Home's "waiting"
  count). Nothing is analyzed or paid for twice.
- **Tips:** another computer's ready tips are never offered, delivered or handed back here; detectors never rerun
  on its prompts. Its deliveries do count toward tips a day, the quiet period and the 72-hour same-tip rule.
- **Right now** shows every computer's open tasks (Imran 2026-11-14: a Devbox Copilot task was missing on the
  laptop while the card's last-turn line showed that Devbox turn as the laptop's). Another computer's task carries
  its name and "up to a minute behind"; each tool's last-turn line keeps this computer's own turn and names the
  other computer's. Tips stay with the computer the task runs on. `aiq show` with no id stays this computer's own.

## The experience

- The Devbox, inside a Remote Desktop session, sees `\\tsclient\c\Users\*\.aiq\aiq.db` and Home shows one card:
  "AIQ on LAPTOP is here too" with **Keep in sync** and **Not now**. No code. Nothing is written anywhere before the
  person chooses.
- The laptop turns sync on by itself the first time the Devbox's folder appears (the person chose on the Devbox).
- **Settings > Your computers:** this computer, each other computer with "In sync · 1 min ago", "Sync is off on
  …", "runs a newer AIQ", or "catches up next time you connect", and one **Keep in sync** switch. Off = each keeps
  what it has, and the other side shows "Sync is off on …".
- Tasks from the other computer carry its name as a tag, in the Tasks list and on the task's page. Tasks (All tasks
  and Day) has a Computer filter: All computers, this one, or each other computer. It shows only once another
  computer's task is here.

## Instrumentation

Every pass writes `sync-status.json` (Home reads it) and, while sync is on or offered, one `sync.pass` log line:
setting, computers in reach, peers, rows waiting to be sent, error codes (`AIQ-SYNC-WRITE`, `AIQ-SYNC-APPLY`,
`AIQ-SYNC-READ`), and the pass time. No file content is logged.

## Not in slice 1

- Sync without Remote Desktop. Parked by Imran (2026-11-12) until the Remote Desktop path has been used for a while.
  Options weighed, ranked: (1) encrypted change files in the person's own OneDrive folder, which works for a laptop
  and a Devbox (about two PRs; ciphertext only on OneDrive; not yet checked that the Devbox has the same OneDrive);
  (2) direct pairing with a code on the same network (about three PRs; may raise a firewall prompt; doesn't reach a
  Devbox, whose private address the VPN doesn't route, as measured in the 2026-11-12 spike).
- Preferences that follow you (tips a day, tips turned off).
- "Delete its tasks here" when removing a computer, and "Delete everything on all my computers".
- A split by computer on Home, Progress or Tips (Tasks has the filter).
- Old change files are kept (≤ ~0.2 MB a day); pruning comes with direct pairing.
- Encryption at rest of the store (a lost computer keeps its copy).
