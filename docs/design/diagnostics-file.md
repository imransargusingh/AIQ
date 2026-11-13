---
id: design-diagnostics-file
title: "Design: the diagnostics file"
type: design
status: accepted
owner: "Imran"
phase: "5"
tags: [design, privacy, diagnostics, instrumentation]
links:
  - { to: design-tip-funnel, rel: relates-to }
  - { to: design-kind-label, rel: relates-to }
review-by: 2027-05-10
summary: >-
  One file a person can save from Your data (or `aiq export --diagnostics`) and choose to share: how AIQ ran over
  the last 14 days, scrubbed of names, paths and addresses, with no prompt text unless they tick "Include my
  prompts". AIQ sends it nowhere.
---

# The diagnostics file

**Why.** A problem on someone else's computer can't be measured from here. A devbox install showed 15 of 15 new
Copilot CLI tasks ending at "Kind label failed", but Claude can't reach that machine, and the person shouldn't have to
copy files or run commands. `aiq export` copies everything, prompts included, so it is a private copy and not meant
for sharing. `aiq status --report` is text for a person to read, not per-task data.

**Decision (Imran, 2026-11-10).** A **Save a diagnostics file…** button on Your data, and `aiq export --diagnostics`.
There is one choice, **Include my prompts**, and it is off by default. Nothing is uploaded.

## What the file holds

`aiq.diagnostics.v1`, pretty-printed JSON, last 14 days (`DAYS`):

| Section | Holds |
|---|---|
| `about` | format, time made, window, whether prompts are in, what is left out |
| `machine` | OS family, CPU architecture, UTC offset |
| `aiq` | version, whether the core answered, paused, setup done, updater state (with the failure `reason` and update.json itself, every text value scrubbed), AIQ's folder (scrubbed) |
| `tools` | each tool: present, connected |
| `settings` | every current setting; project names only as a count (their keys are folder paths) |
| `counts` | schema version, events, tasks, set-aside sessions, store size |
| `label_calls` | label calls grouped by tool, outcome and the tool's own message |
| `analysis_calls` | analysis calls grouped by tool, outcome and stable code |
| `tip_path_ends` | how many tasks ended at each tip-path step |
| `tasks` | per task: id, tool, origin, start, folder letter, prompt count, word guesses, labels (outcome, model, job and kind with confidences, ms, message), analysis numbers (job, kind, rung, confidences, state, fact values, versions), analysis calls, tip path, tips (what matched: detector, opportunity and score; decision, reason code, pack version, tip id and its wording as the card held it; shown, opened, answer); `prompt_text` only when chosen |
| `set_aside` | set-aside sessions: tool, folder letter, reason, time |
| `logs` | last 200 lines of label.log, judge.log, hook-errors.log, upgrade-log.txt and upgrade-log-held.txt, and aiq.log's warnings and errors, scrubbed (DC-071: a failed update is diagnosable from the file alone) |

## Never in the file

The agent's replies. The analysis's sentences (evidence, gap, placement, blockers, what was read): the model wrote
them about the person's work, so they can name it. Team tip text (team tips show as tip ids only; their "Tool to
try" line is added when a card is drawn and never stored). Built-in tip wording is in: it is pack text with the
person's count or kind filled in, never a quote from a prompt, and it is what's needed to judge whether the right tip
showed (Imran, 2026-11-11). Folder and file
names: each folder becomes a letter, new in every file. User, account, email and computer names. Any web address
except `https://github.com/imransargusingh/`, and the team tips address in every form (URL, host, path).

## Scrubbing (`aiq_domain::scrub`)

This runs on every free-text line: log lines, tool messages, setting values, and prompts when they are chosen. The
order is: secrets (`secrets::redact`), then the team tips address, then the profile folder in every spelling
(→ `%USERPROFILE%`), then web addresses (→ `<url>`), then emails (→ `<email>`), then other absolute paths
(→ `<path>`), then the computer name and the user name as whole words. Lines are cut to 300 characters and prompts
to 4,000.

Residual risk: a tool's error message can echo part of a model answer, and a prompt (when chosen) can name a
project, a person or a customer. The cap, the scrub and the preview are the mitigation. The dialog shows the whole
file before saving, and a warning appears when prompts are ticked.

## The label call's own words

`kind_label` stores only the outcome. The tool's message now goes to label.log as `why <task> <message>`. That log
is local, owner-only and bounded, like judge.log, and the message is never stored in the database. The file joins it
to each failed label, so "15 × error" comes with the reason.

## Home

The dialog has two lists ("In the file" and "Left out"), the checkbox, "See the file" with the full text, and Save to
Downloads. Save writes exactly the text the dialog showed, and never overwrites an existing file (`-2`, `-3` are
added). After saving, a line reads "Saved <name> in <folder>. AIQ sent it nowhere." with **Show in folder**. The
window never passes a path: the shell remembers the file it saved. The button also shows on an empty Your data
page, because a broken capture is exactly when the file helps.

## Tests

- `crates/aiq-domain/src/scrub.rs`: each scrub rule, plus lines that must stay unchanged.
- `crates/aiq-home/tests/diagnostics.rs`: without prompts, no seeded prompt, analysis sentence, folder,
  project name or personal name reaches the file. With prompts, the prompts are in and the rest stays out. Also
  folder letters, a failed label's scrubbed message, a missing store, and that saving never overwrites.
- `crates/aiq/src/label.rs`: a failed label writes its `why` line, and a label that worked writes none.
- `apps/home/tests/e2e.mjs`: off by default, the preview holds prompts only when ticked, the saved line, Show in
  folder, a failed save, the button on an empty store, and no axe violations in light and dark.
