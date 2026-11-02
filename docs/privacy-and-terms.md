---
id: privacy-and-terms
title: "AIQ privacy and terms (private alpha)"
type: doc
status: draft
owner: "Imran"
phase: "alpha"
tags: [privacy, terms, alpha, users]
links:
  - { to: aiq-privacy-review, rel: documents }
  - { to: adr-0007, rel: documents }
  - { to: design-private-alpha, rel: documents }
  - { to: design-auto-update, rel: documents }
  - { to: note-20261031-encryption-deferred, rel: relates-to }
review-by: 2027-02-01
summary: >-
  The plain-language page for alpha users: what AIQ keeps, where, what leaves the computer (judge calls
  through the person's own Copilot or Claude Code, and a daily update check to GitHub), who can see it,
  every control, and the terms of the private alpha. Each claim was checked against the code on main;
  what is not built yet is marked "Planned".
---

# AIQ privacy and terms

**Private alpha, version 0.1.0. Last changed 2026-11-02.**

AIQ is a coach for how you work with AI agents such as GitHub Copilot and Claude Code. It runs on your
own Windows computer. This page says, in plain words, what it keeps, what it sends, and what you can
do about it. Anything marked **Planned** is not in this version.

## The short version

- **Only you see your data.** AIQ has no server, no account and no manager or team view.
- **Your prompts stay on your computer.** AIQ keeps them in a folder only your Windows account can read.
- **Two things go out, both through tools you already use:**
  1. If you turn on judging, AIQ asks **your own** Copilot or Claude Code to judge your finished tasks.
     That uses your plan, and your employer's rules for that account apply.
  2. Once a day AIQ asks GitHub whether a newer AIQ is out. It sends nothing about you.
- **You are in control.** Pause, export, delete or uninstall at any time. Uninstalling keeps your data
  until you delete it.

## What AIQ keeps on your computer

When you use a connected tool, AIQ records:

- **Your prompts**, as you typed them. Before storing a prompt, AIQ replaces common secret shapes
  (API keys, access tokens, private keys, connection strings) with `[secret removed]`. It cannot catch
  every secret, so avoid pasting them.
- **When things happened**: when you sent a prompt, when the agent stopped, which tool, and the folder
  you worked in.
- **A summary of what the agent did** in each turn: which tools it used, which files it touched and
  which commands it ran. AIQ reads this from your tools' own history files on your computer. It does
  not store the agent's replies or your file contents.
- **Past sessions**, if you run `aiq import` (the installer starts it once). It reads your Claude
  Code and Copilot CLI history from this computer. Runs that look automated (a script or app driving
  the tool) are set aside; `aiq include <folder>` brings a folder back.
- **AIQ's own results**: tips shown, what you said about them, judgments of your tasks, and your
  settings.

## Where it is kept, and how it is protected

- Everything lives in `%USERPROFILE%\.aiq` on your computer. The files are readable only by your
  Windows account.
- **The store is not encrypted in this version.** Anyone who can sign in as you, an administrator on
  the computer, or someone with your disk or an unencrypted backup could read your prompts.
  Encryption at rest is **Planned** only if it proves to be needed.
- If AIQ's background process is not running, up to 1 MB of recent prompts wait in a plain file in the
  same folder until it starts again.
- AIQ's log files never contain prompt text.
- **There is no backup and no copy anywhere else.** If you lose the computer or reinstall Windows,
  your AIQ history is gone. Your tools and your work are not affected.
- AIQ keeps your data until you delete it.

## What leaves your computer

### 1. Judge calls (only after you turn judging on)

AIQ places each finished task on a ladder (Assisted, Directed, Delegated, Orchestrated, Factory) and
suggests a next step. To do that, it asks an AI model to read the task.

- **Off until you start it.** Judging starts the first time you run `aiq judge`. That command first
  shows what it sends, then judges in the background from then on. `aiq judge --dry-run` shows what
  would be sent and sends nothing. Turn it off any time: `aiq config set judge off`.
- **What is sent:** the task's prompts (with secrets removed) and the summary of what the agent did
  (tools, files, commands). Never your own edits or diffs.
- **Where it goes:** only to the tool that already saw the task, through your own sign-in.
  Copilot tasks go to your Copilot CLI. Claude Code tasks go to your Claude Code. AIQ never sends a
  task to a different provider. If the tool is not available, the task waits.
- **AIQ never reads or stores your sign-in.** It runs your own copy of the tool, which signs in as it
  normally does.
- **Sealed calls.** The judge call runs with no tools, none of your plugins or MCP servers, and no
  project instructions, so it cannot act on your computer or repos. A Copilot judge call runs in its
  own empty AIQ folder, which AIQ deletes after reading the answer (to keep them instead:
  `aiq config set judge-copilot-sessions keep`). A Claude Code judge call saves no session.
- **It uses your plan.** Each task takes two calls. AIQ judges at most 50 tasks a day by default; you
  can set 0 to 500 (`aiq config set judge-daily-tasks <n>`, or in Settings).
- **Your account's rules apply.** On a work account, your organisation's policies, logging and
  retention cover these calls, as they cover anything else you send through that tool. The provider
  sees the same kind of content it already saw when you did the task.
- **Accuracy check, only when you run it.** `aiq judge --check` asks a second model, through the same
  sign-in, to re-read a sample of tasks so AIQ can measure how often the two agree. It also uses your
  plan; `--dry-run` shows the cost first.

### 2. The daily update check

- Once a day AIQ asks GitHub's public release page whether a newer version of AIQ is published. It
  sends no data about you. GitHub sees your network address and that a copy of AIQ checked, as with
  any web request.
- A download is installed only if its SHA-256 checksum matches the list published with the same
  release. It is installed when AIQ is idle and AIQ home is closed, and the previous version is kept.
- In Settings choose **Update automatically** (the default) or **Tell me first**
  (`aiq config set update ask`). **Go back to the previous version** undoes an update, and AIQ will not
  reinstall that release by itself.
- In this version there is no setting that stops the daily check itself.

### 3. Nothing else

- **No telemetry.** AIQ sends no usage statistics, crash reports or analytics to anyone.
- AIQ home makes no network calls.
- Two files are made only when you ask, and stay on your computer until you choose to send them:
  `aiq export` (all your data, prompts included) and `aiq status --report` (a help file with counts,
  timings and error codes; no prompt text, and folders shown as `folder-1`, `folder-2`).

## Who can see your data

- **You.** No one else: not your manager, your team, your company or the people who make AIQ. There is
  no AIQ server to send it to.
- **The AI provider** sees judge calls, as described above, under the same account and rules as your
  normal use of that tool.
- **Planned:** opt-in, combined statistics across many users, so the team can see whether coaching
  works. They are not built. If they come, they will be off unless you turn them on, will contain no
  prompt text, and can be turned off again.

## Your controls

| You want to | Do this |
|---|---|
| Stop AIQ for a while | `aiq pause`, then `aiq resume` (also in AIQ home) |
| See what AIQ has | AIQ home, **Your data**; `aiq show <card-id>` for one tip |
| Get a copy of everything | `aiq export` |
| Delete one prompt | AIQ home, **Your data** |
| Delete only imported history | `aiq forget --imported` |
| Delete everything | `aiq forget --all` |
| Stop judging | `aiq config set judge off` |
| Judge fewer tasks a day | `aiq config set judge-daily-tasks <n>` |
| Fewer tips | `aiq config set daily-cap <n>` (default 5 a day), or turn one tip off |
| Be asked before updates | `aiq config set update ask` |
| Remove AIQ from your tools | `aiq uninstall --all`. Your data stays; run `aiq forget --all` to delete it too |

When AIQ connects to a tool, it adds its own hook file or hook entries, backs up any settings file it
edits, and lists every file it changed. `aiq uninstall --all` takes out the hooks, the PATH entry and
the Start menu shortcut it added.

## Terms of the private alpha

- **Invitation only.** The alpha is a test with a small group of people. AIQ is free during it.
- **It is early software.** It may be wrong, slow or broken, and it may change or stop without
  notice. Tips and judgments are suggestions, not assessments of you or your work.
- **Windows only, unsigned.** The program files are not code-signed yet (**Planned**), so Windows or
  your company's device rules may warn about or block them. Check your employer's rules before you
  install software on a work computer.
- **Your tools' terms still apply.** Judge calls run through your own Copilot or Claude Code and count
  as your use of them. Your plan's limits, your provider's terms and your employer's policies apply.
- **This page can change.** Its history is kept in the AIQ repository. If a change affects what AIQ
  keeps or sends, the alpha group will be told before the version that makes it.
- **Questions or problems:** contact the person who invited you to the alpha.

---

*Where each statement comes from:* `docs/adr/0007-rung-judge-via-own-account.md` (judge),
`docs/design/auto-update.md` (updates), `docs/design/private-alpha.md` (install, export, uninstall,
help report), `docs/notes/note-20261031-encryption-deferred.md` (no encryption),
`docs/security/privacy-review.md`, and the code on main (`crates/aiq/src/judge.rs`, `main.rs`,
`crates/aiq-domain/src/secrets.rs`, `packaging/windows/`). Checked 2026-11-02 against main `6f89c0f`.
