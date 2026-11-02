---
id: design-judge-accuracy-check
title: "Design: accuracy check of the rung judge on the person's own tasks"
type: design
status: accepted
owner: "Imran"
phase: "5"
tags: [design, rung-model, judge, accuracy, eval, instrumentation, copilot-cli]
links:
  - { to: design-rung-judge, rel: depends-on }
  - { to: design-coaching-from-rungs, rel: depends-on }
  - { to: spec-rung-model, rel: implements }
  - { to: aiq-architecture, rel: implements }
review-by: 2027-05-02
summary: >-
  AIQ checks its own judge on the person's real tasks. `aiq judge --check` takes a random sample of
  judged tasks, has a second, stronger model read each one blind through the person's own Copilot or
  Claude Code, and stores that reading. AIQ derives agreement on job, kind of task, rung and each fact,
  shows it with "checked by a second model, not by you", and stops coaching a kind of task it can't
  get right. Four more numbers need no grader and are measured directly from the store.
---

# Design: accuracy check of the rung judge

Produced with AI-Forward `/design-slice`. Imran picked this slice on 2026-11-02 ("Accuracy check on my
tasks first", next-slice card) over the private alpha. Design only. No code in this change.

## Goal state

| | |
|---|---|
| **Goal** | Know, with numbers marked measured or Inferred, how often the judge gets Imran's real tasks right: job, kind of task, rung, and each fact |
| **Done when** | (design) merged with its gate record; (build) one `aiq judge --check` run on Imran's laptop prints the report below, Home shows it, and the numbers land in the rung-judge design's eval section |
| **Not in scope** | Tuning the judge prompt or rules (next slice, driven by these numbers); a person labelling tasks (Imran: human triage is unreliable and a burden); the private alpha (deferred, design kept on branch `claude/project-thread-wjefak`) |
| **Tier** | T2 (new judge calls on the person's data; new stored fact) |

## Grounding

- Rung-judge design, eval (measured 2026-11-02): 20 **synthetic** held-out tasks, exact 10/20, within
  one 15/20, job 18/20. 11/20 Uncertain, 6 of them from F6. The design says: "Measure on Imran's real
  Copilot tasks first."
- Fact-check rerun (PR #27, 15 hand-picked real Copilot sessions, model-graded): model facts 84/93; F4
  10/15, F6 can't be settled on 9/15. Done by copying sessions into the cloud. That route does not scale
  and is not something AIQ users can do.
- Coaching design: task-kind agreement is **not measured** (needs an eval rerun); thresholds are guesses.
  The live type-guess self-check exists (auto-off below 8 of 10).
- Rung-judge design, conformance notes: "Not measured yet: how often the judge picks `ask_research`
  correctly" (pack added in PR #33).
- Project rules: data stays on the device; judge calls go only to the provider that already saw the task,
  under the person's own sign-in (ADR-0007); AIQ decides itself with an easy override (Imran).

## Decisions

| # | Question | Decision | Why |
|---|---|---|---|
| K1 | Where does the check run? | **In the product**, on the person's machine: `aiq judge --check`. No copying of sessions or the store to the cloud | Same privacy path as the judge (ADR-0007). Works for every future user, so accuracy becomes a measured property of AIQ, not a one-off study |
| K2 | What is the reference answer? | A **second reading** by a stronger model, **blind** (it never sees the judge's answer), with a fuller transcript (up to 40,000 characters, as in PR #27) and the option to say *can't settle* per fact | No person labels (Imran's rule). Blind avoids anchoring. All grader-based numbers are labelled **Inferred: checked by a second model, not by you** |
| K3 | Which model reads? | Same provider as the task. Copilot tasks: sealed `copilot -p` with the strongest model the person's plan allows (setting `check-model`, default: Copilot's own pick). Claude Code tasks: `claude -p --model opus` | ADR-0007: never a provider that didn't already see the task. If the plan forces one model (Imran's admin forces `gpt-6.1-sol`, measured), the report says the grader is the same model as the judge |
| K4 | Which tasks? | A seeded random sample of judged tasks (latest judgment per task, automated sessions excluded), **stratified by job** so each pack gets at least 5 when it has them. Default 30, `--sample <n>`. Seed and list stored | Random, so the result is not hand-picked like PR #27. 30 is a guess at a useful floor for a first read; the report prints the interval so a small sample can't look precise |
| K5 | Cost? | One grader call per task. Counts against the judge's daily budget (50 a day). Copilot: about 1 premium request and ~11 s a call (measured, run 3). `--dry-run` prints the sample and the cost estimate first | The person sees the cost before spending it |
| K6 | What happens with the numbers? | (a) Report in the CLI and on Home > Status. (b) **AIQ acts by itself:** a kind of task whose checked agreement is under 8 of 10 (at least 5 checked) stops being coached, with "AIQ paused this: its judge agreed with a second reading on n of m". Override: `aiq on-kind <job>/<type>` | Imran: AIQ decides, easy override. Same bar as the type-guess self-check, so one rule, not two. The bar is a guess until the first run |
| K7 | Re-run? | `aiq judge --check` again draws a new sample; earlier checks stay. A judgment re-made by a newer judge version is checked again only in a new run | Append-only history; agreement is derived per judge version |

## Numbers in the report

**Measured from the store (no grader, exact):**

| # | Number | Source |
|---|---|---|
| N1 | Uncertain rate, per job, and which fact caused it | `judgment.state`, `facts` |
| N2 | Repeat stability: on a re-judge of the same sample by the same judge, how often job, kind and rung come out the same | a second judge run on 10 of the sampled tasks (K5 budget) |
| N3 | Live kind-guess hit rate (nearest neighbour vs judged kind) | `type_guess` (already stored) |
| N4 | Tip feedback: helpful, not helpful, snoozed, off, "This doesn't fit", per move | `tip_feedback`, `guidance_flag` |

**Agreement with the second reading (Inferred, labelled so):**

| # | Number |
|---|---|
| A1 | Job agreement (incl. `ask_research`) |
| A2 | Kind-of-task agreement, per job |
| A3 | Rung exact and within one; model rung and final rung separately |
| A4 | Per fact: agree, disagree, can't settle (F4, F6 and F13 called out) |
| A5 | Confident judgments only: the same three numbers, since only these are coached |

Each rate prints as `k of n` with a 90% Wilson interval. Nothing prints as a bare percentage.

## Data model

- **Bounded context:** Rung assessment. **New fact table `judge_check`, schema v7 (expand only).**
- **Grain:** one row is exactly one second reading of one judgment, identified by `check_id` (UUIDv7),
  recorded when the grader call finishes.
- Columns: `check_id`, `run_id`, `judgment_id` (by identity), `task_id`, `grader_provider`,
  `grader_model`, `answer` (JSON: job, task type, rung, facts with value and *can't settle*),
  `call_id`, `checked_utc_ms`, `check_version`. Append-only trigger, as for `judgment`.
- **`judge_check_run`:** one row is exactly one `aiq judge --check` run: `run_id`, `seed`, `sample` (JSON
  list of judgment ids), `requested`, `started_utc_ms`.
- **Derive, don't store:** every agreement number (A1–A5) and the pause rule (K6b) are computed from
  `judgment` + `judge_check`. No stored percentages. Additivity: counts additive, rates non-additive.
- History rule: rows are never updated; a new judge version gets new checks.
- `aiq forget --all` and delete-one remove these rows too (they hold the grader's reading of the text).

## Change surfaces (E7)

| Surface | Change |
|---|---|
| Store | v7: `judge_check`, `judge_check_run`, append-only triggers; forget paths |
| Contracts | `aiq.check.v1` grader answer schema (strict parse, like `aiq.judge.v2`) |
| Domain | `aiq_domain::check`: sample (seeded, stratified), agreement, Wilson interval, pause rule |
| Runner | `aiq judge --check [--sample n] [--dry-run]`; grader prompt (blind, 40k excerpt, *can't settle*) through the existing sealed Copilot / Claude runners |
| Coaching | Pause rule feeds the same "not coached" state as flags; `aiq on-kind` override |
| Home read model | `Snapshot.check` (latest run's derived numbers) |
| UI | Status: "Judge check" card; kind page: paused-by-check state and Turn back on |
| Docs | Rung-judge design eval section gets the real numbers |

## Failure modes

| Mode | Disposition |
|---|---|
| Grader call fails, times out, or connects a tool/MCP server | Same as the judge: thrown away, counted as "not checked", never guessed |
| Grader answer malformed | Strict parse; counted as "not checked" |
| Fewer judged tasks than the sample | Use all; the report says so |
| Plan forces the same model for grader and judge | Report says "grader is the same model as the judge", and agreement is labelled weaker |
| Budget used up mid-run | Run stops; partial results kept and labelled partial |
| Pause rule fires on noise (small n) | Needs at least 5 checked for a kind; override is one command |

## Adversarial and privacy analysis

Same trust boundary as the judge (B5: core to the person's AI provider). Prompt-injection in task text
can steer the grader as it can the judge: content is fenced the same way, and the grader's answer only
changes a report and a pause, never a rung. The grader sees the same task text the provider already
holds; a fuller excerpt (40k vs 18k) is a larger disclosure to that same provider, accepted under the
person's run of the command. No new data leaves the device. Rows are deleted with the person's data.

## Test plan

- D0: fmt, clippy, existing suites.
- Domain: seeded sample is reproducible and stratified; Wilson interval against known values; pause
  rule at 7/10, 8/10, 4 checked; agreement ignores *can't settle*.
- Store: v7 migration; append-only update refused; forget removes check rows.
- Runner: blind prompt never contains the judge's answer (test greps the prompt); malformed and
  tool-connected answers counted as not checked; dry-run spends nothing.
- UI e2e: Judge check card states (none yet, partial, full), paused kind with Turn back on; axe.
- Real run (laptop, measured): one `aiq judge --check` on Imran's store; numbers recorded.

## Run plan

1. `/implement` this design in the cloud (no laptop needed).
2. The thread that has the laptop builds main and gives Imran the upgrade line.
3. `aiq judge --check --dry-run` (shows the sample and cost), then `aiq judge --check`. Laptop work is
   read-only apart from AIQ's own store.
4. Results go into the rung-judge design's eval section and decide the next slice: tuning the facts that
   miss (F4, F6 are the known suspects), or opening the alpha.

## Flagged risks

- Model-graded agreement is not truth. A shared blind spot between judge and grader looks like a match.
  Mitigation: blind reading, fuller excerpt, a stronger model where the plan allows; the label says so.
- If Imran's plan forces one model for every Copilot call, grader and judge are the same model and
  agreement will read high. The report shows this.
- 30 tasks from one person is a small sample; intervals will be wide (for example 24 of 30 gives
  roughly 66% to 90%, computed with the Wilson formula).

## Gate record

`GATE design-slice · 2026-11-02 · Patterns, Simplifier, Test Architect, Data & Persistence, Security,
SRE (all seats played by Claude in one session; not independent) · verdict: pass with conditions`
- Simplifier: a one-off cloud study (as in PR #27) is smaller, but it copies the person's sessions out
  and can't be repeated by users. Kept in-product; scope trimmed to one command, one table, one card.
- Test Architect (hard): blindness must be tested (the prompt must not contain the judge's answer).
- Data & Persistence: grain declared; agreement derived, not stored. Pass.
- Security: no new boundary; fuller excerpt to the same provider accepted.

## Implementation record (`/implement`, 2026-11-02)

Built on branch `claude/project-thread-wjefak-check`. What landed, by surface:

| Surface | Change |
|---|---|
| Contracts | `aiq_contracts::check`: `aiq.check.v1` schema; each fact typed boolean-or-null; strict parse (every key required, so a missing fact never reads as "can't settle"; job, task type and rung validated) |
| Domain | `aiq_domain::check`: seeded splitmix64 sample, stratified by job (floor 5); `Count` with 90% Wilson interval; `agreement()` for A1–A5 and per-kind counts. `rung::check_system()` builds the blind grader prompt from the judge's own rubric minus its answer; excerpt caps 5,000 per prompt, 40,000 total |
| Store | Schema v7: `judge_check_run`, `judge_check`, update-refusing triggers. `forget_imported` removes checks of imported tasks; `forget --all` deletes the whole store file, so check rows go with it |
| Runner | `aiq judge --check [--sample n] [--dry-run]`, `aiq judge --check-report`. Uses the sealed runners and the judge lock; a tool-connected, malformed or failed call is stored as not checked |
| Settings | `check-claude-model` (default `opus`), `check-copilot-model` (default: the judge's model) |

Tests: contracts 3, domain 5 + grader-prompt test, store v7 round trip and append-only, runner 3 (blindness:
the grader's system prompt and message are searched for the judge's answer text; malformed answer counts as
not checked; report before any run). `cargo test --workspace`, fmt and clippy green (measured, this branch).

### Deviations from the design (recorded, not silent)

- **K5:** check calls do **not** count against the daily judge budget. The check is one command the person
  runs, `--dry-run` shows the count and cost first, and counting it would starve background judging that day.
- **K3 (revised 2026-11-02 after the first run):** the second reader is a different model from the **same
  provider** (Imran). The first run used the judge's model on 29 of 30 because no Copilot model was set.
  Copilot CLI lists no model ids, so AIQ tries `gpt-6-astra`, then `claude-sonnet-5.5`, then `gpt-5.6-sol`.
  It skips any model the judge used. When Copilot answers "from --model flag is not available", AIQ moves
  to the next model for the rest of the run. That refusal takes about 8 s, measured. If no candidate is
  offered, Copilot picks the model itself. `check-copilot-model` overrides all of this. Claude Code stays
  `opus`, and the judge defaults to `sonnet`. Ids measured on Imran's laptop: `recentModelIds`, and a
  sealed `--model claude-sonnet-5.5` call that answered as that model. His plan sets Sol as the default but
  does not lock it. The report still flags a same-model reading.
- **N2 (repeat stability):** deferred. It needs a second judge run on the sample; add it once the first
  numbers show whether it matters.
- **K6b (pause a kind under 8 of 10) and the Home "Judge check" card:** deferred until the first real run.
  The bar is a guess; pausing coaching on an unmeasured bar would be acting on a guess. The CLI report already
  lists kinds under 8 of 10 with at least 5 checked.
- **Delete-one:** check rows hold ids, the grader's job, type, rung and true/false/null facts, never task text,
  so deleting one prompt's text needs no change here.

## Status and next action

| | |
|---|---|
| **Completed** | Design; build of `aiq judge --check` and its report |
| **Remaining** | One laptop run; record numbers in the rung-judge design's eval section; then K6b and the Home card |
| **Best next action** | Upgrade the laptop, run `aiq judge --check --dry-run`, then `aiq judge --check` |
