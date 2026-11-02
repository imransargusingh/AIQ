---
id: design-private-alpha
title: "Design: private alpha for Copilot users (install, upgrade, first run, export, alpha gates)"
type: design
status: accepted
owner: "Imran"
phase: "alpha"
tags: [design, alpha, install, upgrade, release, copilot-cli, first-run, export, privacy]
links:
  - { to: aiq-architecture, rel: implements }
  - { to: spec-coaching-loop, rel: implements }
  - { to: spec-aiq-home, rel: implements }
  - { to: design-aiq-home-deferred-pages, rel: depends-on }
  - { to: design-copilot-vscode-slice, rel: depends-on }
  - { to: design-rung-judge, rel: depends-on }
  - { to: design-coaching-from-rungs, rel: depends-on }
  - { to: design-run-or-ship-pack, rel: depends-on }
  - { to: design-judge-accuracy-check, rel: depends-on }
review-by: 2027-05-02
summary: >-
  What it takes for someone other than Imran to install and use AIQ. A tagged release builds one zip
  with aiq.exe and aiq-home.exe; one per-user install command hooks the AI tools it finds (Copilot
  first), imports history and opens setup; `aiq upgrade` replaces the hand-made upgrade script; `aiq
  export` fills the spec's missing privacy control. Named go/no-go checks decide when the alpha opens.
---

# Design: private alpha for Copilot users

Produced with AI-Forward `/design-slice` on 2026-11-02 and parked while the accuracy check ran
(PR #31 closed, kept on a branch). Brought onto main on 2026-11-02 with a gate review against the
current code and the measured numbers: see **Gate review (2026-11-02, evening)** below. The build
follows in small PRs, one per missing piece.

## Goal state

| | |
|---|---|
| **Goal** | A Copilot user who is not Imran can install AIQ in one step, get coached, upgrade it, export and delete their data, and remove it cleanly |
| **Done when** | (design) this doc is merged with its gate record; (build) a tagged release installs on a clean Windows user account, every alpha gate below reads pass, and the uninstall test leaves no AIQ hook or shortcut behind |
| **Not in scope** | macOS/Linux packages; MSI, winget or code signing (flagged, see A2); signed pack CDN (Phase 4); opt-in aggregates; the question/research pack (separate card); quiet hours (A9); measuring judge accuracy (runs in parallel, not part of this slice) |
| **Tier** | T2 (writes into other tools' settings on someone else's machine; downloads and runs executables) |

## Grounding: what this design must satisfy

- Architecture, Delivery phasing: Phase 6 "Status page, pause, export, delete (privacy controls)"
  unblocks **Private alpha**. Status page, pause and delete exist (Phase 3, deferred pages). **Export
  does not**: `crates/aiq/src/main.rs` has no `export` command (checked 2026-11-02, `9281456`).
- Spec coaching-loop NFR Privacy: "All data on device … capture log viewable; pause on every surface;
  full export and delete".
- Architecture context: "nothing leaves the device in normal operation except pack and update checks".
  So an update check is allowed. Nothing else is added to the network.
- Architecture risks: Anthropic terms review "owed before AIQ is given to others"; Copilot terms
  "confirm against current product terms before alpha".
- Imran (memory, 2026-11-02): target users almost always use Copilot; prefer AIQ deciding with an easy
  override over asking users to triage.
- Today's install path (checked): build from source (README "Build"), or CI artifacts `aiq-windows-x64`,
  `aiq-home-windows-x64`. No release workflow exists (`.github/workflows/` holds only `rust.yml`,
  `home.yml`). The upgrade that works on Imran's laptop, `%USERPROFILE%\.aiq\bin\upgrade.ps1`, **is not
  in the repo** (grep found no copy).
- The repo is **public** (GitHub search API, 2026-11-02), so release files can be downloaded without
  a GitHub sign-in.

## Drift found

| # | Drift | Settled here |
|---|---|---|
| R1 | Spec aiq-home H11 and the setup design say the Copilot judge uses VS Code's language model API. The built judge uses Copilot CLI `copilot -p` (Imran's decision, rung-judge design) | Setup's judge step follows the build: Copilot CLI first. Spec H11 gets a review-suggested flag |
| R2 | Setup's judge step is Claude-first ("Claude Code signed in?") | Copilot-first: check Copilot CLI sign-in first; Claude only if Claude Code is found |
| R3 | Architecture "Best next action" still names the 3b design, which is done | Status table updated in this change |

## Decisions (reversible unless marked)

| # | Question | Decision | Why |
|---|---|---|---|
| A1 | How does AIQ reach people? | A **GitHub Release** per version tag `v*`: `aiq-<ver>-windows-x64.zip` (aiq.exe, aiq-home.exe, `install.ps1`, `LICENSE`, `README.txt`) plus `SHA256SUMS`. Built once by a new `release.yml` from the same jobs CI already runs | Rung 2 of the ladder: reuse the CI builds. The repo is public, so no account is needed to download |
| A2 | MSI, winget, code signing? | **Not for the private alpha.** Per-user zip, no admin rights | Signing needs a certificate (cost, identity). `assume:` an unsigned exe downloaded from a release runs on an alpha user's work laptop. Confirm: run the zip on Imran's laptop and on one other machine at the alpha gate. If false: SmartScreen or AppLocker blocks it, and signing becomes this slice's first task |
| A3 | What does install do? | `install.ps1` (and the same steps behind `aiq setup --yes`) copies both exes to `%USERPROFILE%\.aiq\bin`, adds that folder to the **user** PATH, adds a Start menu shortcut for AIQ home, then **installs hooks for every tool it finds**: Copilot CLI first, then Claude Code. It prints each file it changed, where the backup is, and the one undo command. Then it opens AIQ home on Setup | Imran's rule: AIQ decides, easy override. **Decided by Imran 2026-11-02** ("Why isn't this done automatically when AIQ is installed?"). The connect step itself is already on its way to main ahead of the alpha (PR #32): `aiq install` with no tool connects every tool found, and AIQ home's first-run setup runs it, shows "connected just now" and names the undo; run setup again offers Connect instead of reconnecting by itself. The alpha install script reuses `aiq install --yes` |
| A4 | History on day one? | Install starts `aiq import` in the background (automated sessions set aside as today). Background judging stays at 50 a day | A new user otherwise sees an empty Home for days. Import and the automated rule are already measured on Imran's data (46 s, no false alarms) |
| A5 | How does someone upgrade? | `aiq upgrade`: asks GitHub's release API for the latest tag, downloads the zip, **checks SHA-256 against `SHA256SUMS` from the same release**, stops `aiq core` and AIQ home, swaps the exes, keeps `.old` copies, and runs the store migration on next start. `aiq upgrade --rollback` puts the `.old` copies back. `aiq status` shows "Update available" at most once a day. `aiq config set update-check off` turns the check off | Moves the hand-made `upgrade.ps1` into the product. The update check is the one network call the architecture allows |
| A6 | Export format? | `aiq export <folder>` and a button on Your data: one folder with `events.jsonl`, `prompts.jsonl` (text), `judgments.jsonl`, `cards.jsonl`, `feedback.jsonl`, `settings.json`, and `README.txt` that says what each file holds. Files are user-only. Secrets are already replaced in the store, so the export holds none | Spec "full export". Plain JSON lines are readable without AIQ and match the append-only facts one to one |
| A7 | Uninstall? | `aiq uninstall --all` removes AIQ's hooks (both tools), the status line it added, the shortcut and the PATH entry. **Data stays** unless the person also runs `aiq forget --all`; the command says so | Removing software and deleting data are different choices. Never delete data as a side effect |
| A8 | Help when something breaks? | `aiq status --report` writes a text file with versions, hook timings, counts, error codes and settings. **No prompt text, no folder names** (folders shown as `folder-1`, `folder-2`). The person reads it and decides whether to send it | Architecture SRE gate: "field debugging relies on the person sharing a help packet". It does not exist yet |
| A9 | Quiet hours (architecture privacy list)? | **Deferred.** Pause, the daily cap (5) and the 30-minute quiet period exist | YAGNI until an alpha user asks. Flagged |
| A10 | Claude Code users in the alpha? | Allowed, with the setup notice that judge calls use their Claude plan. The Anthropic terms check (gate G6) must pass before inviting anyone who uses Claude Code; Copilot-only users do not need it | Target users are Copilot users. This keeps the Claude question off the critical path |

## Alpha gates (go/no-go before the first invite)

Each gate is checked by a command or test, and the result is recorded with "measured" or "not run".

| # | Gate | Pass when | Today |
|---|---|---|---|
| G1 | Hook speed | `aiq status` on Imran's laptop: AIQ's own part of each hook ≤ 100 ms p95 | **Fails** (measured 2026-11-02: UserPromptSubmit p95 521 ms over 249 runs). The judge thread is looking at it |
| G2 | Copilot hooks live | After install, `aiq status` shows Copilot hooked and a live Copilot prompt shows up in Home | **Fails** on Imran's laptop (measured: "not installed"). The judge thread is looking at it |
| G3 | Clean install | Release zip installs, coaches and uninstalls on a Windows account that never had AIQ | Not run |
| G4 | Upgrade | `aiq upgrade` from the previous release keeps the store and settings; `--rollback` works | Not run |
| G5 | Copilot terms | Current GitHub Copilot terms read for running `copilot -p` as the judge on the user's own sign-in; finding recorded | Not run (architecture risk) |
| G6 | Anthropic terms | Only for Claude Code users (A10) | Not run |
| G7 | Coverage | A pack covers question and research work | **Done** (PR #33 merged 2026-11-02: Ask or research pack). Before it, 9 of Imran's first 10 judged tasks (measured) had no pack |
| G8 | Self-check | The task-kind self-check (auto-off below 8 of 10, built in PR #29) is on by default | Built; its accuracy on real tasks is **not measured** |
| G9 | Harmless to other tools | Connecting AIQ changes nothing another tool depends on. VS Code and the Copilot app read `~/.copilot/hooks/` **and** `~/.copilot/settings.json` (VS Code agent hooks docs). Pass when, on a machine with MCP servers set up in VS Code and Copilot CLI, every server's state is the same before and after connect and after uninstall, and connect lists every file it changed | **Open.** Imran's VS Code Insiders MCP trouble began 10 min after setup auto-connected Copilot (measured times; cause not proven, reversible test held by Imran). PR #35: setup never turns on Copilot's experimental mode by itself and lists the files it changed |

## Gate review (2026-11-02, evening)

Read against main at `ae64a72` (PR #52) and the numbers measured on Imran's laptop today. The
"Today" column in the gate table above is the morning reading; this table replaces it.
**Measured** means read from `aiq status`, `aiq judge --check` or `aiq tips` output on the laptop.
**Checked** means read in the code on main. **Inferred** means neither.

| # | Gate | Verdict | Evidence | What is missing |
|---|---|---|---|---|
| G1 | Hook speed | **Not proven** | Measured after PR #34 and #39 (Claude Code): prompt hook about 1.7 ms typical, AIQ's preparing phase at most 0.6 ms on every kept prompt since 15:14Z. The 24-h p95 that `aiq status` still prints (495 ms at 18:56Z) is the "reading input" phase, the host's own stdin wait, plus lines from before the fix and from automated runs. Checked: `aiq status` prints the p95 by phase (`cli.rs`, `hook_phase_lines`) | Read the **Hook budget** lines in `aiq status`. PR #54 leaves automated runs out, and the budget lines count AIQ's own part (total less reading input) since the last upgrade in `upgrade-log.txt`, so there is no wait for pre-fix lines to age out. G1 passes when every event there, Copilot's included, is within 100 ms: the alpha is Copilot-first and no Copilot hook p95 is recorded yet |
| G2 | Copilot hooks live | **Likely met** (Inferred) | Measured: Copilot was connected on the laptop at 04:46Z. Not read today: a live Copilot prompt arriving in Home after a fresh install | Covered by G3 on a clean account. No separate work |
| G3 | Clean install | **Built; needs a human run** | Built (PR #58, #61): `release.yml` makes the zip and `SHA256SUMS`; `install.ps1` installs per user. Measured on the GitHub Windows runner, every packaging PR: install from the zip puts both exes in `bin`, adds the user PATH entry and shortcut, connects Copilot (hook self-test 13 to 14 ms on the runner), and `uninstall --all` takes every one of those back | A first release (Imran's call), then one run on a Windows account that never had AIQ, ideally a managed work laptop (tests the unsigned-exe assumption A2) |
| G4 | Upgrade | **Built; needs a human run** | Built (PR #60) as `upgrade.ps1`, not a Rust `aiq upgrade` (native PowerShell instead of three new crates). Measured on the Windows runner: a tampered zip is refused with `AIQ-UPGRADE-CHECKSUM` and `aiq.exe` is unchanged; a good zip upgrades; `-Rollback` restores the previous exes. Since PR #69 AIQ also updates itself: a daily check, the same checksum, a swap only when AIQ is idle, a Settings switch ("Update automatically" / "Tell me first") and a Go back button (design `docs/design/auto-update.md`) | Two published releases, then on a real machine with data: do nothing and confirm AIQ updated itself (auto-update, PR #69), that the store and settings survive, and that "Go back to the previous version" works |
| G5 | Copilot terms | **Met** (decided by Imran, 2026-11-02 21:32Z) | Imran: running the judge through the user's own Copilot sign-in "is not an issue/concern" | Nothing |
| G6 | Anthropic terms | **Not needed for a Copilot-only alpha** | A10: needed only before inviting someone who uses Claude Code | Skip unless Imran wants Claude Code users in the first wave |
| G7 | Coverage | **Met** | Measured: four packs now (Code a change, Write a spec or doc, Ask or research, Run or ship it, PR #33 and #52). Before the fourth pack, 101 of 473 judged tasks had no job; the survey of 30 of them found 17 were running or operating systems, which the fourth pack now covers (one model's labels, Inferred) | How many tasks still get no job under judge j5 is not measured yet; re-read after the laptop re-judges |
| G8 | Self-check | **Met** | Checked: `next-rung-tips` defaults to `auto`, under which the self-check turns next-step tips off below 8 of 10 (`coach.rs`, test `self_check_turns_tips_off_at_seven_of_ten`; PR #29). Measured: where the judge named a kind of task, a different-model reader agreed 27 of 27; all 6 misses in 35 were the fallback "Other", which gets no tip | Nothing for the gate. Live kind accuracy after tips fire is a later measurement |
| G9 | Harmless to other tools | **Dropped** (Imran, 2026-11-02 21:33Z) | Since PR #35, connect adds only AIQ's own hook file and leaves Copilot's settings alone unless experimental mode is already on, and lists every file it changed | Nothing. Residual risk accepted: the cause of the 2026-11-02 VS Code MCP trouble is still unproven |

Pieces built since this review was written (all merged, one PR each): `aiq export` (A6, PR #55),
`aiq status --report` (A8, PR #56), `aiq uninstall --all` (A7, PR #57 and #61), `release.yml` and
`install.ps1` (A1, A3, PR #58), `upgrade.ps1` (A5, PR #60). Auto-update with AIQ home's update line and Go back button (PR #69) replaces the
once-a-day "Update available" line. Still not built: the AIQ home "Export everything" button, and `aiq setup --yes` as a separate
command (`install.ps1` runs `aiq install --yes`, which does the same job). A tag makes only a
**draft** release; **publishing the first release is Imran's call**, because the repo is public and
anyone could then download it.


## Responsibility

One job: make AIQ installable, upgradable, exportable and removable by someone other than its
developer. It adds no coaching behaviour.

## Data model

No new stored facts and no schema change. Export reads the existing tables (events, content,
judgments, cards, feedback, settings); one exported line is exactly one stored row, so nothing is
summed or derived. Install and upgrade write only files outside the store: `bin\`, `install.json`
(what install changed, so uninstall puts back exactly that; same pattern as Copilot's existing
record), and `update-check.json` (last check time and latest tag seen; a cache, safe to delete).

## Change surfaces (E7)

| Surface | Change |
|---|---|
| CI | `release.yml` on tag `v*`: reuse the Windows build jobs, zip, `SHA256SUMS`, publish the release |
| Binary / CLI | `aiq setup [--yes]`, `aiq upgrade [--rollback]`, `aiq export <folder>`, `aiq uninstall --all`, `aiq status --report`, `update-check` setting |
| Core paths | `install.json`, `update-check.json` in the data folder |
| Store | Read-only export iterator per table (no schema change) |
| Shell (Tauri) | Command `export(folder)`; capability allow-list grows by one |
| Client type | `types.ts`: `update_available`, export result |
| UI | Setup step 2 shows what install hooked and the undo command; step 3 Copilot-first (R2); Export on Your data; "Update available" line on Status |
| Docs | README "Install" for users first, "Build" second; `install.ps1` |

## Contracts consumed

| Contract | Source | Confidence |
|---|---|---|
| GitHub Releases API `GET /repos/{owner}/{repo}/releases/latest` and asset download, no auth for a public repo | GitHub REST docs | Verified by reading; **spike at build** (rate limit 60/hour unauthenticated: one check a day is far under it) |
| `actions/upload-artifact`, release publishing in Actions | GitHub Actions docs | Verified by reading; spike at build |
| User PATH change without admin (`HKCU\Environment`, then broadcast `WM_SETTINGCHANGE`) | Windows docs | Inferred; spike at build |
| Copilot CLI and Claude Code settings files | Existing installers (Phase 1, 2) | Verified by running (laptop runs) |

## Failure-mode analysis

| Mode | Disposition |
|---|---|
| Download corrupted or swapped | **Prevent**: SHA-256 check against `SHA256SUMS`; mismatch keeps the old exes, error `AIQ-UPGRADE-CHECKSUM` |
| Upgrade fails half way (exe locked) | **Recover**: stop processes first, swap by rename, keep `.old`; on any failure put `.old` back; error `AIQ-UPGRADE-SWAP` |
| New build can't open the store | **Detect**: clear error (PR #26 pattern); **recover**: `aiq upgrade --rollback` |
| Offline or GitHub down at update check | **Degrade**: say nothing; try again tomorrow |
| Tool not installed or settings file invalid JSON | **Prevent**: skip that tool, say why; existing installers already refuse invalid JSON |
| Another status line already in Copilot settings | **Mitigate**: existing rule, never replace; print the line to add |
| Export folder not writable or disk full | **Detect**: error `AIQ-EXPORT-WRITE`; partial folder deleted |
| Export while core writes | **Prevent**: export reads in one read transaction (SQLite snapshot) |
| Unsigned exe blocked by the OS | **Detect** at gate G3; **accept** for now (A2 assumption) |
| Uninstall leaves hooks behind | **Prevent**: `install.json` records every change; uninstall test checks each file |

## Adversarial analysis (STRIDE-lite)

| Boundary | Threat | Disposition |
|---|---|---|
| Release → device | **S/T**: a fake or tampered zip | **Mitigate**: HTTPS to github.com only, SHA-256 from the same release. **Accept** (residual): someone who controls the GitHub repo can publish both a bad zip and a matching checksum. Signing (A2) would close this |
| Release → device | **E**: install needs admin | **Prevent**: per-user only; install refuses to run elevated |
| Install → other tools' settings | **T**: AIQ overwrites a user's hooks or status line | **Mitigate**: existing rules (one AIQ-owned hook file, diff, backup, never replace someone else's status line) |
| Help report | **I**: report leaks prompts or folder names | **Mitigate**: report built from counts only; test plants a canary prompt and folder name and checks neither appears |
| Export folder | **I**: export readable by other users | **Mitigate**: user-only file mode / ACL, same as the data folder |
| Update check | **I**: the check tells GitHub that someone runs AIQ | **Accept**: allowed by the architecture; it sends no user data; can be turned off |

## Privacy analysis (LINDDUN-lite)

| Threat | Disposition |
|---|---|
| Disclosure through export or report | Export holds the person's own data on their own disk, by their action; report holds no content (tested) |
| Detectability through the update check | GitHub sees an IP and a download. Accepted, off switch (A5) |
| Unawareness | Install prints every change; setup step 1 is the privacy promise; README says what leaves the device (only the update check and judge calls through the person's own tools) |
| Non-compliance (rights) | Export (A6) gives the access right; `aiq forget --all` the delete right; uninstall never deletes data silently |

## UI design

Reuses `DESIGN.md` tokens and the existing Setup and Your data screens. New copy:
- Setup step 2: "AIQ hooked Copilot CLI and VS Code agent mode. Changed: `~/.copilot/hooks/aiq.json`,
  `~/.copilot/settings.json` (backup kept). Undo: `aiq uninstall copilot`."
- Your data: button "Export everything". Done state: "Exported to `<folder>`. 6 files. Only you can
  read them." Error state: "Couldn't write to `<folder>`. Nothing was exported."
- Status: "AIQ 0.x.y is available. Run `aiq upgrade`." (shown at most once a day).
States per new control: default, focus, busy, done, error; keyboard and axe checks as in Phase 3.

## Telemetry (local only)

Upgrade, export and install each write one local log line with version, duration, result and error
code. `aiq status` shows the installed version, last upgrade and last update check. No remote
telemetry.

## Test plan

- D0 hygiene: fmt, clippy, existing suites.
- Unit: checksum match and mismatch; release-tag compare; report redaction.
- Integration (Windows CI): install into a temp `USERPROFILE`, then uninstall; every changed file goes
  back byte for byte. Upgrade from a fake previous build, then rollback.
- Export: round trip row counts per table equal the store's; secret canary absent; export during
  writes is consistent.
- Negative security: tampered zip refused; elevated install refused; report canary.
- UI e2e: Export button states, Setup step 2 copy, update line; axe 0 violations, three themes.
- Human (gate G3): clean Windows account, release zip, one Copilot prompt, tip, uninstall.

## Flagged risks

- A2: unsigned exes may be blocked on managed laptops. Gate G3 tests it.
- G1 and G2 failed on the morning of 2026-11-02 (measured). Fixes landed since; both are re-read per the gate review. The alpha cannot open until they pass.
- G7: without a pack for questions and research, most of a typical Copilot user's tasks may get no
  coaching. This is inferred from Imran's 10 tasks only.
- Judge and task-kind accuracy on real tasks are unmeasured. The self-check limits the damage; the
  accuracy check runs in parallel.

## Gate record

`GATE design-slice · 2026-11-02 · Patterns, Simplifier, Security, SRE, Test Architect, Privacy (all
seats played by Claude in one session; not independent) · verdict: pass with conditions`
- Simplifier: MSI/winget/signing, quiet hours and an auto-updater that runs by itself were cut; a
  manual `aiq upgrade` plus a once-a-day notice is the smallest correct path.
- Security (hard): checksum from the same release does not protect against a compromised repo;
  accepted for a private alpha, recorded, signing is the upgrade trigger. Not self-cleared.
- Test Architect: install/uninstall round trip and upgrade rollback must run on Windows CI, not
  only Linux.
- Caveat: independent review recommended before build.

## Status and next action

| | |
|---|---|
| **Completed** | Design (2026-11-02, morning); gate review against main `ae64a72` (2026-11-02, evening); build of A1, A3, A5, A6, A7, A8 (PR #55 to #61) |
| **Remaining** | G1 Copilot hook p95 read on the laptop; first release (Imran); G3 and G4 human runs |
| **Best next action** | Imran decides whether to publish a first release (v0.1.0) so G3 can run |
