---
id: design-first-run-analysis
title: "Design: first-run analysis state on Home"
type: design
status: accepted
owner: "Imran"
tags: [design, aiq-home, analysis, first-run]
links:
  - { to: design-aiq-home-app, rel: extends }
  - { to: design-rung-judge, rel: depends-on }
---

# First-run analysis state on Home

**Trigger.** 2026-11-05, an outside user on 0.1.4 (Copilot only) saw "Not judged yet" over 221 imported tasks
and could not tell what AIQ was doing or whether to act. Imran picked option A of the mockups
(`/mnt/project-files/first-run/first-run-options.html`, project files) on the card 01:26Z.

## What was wrong (read from the code)

- Analysis runs from the core's kicker (first check 2 min after core start, then every 10 min). On 0.1.4 the core
  starts only from a hook, so nothing ran until the first prompt after setup.
- Up to 50 tasks a day (the old default), newest first: 221 tasks took about 5 days. Each task is 2 calls through
  the person's own tool. Imran raised the default to 500 a day (01:40Z 11-05), so a first install's history is
  analyzed in a day.
- A missing or signed-out tool was recorded as a call outcome and in `judge.log`; Home never said so.

## Design (option A)

- **Read model.** `home.analysis` over Home's 30-day window, so "of N" matches the weekly chart:
  `state` (off · paused · blocked · working · today_done · done), `waiting` (finished, a tool AIQ analyzes
  through, no analysis, not given up after 6 failures), `analyzed`, `today`, `daily`, `done_by_ms` (the only
  forecast: waiting at `daily` a day after today's share), `blocked` (the provider whose newest call was
  `missing` or `not_signed_in`, only when its tasks wait).
- **AIQ starts it.** When `state` is working and no call ran for 10 minutes, Home starts `aiq judge --auto`
  hidden, at most once per 10 minutes per Home. The judge lock keeps it to one run with the core's kicker.
- **Card.** "Tasks agents finished" until 10 analyzed tasks: a headline for the state, a progress bar
  ("7 of 221 analyzed · about 500 a day, all by today"), "✓ Nothing to do.", early results (the rung mix so
  far), and how many analyzed tasks the numbers need. Blocked: the reason, one button (Sign in to Copilot opens
  `copilot` in a new terminal; a missing tool opens its install page), and "AIQ tries again by itself every 10
  minutes". With one project picked the card says only "Not enough analyzed tasks yet" (progress is global).
- **Rung header.** One status line under the rung until all tasks are analyzed.
- **Getting started.** Its three judging steps become one analysis step that says the same as the card.
- **Wording.** New text says analyze/analyzed (Imran 01:12Z 11-05). Internal names stay.

## Not done

- The core starting from Home (PR #128) is separate; with it the kicker runs too. This change does not depend on it.
