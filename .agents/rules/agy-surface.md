# Antigravity (agy) — AI-Forward Pack surface

This repository installs the AI-Forward Pack. Antigravity automatically loads root `AGENTS.md` as project rules (the constitution). This file is only the **Antigravity path map** so you do not follow Copilot-only or Claude-only locations. Do not paste knowledge docs here — that would attach them on every turn (defect class CTX-B).

## Skills

Project skills live in `.agents/skills/<name>/SKILL.md` (and are discoverable via `.agents/skills.json`). Invoke as `/specify`, `/implement`, `/design-slice`, `/optimize-graph`, and the other pack skills. Stage files are `reference/` beside each `SKILL.md`; read them at the stage via `view_file`, never by re-invoking the skill.

The pack `/implement` **overrides** any generic implementation behavior. In a pack-installed repo, follow the Rigor Protocol implement loop.

## Knowledge

Read the shared copies at `.claude/knowledge/<name>.md`. Where `AGENTS.md` cites `.github/instructions/<name>.instructions.md`, that is the Copilot wrap of the same document — do not treat the wrap as a second source. Docs with `load: skill` or `load: reference` are also under `.github/knowledge/`; prefer `.claude/knowledge/`.

## Personas & Subagents

To convene the persona council:
1. **Inline turn (default)**: For standard turns, enact the peer/adversary perspectives inline with explicit PASS/BLOCK ratings.
2. **Subagent `self`**: Call `invoke_subagent(TypeName="self", Role="<Persona>", Prompt="...")`. Subagent `self` inherits repo rules (`AGENTS.md`) and tools.
3. **Dynamic subagents (`define_subagent`)**: For persistent specialists, register the persona via `define_subagent(name="...", description="...", system_prompt="...")` reading prompts from `.claude/agents/<name>.md`.

Author in Peer Mode, review in Adversary Mode; the author never clears its own hard veto.

## Hooks

`.agents/hooks.json` wires one named section, `ai-forward` (`enabled` is explicit; the host defaults it to true), with one command per event: `dispatch.py --host agy --event <event>` runs that event's hooks in one process (B15). A refresh removes the five per-hook sections earlier revisions shipped and keeps project-owned sections. Shape per event, from the host's own docs: `PreToolUse`/`PostToolUse` handlers sit under a `matcher` + `hooks` wrapper (`""` matches every tool); `PreInvocation` and `Stop` take handlers directly — a `PostToolUse` handler written in the direct form loads without complaint and never fires (measured 2026-09-20). The hooks it runs:

- **re-read guard** (`reread-guard.py`, CTX-D) on `PreToolUse`, matcher `view_file`
- **session-start audit marker** (`session-start.py`, AL4a) on `PreInvocation`
- **mail doorbell** (`mail-doorbell.py`) on `PreInvocation` — the inbox count and a pointer, injected as steps, never a body
- **heartbeat** (`heartbeat.py`) on `PostToolUse` (wrapped, matcher `""`) and `Stop` — a progress row in `$AGENT_SESSION`'s ledger
- **owner review gate** (`owner-review-gate.py`) on `Stop` — answers `{"decision": "continue"}` while the session holds an unresolved decision request it sent (at most twice per stop sequence), the reason on stderr

Every coordination hook reads the session identity from `AGENT_SESSION` in the process environment and exits silently
without it, or when the repository holds no coordination record: launch `agy` with `AGENT_SESSION=<id>` exported. On Antigravity the doorbell and heartbeat are
`observed-only` until a live session shows each one work end to end; `Stop` does fire here (heartbeat rows on `Stop`, 2026-09-20), so the owner review gate is wired and `observed-only`, no longer `unsupported` (CO12; `pack/adapters/hooks/README.md`).

When a brief asks for `coord request ack <id> --blob $(git hash-object <path>)`, run it as two commands on Antigravity — `git hash-object <path>` first, then `request ack <id> --blob <sha>` — because in the S1 test (2026-09-20) the single-line form was approved and never returned a result (`note-20260920-agy-hooks-loaded-but-not-enabled`).

## Scripts

On Windows use `python` or `py -3`; on Linux/macOS use `python3`. `docs/ai-forward-pack/scripts/pack-doctor.py` names the working form for this machine.
