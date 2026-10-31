---
name: auditlog
description: The command-line lens over the project's audit & change log — see the last N actions, search the history by session, date, or keyword, copy or re-run a past prompt, toggle to the meaningful-change timeline, and surface decisions not yet captured. Use to recall what was done or decided in this repo (across sessions), to redo a prior prompt, or to open the interactive timeline viewer. Reads the durable, committed history every skill writes to.
runs_as: either
---

# Skill: /auditlog

The **CLI over the audit & change log** (`knowledge/audit-and-change-log.md`). The project keeps a durable, committed history of every meaningful prompt, skill run, and decision in `docs/audit/` — append-only JSONL that every skill writes to (the Audit Mandate) so work compounds across sessions instead of evaporating when a session ends. This skill is the **reader/dispatcher**: it lets you recall, search, and re-run that history, and open the interactive timeline. It adds no behavior of its own — every action shells to `docs/ai-forward-pack/scripts/audit-log.py` — and it does **not** log its own browsing.

> **Where it sits.** Not a workflow skill — a utility you reach for at any time: at the **start** of work (what was done/decided here before?), to **redo** a past prompt, or to **review** the project's activity. It is the activity-history companion to the Docs Explorer (which shows the artifact graph); `/auditlog` shows the *timeline of actions and decisions* behind it. For fast prompt **reuse**, the companion lenses **/prompts** and **/searchprompts** read the *same* audit log — `/auditlog` is the broad timeline/change-log/viewer lens; they are the quick reuse lens.

## Grounding (first action)
Confirm the bundle exists: `docs/audit/audit-log.jsonl` (and `change-log.jsonl`). If `docs/audit/` is absent, the repo has no history yet — say so and point to the Audit Mandate (any skill run, or `audit-log.py append`, creates it). Resolve the script at `docs/ai-forward-pack/scripts/audit-log.py`. A `kind:compilation` entry is a prompt's CO-S0 twin (`/prompts` shows both; `--raw <id>`).

## Input
From the prompt, the user wants one of: **see** recent activity, **search** for something, **redo** a past prompt, view **changes** (decisions) rather than the full history, **open** the viewer, or **suggest** unlogged changes. Map the request to the matching `audit-log.py` subcommand; ask only if genuinely ambiguous.

## Flow (map the request → the script)

- **See the last *n* actions** (default):
  `python3 docs/ai-forward-pack/scripts/audit-log.py list --n <n>` (add `--kind change` for the decision timeline).
- **Search** by session, date range, or keyword:
  `audit-log.py search --keyword "<term>"` · `--session "<id>"` · `--since YYYY-MM-DD --until YYYY-MM-DD` (combine freely; `--kind change` to search decisions). Present the matches as a compact table (id · datetime · session · label · summary).
- **Redo a past prompt** — the headline capability: list/search to find the entry, then
  `audit-log.py get --id <al-NNNN> --field prompt` to emit the **exact prompt verbatim**. Show it, offer to **re-run it now** (run it as the user's instruction) or hand it back to copy. Never paraphrase the recalled prompt — re-run it as recorded.
- **Toggle to changes** — the meaningful-decision timeline: `list --kind change` / `search --kind change …`, showing each decision's title, rationale, artifacts, and git before/after.
- **Open the viewer** — ensure it is current and point the user to it:
  `audit-log.py render` then open `docs/audit/index.html` (timeline · search · copy-prompt · full-history/changes toggle · **Messages** — the board; works over `file://`).
- **Suggest unlogged changes** — discern decisions not yet in the change log (new ADRs/notes, decision-signalling commits since the last entry): `audit-log.py suggest`; offer to promote any with `audit-log.py change …`.

Present results tersely — a table for lists/searches, the verbatim prompt for a redo. For a redo, confirm before re-running anything that changes the repo.

## Output
The requested view of the history (a table, a single entry, or the re-run of a recalled prompt), or the path to the opened viewer. This skill **reads**; it does not append to the logs.

## Definition of done (exit gate)
- [ ] The request was mapped to the right `audit-log.py` subcommand and run.
- [ ] For a **redo**, the recalled prompt was shown **verbatim** (from `get --field prompt`) and the user chose to re-run or copy — never paraphrased.
- [ ] Searches/lists are presented compactly; the user can find the entry they wanted.
- [ ] For **open**, the viewer was rendered current (`render`) before pointing to it.
- [ ] No entry was appended on behalf of browsing (this skill does not log itself).

## Documentation & discoverability
This skill is a **reader** — it produces no new artifact and therefore writes **no** frontmatter and runs **no** index sync (it is exempt from the Discoverability Mandate's "creates an artifact" trigger, like the pack-lifecycle skills). The history it reads is itself a graph node (`docs/audit/audit-log.md`), created and maintained by the writing skills.

**Handoff:** → re-run a recalled prompt as a fresh instruction; or → the relevant skill if a `suggest` finding should be promoted to the change log.
