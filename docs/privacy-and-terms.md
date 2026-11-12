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
  The plain-language page for alpha users: what AIQ keeps, where, what leaves the computer (analysis calls
  through the person's own Copilot or Claude Code, and a daily update check to GitHub), who can see it,
  every control, and the terms of the private alpha. Each claim was checked against the code on main;
  what is not built yet is marked "Planned".
---

# AIQ privacy and terms

**Private alpha, version 0.1.0. Last changed 2026-11-12.**

AIQ is a coach for how you work with AI agents such as GitHub Copilot and Claude Code. It runs on your
own Windows computer. This page says, in plain words, what it keeps, what it sends, and what you can
do about it. Anything marked **Planned** is not in this version.

## The short version

- **Only you see your data.** AIQ has no server, no account and no manager or team view.
- **Your prompts stay on your own computers.** AIQ keeps them in a folder only your Windows account can read.
  If you choose **Keep in sync**, they also go to your other computer, and only there.
- **Two things go out, both through tools you already use:**
  1. While analysis is on (installing AIQ turns it on; one switch turns it off), AIQ asks **your own**
     Copilot or Claude Code to analyze your finished tasks.
     That uses your plan, and your employer's rules for that account apply.
  2. Once a day AIQ asks GitHub whether a newer AIQ is out. It sends nothing about you, and you can
     turn it off.
- **You are in control.** Pause, export, delete or uninstall at any time. Uninstalling keeps your data
  until you delete it.

## What AIQ keeps on your computer

When you use a connected tool, AIQ stores:

- **Your prompts**, as you typed them. Before storing a prompt, AIQ replaces common secret shapes
  (API keys, access tokens, private keys, connection strings) with `[secret removed]`. It cannot catch
  every secret, so avoid pasting them. This includes Copilot messages you send while the agent is
  still working: Copilot doesn't pass those to AIQ, so AIQ reads them from Copilot's own history file
  on your computer when the agent stops. Settings > What AIQ keeps turns this off.
- **When things happened**: when you sent a prompt, when the agent stopped, which tool, and the folder
  you worked in.
- **A summary of what the agent did** in each turn. For past sessions (`aiq import`): which tools it
  used, which files it touched and which commands it ran. For live turns: counts only (tool calls by
  tool name, commands, files and helper agents), with no command text or file names. AIQ reads this
  from your tools' own history files on your computer when the agent stops, and only reads them. It
  does not store the agent's replies or your file contents. Settings > What AIQ keeps turns the
  live counts off.
- **Past sessions**, if you run `aiq import` (the installer starts it once). It reads your Claude
  Code and Copilot CLI history from this computer. Runs that look automated (a script or app driving
  the tool) are automated and not counted; `aiq include <folder>` counts a folder again.
- **AIQ's own results**: tips shown, what you said about them, analyses of your tasks, and your
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

### 1. Analysis calls (only while analysis is on)

AIQ places each finished task on a ladder (Assisted, Directed, Delegated, Orchestrated, Factory) and
suggests a next step. To do that, it asks an AI model to read the task.

- **On from install, easy to turn off.** When installing AIQ connects your tools (`install.ps1` or AIQ
  home's first-run setup), it turns analysis on and prints what is sent, where, the daily cap and how to
  undo it. Turn it off any time: the switch in AIQ home **Settings > Analysis**, or
  `aiq config set analysis off`. If you turned it off, installing again keeps it off. `aiq analyze --dry-run`
  shows what would be sent and sends nothing.
- **What is sent:** the task's prompts (with secrets removed) and the summary of what the agent did
  (tools, files, commands). Never your own edits or diffs.
- **Where it goes:** only to the tool that already saw the task, through your own sign-in.
  Copilot tasks go to your Copilot CLI. Claude Code tasks go to your Claude Code. AIQ never sends a
  task to a different provider. If the tool is not available, the task waits.
- **AIQ never reads or stores your sign-in.** It runs your own copy of the tool, which signs in as it
  normally does.
- **Sealed calls.** The analysis call runs with no tools, none of your plugins or MCP servers, and no
  project instructions, so it cannot act on your computer or repos. A Copilot analysis call runs in its
  own empty AIQ folder, which AIQ deletes after reading the answer (to keep them instead:
  `aiq config set analysis-copilot-sessions keep`). A Claude Code analysis call saves no session.
- **It uses your plan.** Each task takes two calls. AIQ analyzes at most 500 tasks a day by default; you
  can set 0 to 500 (`aiq config set analysis-daily-tasks <n>`, or in Settings).
- **Your account's rules apply.** On a work account, your organisation's policies, logging and
  retention cover these calls, as they cover anything else you send through that tool. The provider
  sees the same kind of content it already saw when you did the task.
- **Quick kind label of a new task.** While analysis is on, when AIQ can't tell from your own past
  tasks what kind of task you just started, it sends **that task's first prompt** (secrets removed, cut
  to 4,000 characters) once to the same tool, sealed the same way, with a small model (Claude Code:
  `haiku`). The answer is only a job and a kind of task, so a tip can show when your turn ends. One call
  per new task, at most 20 a day by default (`aiq config set kind-label-daily <n>`, 0 to 500). On
  Copilot each label is one premium request. Turn labels off and keep analysis on:
  `aiq config set kind-label off`.
- **Accuracy check, only when you run it.** `aiq analyze --check` asks a second model, through the same
  sign-in, to re-read a sample of tasks so AIQ can measure how often the two agree. It also uses your
  plan; `--dry-run` shows the cost first.

### 2. The daily update check

- Once a day AIQ asks GitHub's public release page whether a newer version of AIQ is published. It
  sends no data about you. GitHub sees your network address and that a copy of AIQ checked, as with
  any web request.
- A download is installed only if its SHA-256 checksum matches the list published with the same
  release. It is installed when AIQ is idle and AIQ home is closed, and the previous version is kept.
- In Settings choose **Update automatically** (the default), **Tell me first**
  (`aiq config set update ask`) or **Don't check for updates** (`aiq config set update off`). With
  checks off, AIQ makes no network call of its own; **Check for updates now** in Settings, or
  `aiq update --now`, checks once when you ask.
- **Go back to the previous version** undoes an update, and AIQ will not reinstall that release by
  itself.
- To see which version you have: AIQ home shows it at the bottom of the side panel and in Settings,
  under Updates. In a terminal, run `aiq version` or `aiq update`.

### 3. Team tips (only in builds made with a team tips repo)

- Some AIQ builds can add a **Tool to try** line naming a tool your team uses. Those builds download one
  file, `team-tips.toml`, from your team's repo every hour with `git`, using the sign-in Git Credential
  Manager already keeps for that site. AIQ never sees the sign-in, sends nothing about you, and never
  opens a sign-in window by itself. Without a sign-in or access, nothing is downloaded and tips show
  as usual.
- The file's text is never stored in AIQ's database, logged, exported or sent anywhere. The lines show
  on Copilot CLI, VS Code and AIQ home, never on Claude Code.
- Settings, under **Team tips**, shows where the lines come from and has **Stop using**. In a terminal:
  `aiq team-tips` and `aiq team-tips off`.

### 3. Nothing else

- **No telemetry.** AIQ sends no usage statistics, crash reports or analytics to anyone.
- AIQ home makes no network calls.
- Three files are made only when you ask, and stay on your computer until you choose to send them:
  `aiq export` (all your data, prompts included), `aiq status --report` (a help file with counts,
  timings and error codes; no prompt text, and folders shown as `folder-1`, `folder-2`), and the
  **diagnostics file** (AIQ home, **Your data** > **Save a diagnostics file…**, or
  `aiq export --diagnostics`): how AIQ ran over the last 14 days. It holds no prompt text unless you
  tick **Include my prompts** (or add `--prompts`), and never the agent's replies, the analysis's own
  sentences, team tip text, folder or file names (each folder is a letter), your user, account, email or
  computer name, or web addresses other than this project's GitHub page. Log lines and tool messages in
  it are scrubbed of secrets, paths and those names. You can read the whole file before it is saved.

### Sync between your computers (only if you choose it)

- **Off until you choose.** On a computer you reach with Remote Desktop (a Devbox, say), AIQ home offers
  **Keep in sync** when it sees AIQ on the computer you connected from. Nothing moves until you press it.
- **What moves:** everything AIQ keeps, including your prompts, goes to your other computer, so each shows
  your whole history. Your settings, connected tools, folder rules and accuracy checks stay per computer.
- **How it moves:** as files AIQ copies through your Remote Desktop connection's own drive link into AIQ's
  folder on the other computer. No server, relay or cloud folder is involved, and no network port is opened.
- **Deletes follow:** deleting a prompt, or your imported history, on one computer deletes it on the other
  when they next sync. `aiq forget --all` deletes this computer's copy only.
- **Turn it off:** **Settings > Your computers**. Each computer keeps what it already has.
- A computer you paired keeps its copy if it is lost; the store is not encrypted yet (see above).

## Who can see your data

- **You.** No one else: not your manager, your team, your company or the people who make AIQ. There is
  no AIQ server to send it to.
- **The AI provider** sees analysis calls, as described above, under the same account and rules as your
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
| Share how AIQ ran, to get help | AIQ home, **Your data** > **Save a diagnostics file…**; `aiq export --diagnostics` |
| Delete one prompt | AIQ home, **Your data** |
| Delete only imported history | `aiq forget --imported` |
| Delete everything | `aiq forget --all` (this computer; a synced computer keeps its copy) |
| Stop syncing with your other computer | AIQ home **Settings > Your computers** |
| Stop analyzing | AIQ home **Settings > Analysis**, or `aiq config set analysis off` |
| Analyze fewer tasks a day | `aiq config set analysis-daily-tasks <n>` |
| Stop quick kind labels | `aiq config set kind-label off` (analysis off stops them too) |
| Fewer tips | `aiq config set daily-cap <n>` (default 5 a day), or turn one tip off |
| Be asked before updates | `aiq config set update ask` |
| Stop the daily update check | Settings, **Don't check for updates**, or `aiq config set update off` |
| Remove AIQ from your tools | `aiq uninstall --all`. Your data stays; run `aiq forget --all` to delete it too |

When AIQ connects to a tool, it adds its own hook file or hook entries, backs up any settings file it
edits, and lists every file it changed. `aiq uninstall --all` takes out the hooks, the PATH entry and
the Start menu shortcut it added.

## Terms of the private alpha

- **Invitation only.** The alpha is a test with a small group of people. AIQ is free during it.
- **It is early software.** It may be wrong, slow or broken, and it may change or stop without
  notice. Tips and analyses are suggestions, not assessments of you or your work.
- **Windows only, unsigned.** The program files are not code-signed yet (**Planned**), so Windows or
  your company's device rules may warn about or block them. Check your employer's rules before you
  install software on a work computer.
- **Your tools' terms still apply.** Analysis calls run through your own Copilot or Claude Code and count
  as your use of them. Your plan's limits, your provider's terms and your employer's policies apply.
- **This page can change.** Its history is kept in the AIQ repository. If a change affects what AIQ
  keeps or sends, the alpha group will be told before the version that makes it.
- **Questions or problems:** contact imrans.
- **The code is open.** Everything AIQ does is in its public repository,
  <https://github.com/imransargusingh/AIQ>. You can read how it works, check any statement on this
  page against the code, open an issue, or propose a change.
- **License:** not chosen yet. It will be added to the repository and this page before AIQ goes
  beyond the private alpha.

---

*Where each statement comes from:* `docs/adr/0007-rung-judge-via-own-account.md` (analysis), `docs/design/kind-label.md` (kind label),
`docs/design/auto-update.md` (updates), `docs/design/private-alpha.md` (install, export, uninstall,
help report), `docs/notes/note-20261031-encryption-deferred.md` (no encryption),
`docs/security/privacy-review.md`, and the code on main (`crates/aiq/src/judge.rs`, `main.rs`,
`crates/aiq-domain/src/secrets.rs`, `packaging/windows/`). Checked 2026-11-02 against main `6f89c0f`.
