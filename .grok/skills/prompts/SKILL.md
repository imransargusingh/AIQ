---
name: prompts
description: Browse your logged prompts as a stack (newest on top) and reuse one — arrow keys to move, right-arrow to expand a prompt, left-arrow to collapse, Enter to copy it for paste-and-edit. A utility skill backed by the stdlib prompt-log engine.
runs_as: either
---

# Skill: /prompts

A **utility skill** (not a Rigor-Protocol workflow) for reasoning over your prior prompts. It opens your project-local prompt log as an interactive **stack — newest on top** — navigated with the arrow keys: **↑/↓** move, **→** expand, **←** collapse, **Enter** reuse. Reuse copies the chosen prompt to your clipboard so you can **paste it into your next prompt (Cmd/Ctrl+V) and edit before sending**.

Companion skill: **/searchprompts** (the same stack, pre-filtered by freeform text). Both are reuse lenses over the same unified **audit log** (`docs/audit/audit-log.jsonl`) — the broader timeline/search/change-log/viewer is **/auditlog**.

## Engine
All behavior is in the stdlib script **`docs/ai-forward-pack/scripts/prompt-log.py`** (in this source repo: `pack/scripts/prompt-log.py`). No third-party dependency. **Unified store (one source of truth):** the prompts come from the committed **audit log** `docs/audit/audit-log.jsonl` — the same store `/auditlog` reads — so *every* prompt the Audit Mandate records (skill runs, scripts, and prompts you `add`) is reusable here, and there is no second parallel prompt store. `add` writes a `kind:prompt` entry **through `audit-log.py`** (the single writer of record). Override the store with `--store` (e.g. a legacy `<repo>/.aiforward/prompts.jsonl`) or `$AIFORWARD_PROMPT_LOG`; the reader adapts to either schema.

## How prompts get logged (read this — it is the one honest limitation)
There is **no hook** that auto-captures every prompt you type into the CLI, so logging is *explicit*:
- **You:** `python3 docs/ai-forward-pack/scripts/prompt-log.py add "your prompt text"` (or pipe via stdin).
- **The agent (default-on convention):** the managed block in `AGENTS.md` / `CLAUDE.md` asks the agent to append your **substantive** requests to the log as a lightweight first action — so the stack fills as you work, without you remembering. Tell the agent to stop logging at any time.

## What this skill does
1. **Run the interactive browser:**
   `python3 docs/ai-forward-pack/scripts/prompt-log.py browse`
   - ↑/↓ (or `j`/`k`) move · → (or `l`) expand · ← (or `h`) collapse · `/` filter · **Enter** reuse · `q` quit.
   - On Enter the chosen prompt is printed and copied to the clipboard (pbcopy/xclip/clip when present).
2. **If there is no interactive terminal** (e.g. you ask the agent to do it inside a non-TTY shell), the script prints the newest-first stack as a numbered list. Render that list, let the user pick a number, then run `prompt-log.py show <n>` to expand it and `prompt-log.py get <n> --copy` to put it on the clipboard for paste-and-edit.
3. **Reuse:** the user pastes the copied prompt into their next CLI prompt and edits before executing. (A skill cannot type into the CLI's input line.)

## Quick reference
- `prompt-log.py list` — the stack, newest first (label · time).
- `prompt-log.py show <n|id>` — one prompt in full.
- `prompt-log.py get <n|id> --copy` — raw text to stdout and the clipboard.
- `prompt-log.py browse` — the arrow-navigable expand/collapse stack.
- `prompt-log.py add "<text>"` — log a prompt.
- `prompt-log.py list --raw <id>` — one raw prompt beside its compilations (`⟲ compiled from <raw id>`, CO-S0); reuse copies either, search matches both.

## Definition of done
- [ ] The stack was shown newest-first; the user could expand/collapse and pick one.
- [ ] The chosen prompt is on the clipboard (or printed) for paste-and-edit — the skill never executes it for them.

**Handoff:** none — utility skill. Companion: **/searchprompts**.
