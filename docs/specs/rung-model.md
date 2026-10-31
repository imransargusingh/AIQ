---
id: spec-rung-model
title: "Spec: Rung model (per-task maturity judgment)"
type: spec
status: in-review
owner: "Imran"
phase: specify
tags: [rung-model, judge, calibration, evaluation, phase-1]
links:
  - { to: spec-coaching-loop, rel: relates-to }
review-by: 2026-11-30
summary: >-
  What AIQ's rung model must do: for each task a person does with AI, judge the rung it was done at
  and the next rung it could reach, explain why in plain language, and let the person say when the guidance misses.
  The model answers narrow facts about how the work was done, through the person's own AI account,
  the model places the rung using those facts as evidence, and per-job guardrail rules block any rung the
  facts can't support; held to a measured accuracy bar before it drives coaching.
---

# Spec: Rung model (per-task maturity judgment)

Produced with AI-Forward `/specify` (v2026.10.19.4), run interactively with Imran.
Source of truth: AIQ brief and Design > Rung model tab (https://claude.ai/artifact/6PhqpuQvWMcU6xBD7rBbRw).
Sibling spec: `docs/specs/coaching-loop.md` (consumes this model's output for Opportunity 4).

## Decisions so far

| # | Decision | Source |
|---|---|---|
| R1 | A model-based judge first; rules or peer comparison may be added later for calibration. Refined by Q6 and Q8: the model answers narrow facts and places the rung; rules act as guardrails | Imran, 2026-10-31 (recorded owner ruling) |
| R2 | Ladder: 1 Assisted, 2 Directed, 3 Delegated, 4 Orchestrated, 5 Factory. One ladder for everyone | Imran, 2026-10-31 (recorded owner ruling) |
| R3 | The unit of analysis is the **task**: one unit of intent, spanning prompts and surfaces | Design > Rung model tab |
| R4 | Potential is reported as the **next reachable rung**; the theoretical ceiling is a secondary field | Design > Rung model tab |
| R5 | The rubric is tuned **per job, not per role**. The ladder is shared; each job pack supplies its own rung examples and signal weights. This replaces the tab's "adjusted for each role" and "role-specific rubric emphasis" | Follows from coaching-loop D7 (Imran, 2026-10-31: "aligning to the jobs-to-be-done framework") |
| R6 | Only the person sees their judgments; AIQ runs no server that sees task content | Imran, 2026-10-31 (privacy rule) |
| R7 | Evaluation never uses users' private data. ~~A few hundred expert-labeled golden tasks as the main benchmark~~ superseded by Q6 | Design > Rung model tab; Q6 |
| Q1 | Scope: the judge, its explanation and the person's feedback on it (see Q3). The rung distribution and opportunity views get their own spec | Imran, 2026-10-31 (decision card) |
| Q2 | The judge is a cloud model called **through the person's own AI account**, and only that way in alpha (no on-device judge). It reads the task's prompts and outputs. Conditions: a task goes only to the provider that already saw it unless the person chooses otherwise; setup states plainly that a work account's admin policies, logging and retention apply; offline or no account means the task is queued, never rerouted; judging uses the person's quota | Imran, 2026-10-31 (decision card, after asking why not judge in the cloud under the user's own account) |
| Q3 | People **do not reclassify** a task's rung; the rung is always the judge's. "Easy to correct" means correcting the *guidance*: the person can say the evidence, next step or blockers miss, and AIQ withdraws that guidance and learns which guidance helps them. Rung placement is calibrated only against the evaluation regime (Q6), never against the person's own view | Imran, 2026-10-31 (thread reply: users may not have the experience to classify consistently) |
| Q4 | Accuracy bars as in US-7: model-read facts right on ≥ 90% of check tasks; tasks built at a known rung land on it ≥ 90%; expert sanity check ≥ 70% exact and ≥ 95% within one (potential ≥ 60% / ≥ 90%); in alpha ≥ 70% of next steps not flagged "isn't useful" and "evidence is wrong" on ≤ 10%. Tunable after the first check runs | Imran, 2026-10-31 (decision card) |
| Q6 | **Facts first, plus cheap labels.** The model answers narrow questions about how the work was done (task facts); the rung placement itself is settled by Q8. Accuracy comes from tasks done at a known rung on purpose (labels by construction), the team's own sessions, a small expert sanity check, two-run consistency, and private feedback signals in alpha. Replaces the large golden set | Imran, 2026-10-31 (decision card, after raising that a golden set is costly and tasks vary too much to generalize from few examples) |
| Q5 | Context management (model and reasoning choice, lean context, durable facts in instructions files, handling compaction) is recorded as task facts and may be coached, but **never caps or lowers a rung** until those facts meet the US-7 fact bar | Imran, 2026-10-31 (decision card) |
| Q7 | Credentials per provider: for **Claude**, the judge uses the person's **own Anthropic API key** (stored in the OS keychain), not their Free, Pro or Max sign-in, because Anthropic's published terms do not permit third-party apps to route requests through those credentials (https://code.claude.com/docs/en/legal-and-compliance). For **Copilot**, the judge uses the VS Code Language Model API with the person's consent | Imran, 2026-10-31 (thread reply: "Let's go with this: the spec uses each person's own Anthropic API key for Claude") |
| Q8 | **The model places the rung; rules are guardrails.** The judge reads the task facts and content and places the rung; deterministic per-job guardrails then lower it to the highest rung the facts can support (e.g. no Delegated unless an agent made the edits and the whole outcome was handed over). Chosen after the spike scored rules-only placement at 55–63% exact versus 80–85% for a holistic judge, with stable facts | Imran, 2026-10-31 (decision card) |
| Q9 | **Verifiable facts also set a floor for Orchestrated and Factory.** When log- and repo-verifiable facts show Orchestrated (F7, F8, F3 agent or automated) or Factory (F9 runs by others, F3 agent or automated), the rung is raised to at least that rung, as well as capped by guardrails. Confirmed in the fact check before it drives coaching | Imran, 2026-10-31 (decision card, after the spike: lower-only guardrails 78% exact vs model alone 82%; with floors 88% exact, 98% within one; Orchestrated 8/8, Factory 6/8. Variant designed after seeing the data; F9 counts simulated) |

## Part A — Functional specification

### Problem
AIQ's progress views and its "this task could be delegated" coaching both rest on one judgment: at
what rung was this task done, and what rung could it reach next? People don't know this about their
own work, and a usage count (prompts, agent runs) doesn't tell them. If the judgment is wrong,
unexplained or unfair, every view and tip built on it loses trust. So the judgment has to be
consistent, explained with the person's own evidence, fair to the job being done and the tools
available, and open to feedback when its guidance misses.

### Target users & jobs-to-be-done
The same people as the coaching loop: anyone doing the alpha jobs **Code a change** or **Write a spec
or doc** on VS Code, Claude Code or Copilot CLI (coaching-loop D2, D6, D7).

Their job here: *"When I look back at my work, tell me honestly how I used AI on each task and the one
next step that would let an agent do more, without grading me or showing anyone else."*

Indirect consumers inside AIQ: the coaching loop (Opportunity 4, slow path) and the future views
(rung distribution, opportunity view).

### Core scenario
Priya fixes a flaky upload test. She asks Copilot Chat in VS Code what the test does, then gives
Claude Code three prompts and edits most of its diff by hand before running the tests herself. When
the task closes, AIQ judges it **Directed** (structured prompt with the failing output; output checked
before use) and its next reachable rung **Delegated**. The evidence says: "You gave the agent the
failing output, then made most of the edits yourself." The gap says: "Give the agent the acceptance
criteria and let it run the tests itself." The coaching loop can now offer that as a tip at the next
pause.

Priya thinks the task was really Delegated because the agent wrote the fix and she only renamed
variables, and that the gap is wrong because the agent already ran the tests. She picks "This doesn't
fit: evidence is wrong" and adds a line. AIQ withdraws that next step so it is never coached, keeps
the rung as judged, and marks the task "you flagged this". Nobody else can see the judgment or her
feedback.

### In scope / Out of scope (explicit non-goals)
In scope (Q1):
- Judging each closed task: current rung, next reachable rung, ceiling, confidence, evidence, gap,
  blockers.
- The per-job rubric as pack data, and the placement rule.
- The person's feedback on a judgment's guidance, and how it changes what AIQ coaches them on.
- The task judgment panel that shows one judgment and its feedback control.
- The evaluation regime: check tasks, accuracy bar, consistency and gaming checks (Q6).

Out of scope:
- **Grouping events into tasks and classifying their job.** Defined in the coaching-loop domain model
  and being tested by the Phase 0 feasibility spike; this spec consumes "a closed task with a job or
  Unclassified".
- The rung distribution view and the opportunity view in AIQ home (own spec).
- The fast in-the-moment detectors (coaching-loop spec); they never wait for this model.
- Choosing the model and runtime (`/define-architecture`; where it runs is settled by Q2).
- Any sharing, team or org view, or comparison with other people.
- Judging work on surfaces other than the three alpha ones.
- Rules-based or peer-based calibration (R1: later).
- The person changing a task's rung (Q3).

### Conceptual domain model (bounded context: Rung assessment)
Shares **Task**, **Job**, **Job pack** and **Environment blocker** with the Coaching context
(coaching-loop spec); the meaning is identical.

Ubiquitous language:
- **Rung**: one of the five ladder levels (R2). A value, compared by order.
- **Task fact**: one answer to one narrow question about how a task was done ("did an agent make the
  edits?", "what share of the final change did the person write?"). Read from the logs where possible,
  otherwise by the judge model from task content. Carries its source, a confidence and its evidence.
- **Fact set**: the list of fact questions a job pack asks. Pack data.
- **Rubric**: per job pack, the rung descriptions and examples the judge places against, plus the
  **guardrails**: the minimum facts each rung needs and the facts that rule it out. Versioned pack
  data. Guardrails are deterministic: the same facts always give the same ceiling on placement.
- **Task content**: the task's prompts and AI outputs as kept on the device. The judge reads it while
  it still exists on the device (kept until the person deletes it, per the architecture), and it is sent only to the person's own AI account (Q2).
- **Judge account**: the person's own AI provider account that the judge calls. Chosen by the person;
  AIQ holds no account of its own in the path.
- **Rung judgment**: the verdict on one task: its task facts, current rung, potential rung, ceiling, a
  confidence for each, evidence, gap and blockers, stamped with the fact set, rubric and judge versions.
- **Current rung**: the rung the judge places the task on, raised to a floor when verifiable facts show
  Orchestrated or Factory (Q9), and lowered if needed to the highest rung whose guardrails hold. A guardrail resting on a low-confidence fact counts as not met, so mixed evidence
  places the task on the lower rung.
- **Potential rung**: the next reachable rung, weighed on fit, readiness, availability and
  repetition. Equals the current rung when the task is already at its right rung or is capped.
- **Ceiling**: the highest rung the task type could reach in principle. Secondary, for the
  opportunity view.
- **Evidence item**: one plain-language statement tied to one or more of the person's own events.
- **Gap**: the one change that would move the task to its potential rung. What coaching uses.
- **Blocker**: an Environment blocker that caps potential (e.g. "no agent with repo access"). Reported
  as a cause, never as a low score.
- **Right rung**: potential equals current with no blocker. Shown neutrally and never coached.
- **Uncertain**: a judgment where a fact its placement depends on is below the pack's confidence
  threshold, or two independent judge runs disagree on such a fact. Kept out of coaching.
- **Judgment feedback**: the person's statement that a judgment's guidance misses, with one reason
  (evidence is wrong or incomplete · next step isn't useful or doable · a blocker is missing · doesn't
  fit this kind of task) and an optional note. It never changes the rung.
- **Guidance preference**: the person's private ranking of gap types for one job, learned from their
  own judgment feedback and coaching feedback.
- **Check task**: a task trace used only for evaluation, whose expected facts and rung are known:
  built at a known rung on purpose, taken from the team's own sessions with their consent, or labeled
  by an expert in the sanity-check set.

Entities: Rung judgment, Judgment feedback, Guidance preference, Check task, Rubric and Fact set (versioned).
Value objects: Rung, Task fact, Task content, Evidence item, Gap, Blocker, Confidence.

Aggregates and the invariant each protects:
- **Rung judgment** (root: the judgment of one Task): potential is never below current and never more
  than one rung above it; every rung claim cites at least one evidence item. A re-judgment creates a
  new version and never edits an old one.
- **Task guidance** (root: Task): once the person flags a judgment's guidance, that gap is never
  coached for the task, and no re-judgment brings it back; the rung itself is unaffected.
- **Guidance preference** (root: person × Job): changes only from that person's own feedback, never
  moves a rung, and never leaves their device.
- **Check set** (root: check set version per job): every check task has a known expected fact set and
  rung, and contains another person's work only with that person's explicit consent.

### User stories & acceptance criteria

**US-1 — As anyone working with AI, I want each task I finish judged against a clear rubric, so that I see how I actually worked rather than how much I used AI.**
- **Given** a closed task with at least one AI interaction **When** it closes **Then** a Rung judgment is produced within 60 s (p95) of the judge call starting, with current rung, potential rung, ceiling and a confidence for each.
- **Given** a closed task with no AI interaction **When** it closes **Then** no judgment is produced (there is no rung 0).
- **Given** a task whose facts meet every Delegated rule except "the whole outcome was handed over with done criteria" (F13) **When** judged **Then** it is placed on Directed, and the evidence names the unmet fact.
- **Given** the judge places a task on Delegated but F1 (an agent made the edits) is no **When** guardrails run **Then** the task is lowered to the highest rung whose guardrails hold, and the evidence names the guardrail.
- **Given** the judge places a task on Delegated and the logs show a spec given to the agent (F7), a sub-agent with an automated check between steps (F8), and the agent ran the checks (F3) **When** placement completes **Then** the task is Orchestrated, and the evidence names the facts that set the floor.
- **Given** the same task facts **When** the guardrails run twice **Then** the result is identical (guardrails are deterministic).
- **Given** a fact readable from the logs (e.g. who ran the tests) **When** judged **Then** it is taken from the logs, not asked of the model.
- **Given** a task classified *Code a change* **When** judged **Then** the Code a change rubric is used; **Given** an *Unclassified* task **Then** only the core facts (F1–F6, F10, F12, F13) and the shared rules are used, and the judgment is Uncertain.
- **Given** the same task under the Developer and the PM preset **When** judged **Then** both judgments are identical (R5: the role never enters).

**US-2 — As anyone working with AI, I want the potential rung to be one realistic next step, so that I'm not pushed toward something I can't do yet.**
- **Given** a Directed task where fit, readiness and availability hold for Delegated and the ceiling is Factory **When** judged **Then** potential is Delegated and the ceiling is Factory.
- **Given** a task whose next rung needs a tool the person doesn't have (e.g. no agent with repo access) **When** judged **Then** potential equals current, a Blocker is recorded, the task is **not** marked Right rung, and no habit tip is generated from it.
- **Given** potential equals current and no Blocker **When** judged **Then** the task is Right rung and is never the subject of a tip.
- **Given** a task type the person did fewer than 3 times in the last 30 days **When** judged **Then** potential is never Factory (repetition rule; threshold is pack data).

**US-3 — As anyone working with AI, I want every judgment explained with my own activity, so that I can trust it or see where it's wrong.**
- **Given** any judgment **When** shown **Then** it has at least one evidence item per rung claim, each linked to the events it cites, and exactly one gap sentence when potential is above current.
- **Given** the person deleted the events an evidence item cites **When** the judgment is opened **Then** the item says the evidence was deleted; the rung stays and is marked "evidence removed".
- **Given** any judgment **When** rendered **Then** it never names the person's role or compares them with anyone.

**US-4 — As anyone working with AI, I want to say when a judgment's guidance misses, so that AIQ stops coaching me on the wrong thing.** (Q3)
- **Given** a judgment **When** the person picks "This doesn't fit" with a reason **Then** the task's gap is withdrawn from coaching, the task shows "you flagged this", and the rung stays as judged.
- **Given** a flagged task **When** it is re-judged after a rubric or judge version change **Then** the withdrawn gap is not coached again, even if the judge produces it again.
- **Given** "next step isn't useful or doable" on the same gap type 2 times for one job within 30 days **When** that gap type next qualifies **Then** it ranks below every gap type without negative feedback for that person (same rule as coaching-loop US-4).
- **Given** any feedback **When** saved **Then** no control lets the person set a task's rung, and nothing about the feedback leaves the device.
- **Given** "evidence is wrong or incomplete" **When** saved **Then** it is kept as a local count per job, visible to the person, as a sign the judge may be misreading their work.

**US-5 — As anyone working with AI, I want the judge to understand what I asked for, while my work goes nowhere except back to my own AI account.** (Q2)
- **Given** a closed task **When** it is judged **Then** its content is sent only to the person's chosen judge account, and a network capture over a week shows no task content, signal or judgment going to any other destination, AIQ's included.
- **Given** a task done only in Copilot and a judge account with a different provider **When** it is judged **Then** it is not sent unless the person has allowed cross-provider judging; otherwise it is left unjudged and the panel says why.
- **Given** the person picks Claude as the judge **When** setup runs **Then** AIQ asks for their own Anthropic API key, stores it only in the OS keychain, and never uses their Claude Code sign-in for judging (Q7).
- **Given** the person sets up the judge with a work account **When** setup completes **Then** AIQ has shown, in plain words, that the account's admin policies, logging and retention apply to what the judge sends.
- **Given** the device is offline, the account is signed out, or the quota is exhausted **When** a task closes **Then** it is queued and judged later, never sent to another account or model.
- **Given** a judgment **When** the person opens "What was read?" **Then** the panel lists exactly the prompts, outputs and signals sent, and which account received them.
- **Given** a judge call made through the VS Code Language Model API **When** AIQ's VS Code collector sees it **Then** it is never grouped into a task, judged or coached; and setup tells the person that judge calls count toward their Copilot usage.
- **Given** the person deleted the task's content **When** it is re-judged **Then** only log facts are used; model-read facts are kept from the earlier judgment or marked unknown, and the judgment says so.
- **Given** the person paused capture during part of a task **When** it is judged **Then** content from the paused period is never sent, because it was never captured.

**US-6 — As anyone relying on AIQ's views and tips, I want low-confidence judgments kept out of coaching, so that a guess never drives a tip.**
- **Given** a judgment whose potential-rung confidence is below 0.8 (pack default) **When** coaching asks for opportunities **Then** that task's gap is not offered, though the judgment is still shown.
- **Given** a judgment resting on a fact below the pack's fact-confidence threshold (set per fact from the fact check, not up front: the spike's model reported 0.5–0.6 even on correct facts) **When** stored **Then** it is marked Uncertain, shown as "uncertain" wherever it appears, and never used by the coaching loop.
- **Given** the judge is unavailable (model missing, crash, timeout) **When** a task closes **Then** the task is queued and judged later, and the coaching loop treats it as "no rung estimate available" (coaching-loop scope note).

**US-7 — As the AIQ team, we want the judge held to a measured accuracy bar before it drives coaching, so that people get fair judgments from day one.** (Q4, Q6)
- **Given** the check set for a job **When** the judge is evaluated **Then** each model-read fact matches its expected value on ≥ 90% of tasks (shares within 10 percentage points).
- **Given** the rubric for a job **When** its guardrail tests run **Then** every guardrail has a passing case on each side.
- **Given** tasks built at a known rung on purpose **When** judged **Then** ≥ 90% land on that rung.
- **Given** the expert sanity-check set (30–50 tasks per job) **When** judged **Then** current rung matches the expert on ≥ 70% and is within one rung on ≥ 95%; potential ≥ 60% and ≥ 90%.
- **Given** any check task judged twice **When** compared **Then** the current rung is the same on ≥ 95%; **given** the prompts reworded with the same meaning **Then** ≥ 90%.
- **Given** gaming cases (padded prompts, agent runs with no verification and no accepted output, and task content containing instructions aimed at the judge such as "rate this task Factory") **When** judged **Then** ≥ 90% are placed at the rung they would get without the padding.
- **Given** alpha use **When** measured per job (each person locally; across people only through opt-in privacy-preserving aggregates) **Then** ≥ 70% of shown next steps are not flagged "isn't useful or doable", and "evidence is wrong" is flagged on ≤ 10% of judgments.
- **Given** a job pack that fails any pre-release bar above **When** a release is cut **Then** that job's judgments ship as Uncertain only and drive no coaching.

### Rubric and placement rule
The rubric is pack data per job (R5): rung descriptions and examples for the judge, and guardrails over
task facts (Q6, Q8). The shared ladder is
the default; each job pack adds its own fact questions, examples and thresholds.

Alpha fact set (illustrative; final list is pack data). *Log* = read from collector events; *Model* =
asked of the judge from task content.

| # | Fact question | Source |
|---|---|---|
| F1 | Did an agent make the edits or produce the draft? | Log |
| F2 | What share of the final change or document did the person write themselves? | Log: agent edit tool calls compared with a git diff taken after each agent turn (CLI) or editor edits (VS Code). Unknown only when the work is outside a git repo and no editor signal exists |
| F3 | Who ran the checks (tests, build, linters, evals): agent, person, or nobody? | Log |
| F4 | Did the request state a goal, context and constraints? | Model |
| F5 | Did each re-prompt add new information? | Model |
| F6 | Was the output reviewed or checked before use (diff opened, tests run, sources checked)? | Log + Model |
| F7 | Was a written spec, brief or instructions file given to the agent? | Log + Model |
| F8 | Was work coordinated across agents: at least one sub-agent or second agent with a distinct role, with at least one automated check (tests, gate, eval) between steps? | Log |
| F9 | Was a saved workflow run (prompt file, slash command, agent config, pipeline), and is it reused? Repo reuse facts, counts only: the workflow file is versioned in the repo, its number of distinct authors, and its runs by CI or by others | Log + repo metadata |
| F10 | Which agents and permissions were available to the person? | Log (environment) |
| F11 | How many similar tasks of this job did the person do in the last 30 days? | Log + Job classification |
| F12 | Did the person drive the agent step by step (one small instruction per turn, steering each result)? | Model |
| F13 | Was the whole outcome handed over with done criteria, rather than one piece or step? | Model |

| Rung | Guardrail: needs at least (illustrative) | Guardrail: ruled out by |
|---|---|---|
| 1 Assisted | Any AI interaction | — |
| 2 Directed | F4 yes and F6 yes | F5 no on 2+ re-prompts |
| 3 Delegated | F1 yes, F13 yes, F6 yes | F12 yes; F2 ≥ 50% |
| 4 Orchestrated | Delegated, plus F7 yes, F8 yes and F3 = agent or automated | F8 no (manual handoffs); F3 = nobody |
| 5 Factory | F9 yes with runs by others, and F3 = agent or automated (does not require Orchestrated: a team's versioned single-agent workflow qualifies) | Only this person runs it |

Placement (Q8, Q9): the judge places the rung from the rung descriptions, the job's examples, the task
facts and the content. Then, in order: (1) **floor** — if the Factory guardrail holds on log- and
repo-verifiable facts (F9 runs by others, F3 agent or automated), the rung is at least Factory; else
if the Orchestrated guardrail holds on F7, F8 and F3, at least Orchestrated; (2) **guardrails** lower
the rung to the highest rung whose guardrails hold. Floors use only facts read from logs or the repo,
never model-read facts; a guardrail resting on a low-confidence fact counts as not met, and the evidence names it. When
the judge's mixed evidence leaves it between two rungs, it picks the lower and says why. Judge
outcomes and verification, not activity (anti-gaming).

Potential: fit (Model: is this task well specified, testable, repeatable?), readiness (F7, tests or
acceptance criteria exist), availability (F10) and repetition (F11). Potential is the rung directly
above current when fit holds and every fact it is missing is one the person could change; if a missing
fact needs a tool they lack, potential stays at current with a Blocker; otherwise the task is Right rung.
The ceiling repeats the same test for higher rungs and is not coached.

Context management (model and reasoning choice, lean context, durable facts in instructions files,
handling compaction) is a fact set at every rung above Assisted. It can drive coaching but never caps
or lowers a rung until its facts meet the US-7 fact bar (Q5).

Alpha job examples (pack data, illustrative):
- **Code a change, Delegated:** "Fix the failing upload test" handed to Claude Code with the failing
  output and acceptance criteria; the agent edits and runs the tests; the person reviews the diff.
- **Write a spec or doc, Delegated:** an agent drafts the spec from a brief (audience, goal,
  constraints, sources); the person reviews and edits lightly rather than rewriting.

### Non-functional requirements (ISO/IEC 25010)
| Attribute | Requirement (measurable) |
|---|---|
| Functional suitability | US-7 fact accuracy, rule, construction, sanity-check, consistency and gaming bars per job, measured on each release |
| Performance efficiency | Judgment ≤ 60 s p95 from judge call start; judging adds no measurable load to the core's coaching-loop budgets; judging stays within a daily budget the person sets, and its cost is visible to them (TR-8) |
| Reliability | A judge failure never blocks a collector or the coaching fast path; unjudged tasks are queued and retried |
| Security | See the threat review (TR-1 to TR-9). The judge calls only the person's chosen judge account: the Anthropic API with their own API key (kept in the OS keychain) or the VS Code Language Model API; AIQ never reads provider sign-in tokens; the API endpoint is fixed, not taken from repo or environment overrides; pack rubrics are data and cannot run code; task content is passed to the judge as quoted data, never as instructions (prompt-injection gaming case in US-7) |
| Privacy | R6; content goes only to the person's own judge account (Q2); judgment feedback and guidance preferences stay on device; full export and delete cover judgments |
| Usability | In a 5-person hallway test, ≥ 4 can say why a task got its rung from the panel alone |
| Accessibility | Panel keyboard-operable and screen-reader readable; rung never conveyed by colour alone |
| Compatibility | Same as coaching-loop spec |
| Maintainability | Rubric, thresholds, repetition rule and feedback ranking rule are pack data; every judgment records the rubric and judge versions |
| Portability | Judgment schema is the same on all surfaces and in AIQ home |

### Boundary set
A task with one prompt; a task with 10,000+ events; a task spanning two surfaces and a day boundary;
a task the person deletes after it was judged; a judge or rubric version upgrade mid-week (old
judgments keep their versions); a task with an agent run that produced nothing; a person with no
agent access at all; an Unclassified task; feedback on an Uncertain judgment; a task re-judged
after the person deleted its content; a task spanning two providers; quota running out mid-week;
the person switching judge accounts; a sanity-check task where the expert and judge disagree by two or more rungs (second expert);
a fact the logs cannot answer and the content was deleted (fact unknown, rule not met).

### Evaluation regime (Q6)
- **Labels by construction:** for each alpha job, the same tasks done on purpose at each rung (hand
  edited, directed, fully delegated, orchestrated), by agents scripted to play the person or by the
  team. Expected facts and rung are known without labeling.
- **Dogfooding:** the team's own sessions from the Phase 0 spike, with each member's consent. Log facts
  are checked automatically; model facts are spot-checked by the person who did the task.
- **Expert sanity check:** 30–50 tasks per job, each labeled by one experienced practitioner, a second
  on disagreement with the judge. Guards against the rubric itself being wrong.
- **Consistency:** each judgment runs the model facts twice; disagreement on a fact the placement
  depends on makes the judgment Uncertain.
- **Alpha signals:** "next step isn't useful" and "evidence is wrong" rates, private per person, and
  across people only through opt-in privacy-preserving aggregates with no content.
- **Bars:** US-7 (Q4).
- **Dependency:** task grouping, job classification and the first dogfood traces come from the Phase 0
  feasibility spike. This spec does not repeat that work.

### Comparables & user evidence
| Claim | Source | Confidence |
|---|---|---|
| LLM-as-judge with an explicit rubric agrees with human raters reasonably well on structured tasks, with known biases (position, verbosity) | Zheng et al. 2023, "Judging LLM-as-a-Judge" (MT-Bench) | Inferred (from memory, not re-checked) |
| Maturity models are usually assessed per organisation or team, not per task | CMMI; DORA capabilities | Flagged (from memory) |
| On synthetic tasks, the judge was 88–100% exact on rungs 1–3 when it read prompt excerpts, but only 53–60% with signals alone, biased half a rung low | Phase 0 spike, PR #2 `docs/investigations/phase0-feasibility.md` | Inferred (synthetic data; Claude as labeller and judge) |
| Guarded hybrid (100 synthetic tasks, Sonnet): model alone 82% exact / 94% within one; with lower-only guardrails 78% / 92% (4 correct calls demoted, none fixed); with floors for Orchestrated and Factory 88% / 98% (R4 8/8, R5 6/8) | Phase 0 spike, PR #2, "Guarded hybrid" | Inferred (synthetic; designed after seeing the data; F9 simulated) |
| Facts-first run (50 synthetic tasks, Sonnet and Haiku, 2 runs each): model facts were stable (F4–F7, F12 85–100% run-to-run agreement; F2 75–93%), but rule-based placement was only 55–63% exact versus 80–85% for a holistic judge; the gap came from the rules, not the facts. Fixes applied: F2 from post-turn git diff, F13 added, Factory no longer requires Orchestrated, F8 counts one sub-agent with gates, fact thresholds set from the fact check | Phase 0 spike, PR #2, "Facts-first judge" | Inferred (synthetic data; Claude as labeller and judge). Re-run pending |
| Rung 5 was never judged right (0/8) without repo reuse facts; rung 4 was right 4/8, the misses judged 3 despite sub-agents, a spec and gates | Phase 0 spike, PR #2 | Inferred (same caveat). Addressed by F9 and F8 |
| Job classification was 93–98% with excerpts; a 0.7 threshold gave 100% precision, routing the rest to Unclassified | Phase 0 spike, PR #2 | Inferred (same caveat) |
| Potential was within one rung 100% of the time, but opportunity precision was 0.73–0.85 | Phase 0 spike, PR #2 | Inferred (same caveat). Addressed by the 0.8 gate (US-6) |
| The rule "mixed evidence → lower rung, say why" held | Phase 0 spike, PR #2 | Inferred (same caveat) |
| Decomposing a judgment into narrow, checkable questions makes LLM grading more reliable than one holistic score | Rubric-based and checklist LLM evaluation practice (e.g. G-Eval, Liu et al. 2023) | Inferred (from memory, not re-checked) |
| People accept automated assessments more when they can see the evidence and give feedback on it | HAX guidelines G11, G15 (Amershi et al. 2019) | Inferred |
| People want a per-task rung judgment | None yet | Flagged: validate in Phase 1 alpha |

### Applicable governance lenses
- [x] Quality attributes / NFRs: above
- [x] Threat model (STRIDE): applies (judge sends prompts and code to the person's own provider account). **Gap:** run in `/define-architecture`
- [x] Privacy & data governance: R6, US-5, consent for any dogfood or check task
- [x] Accessibility: NFR row; Part C
- [x] Performance budget: NFR row; to confirm in the spike
- [x] Release / rollback: rubric and judge versions on every judgment; a failing job pack ships Uncertain-only (US-7)
- [x] Observability: local-only judge health, latency and feedback counts; no remote telemetry

### AI-integrated allocation
- **Archetype:** advisory assessor. AIQ judges and explains; the person can always flag its guidance.
- **Tier allocation:** stage 1 deterministic extraction of log facts on device; stage 2 model-read facts
  through the person's own AI account (run twice); stage 3 the judge places the rung, then deterministic floors (Orchestrated, Factory, from verifiable facts) and guardrails adjust it;
  stage 4 personal guidance ranking from the person's own feedback (never moves a rung).

## Part B — UX specification

### Personas & jobs-to-be-done
- **Priya, rung 1–2, doing Code a change:** "Show me why you think this task was Directed and what
  would make it Delegated." Success: she finds most next steps useful, and flags the rest in seconds.
- **Dana, rung 2, doing Write a spec or doc:** "Don't judge my spec work by coding norms." Success:
  her doc tasks are judged against doc examples, never against "did the agent run the tests".
- **Marcus, rung 3, no agent with repo access at work:** "Don't tell me I'm low when the tool isn't
  allowed." Success: he sees a blocker, not a low potential.

### Information architecture
- **Task judgment panel:** Task title · Current rung (with confidence) · Next step (potential + gap) ·
  Why (evidence items) · Blockers · This doesn't fit · History (judgments and feedback).
- **Entry points:** a coaching card's Evidence link; a task row in AIQ home (views spec).
- **Settings > Rung model:** judge account (choose Claude with your own API key, or Copilot with VS Code consent; cross-provider on/off, daily budget, cost so far),
  my guidance preferences (view, reset per job).
- Labels for the glossary: *Rung, Current rung, Next step, Ceiling, Evidence, Blocker, Right rung,
  Uncertain, This doesn't fit, You flagged this.*

### User flows
```mermaid
flowchart TD
  close([Task closes]) --> ai{Any AI interaction?}
  ai -->|no| skip[Not judged]
  ai -->|yes| judge{Judge available?}
  judge -->|"no: offline, signed out, quota"| queue[Queue and retry later] --> close
  judge -->|"no: other provider, not allowed"| np["Not judged; panel says why and links to Settings"]
  judge -->|yes| j[Rung judgment]
  j --> conf{Confidence above threshold?}
  conf -->|no| unc["Uncertain: shown, never coached"]
  conf -->|yes| blk{Blocker caps potential?}
  blk -->|yes| capped["Blocker shown; no habit tip"]
  blk -->|no| right{Potential equals current?}
  right -->|yes| rr["Right rung: neutral, never coached"]
  right -->|no| pconf{Potential confidence at least 0.8?}
  pconf -->|no| shown["Gap shown, not coached"]
  pconf -->|yes| gap["Gap available to coaching"]
  shown --> panel
  unc --> panel
  capped --> panel
  rr --> panel
  gap --> panel
  panel[Person opens the panel] --> act{Person's action}
  act -->|reads evidence| ev{Evidence still there?}
  ev -->|yes| show[Show cited events]
  ev -->|no| del["Say evidence was deleted"]
  act -->|This doesn't fit| pick[Pick a reason, optional note] --> conf2{Confirm?}
  conf2 -->|no| panel
  conf2 -->|yes| corr["You flagged this; gap withdrawn, rung unchanged"]
  corr --> undo{Undo?}
  undo -->|yes| panel
  act -->|"What was read?"| read["Show the prompts, outputs and signals used"]
```

Recovery paths: feedback can be undone from History (the gap returns); guidance preferences can be reset per job in
Settings.

### Wireframe-level structure
- **Panel (AIQ home, and a VS Code side panel when opened from a card):** header with task title and
  surfaces used; a two-column rung line "Done at: Directed → Next step: Delegated"; one gap sentence;
  an evidence list; a blocker note if any; footer with [This doesn't fit] and "What was read?".
- **CLI:** `aiq task <id>` prints the same fields as plain text; `aiq flag <id> <reason>` gives feedback.

### UX acceptance criteria
- From a coaching card, the panel opens in 1 step.
- Feedback takes ≤ 3 steps (open, pick reason, confirm) and can be undone in 1.
- Every branch of the flow ends in a shown state or a queued retry; none is a dead end.
- Uncertain, Right rung and Blocker states are each distinguishable in words, not only by colour.

## Part C — UI specification

### UI Archetype Signature
- **Archetype:** B1 · Keyboard-Velocity GUI (nearest row; the panel is a detail pane in a keyboard-first tool)
- **Signature deviations:** `Layout: DetailPane` (no master list in this spec), `Persistence: LocalOnly`, `Sync: LocalOnly`, `Color: HostTheme` (VS Code) / light and dark (AIQ home), `Feedback: Calm`.
- **Selection:** auto-selected from the JTBD. The job is "look at one task's judgment, understand it and flag it fast", which maps to B-series keyboard-first record detail (Linear issue view), not to A4 gamified progress: the tone is honest feedback, not grading.

### Medium(s) & platform guidelines
VS Code webview view guidelines; AIQ home desktop app (framework decided in `/define-architecture`);
terminal conventions for the CLI (plain text, respects `NO_COLOR`).

### Visual intent & tokens
Qualities: honest, specific, calm, open to feedback. Not: grading, celebratory, comparative, preachy.
Rungs use names first, numbers second ("Directed (2)"). Host theme tokens in VS Code; AIQ home tokens
come from `DESIGN.md`, to be produced in `/design-slice`.

### Key screens & component states
Task judgment panel: loading (judging), judged, uncertain, right rung, capped by blocker, you flagged
this, evidence removed, judge unavailable (queued), not judged (other provider not allowed), error. This-doesn't-fit control: closed, picking,
confirming, saved, undone.

### Motion, copy, accessibility & performance
- No motion beyond host defaults; nothing animates under reduced motion.
- Copy: second person, plain words, no scores beyond the rung name. Example gap: "Give the agent the
  acceptance criteria and let it run the tests itself." Example blocker: "Delegated needs an agent
  with repo access, which isn't available to you here."
- WCAG 2.2 AA; all controls keyboard-operable; the rung line is read as one sentence by screen
  readers.
- Panel renders ≤ 200 ms after open for a judged task.

### AI-UX
HAX G2 (make clear how well AIQ can judge: confidence and Uncertain), G11 (why: evidence), G9 (efficient
feedback), G15 (granular feedback), G16 (convey consequences: what feedback changes).

### UI acceptance criteria
- No panel text compares the person with anyone or names their role.
- Uncertain, Right rung, capped and flagged states each carry a text label.
- All panel controls work by keyboard alone; CLI output is readable with `NO_COLOR=1`.

## Threat review: the judge path (STRIDE)
Run 2026-10-31 at Imran's request, over boundary **B5** (core → the person's own AI provider) and the
rung-model surfaces. It builds on the system threat model on the `define-architecture` branch
(`docs/security/threat-model.md`, T-B5-1 to T-B5-5, T-REPO-2), which already covers the items marked
*existing*. New items are numbered TR-n and proposed for that register.

**Assets on this path:** task content (prompts, agent output, code), the person's own edits, the
judge account's credentials and quota, judgments and evidence (reveal how someone works).

| ID | STRIDE | Threat | Control (requirement on this spec) | Status |
|---|---|---|---|---|
| T-B5-1 | Info disclosure | A task is sent to a provider that had not seen it | Same-provider rule (US-5) | Existing |
| TR-1 | Info disclosure | A task spanning surfaces (Copilot Chat, then Claude Code) sends one provider's prompts to the other | Only content the judge's provider already received is sent; the rest is judged from log facts only. A stricter form of T-B5-1 | New |
| TR-2 | Info disclosure | The person's own edits (editor changes, local git diff for F2) are content no provider saw | F2 and other log facts are computed on device; their raw inputs are never put in a judge call | New |
| T-B5-2 | Info disclosure | Work-account admin logs and retention capture judge calls | Disclosed at setup; judging can be turned off (US-5) | Existing, accepted |
| T-REPO-2 | Info disclosure | F9 repo reuse facts pull in colleagues' names | Local git only; counts, not names; no hosting API calls | Existing |
| TR-3 | Info disclosure | A pack's fact questions are written to pull more content into the judge call | The core alone builds the judge envelope and decides which content is attached; a pack supplies question text and answer schema only | New |
| T-B5-5 | Tampering | Prompt injection in task content steers the judge | Content as quoted data; narrow facts to a JSON schema; deterministic guardrails cap the rung; two runs (US-7 gaming case) | Existing |
| TR-4 | Tampering | Judge output that fails the schema, or carries extra text, is trusted | Any response that fails the schema is discarded and the fact is unknown; free-text fields are length-capped | New |
| TR-5 | Tampering / elevation | Model-written evidence or gap text, steered by injected content, renders as markup or links in the VS Code panel or a card | All model-written text renders as plain text: no HTML, Markdown links or commands | New |
| T-B5-3 | Tampering | AIQ observes its own judge calls | Tagged and skipped at ingest (US-5) | Existing |
| T-B5-4 | Elevation | The judge acts on the repo | Judge calls are plain model API calls with no tools granted; no CLI agent is run for judging (Q7) | Existing, narrowed by Q7 |
| TR-6 | Spoofing | A repo or environment setting (e.g. a base-URL variable in a project `.env`) points the judge at a look-alike endpoint that collects task content and the API key | The judge uses the provider's fixed official endpoint over TLS and ignores repo and environment overrides; same-user malware stays out of scope (T-B1-4) | New |
| TR-7 | Spoofing / info disclosure | AIQ handles the person's provider credentials | Claude: the person's own Anthropic API key, entered at setup and stored only in the OS keychain, never in the store, logs or packs; AIQ never uses or reads their Claude sign-in. Copilot: the VS Code Language Model API holds the credentials and AIQ never sees them (Q7) | New |
| TR-8 | Denial of service (cost) | Runaway judging drains the person's quota or premium requests: event floods creating tasks, two runs per fact, re-judging every task after a rubric update | A daily judge budget the person sets (calls and, where known, cost), visible in Settings; re-judging after an update is lazy (on open) or batched within the budget; when the budget is spent, tasks queue | New |
| TR-9 | Repudiation | The person can't tell what was sent where, or which version judged a task | "What was read?" lists content, account and time per call (US-5); judgments record fact set, rubric and judge versions | New (control already in spec) |

New acceptance criteria from this review (added to US-5 and US-6 scope):
- **Given** a task with content from two providers **When** judged by one **Then** only that provider's own prompts and outputs are sent; facts needing the rest come from logs or stay unknown. (TR-1)
- **Given** any judge call **When** captured **Then** it contains no editor edits or local diff text written by the person. (TR-2)
- **Given** a judge response that fails the fact schema **When** received **Then** it is discarded and the affected facts are unknown. (TR-4)
- **Given** evidence or gap text containing HTML, Markdown links or command syntax **When** rendered **Then** it shows as literal text. (TR-5)
- **Given** the person's daily judge budget is spent **When** a task closes **Then** it is queued, and no further judge call is made that day. (TR-8)

Residual risk: a work-account admin can see judge calls (accepted, T-B5-2); judge quality on
adversarial content is only as good as the US-7 gaming bar; same-user malware is out of scope.
Independent security review still recommended: this pass was done by the spec's author.

## Flagged risks & residual unknowns
- Grouping events into tasks and classifying jobs are unproven. Depends on the Phase 0 feasibility spike.
- **Provider terms (Q7, settled):** Claude judging uses the person's own Anthropic API key, which Anthropic's terms allow; Copilot judging uses the VS Code Language Model API. A person without an API key cannot use Claude as their judge; their Claude-only tasks are queued and the panel says why. Open: how Copilot counts these calls against premium requests.
- Retention and admin visibility differ by account type (personal vs work). Probe: list them per provider before setup copy is written.
- Judging cost per person per day is unknown. Probe: measure on the first check set and a week of the team's own tasks.
- The spike's numbers come from synthetic data with Claude as both labeller and judge, so they likely flatter the judge. Probe: re-run on constructed and dogfood check tasks with a human checking facts.
- Whether model-read facts reach the US-7 bar is unknown. Probe: run them on the first constructed tasks and dogfood traces.
- Tasks built at a known rung on purpose may not look like real work. Probe: compare fact distributions between constructed and dogfood tasks.
- Holistic placement with floors and guardrails scored 88% exact on synthetic data labeled and judged by Claude, in a variant designed after seeing that data, with simulated F9 counts. Floors could over-rate a task if F8 or F9 are misread from logs; guardrails could wrongly lower good placements. Probe: the spike thread's re-run of the guarded variant, then the expert sanity check.
- Running model facts twice doubles judging cost on the person's quota. Probe: measure cost per task; drop to one run for log-heavy jobs if needed.
- Task close detection (idle timeout vs artifact boundary) is not defined here; inherited from task grouping.
- Comparables are from memory (Flagged / Inferred). Probe: `/collectknowledge` on LLM-as-judge reliability.

## Gate record
`GATE specify · 2026-10-31 · Simplifier, Test Architect, Data & Persistence Architect, UX Researcher/IA, UX & Accessibility, Security & Privacy (run by Claude in one session; not independent) · verdict: pass with conditions · vetoes→resolution:`
- Simplifier (soft): ceiling kept as a stored field only, not coached and not in this spec's UI; two-run consistency doubles judge cost, accepted with a fallback to one run for log-heavy jobs. Resolved.
- Test Architect (hard): potential rule contradicted the "at most one rung above" invariant; rewritten. "Drove the agent step by step" had no fact behind it; added F12. Fact-confidence threshold now set per fact from the fact check (spike showed a fixed 0.7 sank most tasks to rung 1). Unclassified handling made concrete. Potential-confidence gate (0.8) added to the flow. Resolved.
- Data & Persistence Architect: every concept has a home; judgments are versioned by fact set, rubric and judge version and never edited; Check set consent invariant added. Grain and durable representation go to `/define-architecture` and `/design-slice`. Resolved.
- UX Researcher/IA: missing states for "other provider not allowed" and "queued" added to flow and panel states. Judge-account setup (with the work-account disclosure) has acceptance criteria in US-5 but no drawn flow; drawn in the AIQ home / Settings spec. Recorded.
- UX & Accessibility: every non-default state has a text label; rung never by colour alone. Resolved.
- Security & Privacy (hard): the judge sends task content to the person's own provider account. STRIDE for the judge path run 2026-10-31 at Imran's request (Threat review section): nine new or confirmed controls, five new acceptance criteria, residual risk stated. **Recorded, not self-cleared:** the reviewer is the spec's author, so an independent security review is still needed before Accepted. Provider terms settled by Q7 (own Anthropic API key).
- Caveat: every seat was played by the same author, so this is not the independent review the pack expects. Recommend a human or separate-session review before marking Accepted.

---
**Handoff:** → `/define-architecture` (judge runtime and placement), then `/design-slice`.
