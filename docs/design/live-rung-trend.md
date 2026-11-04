---
id: design-live-rung-trend
title: "Design: live rung trend (Right now card and status line)"
type: design
status: proposed
owner: "Imran"
phase: "5"
tags: [design, aiq-home, live-trend, copilot, status-line, rung]
links:
  - { to: design-day-view, rel: implements }
  - { to: design-live-agent-actions, rel: depends-on }
  - { to: design-aiq-home-app, rel: relates-to }
review-by: 2027-05-03
summary: >-
  While a task is open, AIQ estimates its rung after each turn from facts it reads on the device (what the agent
  did, the prompts) and shows it in two places. C, a Right now card at the top of Home. D, the Copilot CLI status
  line between tips. No model call. It is an estimate: the judge's rung replaces it when the task ends. Measured
  on the golden set, the estimate matches the label 22 of 32 times and is within one rung 29 of 32 times; the
  judge scores 23 and 29.
---

# Design: live rung trend

Imran, 2026-11-03 06:12Z: *"we could still provide users with a task-oriented view in near-real time with some
indication of what the session is trending towards so they have the ability to correct and shift the session to a
higher rung utilizing the provided tips."* He picked options C and D from the renders
(https://claude.ai/artifact/7LGhvfgLTQTLkUq8w5AsB8). At 20:28Z he said "build C and D". Live capture of the agent's
actions, which this needs, merged as PR #104.

## Goal state

| | |
|---|---|
| **Goal** | While a task is open, show where it is heading and what would lift it, on Home and in the Copilot CLI status line |
| **Done when** | C and D render from real per-turn facts; the estimate is scored against the golden set in CI; the person can turn it off |
| **Not in scope** | Claude Code's status line (AIQ installs none there); a model read per turn; goal segments inside a task |
| **Tier** | T1 (new read path and two views; no new table; nothing new leaves the device) |

## The estimate (T1)

`aiq_domain::trend` reads the log-fact half of the rung guardrails, turn by turn, over the turns that have ended.

| Estimate | Rule (from the log) | Guardrail it stands in for |
|---|---|---|
| Assisted (1) | The agent made no edit (an answer or run job: looked nothing up), or the first prompt is under 20 words and not a slash command | F1 / F14, F4 |
| Directed (2) | The agent did the work, but the person has sent 3 or more prompts | F12 (driven a turn at a time) |
| Delegated (3) | The agent did the work from at most two prompts | F1 / F14 with F4 |
| Orchestrated (4) | Also a spec or brief named, sub-agents ran, and the agent ran checks itself (answer job: sub-agents) | F7, F8, F3 / F15 |

Factory is never estimated: F9 (a saved workflow) is not readable today. The job comes from the first-prompt label
(PR #80) when it clears the judge's own bars (job 0.7, kind 0.7). Without it, edits count as the work.

**Measured, golden set g1 (32 cases with a labelled rung), 2026-11-03.** Exact 22 of 32, within one rung 29 of 32,
higher than the label 4 of 32. The judge's recorded replay on the same set: exact 23 of 32, within one 29 of 32.
`trend_matches_golden` (crates/aiq/src/judge_eval.rs) fails if a rule change does worse. The golden set is
synthetic; the first live check is the estimate at a task's end against its judgment (later step).

assume: 20 words stands in for "goal, context and limits up front" (F4), and 3 prompts for "driven a turn at a
time" (F12). Confirm: the end-of-task comparison above. If false: the estimate leans low or high on those tasks,
and the judge's rung still replaces it.

**Live limits.** Live turns store counts and tool names only (no command text, no file names; design
live-agent-actions). So live, AIQ cannot see that the agent ran a check or read a spec file. A code task then
reads at most Delegated live; an answer task can still reach Orchestrated through sub-agents. A turn that was not
recorded adds nothing; the estimate stays where the recorded turns put it, and no estimate shows until a yes is
seen. A deleted first prompt gives no estimate rather than a guessed one.

## Where it shows

| # | Decision | Why |
|---|---|---|
| D1 | **C: Right now card** at the top of Home, only while a task is open (last event within the 30-minute idle gap) and at least one turn has ended | No empty card. Imran: dislikes wasted whitespace |
| D2 | The card shows the kind, tool, start time ("since 14:49"), prompts and minutes, "Heading for X · could reach Y", a step trace per turn, the tip at the turn it showed, the reasons in words, and Open task | The render he liked. Rung named in text, never colour alone |
| D2b | A turn not recorded says why, in words (e.g. "AIQ restarted during the turn" for `no_start`); with no estimate at all the chart is hidden | Imran, 2026-11-04: "what is the time period that this view covers?"; an empty chart read as data |
| D3 | "Could reach" is the coaching row's potential for the same kind (a coached move one rung up), shown only when above the estimate | Derived from judgments already made, not a new guess |
| D4 | **D: Copilot CLI status line**, between tips: `AIQ: heading for Directed (2) so far · could reach Delegated (3) · turn 4`. Plain text, no yellow badge | A tip still stands out when one comes. Tips always win the line |
| D5 | One switch, Settings > Tips > "Show where an open task is heading", on by default; `aiq config set live-trend off` too | AIQ decides, easy override, setting in the app |
| D6 | The card refreshes with Home's normal refresh; the status line on each Copilot refresh | Near-real time with no new timer |

## Cost, measured from the first build

The status line already asks the core on every refresh. The trend adds one indexed query over one session's last
day of events and one over today's kind labels, and reuses the coaching rows already built (it never rebuilds them
on a status-line refresh). Each call logs `core.trend_line` with `open`, `shown` and `ms`; the status line's own
timing log marks `trend`. assume: well inside the 50 ms render wait. Confirm: `core.trend_line` ms on the laptop
after install. If false: cache the estimate per session and refresh it at each turn end.

## Surface list

store (`Store::session_events`: a session's events with each stop's `turn_action`) → domain (`trend`) → core
(`live_trend::open_task`, `status_text`, `could`; `Service::tip_line` → `TipLineOut::Trend`) → wire
(`TipLineReply.trend`, optional, older cores omit it) → `aiq statusline copilot` → Home read model
(`Snapshot.live`) → `types.ts` `Live` → `pages/RightNow.tsx` on Home → Settings switch (`live_trend`, core
`SWITCH_SETTINGS`, Tauri allowlist, CLI `live-trend`).

## Tests

- `aiq-domain` `trend`: rises when the agent edits, drops when driven a turn at a time, a bare ask reads Assisted,
  an unrecorded turn gives no estimate until a yes, Orchestrated from spec + sub-agents + own checks, an answer job
  counts looking up.
- `aiq` `trend_matches_golden`: the scores above, as a gate.
- `aiq-core` `status_line_shows_the_live_trend_between_tips`: nothing while the first turn runs, the trend after
  it, gone when switched off and after the idle gap.
- `aiq-home` `live_trend_reads_the_open_task_turn_by_turn` and `..._goes_when_the_task_ends_or_is_switched_off`.
- `apps/home/tests/e2e.mjs`: the card in light, dark and contrast with no axe violations, rung in words, tip at
  turn 3, Open task; gone with no open task; the Settings switch; no page scroll at 720 px.

## Risks

| Risk | Handling |
|---|---|
| The estimate disagrees with the judge at task end | Labelled "an estimate"; the judge's rung replaces it; end-of-task agreement is the next measurement |
| People read the trend as a grade | Words say "heading for", never a score; tips stay the only coaching |
| VS Code tasks span sessions in the task list | The card keys on the newest session; a VS Code task that moved session shows that session's turns only |
