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
  What AIQ can observe and show on Copilot CLI and VS Code, and the path forward. Key finding: VS
  Code's Copilot agent sessions share Copilot CLI's hook harness and user-level hook files, so one
  Copilot collector covers both; ask mode, inline chat and completions are not observable.
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

Run-level results (laptop probe): _pending_.

## 3. Path forward (draft, to confirm with run results)
1. **One Copilot collector, two surfaces.** A single user-level hook file in `~/.copilot/hooks/` feeds the
   core from both Copilot CLI and VS Code Copilot agent sessions. Same `aiq hook` client as Claude Code
   (ADR-0005).
2. **The VS Code extension is mostly a renderer.** Hooks give the observation; the extension shows cards
   (notification + status bar), adds editor and terminal signals, hosts `vscode.lm` for the judge, and
   provides Pause. It cannot see Copilot ask mode, inline chat or completions.
3. **Blind spots become summary-only or out of scope:** ask-mode chat, inline chat, completions.
   Undocumented on-disk chat session files are a possible later source (Flagged; not a contract).
4. **Risks:** VS Code hooks are Preview; org policy can disable hooks; the Copilot harness may render
   nothing to the person, so VS Code cards must come from the extension, not hook output.
