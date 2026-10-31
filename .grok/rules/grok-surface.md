# Grok Build — AI-Forward Pack surface

This repository installs the AI-Forward Pack. Grok Build already loads root `AGENTS.md` as project rules (the constitution). This file is only the **Grok path map** so you do not follow Copilot-only or Claude-only locations. Do not paste knowledge docs here — that would attach them on every turn (defect class CTX-B).

## Skills

Project skills live in `.grok/skills/<name>/SKILL.md`. Invoke as `/specify`, `/implement`, `/design-slice`, `/optimize-graph`, and the other pack skills. Stage files are `reference/` beside each `SKILL.md`; read them at the stage, never by re-invoking the skill.

The pack `/implement` **overrides** Grok's bundled implement skill in this repository. That is intended: a pack-installed repo runs the Rigor Protocol implement loop, not the bundled one.

Claude compatibility (default on) also discovers `.claude/skills/`. Same files; a same-named `.grok/skills/` entry wins.

## Knowledge

Read the shared copies at `.claude/knowledge/<name>.md`. Where `AGENTS.md` cites `.github/instructions/<name>.instructions.md`, that is the Copilot wrap of the same document — do not treat the wrap as a second source. Docs with `load: skill` or `load: reference` are also under `.github/knowledge/`; prefer `.claude/knowledge/`.

## Personas

Spawn with `spawn_subagent`. `subagent_type` is the persona `name` (`orchestrator`, `test-architect`, `the-simplifier`, `domain-researcher`, …). Definitions: `.grok/agents/<name>.md`. Author in Peer Mode, review in Adversary Mode; the author never clears its own hard veto.

Grok's built-in `explore`, `plan`, and `general-purpose` types remain available.

## Hooks

`.grok/hooks/ai-forward.json` registers one command per event, `dispatch.py --host grok --event <event>`, which runs that event's hooks in one process (B15: four hook processes per tool call cost a median 11 s per read in bench cells). Project hooks run only after folder trust (`/hooks-trust` or `--trust`). The hooks it runs:

- **re-read guard** (`reread-guard.py`, CTX-D) on `PreToolUse` (it counts only `read_file`/`Read`) and on `UserPromptSubmit`
- **mail doorbell** (`mail-doorbell.py`) on `PreToolUse` and `UserPromptSubmit` — count and pointer as `additionalContext`, never a body
- **heartbeat** (`heartbeat.py`) on `PreToolUse` (Claude-format; this config has no `PostToolUse`)
- **git-identity guard** (`git-identity-guard.py`, PK-07) on `PreToolUse` — exit 2 blocks a git-identity set
- **session-start audit marker** (`session-start.py`, AL4a) on `SessionStart` and `SubagentStart`
- **owner review gate** (`owner-review-gate.py`) on `Stop` — exit 2 with a reason on stderr when this session still holds an unresolved decision request it sent

The doorbell, heartbeat and owner review gate return at once unless coordination is on: a session identity (`AGENT_SESSION`) and a coordination record in the repository (`coord-core.py coordination_active`).

On Grok the doorbell and stop gate are `observed-only` until a live session shows the event fire; the heartbeat was observed live on 2026-09-20 and is `enforced` (CO12; `pack/adapters/hooks/README.md`).

## Scripts

On Windows use `python` or `py -3`; on Linux/macOS use `python3`. `docs/ai-forward-pack/scripts/pack-doctor.py` names the working form for this machine.
