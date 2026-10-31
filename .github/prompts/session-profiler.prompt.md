---
mode: agent
description: Profile one or more pack-consuming repos' Claude Code and Copilot CLI sessions from their local telemetry and produce a findings table and a fixes table for performance, efficiency, task adherence, parallelism and cross-harness coordination — the continuous-improvement loop for how the pack performs on every model and harness.
---
You are running the **session-profiler** workflow (`knowledge/rigor-protocol.md` on the harness telemetry; `instrumentation-over-inference.md` IO1–IO13; `communication-and-task-discipline.md` CT19–CT25; `execution-graph-optimization.md` GO7/GO19; `session-worktree-discipline.md` WT1a). Lead peer: the **AI Systems Engineer**, with the **SRE**; adversaries: **the Simplifier** (soft veto — strike findings that change no decision), the **Test Architect** (hard veto — a fix with no named control does not enter the fixes table), the **Tech Lead** (no ceremony added to remove ceremony). **Tier: T0 · Fan-out cap: 0 — this workflow convenes no sub-agents.**

**Ground first:** `python docs/ai-forward-pack/scripts/audit-log.py start --session <id>`; read `docs/lessons/defect-classes.md` (the CTX-\* classes), `docs/profiles/PROFILES.md`, and `python docs/ai-forward-pack/scripts/session-profile.py fixes`.

**Interdict the rush:** do not tune guidance from a feeling. `session-profile.py --repo <path> [--repo …] --days N discover` — name the sessions first. A missing store is reported as `not recorded`, never estimated around.

**OPEN:** name the operator questions (cost per turn and cache share · context growth and compactions · TTFT p50/p90 · sub-agents vs declared tier and fan-out cap · tool calls per delegation and converge nudges · re-reads and paged-output views · skill re-invocations · goal-state and tier presence · overlapping sessions in one checkout · model family × harness). Write the goal state with `Tier: T0`, `Fan-out cap: 0`.

**INTERROGATE (measure):** `session-profile.py --repo … --days N profile --session-id <id>` → `<profiles>/<sp-id>/profile.md`, where `<profiles>` is `docs/profiles/` (tracked) only in the pack source repo or with `--publish`, else the untracked `<git-dir>/ai-forward/profiles/` (FR-104); every string is scrubbed first (FR-103). Read the findings (SP-01…SP-16, severity, confidence, evidence, fix ids). For every **Inferred** finding, open the turn in the transcript and confirm or strike it, classifying it *drift · tangent · ceremony · legitimate* with the prompt and first reply quoted. The script never judges intent; you do, with evidence.

**EVIDENCE (compare):** `session-profile.py … compare` — per family × harness: requests, cache-read, output, reasoning, cost (where recorded), TTFT p90, context at turn end, wall clock, drift indicators per turn. An SP-14 family gap is a tuning finding only on like-for-like turns with ≥3 comparable turns per family; state the turn mix. Reconcile SP-15 overlaps with `coord-core.py worktree list`.

**DISCONFIRM:** enact the adversary round inline — each reviewer gives a labelled critique with a severity **[Blocker|Major|Minor|Nit]** and, for the Test Architect, **PASS/BLOCK** with the veto-clears-when predicate (every fixes-table row names the pack surface, the control that fails on recurrence, and how it was seen failing). List struck findings with reasons.

**CONVERGE:** emit (a) the findings table, (b) the fixes table (catalog + repo-specific rows), (c) the family × harness table with per-family tuning notes (which guidance/control/gate to adjust, from the numbers), (d) coordination notes. Register new shapes as CTX-\* classes in the register; run `python docs/ai-forward-pack/scripts/docs-graph.py derive` so the profile node enters the index. Close with the status table (Completed | Remaining | Best next action) and the audit entry: `audit-log.py append --shortname "session-profiler-<sp-id>" --session "<id>" --skill session-profiler --kind skill --prompt "<verbatim>" --summary "<…>" --artifact docs/profiles/<sp-id>/profile.md --goal "<…>" --done-when "<…>" --tier T0 --fan-out 0`.

Hand off: → **/dream** (mines the profiles) · → **/apply-learnings** (push approved fixes) · → **/extendaibundle** (pack changes).

${input}
