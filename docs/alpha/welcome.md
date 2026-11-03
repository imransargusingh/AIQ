---
id: alpha-welcome-note
title: "Welcome to the AIQ private alpha (note to testers)"
type: doc
status: draft
owner: "Imran"
phase: "alpha"
tags: [alpha, users, onboarding, privacy]
links:
  - { to: privacy-and-terms, rel: depends-on }
  - { to: design-private-alpha, rel: documents }
review-by: 2027-02-01
summary: >-
  The one-page note Imran sends to private-alpha testers: what AIQ is, how to install and fully undo
  it, what leaves the computer and how to switch each part off, how to send feedback, and what the
  first days look like. Wording matches docs/privacy-and-terms.md and packaging/windows/README.txt.
---

# Welcome to the AIQ private alpha

Thanks for trying AIQ. This page is everything you need. It takes two minutes to read.

## What AIQ is

AIQ is a private coach for how you work with AI agents such as GitHub Copilot. It is not a report
card and not a diagnostics tool. It helps you build better habits, one small step at a time.

AIQ places each finished task on a five-step ladder: **Assisted, Directed, Delegated, Orchestrated,
Factory**. When it sees an easy next step for the kind of task you just did, it shows one short tip
in your tool, marked **AIQ tip:**. The goal is that, over a few weeks, the tasks you repeat move up a
step. Only you see any of this. There is no manager view, no team view and no AIQ server.

## Install (Windows, about two minutes, no admin rights)

1. Download `aiq-0.1.0-windows-x64.zip` from
   <https://github.com/imransargusingh/AIQ/releases/latest> and unzip it anywhere.
2. Double-click `install.cmd` in that folder. A window shows each step and waits for you to close it.

   If you prefer a terminal, run this in a normal PowerShell window in that folder:

   ```
   powershell -ExecutionPolicy Bypass -File .\install.ps1
   ```

   Typing `.\install.ps1` on its own does not work: Windows refuses it because the script is not signed.

That is all. AIQ connects to the AI tools it finds, lists every file it changed, reads your past
sessions on this computer, turns on judging (below) and opens AIQ home. There is nothing to set up.

The program files are not code-signed yet, so Windows or your company's device rules may warn you.
Check your employer's rules before installing on a work computer.

**To undo it fully:** `aiq uninstall --all` takes AIQ out of every tool and removes its PATH entry
and Start menu shortcut. Your AIQ data stays until you also run `aiq forget --all`.

## What leaves your computer

Your prompts are stored only on your computer, in `%USERPROFILE%\.aiq`. AIQ sends no telemetry. Three
things do go out:

| What | Where it goes | What it costs you | Turn it off |
|---|---|---|---|
| **Judging your finished tasks.** On from install. AIQ sends the task's prompts (secrets removed) and a summary of what the agent did. Never your files or diffs. | Only to the tool that did the task, through **your own** sign-in, in a sealed call with no tools, plugins or MCP servers. | Two calls per task, at most 50 tasks a day by default. On Copilot each call counts against your plan's premium requests. | AIQ home **Settings > Judging**, or `aiq config set judge off` |
| **A quick label of a new task.** Only while judging is on, and only when AIQ can't tell the kind of task from your past ones. It sends that task's first prompt once (secrets removed). | Same tool, same sign-in, sealed the same way, with a small model. | At most 20 a day by default. On Copilot each label is one premium request. | `aiq config set kind-label off` (judging off stops it too) |
| **A daily update check.** | GitHub's public release page. It sends nothing about you. | Nothing. | Settings **Don't check for updates**, or `aiq config set update off` |

On a work account, your organisation's rules for that account cover these calls, as they cover
anything else you send through it. Full details: [AIQ privacy and terms](../privacy-and-terms.md)
(<https://github.com/imransargusingh/AIQ/blob/main/docs/privacy-and-terms.md>).

## What to expect in the first days

- **Day one is mostly quiet.** AIQ reads your past sessions and judges your finished tasks in the
  background. You may see no tips at all at first. That is normal, not a fault.
- **Tips are rare on purpose.** At most 5 a day, only when AIQ is confident, and never in the middle of
  your work. It shows a tip when the agent stops, or on the Copilot CLI status line.
- **Some tips will be wrong.** This is early software. If a tip or a judgment doesn't fit how you work,
  press **This doesn't fit** in AIQ home. AIQ also checks itself and pauses a kind of tip when its own
  accuracy drops.
- **AIQ updates itself** about once a day when there is a new version, while AIQ home is closed. Your
  data and settings stay.
- **Need a break?** `aiq pause`, then `aiq resume` when you're ready.
- **Progress takes weeks.** AIQ needs several tasks of the same kind before it can show one moving up
  a step.

## Feedback and help

Tell us anything: what helped, what annoyed you, what was wrong. Contact **imrans**.

If something breaks, run `aiq status --report` and send the file it makes. It has counts, timings and
error codes. It has no prompt text, and folders appear as `folder-1`, `folder-2`. Nothing is sent
unless you send it.

The code is public, so you can check anything on this page or open an issue:
<https://github.com/imransargusingh/AIQ>.
