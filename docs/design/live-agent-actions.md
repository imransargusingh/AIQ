---
id: design-live-agent-actions
title: "Design: capture the agent's actions live"
type: design
status: accepted
owner: "Imran"
phase: "5"
tags: [design, capture, claude-code, copilot, vscode, privacy]
links:
  - { to: design-copilot-vscode-slice, rel: depends-on }
  - { to: privacy-and-terms, rel: relates-to }
review-by: 2027-05-03
summary: >-
  At each turn's end the core reads only the new part of the tool's own log (Claude Code transcript; Copilot
  session log, which VS Code agent mode on the Copilot target also writes) and stores one counts-only row per
  turn in turn_action. Same summariser as aiq import. A turn it cannot read is "not recorded" with a reason.
  Its own switch; read cost per turn shown in aiq status. Prerequisite for the live rung trace and the
  status-line trend.
---

# Capture the agent's actions live

**Decision (Imran, 2026-11-03 06:43Z):** "Let's do it", after the downsides were listed: AIQ records more about
the work, it reads more on a laptop short on disk, and it can break quietly when a tool changes its log format.

## Why

Live capture recorded only prompts and stops. Imported turns carry what the agent did (1,629 imported stops with
actions on Imran's store, 0 live, 2026-11-03). The live rung trace and the status-line trend need per-turn facts
while a task is still open.

## What runs, and where

| Step | Where | Notes |
|---|---|---|
| Stop arrives | hook → core | The hook never opens a log (opening the transcript cost ~0.4 s per hook, measured 2026-11-02). |
| Read new bytes | core, after the stop's reply | Per-session offset; whole lines only; at most 16 MB. First read of a session since the core started reads the last 16 MB, not the whole file. |
| Summarise | `aiq_core::agent_actions` | The same summariser `aiq import` uses, so live and imported turns count alike. Restarts at each typed prompt; a Copilot steering message stays in its turn. |
| Store | `turn_action` (schema v9) | One row per stop: tool calls, tool names with counts, commands, files, sub-agents, bytes read, read ms. |

Sources: Claude Code `transcript_path` (already on every hook event); `<copilot home>/session-state/<id>/events.jsonl`
for Copilot CLI and VS Code agent mode on the Copilot target (6 of 7 sessions seen both ways share ids). One read
serves both this and the mid-turn message reader (PR #89).

## Grain and privacy

One row is exactly one live stop the store kept. Counts are additive across turns except `files` (distinct per
turn, capped at 50). No command text, no file names, no output. Nothing leaves the device. A set-aside or paused
session stores no row. `aiq export` includes the table; deleting AIQ's data deletes it.

## Never a guess

A turn is `not_recorded`, with NULL counts and a reason, when: there is no log (`no_log`), the read failed
(`read_error`), a relevant line did not parse (`unparsed`), more than 16 MB was waiting (`too_large`), or the
first read since a core restart did not reach the turn's start (`no_start`). Each counts
`AIQ-INGEST-ACTIONS-NOT-RECORDED`.

## Measured from day one

`aiq status` shows turns read today, turns not recorded and why, and read cost per turn (p95 and max ms, largest
read). Each read logs `agent.actions` with outcome, bytes and ms.

## Also fixed

Copilot's GPT models edit with `apply_patch`, whose argument is the patch text, so those edits counted 0 files.
The summariser now takes file names from the patch's `*** Update/Add/Delete File:` lines, for import too.
assume: Copilot logs the patch as the raw `arguments` string or under `input`/`patch`. Confirm: a live turn that
ran apply_patch shows files > 0. If false: those turns count 0 files, as before.

## Not covered (later step)

VS Code agent mode on the Local target, ask mode, inline chat and completions fire no Copilot hooks. Seeing them
needs the AIQ VS Code extension to watch the chat. That is separate work, not a wall.

## Composition with "Stop counting sessions twice" (PR #102)

Live actions live in `turn_action`; imported actions stay on `import_event`. Nothing reads `turn_action` yet, and
the judge still reads imported actions only. A reader that joins the two must take the live row for a stop that
has one and the imported turn otherwise, using the same session-id join PR #102 uses.
