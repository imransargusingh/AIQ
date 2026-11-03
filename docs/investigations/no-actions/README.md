# Judgments with no agent actions (2026-11-03)

Read-only measurement on Imran's laptop store, run by the judge thread with `python -I`. Counts only, no text.

**`measure_no_actions.py`** — current judgments 541; with no agent actions in any turn 78 (all rung 1, all
stored F1 no and F3 nobody).

| Count | What |
|---|---|
| 64 | imported, stop rows present, no tool calls stored; the session's log has tool calls somewhere |
| 10 | imported, stop rows present, no tool calls stored; the log has none |
| 2 | imported, no stop rows at all (not recorded) |
| 2 | live, transcript found, no prompt matched (not recorded) |

By tool: 71 imported Copilot, 5 imported Claude Code, 1 live Claude Code, 1 live VS Code.

**`measure_copilot_turns.py`** — the 75 Copilot prompts of those tasks, today's reader replayed per prompt:
51 log `hook.start > hook.end > assistant.turn_start` and nothing more before the next prompt; 53 have no
`assistant.turn_end`; 20 have one, with no tool calls. 2 prompts lose 248 tool calls that come after the first
`assistant.turn_end` (the reader closes the turn there), and 1 prompt has 314 sub-agent tool calls not counted.
Copilot 1.0.81-0 wrote 41 of the 75. Full output: project files `aiq-evals/measure-copilot-turns-20261103.txt`.

**Reading (inferred from the counts):** "tool calls in the log" in the first script was session-wide. Within
their own prompts' windows almost all of these tasks really had no tool calls, so "none" was true for them.
Fact set fs3 changes the 4 not recorded. The `assistant.turn_end` under-read (2 prompts) is DC-020's sweep item.
