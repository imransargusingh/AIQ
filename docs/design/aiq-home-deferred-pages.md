---
id: design-aiq-home-deferred-pages
title: "Design: AIQ home deferred pages (task detail, first-run setup, delete one item)"
type: design
status: draft
owner: "Imran"
phase: "3"
tags: [design, aiq-home, setup, task-detail, delete, privacy, accessibility]
links:
  - { to: design-aiq-home-app, rel: depends-on }
  - { to: spec-aiq-home, rel: implements }
  - { to: design-language, rel: depends-on }
review-by: 2027-04-30
summary: >-
  Adds the three AIQ home screens Phase 3 deferred: a task detail page (the task's own prompts, and
  honest "not judged yet" places for the rung, facts and next step), a first-run setup screen (privacy
  promise, tools found, judge, tips a day, lost-machine notice), and delete one item on Your data. Delete
  one removes the prompt's text from the store, its WAL, freed pages and the core's memory.
---

# Design: AIQ home deferred pages

Produced with AI-Forward `/design-slice` (v2026.10.19.4). Builds on `docs/design/aiq-home-app.md`
(Phase 3, D1 deferred these three screens) and implements `docs/specs/aiq-home.md` US-H1, US-H4 (the
parts that need no judge) and US-H6 (delete one item).

## Goal state

| | |
|---|---|
| **Goal** | In AIQ home the person can open one task and see its prompts, is walked through setup on first run, and can delete one captured prompt |
| **Done when** | Rust tests prove the task detail read, the setup flag, and that a deleted prompt's text is gone from the store file, its WAL and the core's memory; the UI e2e covers each screen and its states with 0 axe violations in light, dark and high contrast; CI green |
| **Not in scope** | Anything the rung judge feeds (rung, facts, why, next step, starter prompt, "This doesn't fit"): placeholders only. Installing hooks from the app. Starting preset (no presets exist yet). Delete older than a date. Deleting one imported prompt. Laptop check (another thread is using the laptop) |
| **Tier** | T2 (a new write path that deletes the person's data) |

## Decisions

| # | Question | Decision | Why |
|---|---|---|---|
| E1 | What does "delete one item" delete? | The prompt's **text** (the `content` row). The event row stays: that a prompt was sent, when, from which tool and folder, and its length. Your data then shows "Text deleted", and card evidence says the evidence was deleted (already built in `aiq show`) | `event` is an append-only fact table by design, and task counts must stay true. The text is the person's content (ADR-0002: "raw prompts stay until the person deletes them"). The confirm dialog says exactly what stays |
| E2 | Is the deletion real? | Yes, at every place the text lives: the row is deleted with `secure_delete` on (freed pages are zeroed), the WAL is checkpointed and truncated, and the core rebuilds its detector working set so the text leaves its memory | "Deletion must be real" (project rule). SQLite otherwise keeps deleted bytes in free pages and in the WAL |
| E3 | Who writes the delete? | The core, through a new local API frame `ForgetItem`. When the core is not running, AIQ home opens the store and deletes directly, the same way delete everything deletes files when the core is down | The core is the single writer and holds prompt text in memory. With the core down nobody else writes, and the person isn't made to start it first |
| E4 | Which items can be deleted one by one? | Live prompts listed on Your data that still have text | Imported prompts are copies of the tool's own history file, which AIQ doesn't own; deleting AIQ's copy would come back on the next import of a changed session. `aiq forget --imported` already deletes all of them. *Flagged* as a follow-up |
| E5 | How does AIQ home know setup ran? | A `setup-done` flag file in the data folder (user-only), written by AIQ home, read into `status.setup_done` | Setup must finish on a fresh install, before the core has ever started (the core starts with the first prompt). A core setting would make setup fail exactly then. It is app state, not the person's data, so delete everything keeps it |
| E6 | What does setup ask? | Five steps, each with Back: (1) the privacy promise, (2) the tools AIQ found, (3) the rung judge, (4) tips a day, (5) the lost-machine notice with "I understand", which enables Start | Spec US-H1 order. The judge does not exist yet, so step 3 says what it will do and that there is nothing to connect now (design G2). Tips a day is written through the core; with the core down the step says the default (5) stays and Settings can change it later |
| E7 | How does setup find tools? | `aiq_core::tools`: Claude Code hooked (AIQ's entries in the Claude Code settings file), Copilot hooked (`hooks/aiq.json` in the Copilot home; VS Code agent mode uses the same hooks). Not hooked shows the one `aiq install …` command | One definition shared with the CLI's install and status (moved from the `aiq` crate, not copied) |
| E8 | Where is task detail? | `#task/<id>` from the Tasks page (Enter or click on a row). It shows when, tool, folder, prompts, imported or live, the prompts in order, and the judge sections with "Not judged yet" | Spec US-H4, minus what needs the judge. The rung judge thread fills the judge sections later in the same places |
| E9 | How is the detail read? | A new command `task(task_id)`: the same task read as the snapshot (one shared function), then the text of that task's prompts | One Task rule; the snapshot stays small (no prompt text per task) |

## Change surfaces (E7)

| Surface | Change |
|---|---|
| Store | `Store::forget_content(event_id)`: secure delete, then WAL checkpoint (TRUNCATE). No schema change |
| Domain | `TaskSpan.prompt_ids` (member prompts in order) |
| Contract | `Request::ForgetItem { token, event_id }` |
| Service / core | `Service::forget_item`: delete, then `rebuild()`; writer `Cmd::ForgetItem` |
| Control | `control::forget_item` (core, else direct); `control::setup_done` / `finish_setup` |
| Tools | `aiq_core::tools` (moved from `aiq` install code) |
| Projection | `Snapshot.status.setup_done`, `Snapshot.setup.tools`; `TaskDetail` |
| Shell | Commands `task`, `forget_item`, `finish_setup`; capability allow-list grows to seven |
| Client type | `types.ts` (`TaskDetail`, `ToolFound`, `setup_done`) |
| UI | Task detail page, Setup screen, Delete on Your data rows, "Run setup again" in Settings, palette entry |

## Failure-mode analysis

| Mode | Disposition | Test |
|---|---|---|
| Delete an id that isn't a prompt with text | Refused with a sentence; nothing changes | `forget_item_unknown_is_refused` |
| Core down at delete | Direct delete through the store | `forget_item_without_core_deletes` |
| Core running at delete | Text gone from the store and from the engine's memory | `forget_item_rebuilds_engine` (service test) |
| Deleted bytes left on disk | `secure_delete` + WAL truncate; test scans the raw files for the text | `forgotten_text_is_not_in_any_file` |
| Task id not in the window | Detail says the task is no longer listed; back to Tasks | e2e |
| Setup with core down | Tips step keeps the default and says so; Start still works | e2e |
| No tools hooked | Says which command adds each; Continue still works | e2e |

## Adversarial analysis (STRIDE-lite)

| Boundary | Threat | Disposition |
|---|---|---|
| WebView → `forget_item` | **T**: script deletes many items | Same CSP and allow-list as Phase 3; one id per call; every delete logged in `home.log` (id only, no text) |
| WebView → `task` | **I**: reads another task's text | Only the person's own store, on their own machine; no new disclosure path |
| App → store (core down) | **T**: concurrent write with a core that just started | SQLite WAL locking and a 5 s busy timeout, as the core itself uses |
| `setup-done` flag | **S**: something else writes it | Only skips the setup screen; accepted (same-user software, as the pause flag) |

## Test plan

| Directive | Test |
|---|---|
| Unit (Rust) | `split_tasks` returns member prompt ids; tools detection on temp homes |
| Integration (Rust) | task detail over the seeded store (live and imported); forget_item through store, service and control; raw-file scan after delete |
| Contract | Fixture JSON includes `setup_done`, `setup.tools` and a `task-<id>.json` detail written by the Rust crate; UI loads them |
| UI e2e | Task detail from the Tasks page by keyboard and click; setup steps, Back, Start disabled until ticked, core-down tips step; delete one: names the item, Cancel focus, announced, row shows "Text deleted" |
| Accessibility | axe on the three new screens, light, dark, high contrast |

## Flagged risks

- If the core starts in the moment between "not running" and AIQ home's direct delete, the core read the
  text at start, so its detector keeps word sets from that prompt in memory until it next restarts or the
  task goes idle (30 minutes). The store and disk are clean either way. *Accepted* (narrow window; memory only).
- Imported prompts can't be deleted one by one (E4). *Flagged* as a follow-up.
- Setup can't connect a tool itself; it names the `aiq install` command. Installing from the app means
  AIQ home editing other tools' settings files, a new trust step. *Flagged* as a follow-up.
- Real WebView2 keyboard and screen-reader pass for the three screens is a laptop check, not done here
  (another thread is using the laptop). *Flagged.*

## Gate record

`GATE design-slice · 2026-11-02 · Patterns, Simplifier, Test Architect, Security, Data & Persistence, UX & Accessibility (seats played by Claude in one session; not independent) · verdict: pass with conditions`
- **Data & Persistence (hard):** no schema change; event facts stay append-only; content deletion was already the designed path (`content`: "delete only by the person"). Pass.
- **Security (hard):** deletion verified at the byte level, not only the row. Pass; condition: the raw-file test must fail if `secure_delete` or the checkpoint is removed.
- **Privacy:** what stays after a delete is said in the dialog (E1). Pass.
- **Simplifier:** setup flag is a file, not a new table; no router (hash). Pass.
- **UX & Accessibility:** setup is serial (enter), detail is reading (parallel), per archetype H1. Laptop WebView2 check owed later.

## Status and next action

| | |
|---|---|
| **Completed** | Design |
| **Remaining** | Build, tests, CI |
| **Best next action** | `/implement` this design |
