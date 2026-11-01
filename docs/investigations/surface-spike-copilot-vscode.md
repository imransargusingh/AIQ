---
id: spike-surfaces-copilot-vscode
title: "Spike: Copilot CLI and VS Code as AIQ surfaces"
type: investigation
status: draft
owner: "Imran"
phase: "0"
tags: [spike, collectors, vscode, copilot-cli]
links:
  - { to: aiq-architecture, rel: refines }
  - { to: adr-0005, rel: refines }
review-by: 2027-01-29
summary: >-
  What AIQ can observe and show on Copilot CLI and VS Code, and the path forward. Copilot CLI hooks
  fire on every event but block the agent and cannot show the person anything, so cards need another
  surface. VS Code's Copilot agent sessions share the same hook harness; ask mode, inline chat and
  completions are not observable.
---

# Spike: Copilot CLI and VS Code as AIQ surfaces

Asked by Imran, 2026-10-31: "spike Copilot CLI and VS code so we have a better understanding of
what's required/path forward." Run-level checks run on Imran's laptop via the Phase 0 spike thread;
read-level checks here. Each claim is labelled **Verified (run)**, **Verified (docs/types)** or
**Flagged**.

## 1. VS Code

### What an extension can observe (stable API, @types/vscode 1.144.0)
| Signal | API | Label |
|---|---|---|
| Prompts and replies in Copilot Chat (ask/agent/inline) | **None.** `chat.createChatParticipant` only receives requests addressed to *your own* participant; `ChatContext.history` "Currently, only chat messages for the current participant are included." No event exposes other participants' traffic | Verified (types) |
| Inline completion accept/reject | **None** for another extension's completions (`InlineCompletionItemProvider` is for providing, not observing) | Verified (types) |
| Edits to files (by anyone, incl. agents) | `workspace.onDidChangeTextDocument`; `TextDocumentChangeReason` only distinguishes Undo/Redo, so AI vs human edits are not labelled | Verified (types) |
| Terminal commands and exit codes (incl. agent-run, when shell integration is on) | `window.onDidStart/EndTerminalShellExecution` | Verified (types) |
| Calling the person's Copilot models (rung judge, ADR-0007) | `lm.selectChatModels` after a consent dialog | Verified (docs) |

### What VS Code's Copilot agent hooks add (Preview)
Per https://code.visualstudio.com/docs/agent-customization/hooks:
- "VS Code, the GitHub Copilot app, and GitHub Copilot CLI share this harness and its hook format."
- Events: `sessionStart`, `userPromptSubmitted`, `preToolUse`, `postToolUse`, `preCompact`,
  `subagentStart`, `subagentStop`, `agentStop`, `sessionEnd`.
- **User-level hook files are supported:** `~/.copilot/hooks/` (or `$COPILOT_HOME/hooks/`), and inline
  `hooks` in `~/.copilot/settings.json`. Repo-level `.github/hooks/*.json` also exists (AIQ must not use it).
- Applies to **Copilot sessions** (Session Target = Copilot). Cloud sessions do not get local hook files.
  Ask mode, inline chat and completions are not described as firing hooks.
- Output to the person is not described for the VS Code harness; command-hook output is ignored for
  `userPromptSubmitted`; `postToolUse` can add model context (AIQ must not).
- "Your organization might restrict which hooks can run" (enterprise hook controls).
- The harness also reads repository `.claude/settings.json`, so repo hooks (e.g. AI-Forward's) can run
  in VS Code Copilot sessions too.
- An older "Local" harness has its own hooks (Claude-style events incl. `Stop` with `systemMessage`
  shown to the user), controlled by `chat.useHooks`; VS Code recommends the Copilot runtime for new hooks
  (https://code.visualstudio.com/docs/agents/reference/hooks-reference).

Label: Verified (docs). Run-level check pending on Imran's laptop.

## 2. Copilot CLI
Docs (https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-hooks-reference): same
harness and events as above; command hooks may print `{"type":"progress","message":…}` lines to the CLI
timeline (`"temporary": true` for a transient line); default timeout 30 s. Latest version on npm:
`@github/copilot` 1.0.105 (2026-10-31).

Run-level results: measured on Imran's Windows laptop, 2026-10-31, Copilot CLI 1.0.104 (`-p`) and
1.0.105 (interactive), model forced by admin policy. Full data in
`prototypes/phase0-spike/results/hook-probe-windows.{md,json}` (PR #6).

| Question | Result | Label |
|---|---|---|
| Do hooks fire? | All six events (sessionStart, userPromptSubmitted, preToolUse, postToolUse, agentStop, sessionEnd) on every run, in `-p` and interactive, at repo level and at **user level** (`$COPILOT_HOME/hooks`, no repo hooks, no extra auth) | Verified (run) |
| Payload | Every event has `sessionId`, `timestamp`, `cwd`; **no event-name field** (pass it as an argument). Extra: sessionStart `source`, `initialPrompt`; userPromptSubmitted `prompt`; pre/postToolUse `toolName`, `toolArgs` (+ `toolResult`); agentStop `transcriptPath`, `stopReason`, `stop_hook_active`; sessionEnd `reason` | Verified (run) |
| Schema | `{"version":1,"hooks":{"<event>":[{"type":"command","exec":"<exe>","args":[…],"timeoutSec":10}]}}` | Verified (run) |
| Can a hook show the person anything? | **No.** `systemMessage` at agentStop goes to the debug log only. `progress` lines, with or without `temporary`, at pre/postToolUse and agentStop, are invisible in the TUI and in `-p` (marker files prove they ran). Only stderr on exit 2 is shown, and exit 2 changes the agent's behaviour | Verified (run) |
| Async? | **No.** `"async": true` is ignored; each hook blocks the agent until it returns | Verified (run) |
| Cost per hook | Copilot's own event-to-process-start: 110–210 ms with `exec` (quiet run); 440–630 ms with the `powershell` field, once a 10 s timeout. Client process on Windows: `.cmd` 77 ms p95, C# 106, Python 134; AIQ's compiled client 26–34 ms p95 with the core up (design spike S3a) | Verified (run) |
| Does `copilot -p` fire the person's hooks? | Yes, so any AIQ use of `copilot -p` would be observed by AIQ itself | Verified (run) |
| Other config Copilot reads | Repo `.claude/settings.json` (so repo Claude hooks also run in Copilot). User-level config listed only under `~/.copilot` | Verified (run + docs); user `~/.claude/settings.json` not tested |
| Status line | Copilot CLI has a command-type `statusLine` in `~/.copilot/settings.json` (enabled via `/statusline`; examples set `"experimental": true`, and on Windows a `STATUS_LINE` feature flag). Contract, refresh rate and stability not documented in what was read | Flagged (third-party READMEs, e.g. [copilot-cli-custom-statusline](https://github.com/pascalvanderheiden/copilot-cli-custom-statusline), [copilot-cli#1311](https://github.com/github/copilot-cli/issues/1311)) |

## 3. Path forward

**Result first:** AIQ can observe Copilot CLI and VS Code agent sessions with one user-level
collector, but Copilot will not show a card. In-the-moment coaching on Copilot needs a surface AIQ
owns. Imran chose the status line for Copilot CLI tips (point 4); the budget wording (point 7) is still his call.

1. **One Copilot collector for both surfaces.** A single user-level file, `~/.copilot/hooks/aiq.json`,
   using `exec` + `args` (never a shell field), calls the same compiled `aiq hook copilot <event>`
   client as Claude Code. It feeds the core from Copilot CLI and VS Code Copilot agent sessions. Never
   repo-level `.github/hooks/`. *Verified for Copilot CLI; VS Code from docs only.*
2. **Subscribe to as few events as possible, because every hook blocks.** Phase 2 needs prompts and
   turn ends only: `userPromptSubmitted` and `agentStop`. That adds roughly 140–240 ms per prompt
   (Copilot's start-up plus AIQ's client). Tool hooks would add about twice that **per tool call**,
   so they wait until a detector needs them (Opportunities 2 and 3). *Inferred from the measured parts;
   the end-to-end figure is not measured.*
3. **ADR-0005's Copilot render path is wrong and must change.** Rendering via a `progress` line does
   not work. AIQ will **not** use `additionalContext` or exit code 2 to get text in front of the person:
   both act on the agent, and coaching is for the person (ADR-0005 rule 1).
4. **Where Copilot CLI cards show — decided (Imran, 2026-11-01): the status line (a).** A desktop
   notification (b), or a combination, may be revisited later. Options considered:
   - **a) Copilot CLI's status line (recommended to try first):** a one-line nudge such as
     `AIQ tip: reworded 3× without new details · aiq show`, read from the core. It is persistent, in
     the person's view, and needs no window switch. It is experimental, needs a laptop check, and on
     Windows a feature flag.
   - **b) A desktop notification from the core at `agentStop`:** works for any terminal, but it pulls
     attention out of the terminal and some work laptops suppress it.
   - **c) No in-the-moment card on Copilot CLI:** only next-pause and summary coaching.
5. **VS Code renders through the extension.** Hooks observe; the extension reads cards from the core
   and shows a notification plus a status-bar item. It also adds editor and terminal signals, hosts
   `vscode.lm` for the judge, and offers Pause. Ask mode, inline chat and completions stay blind
   spots. Undocumented on-disk chat session files are a possible later source. *Flagged; not a
   contract.*
6. **One hands-on check is still owed: VS Code.** About 5 minutes on Imran's laptop, with a temporary
   `~/.copilot/hooks/aiq-probe.json` that only appends to a temp log and is removed straight after. It
   answers whether the user-level hook fires in a VS Code agent-mode session, and whether any hook
   output reaches the person there. While the file exists, Copilot CLI sessions also run it (about
   150 ms per event).
7. **Budget wording (Imran's call).** The spec says a collector adds "≤ 50 ms p95 to any hook it runs
   in". On Copilot, the host's own process start (110–210 ms) comes with any hook at all. Proposed
   wording: AIQ's own client stays ≤ 50 ms p95 (measured 26–34 ms), and the whole hook, including the
   host's start-up, is measured and shown in `aiq status`.

### Changes this implies

| Artifact | Change |
|---|---|
| ADR-0005 | **Done in this PR:** rule 5 (status line, not a progress line) and rule 5a (minimum events, exec form) |
| Architecture Phase 2 | **Done in this PR:** one Copilot collector + status-line renderer + VS Code extension |
| Status-line check | Run on the laptop before Phase 2 design: enable the status line under a temp `COPILOT_HOME`, confirm it refreshes after each turn and can show a one-line tip from a command |
| Spec NFR | Budget wording (point 7), if Imran agrees |
| Threat model | Copilot reads repo `.claude/settings.json`; `copilot -p` fires user hooks (AIQ must not ingest its own calls) |

### Risks
- VS Code hooks are Preview, and the status line is experimental. Either can change or disappear.
- Organisation policy can restrict hooks. This laptop already has an admin-forced model.
- Hook latency on a busy machine had multi-second outliers in the probe. *Inferred* cause: concurrent
  updates.
