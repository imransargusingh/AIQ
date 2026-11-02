---
id: spec-history-import
title: "Spec: History import (seed AIQ from past AI sessions)"
type: spec
status: draft
owner: "Imran"
phase: specify
tags: [history-import, backfill, collectors, rung-model, copilot-cli, claude-code, phase-2]
links:
  - { to: spec-rung-model, rel: depends-on }
  - { to: spec-coaching-loop, rel: relates-to }
review-by: 2026-11-30
summary: >-
  Let a person seed AIQ with AI sessions that happened before AIQ was installed, so those tasks are
  grouped, classified by job and judged for a rung. Reads the transcripts the AI tools already keep on
  the device (Copilot CLI, Claude Code; VS Code chat later), never sends anything to a new place, and
  judges within the person's daily budget. Imported tasks never produce coaching cards.
---

# Spec: History import (seed AIQ from past AI sessions)

Produced with AI-Forward `/specify` (Functional layer complete; UX outline; UI deferred to the
`aiq-home` spec). Requested by Imran, 2026-11-01: *"Is there a way to seed AIQ with data from sessions
over the past few months (sessions that existed before a given repo was set up with AIQ) to classify
those tasks?"*

## Decisions so far

| # | Decision | Source |
|---|---|---|
| H1 | Import reads only stores already on the device. Nothing is uploaded, and no new provider sees any content | Privacy rule (only the person can access their data), applied here |
| H2 | Order of sources: Copilot CLI first, Claude Code second, VS Code Copilot Chat later | Laptop inventory below: Copilot CLI holds the longest and largest history |
| H3 | Imported tasks are judged and shown in history, but never raise a coaching card or count against the daily tip cap | Coaching is in-the-moment; a card about a task from August is noise |
| H4 | **No triage by default.** AIQ detects automated sessions itself and sets them aside: never judged, never in the task history or rung views, never coached. The person sees one line per set-aside folder and can undo it in one click. Reuse evidence (F9) comes from the repo (versioned workflow files, runs by CI or others), so set-aside sessions are not needed for it | Imran, 2026-11-01: Executive_Presence's ~6,000 sessions are an app's data-collection pipeline; Imran rejected per-folder sorting as too much manual work for users |
| H5 | Imported Copilot tasks are judged by **Copilot**, through the person's own signed-in Copilot CLI (`copilot -p`), not VS Code. Same provider that saw the session, so no cross-provider opt-in is needed | Imran, 2026-11-01 (in thread, after the decision card). Laptop probe below |

## What is on the device (measured 2026-11-01, Imran's laptop)

Counts, dates and folder names only; no content was opened.

| Source | Store | Sessions | Range | Notes |
|---|---|---|---|---|
| Copilot CLI | `~/.copilot/session-state/<id>/events.jsonl` + `~/.copilot/session-store.db` | ~6,500 | 2026-02-19 .. 11-01 | ~6,000 in Aug–Sep in Executive_Presence (an app pipeline that calls Copilot, per Imran); ~500 others across ~15 repos. `repository` column null for most rows, `cwd` set |
| Claude Code | `~/.claude/projects/<slug>/*.jsonl` | 592 | 2026-10-15 .. 11-01 | `cleanupPeriodDays` unset, so the 30-day default deletes older transcripts |
| VS Code Insiders chat | `%APPDATA%\Code - Insiders\User\workspaceStorage\<hash>\chatSessions\*.json` | 272 | 2026-02-23 .. 10-29 | Workspace path in `workspace.json` |
| VS Code stable chat | same, under `Code` | 31 | 2025-09 .. 2026-06 | |

Confidence: **Verified** for one laptop (read-only inventory). Other people's machines: **Inferred**.

## Copilot CLI as judge (probe, 2026-11-01, Imran's laptop)

Copilot CLI 1.0.105 is installed (winget, plus a WindowsApps alias first on PATH and a VS Code shim).
One test call ran from an empty temp folder:

`copilot -p "<judge prompt>" -s --model <id> --available-tools= --no-custom-instructions --no-ask-user --disable-builtin-mcps --sync-session=off --no-auto-update --disable-client-telemetry --stream off --log-dir <aiq-dir>`

| Check | Result | Confidence |
|---|---|---|
| One-shot answer | Exit 0, stdout exactly the answer (`-s` prints the answer only) | Verified |
| Time | 20.6 s wall for a one-word answer (mostly start-up) | Verified, n=1 |
| Model choice | `--model` accepts Claude, GPT, Gemini and other ids, and `auto` | Verified (help) |
| Hooks | None fired (no user-level Copilot hooks installed); there is no flag to turn hooks off, only the `disableAllHooks` setting | Verified on this laptop |
| Session saved | **Yes.** Each call creates a new `~/.copilot/session-state/<id>` folder and a store row. No flag prevents it | Verified |
| Plugins and MCP servers | **Still load.** Installed plugins and ~15 user MCP servers started despite the flags; `--disable-builtin-mcps` covers only the built-in two | Verified |
| Tools off | `--available-tools=` given; an empty tool list not yet confirmed from the log | Unverified |

Design consequences:

- **Judge sessions must never be imported or observed as tasks.** AIQ sets `--session-id` to a UUID it
  generates, records it, and runs from its own working folder; readers and hooks drop those sessions.
- **Sealing.** Pass `--disable-mcp-server <name>` for every server in the person's Copilot config, read
  fresh before each batch, and confirm the tool list is empty from `--log-dir` once per batch; abort the
  batch if any tool is available. A throwaway `COPILOT_HOME` would seal fully but loses the sign-in and
  would need a token AIQ holds, which conflicts with "AIQ never reads the token". Installed plugins still
  load; accepted while tools are off and MCP servers are disabled (Imran, 2026-11-01).
- **Cost and speed.** Batch several tasks' facts into one call where the prompt fits, to spread the
  ~20 s start-up; record `--usage-output-file` per call so judging cost is measured (TR-8).
- AIQ deletes its own judge session folders (by recorded id) after reading the answer, by default;
  a setting keeps them. `aiq forget` deletes any left over (Imran, 2026-11-02; rung-judge review S1).

## Part A — Functional specification

### Problem

AIQ only sees work from the moment its hooks are installed. A new user starts with an empty history,
so the rung distribution and "opportunity to move up" views are blank for weeks. Most people already
have months of AI sessions on disk. Those sessions can fill the history on day one.

### Core scenario

1. During setup, or later from *Your data > Import history*, AIQ lists the stores it found, with
   session counts and date ranges per tool and per repo. Nothing is read beyond file metadata yet.
2. The person picks which tools, repos and date range to import (default: everything found, Imran 2026-11-01), and sees the estimated judging
   time at their daily budget.
3. AIQ reads the chosen transcripts on device, groups turns into tasks, classifies each task's job,
   computes log facts, and queues tasks for the judge.
4. The judge works through the queue within the daily budget. Tasks appear in history marked
   **Imported** as they are judged.
5. The person can delete one imported task, all imported tasks from a source, or all imported tasks.

### What import can and cannot recover

| Rung fact | From a past transcript | Note |
|---|---|---|
| F1 agent made the edits | Yes | Edit/write tool calls |
| F2 share written by the person | **Approximate** | No post-turn git diff was taken. Estimate from git commits in the session window, else Unknown |
| F3 who ran checks | Yes | Shell tool calls running tests, build, lint |
| F4, F5, F12, F13 (model facts) | Yes | From prompts and replies, as live |
| F6 output reviewed | Partly | Log part yes; editor diff views not recorded |
| F7 spec or instructions given | Yes | Files read or attached; instruction files in repo at that commit |
| F8 sub-agents with checks | Yes | Sub-agent records (Claude Code `subagents/`, Copilot events) |
| F9 reused workflow, runs by others | Yes | Git history is retroactive; automated sessions add run counts |
| F10 agents and permissions available | Partly | Settings now, not as they were then |
| F11 similar tasks in last 30 days | Yes | Computed across imported + live tasks |
| Hook timing, tip shown, "This doesn't fit" | **No** | Did not exist before AIQ |

### In scope / out of scope

In scope: Copilot CLI and Claude Code readers; discovery with counts; grouping, job classification,
log facts and judge queue (reusing the live pipeline); automated-session detection; delete by source;
re-import that skips sessions already imported.

Out of scope: Claude.ai web chats and M365 Copilot (not on disk); VS Code chat (later, H2); reading
another person's sessions; recovering transcripts the tool already deleted; any upload.

### User stories and acceptance criteria

- **US-1 Discover.** **Given** AIQ is installed **When** the person opens Import history **Then** AIQ
  shows each tool with session count, date range and size per repo, having read only file metadata
  and database row counts.
- **US-2 Choose.** **Given** the discovery list **When** the person picks tools, repos and a date
  range **Then** AIQ shows the number of sessions, the number left after automated sessions are set
  aside, and the estimated days to judge at the current daily budget.
- **US-3 Import.** **Given** a confirmed selection **When** import runs **Then** transcripts are read
  on device, tasks are grouped and classified, log facts are computed, and tasks are queued for the
  judge. The source transcript files are never modified.
- **US-4 Judge within budget.** **Given** queued imported tasks **When** the daily judge budget is
  spent **Then** judging stops for the day, and live tasks are always judged before imported ones.
- **US-5 No cards.** **Given** an imported task is judged **Then** no coaching card is shown and the
  daily tip cap is unchanged.
- **US-6 Automated sessions set aside.** **Given** sessions that meet the automated rule (below) **When**
  import runs **Then** they are set aside with no question asked: not judged, not shown as tasks, no
  fact counted. Discovery and *Your data* show one line per folder ("Executive_Presence: 5,946 automated
  sessions set aside · Include them"). **Include them** imports that folder's sessions as normal tasks;
  the choice can be reversed.
- **US-12 Automated sessions ignored live.** **Given** a live hook fires for a session that meets the
  automated rule, or in a folder whose sessions are set aside **Then** AIQ records nothing and never
  shows a tip. (Copilot hooks are user-level, so a pipeline's own CLI runs would otherwise reach AIQ.)
- **US-7 Idempotent.** **Given** a session was imported before **When** import runs again **Then**
  it is skipped (keyed by tool + session id + last-modified time); a session that grew since is
  re-read.
- **US-8 Delete.** **Given** imported tasks **When** the person deletes one, a source, or all imported
  **Then** the tasks, facts, judgments and any stored prompt text are removed from AIQ's store; the
  tool's own transcripts are untouched.
- **US-9 Lost history warning.** **Given** Claude Code's `cleanupPeriodDays` is unset or under 90
  **When** discovery runs **Then** AIQ says older transcripts are being deleted by Claude Code and
  shows how to keep them. AIQ never edits that setting itself.
- **US-10 Provider rule.** **Given** an imported task from provider P **When** it is queued **Then** it
  is judged only by P: Copilot tasks through `copilot -p`, Claude Code tasks through `claude -p` (H5).
- **US-11 Judge sessions excluded.** **Given** AIQ ran a judge call through a CLI **When** discovery,
  import or live capture sees that CLI's sessions **Then** sessions with AIQ's recorded judge ids are
  ignored.

### Automated rule (tested on real sessions, 2026-11-01)

AIQ decides; the person only corrects. H = human turns in the session. A session is **automated** when
H ≤ 1 and any of:

- **NI, non-interactive start.** Claude Code: `entrypoint = sdk-cli` on its user records. Copilot CLI
  records no prompt-mode marker (`session.start` keys checked), so NI is unknown there.
- **T, template.** Its normalized first prompt (digits, URLs, GUIDs, paths, quoted strings replaced; first
  200 chars; hashed on device) matches 11+ sessions in the same folder. Empty first prompts are excluded
  from template groups (they all share one hash).
- **B, burst.** 50+ sessions started in the same folder within ±30 minutes.
- **F, folder prior** (added after the test). The folder is over 80% automated by the signals above.

Separately, whatever H is:

- **RC, Projects / Remote Control session** (added 2026-11-02). Claude Code run by the Claude desktop app for
  a claude.ai Project thread is Claude's own session, not the person's hand-typed work, though it has many
  turns. Its `entrypoint` is `claude-desktop`, like a session typed in the app, so the marker is on the
  transcript records: `projectsUserTurn: true`, `origin.subkind = projects-relay`, `turnOrigin = peer`, or a
  desktop `turnOrigin = system` first turn. Laptop measurement (Claude Code 2.1.299–2.1.316): 14 of 27
  desktop transcripts carry a marker, 0 of 9 terminal and 0 of 639 headless ones; in all 11 live sessions
  the first record is written 64 ms–3.7 s before the first prompt's hook fires, so the live hook reads the
  transcript head (256 KB) and sets the session aside from its first prompt. Set aside per session, not per
  folder (the same folder holds the person's own sessions); `aiq include <folder>` counts them again.
- **AIQ, AIQ's own check or probe session** (added 2026-11-02). A session whose folder is `<temp>\aiq-*` or
  below it (`%TEMP%\aiq-phase2-check`, `%TEMP%\aiq-copilot-probe-…\run0`) is a test of AIQ. Measured on
  Imran's laptop: 8 imported Copilot sessions, all one-prompt probes run by hand or by Remote Control, so
  they carry no `AIQ_INTERNAL` mark and Copilot's `session.start` shows no one-shot marker; the folder is the
  only handle. Import sets them aside (why `AIQ`, text dropped; an earlier import of them is re-read and
  set aside) and writes a `set_aside` folder rule (reason `aiq-check`); live capture does the same at the
  folder's first event and ignores every event there. `aiq include <folder>` counts them again.

Test on Imran's laptop (counts only; prompts hashed in memory, never printed or stored):

| | Copilot CLI | Claude Code |
|---|---|---|
| Sessions | 6,431 | 617 |
| Automated (NI/T/B) | 5,936 | 590 |
| Caught by NI / T / B | – / 5,872 / 4,859 | 586 / 590 / 0 |
| H ≤ 1 with no signal | 330 | 15 |

- **False alarms: none found.** No folder where most sessions have H ≥ 3 had any automated session.
- **Misses:** Executive_Presence 103 sessions not caught, ~95 of them H ≤ 1 (empty or unique first prompt,
  outside a burst). Rule F is meant to catch these; not yet re-measured.
- Claude Code: all 586 `sdk-cli` sessions are automated (Foresight 486, a temp folder 92, Tacit 12). Only
  9 `cli` and 22 `claude-desktop` sessions remain (**Verified**, counts).
- The Foresight Claude Code sessions (489, one prompt each, all hours, one `autoresearch/*` branch) were
  started by **Tacit**, a tool of Imran's that a Copilot hook in that repo triggers and that runs `claude`
  headless (Imran confirmed as likely, 2026-11-01). Other AI tools calling a CLI in the background is a
  real pattern; the rule set them aside correctly.
- H ≤ 1 is common in hand-run Copilot work too (median H = 1 in several hand-run folders), so H alone must
  never decide; it always needs a second signal.

### Domain model

Bounded context: *History import*, upstream of *Task capture*. One new aggregate:

- **ImportRun** (entity): id, started, selection (tools, repos, range), counts found / imported /
  set aside / failed. Invariant: a session belongs to at most one ImportRun.
- **ImportedSession** (append-only fact; grain: one row per tool session read): tool, session id,
  source path, last-modified, repo, ImportRun id, outcome (task / automated / skipped / error).
- Tasks created by import are ordinary Tasks with `origin = imported` (Type-1 attribute, never
  changes). Everything downstream (job, facts, judgment) is shared with live tasks; nothing is
  duplicated.

### Non-functional requirements

| Quality | Requirement |
|---|---|
| Performance | Import runs in the background core at low priority and never adds latency to a live hook. Discovery under 10 s for 10,000 sessions (metadata only) |
| Resource use | Read transcripts as streams; never load a whole multi-GB store into memory; skip non-transcript files in Copilot session folders (the measured folder is 334 GB, mostly build trees) |
| Reliability | A malformed file is logged as `error` for that session and import continues |
| Cost | Judging uses the existing daily budget (rung-model TR-8); imported tasks never exceed it |
| Measurability | Emit per run: sessions found, imported, automated, errors, read time, and queue depth; judge calls per day split live vs imported |

### Privacy and threats

- Same rules as live capture: data stays on device; raw prompts kept until the person deletes them;
  only the person sees it.
- Transcripts hold more than live capture does (tool output, file contents). Import keeps **only**
  prompts, replies needed by the model facts, and computed facts. Tool output and file contents are
  read to compute facts and then dropped. (**To confirm** in design: which reply text the judge needs.)
- Judge calls go only to the provider that already saw the session (H5). A Copilot judge call must not
  reach the person's MCP servers (work systems such as ticketing or mail); see sealing above.
- A work account's transcripts may include work content; the setup caveat from rung-model Q2 is
  shown again before import.

## Part B — UX outline

Entry points: setup (step "Start with your past work?", skippable) and *Your data > Import history*.
Flow: discover → choose → confirm (counts, set-aside automated sessions, estimated days) → progress
(queue depth, judged so far) → history with an **Imported** filter. Failure paths: no stores found;
store unreadable (permissions); budget spent (shows when judging resumes). Detailed screens go into
the `aiq-home` spec once this spec is accepted.

## Part C — UI

N/A here; follows `DESIGN.md` and the `aiq-home` spec.

## Implementation status

Slice 1 (`aiq import`, 2026-11-01): discovery of Claude Code and Copilot CLI transcripts; the automated
rule (NI, T, B, F) over the whole history, with the live side's folder key (Windows paths compare
case-insensitively), template normalisation and thresholds (pack `[automated]`); imported sessions stored
in their own tables (schema v4), which the live loop never reads, so no cards (US-5); set-aside sessions
keep no content; unchanged sessions are not re-read on a re-run, because the rule's inputs are stored with
each session (US-7); `aiq forget --imported` (US-8); judge session ids in `~/.aiq/judge-sessions.txt`
skipped (US-11); `aiq status` line; `--dry-run`.
Shared with live capture (US-12, PR #17): a folder where the import finds a burst or a repeated first
prompt gets the same `folder_rule` row the live hooks write (source `import`), so live capture ignores
that app's runs too. "Include them" is `aiq include <folder>` for both; the next `aiq import` then
imports the folder's sessions. Sessions set aside only because they were non-interactive starts
(`claude -p`) or by the folder prior set no folder rule, so a person's own work in that folder stays
watched live.
Laptop run (2026-11-01, slice-1 build before this change, **Verified**, counts only): 650 Claude Code
and 6,520 Copilot CLI sessions found, 333 imported, 6,495 set aside; 17-21 s per run; store 4.45 MB.
Not yet: grouping into tasks, job classification and judging (waits for the rung judge), the
`cleanupPeriodDays` warning (US-9), VS Code chat.

## Open questions for Imran

None open.

## Flagged risks and unknowns

- Copilot CLI and VS Code transcript formats are undocumented and change between versions. Readers
  must fail per session, not per run. AI-Forward's `session-profile.py` already parses both CLI
  formats and is the starting point (**Verified** it reads them; field coverage for facts **Inferred**).
- F2 on imported tasks is weaker than live; imported rungs should carry lower confidence where F2
  decides a guardrail.
- Most history may be on other machines; import is per device.
