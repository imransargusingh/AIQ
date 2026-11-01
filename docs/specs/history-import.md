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
| H4 | Sessions that look automated are not judged as hand-done tasks. They count as reuse evidence (F9) for the workflow they ran | Laptop inventory: ~6,000 Copilot sessions in Aug–Sep in one repo look automated |
| H5 | Judge for imported Copilot tasks: **open** (decision card in thread, 2026-11-01). Recommended: Claude via `claude -p`, opt-in cross-provider | Rung-model Q2 allows another provider only when the person chooses |

## What is on the device (measured 2026-11-01, Imran's laptop)

Counts, dates and folder names only; no content was opened.

| Source | Store | Sessions | Range | Notes |
|---|---|---|---|---|
| Copilot CLI | `~/.copilot/session-state/<id>/events.jsonl` + `~/.copilot/session-store.db` | ~6,500 | 2026-02-19 .. 11-01 | ~6,000 in Aug–Sep in one repo (likely automated); ~500 others across ~15 repos. `repository` column null for most rows, `cwd` set |
| Claude Code | `~/.claude/projects/<slug>/*.jsonl` | 592 | 2026-10-15 .. 11-01 | `cleanupPeriodDays` unset, so the 30-day default deletes older transcripts |
| VS Code Insiders chat | `%APPDATA%\Code - Insiders\User\workspaceStorage\<hash>\chatSessions\*.json` | 272 | 2026-02-23 .. 10-29 | Workspace path in `workspace.json` |
| VS Code stable chat | same, under `Code` | 31 | 2025-09 .. 2026-06 | |

Confidence: **Verified** for one laptop (read-only inventory). Other people's machines: **Inferred**.

## Part A — Functional specification

### Problem

AIQ only sees work from the moment its hooks are installed. A new user starts with an empty history,
so the rung distribution and "opportunity to move up" views are blank for weeks. Most people already
have months of AI sessions on disk. Those sessions can fill the history on day one.

### Core scenario

1. During setup, or later from *Your data > Import history*, AIQ lists the stores it found, with
   session counts and date ranges per tool and per repo. Nothing is read beyond file metadata yet.
2. The person picks which tools, repos and date range to import, and sees the estimated judging
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
- **US-6 Automated runs.** **Given** a session classed as automated (rules below) **Then** it is not
  queued as a task; it adds a run to the F9 count for the workflow it ran; the person can see and
  override the count of sessions set aside.
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
  is judged only by P, unless the person has opted in to cross-provider judging (H5).

### Automated-session rule (first cut, tunable)

A session is **automated** when any holds: it was started non-interactively (`-p`/prompt flag or no
TTY, where recorded); it has exactly one user turn and the same first prompt as 10+ other sessions in
the same repo; or 50+ sessions started in the same repo within one hour. Thresholds to be checked on
the Aug–Sep Copilot sessions before they drive anything (**Inferred**, not yet measured).

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
- Judge calls go only to the provider that already saw the session, unless the person opts in (H5).
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

## Open questions for Imran

1. **H5** judge for imported Copilot tasks (decision card open).
2. Default date range for a first import: everything found, or last 90 days?
3. Should automated runs ever be shown as their own tasks (a Factory-rung view), or only as F9
   evidence?

## Flagged risks and unknowns

- Copilot CLI and VS Code transcript formats are undocumented and change between versions. Readers
  must fail per session, not per run. AI-Forward's `session-profile.py` already parses both CLI
  formats and is the starting point (**Verified** it reads them; field coverage for facts **Inferred**).
- F2 on imported tasks is weaker than live; imported rungs should carry lower confidence where F2
  decides a guardrail.
- Most history may be on other machines; import is per device.
