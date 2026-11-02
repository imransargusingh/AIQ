---
id: design-auto-update
title: "Design: auto-update for the installed app"
type: design
status: accepted
owner: "Imran"
phase: "alpha"
tags: [design, alpha, upgrade, release, auto-update, settings, home]
links:
  - { to: design-private-alpha, rel: depends-on }
review-by: 2027-05-02
summary: >-
  AIQ updates itself: once a day the core asks GitHub for a newer published release; upgrade.ps1 downloads
  it, checks its SHA-256, and swaps both exes when AIQ is idle, keeping the old ones for "Go back". On by
  default; Settings has "Update automatically" / "Tell me first"; Home shows one line. One mechanism,
  the existing upgrade.ps1, for aiq.exe and aiq-home.exe. Nothing is signed or published by this change.
---

# Design: auto-update for the installed app

Imran asked on 2026-11-02 whether the installed app can update itself after install. Yes. This is how.

## Goal state

| | |
|---|---|
| **Goal** | An installed AIQ moves to each new published release with no action from the person, and they can see it and undo it |
| **Done when** | A daily check, download, checksum and idle swap run on the Windows runner from a stand-in release; Settings switch and Home line pass the UI tests; CI green |
| **Not in scope** | Signing the exes, publishing a release, pre-release channels, macOS/Linux updates, delta downloads |
| **Tier** | T1 (new behaviour on the person's machine, reversible, no store change) |

## Decisions

| # | Question | Decision | Why |
|---|---|---|---|
| U1 | Tauri updater plugin, or upgrade.ps1 for both exes? | **upgrade.ps1 for both.** | Tauri's updater only replaces AIQ home's own bundle, so `aiq.exe` (the core and the hooks) would need a second mechanism. It also requires every update to be signed with an updater key, and the release builds AIQ home with `--no-bundle` (no installer for it to update). upgrade.ps1 already checks SHA-256 against the release's SHA256SUMS, keeps `.old` copies, and is tested on the Windows runner (PR #60). One mechanism, no new crates. (Tauri facts from its updater docs, not re-checked today: Inferred.) |
| U2 | Who decides when? | The **core**: 5 min after it starts, then hourly, it reads `update.json` and the `update` setting. A check is due 24 h after the last one, whatever its outcome. Under "Update automatically", a downloaded update is retried hourly until AIQ is idle. | The core is the only long-running process. It never waits on the network: it starts upgrade.ps1 hidden and moves on. |
| U3 | What is idle? | AIQ home is closed, and no `aiq.exe` from `bin` runs except `aiq core` (so no hook mid-run, no judge, no import). Decided by the script at the moment of the swap. | Only the script sees the processes when it swaps. The core is stopped by the swap; the next hook starts the new one. |
| U4 | What does the auto swap change? | Only `aiq.exe`, `aiq-home.exe` and `upgrade.ps1` in `bin`. It does **not** rerun `aiq install`, so a tool the person disconnected stays disconnected, and nothing under `~/.copilot` or `~/.claude` is touched (lesson of the 2026-11-02 MCP incident). | Hooks call `bin\aiq.exe` by path, so the new exe is picked up without rewriting them. Residual risk: a release that changes hook entries must refresh them itself. |
| U5 | What does the person see? | Settings, "Updates": **Update automatically, when AIQ home is closed** (default) / **Tell me first**, last check time, **Go back to the previous version**. Home: one line while an update is waiting ("AIQ 0.2.0 is ready… **Install now**"), and for 7 days after one installed, failed or was undone. "Install now" and "Go back" close AIQ home, swap, and open it again. | Imran's rules: AIQ decides, easy override, settings in the app, no manual burden. |
| U6 | CLI | `aiq update` says what the updater last did; `aiq update --now` checks now; `aiq config set update auto\|ask`. | Same words as Home. |
| U7 | Safety | Checksum checked at download **and** again just before the swap. If the swap fails part-way, the exes it changed are put back (only those, so an older `.old` is never swapped in). After "Go back", that release is never installed again by itself; a newer one is. One run at a time (`update.lock`). Runs only for a copy installed by install.ps1 (the exe sits in `%USERPROFILE%\.aiq\bin` next to upgrade.ps1), and the core starts only the shipped script (it carries the line `AIQ-AUTO-UPDATE: 1`), so a hand-written upgrade.ps1 such as the dogfood laptop's dev-build swapper is never run by itself; a developer build never updates itself. | Failure modes below. |

## Data

No store format change. One new setting key, `update` (`auto`/`ask`), in the existing `setting_change`
table. The script writes `%USERPROFILE%\.aiq\update.json` (outside the store): `state` (current, waiting,
applied, failed, rolled_back), `checked_at`, `check_error`, `available`, `staged`, `blocked`, `from`,
`installed_tag`, `at`, `message`, `skip`. A missing or unreadable file reads as "never checked", which only
makes a check due. The download waits in `%USERPROFILE%\.aiq\update\` and is deleted after install.

## Failure modes

| Failure | Detect | Recover |
|---|---|---|
| No published release yet, no network, proxy blocks GitHub | `check_error` in update.json, `AIQ-UPDATE-CHECK` in upgrade-log.txt | Retries in 24 h; Settings says the check didn't reach GitHub |
| Download altered | `AIQ-UPGRADE-CHECKSUM` | Deleted, state failed, exes untouched; next day's check downloads again |
| AIQ home left open for days | `blocked` = "AIQ home is open" | Home line offers Install now |
| Exe in use mid-swap (a hook started in the same second) | `AIQ-UPGRADE-SWAP` | Changed exes put back; retried next hour |
| New version is bad | The person notices | Go back (Settings or Home line), never reinstalled by itself |
| Unsigned exe blocked by SmartScreen/AppLocker on a managed laptop | The first install already meets this (G3) | Guessed, not measured: the same block applies to the first install, so G3 covers it |

## What is measured and what is not

Measured on the GitHub Windows runner (release.yml): a tampered download is refused and changes nothing;
with AIQ home open the update downloads and waits (`blocked`); once idle it installs and keeps `.old` copies;
"Go back" records the release left and a later check does not reinstall it. Measured in CI on Linux: the
schedule rule, the state file reader (with PowerShell's byte-order mark), the CLI words, and the Settings and
Home UI (end-to-end, axe). **Not measured:** a real update between two published releases (needs two
releases, G4), the hourly kicker on a real laptop, behaviour behind a corporate proxy.
