---
id: adr-0005
title: "ADR-0005: Collectors are fail-open, fire-and-forget, schema-versioned pipes"
type: adr
status: proposed
owner: "Imran"
phase: "0-1"
tags: [architecture]
links:
  - { to: aiq-architecture, rel: refines }
review-suggested: { by: design-claude-code-skeleton, on: 2026-10-31, reason: "Phase 1 design sends seq=null for Claude Code (no sequence in hook payloads; one process per hook); Windows hooks run via Git Bash and need quoted forward-slash paths." }
review-by: 2027-04-29
summary: >-
  Collectors never block or slow the person's tools: they send versioned, idempotent events to the core with a strict time budget, spool briefly if the core is down, and install at user level only.
---

# ADR-0005: Collectors are fail-open, fire-and-forget, schema-versioned pipes

- **Status:** Proposed
- **Date:** 2026-10-31
- **Deciders:** Imran (owner); drafted with Claude via /define-architecture

## Context
The person's IDE and agents are the hot path (LOA F). NFRs: ≤ 50 ms p95 added to any hook; if the
core is down the tools work unchanged. Claude Code's default command-hook timeout is 600 s and
Copilot CLI's is 30 s ([Claude Code hooks](https://code.claude.com/docs/en/hooks),
[Copilot CLI hooks](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-hooks-reference)),
so the host will not protect us; the collector must enforce its own budget.

## Decision
1. **Never block, never decide.** Collectors never return a block/deny decision, never modify
   prompts or tool results, and never write `additionalContext` (coaching is for the person, not the
   agent). They always exit success.
2. **Budget:** each hook invocation does one non-blocking write to the core's local socket/pipe with a
   20 ms connect+write timeout; total hook time ≤ 50 ms p95. On Claude Code, observation hooks run with
   `"async": true`; only the Stop hook that may render a card runs synchronously, and it waits ≤ 50 ms
   for a ready card.
2a. **Runtime:** the hook command is a tiny compiled client (`aiq hook <event>`, shipped with the
   core) that hands the payload to the running core over the socket and exits. No interpreter per
   hook: the Phase 0 probe measured a Python process at 35 ms p50 / 47 ms p95 / 58 ms max cold start,
   too close to the 50 ms budget for the synchronous Stop hook.
3. **Fail open with a bounded spool:** if the core is unreachable, append the event to a user-only
   spool file (≤ 1 MB, oldest dropped) and exit. The core drains it on start.
4. **Contract:** `aiq.event.v1` JSON Schema: `event_id` (UUIDv7), `surface`, `surface_session_id`,
   `seq`, `kind`, `ts`, payload by kind. At-least-once delivery; the core upserts by `event_id`.
   Content fields are optional and subject to the retention policy (ADR-0002).
5. **Rendering:** cards come back as `aiq.card.v1`. Claude Code: `systemMessage` from the Stop hook.
   Copilot CLI: a `progress` line at `agentStop`. VS Code: notification + status bar.
6. **Install at user level only:** `~/.claude/settings.json` and `~/.copilot/hooks/`, never a repo's
   `.claude/` or `.github/hooks/`, so teammates never see or run AIQ hooks.

## Alternatives considered
- **Synchronous request/response per hook:** rejected; puts the core on the hot path.
- **Read session stores only (no hooks):** kept as a fallback and for backfill, but alone cannot
  coach in the moment.
- **Repo-level hook config:** rejected; visible to and executed for teammates.

## Consequences
- **Positive:** AIQ can crash, upgrade or be paused with zero impact on the tools.
- **Negative:** events can arrive late or (rarely) be lost when the spool is full; Tasks tolerate it.
- **Follow-ups:** the hook semantics above are verified from docs only; Phase 0 spike must run them.

## Evidence
Hook docs linked above (read 2026-10-31). Phase 0 hook probe (Claude Code 2.1.316, headless, 3 live
runs + 50-run bench; spikes/phase0/hook_probe/): async UserPromptSubmit and PostToolUse fire every
turn and never block; a sync Stop hook's `systemMessage` renders to the person and does not enter
model context; payloads carry prompt, tool name/input/response, duration, transcript path and
session id, so hooks alone can feed Task grouping. Not yet exercised: Copilot CLI, interactive TUI
rendering.
