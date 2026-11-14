---
id: design-kinds-recut
title: "Design: the task Kinds list, re-cut from real work (analysis j7)"
type: design
status: accepted
owner: "Imran"
phase: "5"
tags: [design, rung-model, judge, coaching, kinds, eval]
links:
  - { to: design-coaching-from-rungs, rel: depends-on }
  - { to: design-kind-of-task, rel: depends-on }
  - { to: design-kind-label, rel: depends-on }
  - { to: design-judge-evals, rel: depends-on }
  - { to: design-run-or-ship-pack, rel: depends-on }
  - { to: spec-rung-model, rel: implements }
review-by: 2027-05-14
summary: >-
  The list of task Kinds was a first guess. Measured on Imran's laptop, half of the last 30 days' tasks had
  no named Kind, among them most of his Delegated work. This design re-cuts the list once, as analysis
  version j7: still 27 named Kinds, 11 reworded, 5 new, 5 retired, with the job lines widened to match. Every
  past task whose Kind is not on the new list is analyzed again, once, on the computer that ran it. A task
  the analysis names "Checking on progress" is a progress check and gets no rung.
---

# Design: the task Kinds list, re-cut from real work

Imran asked on 2026-11-14 why a task had no Kind, what the full list was, and whether the list came from
research into what engineers, PMs and data scientists do. It did not: it was a first guess of 2026-11-02
(design coaching-from-rungs, "Pack data"), adjusted twice from his own tasks. The research compared the list
with his analyzed tasks and with published role studies, and proposed one re-cut. He chose **"New list,
re-analyze all unnamed"** on a decision card the same day (19:49 UTC). This note records what ships and why.

Words: the code says `judge` and `task_type`. People read **analysis** and **Kind**. This note uses the
people's words, and the code's names only in code font.

## Goal state

| | |
|---|---|
| **Goal** | A task gets a Kind a person in that role would recognise, so recurring work can be seen moving up the ladder. Past tasks with no named Kind get the same chance, once |
| **Done when** | The new list is the answer contract of the analysis and of the first-prompt label; every past analysis that names no Kind on the list is queued again exactly once; both accuracy sets are re-recorded and their gates pass; Imran approved the merge card that states the cost |
| **Not in scope** | A fifth job. Grouping the Kinds by area of responsibility on screen (proposed in the research, not decided). Kinds for data roles and for email (held until 10 real tasks exist). Changing the rung rules. The word rule for progress checks (#294) |
| **Tier** | T1 (analysis version bump, answer contract change, no schema change) |
| **Fan-out cap** | 0 |

## Grounding

Measured on Imran's laptop on 2026-11-14 (07:58 UTC), read-only and counts only, on the 708 analyzed tasks
Home shows. The counting scripts are in `docs/investigations/rung-groups/`. They rebuild Home's task list
first, because the analysis table also holds analyses for prompts Home no longer counts as task starts.

| Finding | Measured |
|---|---|
| Tasks with a named Kind | 67% all time, 47% of the last 30 days (135 of 285), 25% of the last 7 days |
| The first design's own bar | Re-cut the list if `other` is over 25% of analyzed tasks. Last 30 days: "Other" 29%, no job 24% |
| Advanced work with no Kind | 26 of the 44 tasks at Delegated or higher in the last 30 days |
| Accuracy where a Kind is named | A second model named the same Kind in 62 of 62 checked tasks, and the same job in 84 of 86. The gap is coverage, not accuracy |
| Why a task ends unnamed (150 tasks, 30 days) | The two runs named different things 57. They agreed under the 0.7 bar 53. Both said `other` 34. Runs not recorded 6 |
| What the unnamed tasks are (keyword counts, approximate) | Carrying out or continuing a build plan 33. A one-line notice that a release is out 28. Checking on progress 12. Checking claims against sources 12. Starting or stopping something 6. Commit, pull, push or merge 5 |
| Rules already written and switched off | Would not fix it: the rule that accepts a Kind both runs agree on names 19 of the 150 |

A trial of the new list ran on the laptop the same day (two runs, 19:07 to 20:11 UTC). A test build asked
103 of the laptop's 104 tasks with no sure rung again, two calls per task, and stored nothing. The analysis
prompt in that build is the prompt that ships (checked: `rung::SYSTEM` is unchanged since).

| Trial result | Measured |
|---|---|
| A sure rung under the new list | 58 of 103: 43 Assisted, 5 Directed, 4 Delegated, 6 Orchestrated |
| A named Kind under the new list | 54 of 103 |
| The same tasks under the old list, asked again (62 of them) | 5 get a sure rung; the new list gives one to 33 of those 62. The gain is the list, not the second ask |
| Release work | 21 of the 58 are filed as "Releasing or publishing", all Assisted, 19 of them a single prompt |
| Not monotone | 2 tasks were sure under the old list and not under the new one |
| Cost of the trial | 140 Copilot premium requests and $3.25 of Claude use |

From published studies (read on 2026-11-14 for the research; not read again for this note): role studies
list activities, not kinds of result. Debugging, code review, documentation and planning are tasks of their
own in the DORA 2025 report and the Stack Overflow 2025 survey. The old list had one Kind for all data work
and none for decks, planning, review that changes no code, or steering work already running.

## The list (analysis j7)

27 named Kinds, as before. Against the old list: 11 keep their name, 11 are reworded, 5 are new and 5 are
retired. Each job keeps its `other`. The list of record is `TASK_TYPES` in
`crates/aiq-contracts/src/judge.rs`; the definitions the model reads are in `rung::SYSTEM`.

| Job | id | Kind as people read it | Change |
|---|---|---|---|
| Code a change | `fix_bug` | Fixing a bug, test or build error | Reworded. Takes in lint and build errors |
| | `feature` | Building or adding a feature | Reworded. Covers carrying out or continuing a build plan |
| | `tests` | Writing tests | Same |
| | `refactor` | Refactoring or cleaning up | Same |
| | `deps` | Upgrading or migrating | Reworded. Covers a move to a new version, framework or platform |
| | `scaffold` | Scaffolding or prototyping | Reworded |
| Write a spec or doc | `spec` | Writing a spec or PRD | Same name. Takes in acceptance criteria |
| | `design_note` | Designing an architecture or plan | Reworded. A plan written down as a document |
| | `status` | Writing an update or announcement | Reworded. Covers a newsletter |
| | `research` | Summarising notes or research | Reworded. Takes in notes and transcripts |
| | `howto` | Writing documentation or a how-to | Reworded |
| | `deck` | Preparing a deck or presentation | New |
| Ask or research | `how_to` | Asking how to do something | Same |
| | `explain` | Understanding code or a concept | Reworded. Takes in exploring code |
| | `troubleshoot` | Investigating a bug or problem | Reworded |
| | `compare` | Comparing options | Same |
| | `find` | Looking up a fact | Same |
| | `research` | Researching a topic | Same |
| | `analyse` | Analyzing data | Same |
| | `progress` | Checking on progress | New. The old prompt sent these to `other` |
| | `review` | Reviewing code or other work | New here. Findings, no changed files |
| | `plan` | Planning or brainstorming | New. No document written |
| Run or ship it | `restart_fix` | Getting something running again | Same |
| | `release` | Releasing or publishing | Same name. Covers telling the agent a release is out |
| | `run_job` | Running an app, script or pipeline | Reworded. Covers start and stop |
| | `setup` | Setting up or connecting a tool | Same |
| | `source_control` | Committing, merging or syncing code | New. When no code change was asked for |

Retired, each folded into a neighbour (`RETIRED_TASK_TYPES`): `code_change/build_fix` into `fix_bug`,
`code_change/review` into `ask_research/review`, `code_change/explain` into `ask_research/explain`,
`write_doc/criteria` into `spec`, `write_doc/notes` into `research`. Measured: the five had 1 use between
them in the 708 tasks.

## Decisions

| # | Question | Decision | Why |
|---|---|---|---|
| K1 | One version or several? | **One, `j7`.** All list and job-line changes ship together | Each new version decides again which past tasks to analyze. Several small versions would cost more than one |
| K2 | What is a Kind named after? | **The work, in the role's words.** Not how it was handed over. So there is no "Building from a plan" Kind: `feature` covers it | The rung already measures the hand-over. A Kind must stay the same while its rung moves, or no Kind can be seen moving up |
| K3 | How long may a name be? | **Short enough for the status line.** The longest is 35 characters; with today's tip texts there is room for 36. Eight names are shorter than the research proposed | A name is printed inside a tip and a milestone on the Copilot CLI status line, which cuts a long line. Two tests fail on a name that no longer fits: `aiq-core` `pack.rs` (tip line, 125 characters) and `tests/milestone_note.rs` (milestone, 104) |
| K4 | What about a task that spans two jobs? | **One fixed order** in the prompt: changed code, then something run or shipped, then a document, then an answer. The job lines are widened to name build plans, release follow-ups, progress checks, reviews, planning and source control | Measured: 34 of the 92 tasks with no job were splits between two jobs. A task with no job gets no rung |
| K5 | What happens to a stored retired Kind? | **Its label still resolves** (`task_type_label` reads both lists) until the task is analyzed again. A new answer may not name it. The analysis parse refuses an id no job offers (`build_fix`, `criteria`, `notes`). A retired pair whose id lives on under another job (`code_change/review`, `code_change/explain`) is placed as `other`, and the rules that accept a Kind never give one (`task_type_named`). The label parse refuses any pair its job does not offer | The store is append-only, so old rows keep old ids. One function decides "named on today's list" for the placement and the re-analysis rule |
| K6 | Which past tasks are analyzed again? | **Every task whose current analysis names no Kind on today's list**, once: no job, the Kind `other`, no Kind recorded, or a retired Kind (`list_outdated` in `crates/aiq/src/judge.rs`). An analysis already at `j7` is current whatever it named. A named Kind the list kept is never sent again | Imran's choice. The trial shows why: the new list named a Kind for 54 of 103 such tasks. A kept name is not worth a model call. Retired Kinds are included so that the rule is "not on the new list", with no exception to explain; that is about 1 task on the laptop |
| K7 | Who pays, and when? | **Each computer analyzes its own tasks**, inside its own daily limit (500 tasks a day by default). The background check runs 2 minutes after AIQ starts and then every 10 minutes, while analysis is on and not paused. Order: tasks with no job first, then tasks never analyzed, then the rest, newest first | Existing queue (`queue` in `crates/aiq/src/judge.rs`); nothing new is built. A task is never analyzed, or paid for, on two computers |
| K8 | The first-prompt label | **Prompt version `label-3`.** The label prompt cuts its job and Kind lines from the analysis prompt, so it changes with the list. Past tasks are not labelled again | One list for both readers. A label reads only new tasks |
| K9 | Is "Checking on progress" a Kind AIQ coaches? | **No.** A task the analysis names `progress`, read as a sure Assisted with no goal given, is a progress check: no rung of its own, never coached (`coach::is_progress_check`). The word rule of #294 stays for analyses older than `j7` | Imran chose on 2026-11-14 that progress checks get no rung. Without this, three such tasks in 30 days become a coached Kind: a Home snapshot test showed the row reading "coach". The words found 55 of his 68 progress checks, so the Kind can catch the rest. Marked `assume:` in the code, to be confirmed by the second-reader check |
| K10 | A one-line release notice | **No rung, like a progress check** (Imran's card, 2026-11-14 20:05 UTC). **Not built yet**: the words that tell a notice from a real release request must be counted on real tasks first (`docs/investigations/rung-groups/release_notice_match.py`, read-only, counts only) | In the trial, 21 of the 58 tasks that gained a rung were release work at Assisted. Counted as tasks, they would pull the ladder down. A word list drawn from memory would be a guess |
| K11 | How does a screen know that a task has no rung? | **One field that names the reason** (`no_rung`: `progress_check` today, a second id when K10 is built), in place of the yes-or-no flag of #294. Every screen shows the reason where the rung would be: Home's counts, the task list, the Day view and the task's own page. The rung, the next rung and the next step the analysis gave stay stored and are never shown for such a task | A second reason is coming (K10), and one flag per reason lets a screen honour one and miss the other. Building it found two places #294 had missed: the Day view drew a progress check as an Assisted block, and the task's page gave a next step under "AIQ won't coach on it" (defect class DC-083). A screen test now opens each of these screens on a task with no rung, in the three themes |

## Change surfaces

| Surface | Change |
|---|---|
| Store | None. `judgment.task_type` holds the id. Old rows keep retired ids |
| Contracts (`aiq-contracts` `judge.rs`) | `TASK_TYPES` re-cut; `RETIRED_TASK_TYPES`; `task_type_offered` (a new answer may name it); `task_type_named` (offered and not `other`) |
| Analysis prompt and placement (`aiq-domain` `rung.rs`) | `SYSTEM` job and Kind lines; `JUDGE_VERSION` `j7`; `j6` joins `JUDGE_VERSIONS_KEPT`; `kind_other_cause` and `candidate_type` use `task_type_named`. A test holds that the prompt offers exactly the list |
| Re-analysis (`aiq` `judge.rs`) | `current_version` reads `list_outdated`. `aiq analyze --versions` prints, per tool, why each outdated analysis is sent again |
| Label (`aiq-domain` `label.rs`) | `PROMPT_VERSION` `label-3` |
| Coaching (`aiq-domain` `coach.rs`) | `is_progress_check` also takes the analysis's Kind. `no_rung` gives the reason a task gets no rung (`NoRung`). Both entry points ask it: Home's task list and the core's coaching read |
| Read model (`aiq-home`) | `task_type_label` resolves retired ids. `TaskJudgment.no_rung` carries the reason's id in place of the `progress_check` flag (K11). The counts keep their names |
| Home (`apps/home`) | The new names. The task list, the Day view and the task page read `no_rung`: the reason in place of the rung, a plain block with a dash in the Day view and one more entry in its key, no next step on the task page. Fixtures regenerated; a new one, `no-rung`, holds tasks with no rung of their own. The screen tests pin the new names and open each screen on such a task |
| Accuracy sets | `evals/judge/golden.json` `g2` (42 cases), `evals/label/golden.json` `l2` (67 prompts), both recordings |
| Docs | This note; coaching-from-rungs "Pack data"; judge-evals; kind-label; the cost lines in the welcome page and the privacy page (an update can analyze some past tasks again, once, inside the daily limit); the README's tip example; the README screenshots, retaken from the test data (`npm run readme-shots`) |

## Measured: the accuracy sets

Both golden sets were re-cut from the list's definitions **before** any `j7` answer was recorded, so no label
was fitted to an answer. In the analysis set six cases are relabelled for the new list and eight are new, one
per new or widened Kind. The label set takes the first prompts of the same fourteen and relabels two more of
its own (`p03`, `p08`). Both recordings are live calls on the synthetic cases only, through Claude Code in the
cloud, two runs per case. They used no premium requests. Expected values are the case author's, so these
figures are **Inferred** until a person labels the cases.

Analysis (`claude-sonnet-5-5`, 84 calls, all answered, $1.48), on the 34 cases both recordings hold:

| Measure | `j6` on `g1` (2026-11-13) | `j7` on `g2`, the recording | `j7` on `g2`, a second run |
|---|---|---|---|
| Job right | 30 of 34 | 31 of 34 | 31 of 34 |
| Kind right | 24 of 32 | 26 of 32 | 27 of 32 |
| Rung exact | 23 of 32 | 21 of 32 | 22 of 32 |
| Rung within one | 29 of 32 | 30 of 32 | 30 of 32 |

The 8 new cases, in both runs: job 8, Kind 8, rung exact 6, rung within one 8 of 8. Injection resisted 3 of
3. The second run was made to measure run-to-run movement: between two identical runs, 6 of the 42 cases
showed a different answer and each total moved by at most 1. So the Kind gain (2 to 3 cases) is at the edge of
that movement, and the rung change (1 to 2 cases) is inside it, with one exception that is the list's doing:
h12, a review, now settles as a question and shows the model's own rung, one above the author's label. Under
`j6` its job did not settle.

First-prompt label (`claude-haiku-5-5`, 134 calls a run), on the 59 prompts both sets hold (118 answers):

| Measure | `label-2` on `l1` (2026-11-08) | `label-3` on `l2`, the recording | `label-3` on `l2`, a second run |
|---|---|---|---|
| Answers that parse | 118 | 117 | 118 |
| Job right | 112 | 109 | 112 |
| Kind right | 108 | 102 | 105 |
| Confident labels, all with the right job (the bar that lets a label coach) | 49 | 50 | 52 |

The 8 new prompts are right on job and Kind in all 16 answers of both runs. Over all 67 prompts every
confident label named the right job: 58 of 58 in the recording, 62 of 62 in the second run. Between the two
`label-3` runs, 4 of the 67 prompts got a different verdict; job right and Kind right each moved by 3 and the
confident count by 4. So the fall in job right is inside that movement. Two Kind misses hold in both runs and
are the list's doing (Inferred from the new wording): a migration carried out from a spec reads as `feature`,
and a tool setup that edits a config file reads as `code_change` with the Kind `other`. Labels coach by job,
so neither changes a tip. One answer in the recording named a Kind from another job's list. The strict parse
refuses it, so live it would be no label and no tip. `label_replay_gate` asked that every recorded answer
parse. It now asks for 19 in 20 and pins the exact count, so the recording is kept as measured, not run again
until clean (design kind-label has the detail; defect class DC-081).

The live trend (no model call) is re-measured on `g2`: exact 27, within one 36, higher than the label 8 of 40
(`trend_matches_golden`). It reads only the log, so a review or a plan asked in a long first prompt reads as
handed over whole.

## Cost, and how to measure it

Forecast for Imran's laptop, from the store as of 2026-11-14 07:58 UTC: 212 tasks are analyzed again (125
with an "Other" Kind, 87 with no job). That is about 312 Copilot premium requests for the 156 tasks Copilot
analyzes and about $1.20 of Claude use for the other 56. It fits inside one day's limit. Read it as a ceiling
plus the tasks that arrived since. Earlier forecasts in this project missed both ways. Every other computer,
and every other install, re-analyzes its own tasks at its own user's cost, under its own daily limit.

Measure, do not model: `aiq analyze --versions` is read-only and prints, per tool, how many analyses are
outdated and why ("no job", "the kind other", "no kind recorded", "a kind the list retired"). Run it before
the update for the size and after the queue empties for the proof. `judge.log` holds each call. Home shows
the re-analysis only as today's count of analyzed tasks: its "waiting" number counts tasks never analyzed.

## Failure modes

| Failure | Result | How it shows |
|---|---|---|
| Re-analysis gives a task a different rung, or none | The newest analysis wins, as for any re-analysis. In the trial 2 of 62 lost a sure rung | Compare the ladder counts before and after (`kinds_home.py`) |
| The daily limit is reached part-way | The rest waits for the next day. Old analyses keep showing | Home: today's count at the limit |
| A re-analysis call fails | The task keeps its old analysis and is tried again under the existing retry rules (three failed tries, then it stops) | `judge.log`, the task page |
| The model names a retired Kind | An id no job offers: the call's outcome is `schema`. A pair another job offers: the Kind is `other` | `judge.log`; the task page |
| "Checking on progress" is given to a real question | The task leaves the ladder and coaching (K9) | The second-reader check on this Kind; under 8 in 10, take the Kind out of the rule |
| A wide Kind pulls in its neighbour (`feature` took a migration in the label set) | A wrong Kind with the right job. Labels coach by job, so the tip is the same | The label self-check; `aiq analyze --check` |

## Gaps that stay open

- **The release-notice rule (K10)** waits for the word count on the laptop.
- **Tips at task start.** A new task's tip is decided before any analysis exists, so neither progress-check
  rule can hold it back. A first prompt labelled `progress` can still get its job's tip once question labels
  earn tips (today they are held). This was so before this change. It belongs to the tip path, not the list.
- **Home does not say it is analyzing past tasks again.** Only the day's count moves.
- **One accuracy check after the re-analysis** (about 60 premium requests): keep each changed Kind only if a
  second reader agrees at least 8 in 10. It also settles the `assume:` in K9.
- **Human labels** for the 8 new golden cases.
- **No gate keeps the README screenshots current.** They still showed the old names after the text sweep, and
  were four versions old (defect class DC-082). They are retaken here by hand.

## Gate record

| Gate | Result |
|---|---|
| Red first | The re-analysis rule, the retired-Kind placement, the golden validity checks and the progress-check Kind each failed before the change and pass after. So did the screen test for a task with no rung: it failed on the Day view, in each theme |
| `cargo test --workspace` | Pass |
| `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check` | Clean |
| Home: `npm test`, `npm run build`, `npm run e2e` | 89 unit tests and 111 screen tests pass; fixtures regenerate with no diff |
| Replay gates | Both pass on recordings made with today's prompts. No test is skipped. Two bars were set again from measurement, each named above: the label gate's "every recorded answer parses" is now 19 in 20 with the exact count pinned by the totals, and the live-trend floors are re-measured on `g2` |

## Status

| | |
|---|---|
| **Completed** | The list, the prompt, the re-analysis rule, the label prompt, both accuracy sets and recordings, Home test data, the progress-check Kind rule, one "no rung, and why" field for every screen (K11) |
| **Remaining** | The release-notice rule (K10), after the word count. The refreshed cost forecast. Imran's go-ahead on the merge card |
| **Next** | After the merge: `aiq analyze --versions` and the counting scripts before and after the re-analysis; the accuracy check; the same counts on his other computers |
