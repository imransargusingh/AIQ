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
| U2 | Who decides when? | The **core**: 5 min after it starts, then hourly, it reads `update.json` and the `update` setting. A check is due 24 h after the last good one, or about an hour (55 min) after one that failed (U5c). Under "Update automatically", a downloaded update is retried hourly until AIQ is idle. | The core is the only long-running process. It never waits on the network: it starts upgrade.ps1 hidden and moves on. |
| U3 | What is idle? | AIQ home is closed, and no `aiq.exe` from `bin` runs except `aiq core` (so no hook mid-run, no judge, no import). Decided by the script at the moment of the swap. | Only the script sees the processes when it swaps. The core is stopped by the swap; the next hook starts the new one. |
| U4 | What does the auto swap change? | Only `aiq.exe`, `aiq-home.exe` and `upgrade.ps1` in `bin`. It does **not** rerun `aiq install`, so a tool the person disconnected stays disconnected, and nothing under `~/.copilot` or `~/.claude` is touched (lesson of the 2026-11-02 MCP incident). | Hooks call `bin\aiq.exe` by path, so the new exe is picked up without rewriting them. Residual risk: a release that changes hook entries must refresh them itself. |
| U5 | What does the person see? | Settings, "Updates": **Update automatically, when AIQ home is closed** (default) / **Tell me first**, last check time, **Go back to the previous version**. Home: one line while an update is waiting ("AIQ 0.2.0 is ready… **Install now**"), and for a day after one installed, failed or was undone (U5g). "Install now" and "Go back" close AIQ home, swap, and open it again. | Imran's rules: AIQ decides, easy override, settings in the app, no manual burden. |
| U5b | Can the person stop the check? (Imran, 2026-11-02 23:03Z) | **Yes: a third choice, Don't check for updates** (`update` = `off`). The core then never starts the script; **Check for updates now** in Settings (or `aiq update --now`) checks once and downloads without installing, and Home's line offers Install now. The running version shows at the foot of AIQ home's side panel on every page and in Settings, Updates. | With checks off AIQ makes no network call of its own. The person still needs to know what they run and how to get a newer one. |
| U5c | Can a person see a new build before auto-update installs it? (Imran, 2026-11-03 20:28Z) | **Yes, within 15 minutes** (was the hour; Imran, 2026-11-05 19:05Z, after 0.1.8 published at 18:30Z waited until the 18:58Z check). While AIQ home is open it asks the shell on open, every 15 minutes and on focus (`run_update check_if_due`); the shell starts a check (download, no install) when `home_check_due` says so: 13 minutes since the last check (`HOME_POLL_MS`), updates not off, nothing already waiting. The check reads the releases/latest redirect, not the rate-limited API; the core's own daily check is unchanged. Home's existing line then says "AIQ 0.1.1 is ready. It installs by itself when you close AIQ home." with **Install now** (closes Home, swaps, opens it again). A failed check is retried after an hour, not a day. Settings offers **Check for updates now** under every choice; `aiq status` names a waiting update; `aiq update --now` installs one already downloaded instead of fetching it again. | Measured on the laptop 2026-11-03: the only check ran at 04:20Z, got 404 (no release yet) and waited 24 h, and Home stayed open from 04:14Z, so 0.1.1 (published 18:17Z) never showed. Auto-update stays the default; the person only gets an earlier, optional click. |
| U5d | Where does the check ask? (2026-11-04, laptop 403) | **The `github.com/<repo>/releases/latest` redirect, then the API only as a fallback.** A HEAD of that page answers 302 to `.../releases/tag/<tag>`; the zip and SHA256SUMS are fetched from `releases/download/<tag>/` (the release workflow fixes their names). `api.github.com/repos/<repo>/releases/latest` is asked only when the redirect names no tag. A 403/429 from it writes `retry_at` (from `X-RateLimit-Reset` or `Retry-After`) to update.json; the core's next check waits for that time instead of the hour, and the kicker wakes a minute after it. | Measured 2026-11-04: the laptop's address had used GitHub's 60 unauthenticated API calls an hour (403, Remaining 0) while release downloads worked; from the cloud, a HEAD of releases/latest answered 302 to `.../tag/v0.1.1` and both asset URLs answered 302 to the asset store. Inferred, not measured: that the web redirect is not limited per address the same way on the laptop's office network. Only an upgrade.ps1 shipped with the fix benefits; a 0.1.0 install keeps its copy until it updates once. |
| U5g | The "updated itself" line (Imran, 2026-11-04: "why is this taking up prime real-estate and not dismissible?") | **A slim line, not a banner; a × dismisses it; it shows on one Home session only and for a day at most. Nothing shows, and no Go back, when the version before equals the version now.** The core also stops offering a waiting release that is not newer than the running one. (upgrade.ps1 skipping such a release at the swap is U5f.) | Seen on the laptop: a stale waiting 0.1.2 was applied over the hand-installed 0.1.2 at ~04:17Z; Home then said "updated itself to 0.1.2" with "Go back to 0.1.2", for a week, at the top of Home. |
| U5h | Seeing and starting a pending update (Imran, 2026-11-04: "I need an option in the app that allows the user to notice that there is an update pending and be able to initiate the update without waiting for auto-update") | **A waiting update is an "Update to x.y.z" marker in the titlebar on every page (replacing the Home banner); one press installs (the U5 path: -Apply -WaitPid -Relaunch). "Update now" beside the version in the nav, in Settings > Updates (replacing "Check for updates now") and in the command palette checks with AIQ home open, shows each step on screen (Checking…, Installing x.y.z, You have the newest, Couldn't reach GitHub), ignores a second press, and installs as soon as the download is checked. Home checks on open when the last check is 10 minutes old (hourly timer and focus stay at 55).** No change to upgrade.ps1, so it works with the script already installed. | Laptop 2026-11-04 14:39Z: "Check for updates now" showed nothing on screen, was pressed twice and downloaded 0.1.3 twice; the ready line showed only on the Home page; a Home opened after a 14:17Z check would not see 0.1.3 (out 14:33Z) until ~15:12Z. |
| U5i | A ready update beside the version, bottom left (Imran, 2026-11-05: "Is there a way to provide that indication in the same general area where the update link is shown at the bottom left?") | **While an update waits, a small box replaces the nav foot's version line: a dot and "AIQ x.y.z is ready", then "You have a.b.c" and a filled Update button; one press installs. Nothing waiting: "AIQ a.b.c · Update now" as before.** Imran picked option B of three rendered options (/mnt/project-files/update-indicator/). | Laptop 2026-11-05: Imran saw only "Update now" by the version and could not tell an update was ready; the titlebar chip is the one marker and sits away from the version. |
| U5e | A person installs with install.cmd while an update waits (2026-11-04, laptop) | **install.ps1 drops a waiting release that is this version or older** (state `current`, no `available`/`staged`/`blocked`, `.aiq\update` removed, `AIQ-INSTALL-CLEARED-WAITING` logged). A newer waiting release stays. | Seen on the laptop: 0.1.2 staged by 0.1.1's check, then installed with install.cmd; update.json still said waiting. Tested on the Windows runner (release.yml). |
| U5f | A waiting release that is not newer, and the core after a swap (2026-11-04 04:17Z, laptop) | **`-Apply` drops a staged release that is not newer than the installed one** before stopping anything (state `current`, `.aiq\update` removed, `AIQ-UPDATE-NOT-NEWER`), so the `.old` copies and Go back survive. **After a swap or rollback the script starts `aiq core` again** when it stopped one (`AIQ-UPDATE-CORE`). | Measured on the laptop: Home's Install now swapped 0.1.2 over 0.1.2 and lost the 0.1.1 `.old` copies; the core stayed down 04:17:05Z to 04:22:53Z. Tested on the Windows runner (release.yml). |
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
| No published release yet, no network, proxy blocks GitHub | `check_error` in update.json, `AIQ-UPDATE-CHECK` in upgrade-log.txt | Retries within the hour (U5c); Settings says the check didn't reach GitHub |
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
