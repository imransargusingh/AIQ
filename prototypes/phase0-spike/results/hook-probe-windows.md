# Hook probe on Windows: Claude Code and Copilot CLI

Measured 2026-10-31 on Windows 11 Enterprise (10.0.26300), Python 3.14.7. Raw numbers are in
`hook-probe-windows.json`. Everything here was observed unless it is labelled **Inferred**.

## Result first

- **Claude Code works end to end.** All three hooks fired in 3 of 3 runs. The async hooks did not block
  the turn. The Stop card renders in the interactive TUI as
  `⎿  Stop says: AIQ (probe): this is where a coaching card would render.`
- **Copilot CLI observes the turn, but cannot show a card.** Every event fired, in `-p` mode and
  interactively, at repo and user level. But no hook output reached the person. A `systemMessage` on
  `agentStop` is not shown. Documented `progress` lines were not visible in the interactive TUI,
  with or without `temporary`, at `preToolUse`, `postToolUse` or `agentStop`.
- **Copilot hooks are synchronous.** `"async": true` is accepted and ignored. The tool result waits
  for `postToolUse`, `assistant.idle` waits for `agentStop`, and exit waits for `sessionEnd`.
- **Process start-up is the whole collector cost.** The collector's own work takes about 1–2 ms. A
  PowerShell-wrapped hook adds about 0.4–1 s. Use `exec` + `args` (no shell) on Copilot.

## Claude Code

| Item | Result |
|---|---|
| Versions | 2.1.300 for the `-p` runs; 2.1.312 for the TUI (through the Agency wrapper) |
| Python collector, whole process, 50 runs | p50 98.2 ms · p95 137.5 ms · max 144.8 ms |
| Collector work inside the process | p95 1.57 ms |
| `claude -p` runs (Haiku 5.5) | 3/3 exit 0, 9.5–10.8 s each |
| Hooks fired | UserPromptSubmit 3 · PostToolUse 3 · Stop 3 |
| Async observers | UserPromptSubmit and PostToolUse responses arrived after Stop's response, so they did not block |
| Stop card in stream-json | yes, 3/3 |
| Stop `systemMessage` in interactive TUI | **visible**, shown under the reply with a `Stop says:` prefix |

To reach the TUI without the folder-trust prompt, the probe ran `claude --settings <temp hooks file>`
in the already-trusted AIQ checkout. The real settings were not changed.

**Windows fix.** `collector.py` now defaults its log to `%TEMP%\aiq-hookprobe-log.jsonl`. On Windows,
`run_probe.py` drops the POSIX `AIQ_PROBE_LOG=...` prefix and writes quoted forward-slash paths,
because Claude Code runs hook commands through Git Bash. The collector also emits its card on Copilot's
`agentStop`, and records the payload `timestamp`.

## Copilot CLI

**Versions:** GitHub Copilot CLI 1.0.104 for the `-p` runs; 1.0.105 for the TUI (through Agency).
**Signed in:** yes. **Model:** `gpt-6.1-sol`, forced by an administrator-managed setting.

**Where hook config lives** (docs.github.com hooks-configuration reference, `copilot help config`):

- **Repo-level** — any of:
  - `.github/hooks/*.json`;
  - inline `hooks` in `.github/copilot/settings.json` or `settings.local.json`;
  - `.claude/settings.json`. Copilot reads this file too.
- **User-level** — any of:
  - `%USERPROFILE%\.copilot\hooks\*.json`, or `$COPILOT_HOME/hooks/`;
  - inline `hooks` in `~/.copilot/settings.json`.

In `-p` mode, repo hooks load only when the folder is trusted (`--trust-folder`).

**Schema used** (`.github/hooks/aiq-probe.json`):

```json
{"version": 1, "hooks": {"postToolUse": [
  {"type": "command", "exec": "C:\\Python314\\python.exe",
   "args": ["...collector.py", "postToolUse"], "timeoutSec": 10}]}}
```

The shell arm used `"powershell": "& \"python.exe\" \"collector.py\" <event>"` plus a matching `bash` field.

**Payload keys.** All events carry `sessionId`, `timestamp` and `cwd`. There is no event-name field,
so the event is passed as an argument.

| Event | Extra keys |
|---|---|
| sessionStart | `source`, `initialPrompt` |
| userPromptSubmitted | `prompt` |
| preToolUse | `toolName`, `toolArgs` |
| postToolUse | `toolName`, `toolArgs`, `toolResult` |
| agentStop | `transcriptPath`, `stopReason`, `stop_hook_active` |
| sessionEnd | `reason` |

**Events fired.** All of these fired in every run: sessionStart, userPromptSubmitted, preToolUse
(user-level run), postToolUse, agentStop and sessionEnd. That covers `-p` mode (6 runs, repo-level), the
interactive TUI, and user-level hooks under a temp `COPILOT_HOME` with no repo hooks. The temp home needed
no auth files.

**Per-hook latency.** Measured from the payload `timestamp` to collector start, in ms. Each row shows
p50, with the max in brackets.

| Arm | sessionStart | userPromptSubmitted | preToolUse | postToolUse | agentStop | sessionEnd |
|---|---|---|---|---|---|---|
| Repo, `powershell` field, 3 runs | 2261 (10564) | 1017 (4281) | – | 1129 (3416) | 1664 (3152) | 1129 (1859) |
| Repo, `exec` field, 3 runs | 214 (977) | 210 (2671) | – | 4361 (5199) | 560 (7110) | 436 (6279) |
| User-level, `exec`, 1 quiet run | 132 | 135 | 140 | 136 | 167 | 121 |

- The quietest runs give about 110–210 ms with `exec`, and 440–630 ms with the `powershell` field.
- One sessionStart hook hit the 10 s timeout (powershell arm). Copilot logged it and carried on.
- Both arms had multi-second outliers. **Inferred:** these come from other load on the machine.
  Agency and the CLIs were downloading updates during those runs. This was not isolated.

**Async.** Not supported for these events. `"async": true` is ignored. The docs describe only
`notification` as fire-and-forget, plus `sessionEnd` on `/clear`. Two progress emitters that each held
1.5 s made the interactive shell step take 7 s.

**Shown to the person.**

- **`systemMessage` (agentStop):** no. It appears only in the debug log, as `[hook stdout]`.
- **`{"type":"progress",...}`:** no, in both the TUI and `-p`. This held at preToolUse, postToolUse and
  agentStop, each with and without `temporary: true`. A marker file proved all 6 emitters ran.
- **Documented channels:** stderr on exit 2 is shown. The rest (`additionalContext`, `decision`/`reason`,
  `permissionDecisionReason`) goes to the agent, not the person.

## Collector client latency (whole process, 50 runs each)

| Client | p50 ms | p95 ms | max ms |
|---|---|---|---|
| Python 3.14 collector | 115.2 | 133.9 | 1324.2 |
| C# .NET 10 (framework-dependent exe) | 79.7 | 105.8 | 776.8 |
| `.cmd` batch | 60.0 | 76.7 | 108.2 |
| Windows PowerShell 5.1 `-NoProfile` | 334.9 | 508.0 | 704.5 |
| pwsh 7 `-NoProfile` | 582.9 | 682.5 | 778.9 |
| pwsh 7 with this user's profile | 821.6 | 1924.9 | 2466.0 |

No Go or C toolchain is installed. The C# client is `Program.cs`: read stdin, append one line, exit 0.

## What this means for AIQ

- **Claude Code:** the ADR 0005 render path (a Stop hook with `systemMessage`) works on Windows as designed.
- **Copilot CLI** — a hook can observe the turn but cannot show the person a card:
  - Coaching must render somewhere else, such as a status line, the next session start via
    `additionalContext` to the agent, or a separate surface.
  - Hooks must be `exec` + `args` and do very little, because each one blocks the agent.
  - The collector should hand work off rather than do it inline.
