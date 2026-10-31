---
mode: agent
description: Browse your logged prompts as a stack (newest on top) and reuse one — ↑/↓ move, → expand, ← collapse, Enter copies it for paste-and-edit. Utility skill over the stdlib prompt-log engine.
---
You are running the **/prompts** utility skill (not a Rigor-Protocol round-table — no personas to convene). It lets the user reason over their prior prompts and reuse one.

The engine is the stdlib script `docs/ai-forward-pack/scripts/prompt-log.py` — a reuse lens over the committed **audit log** `docs/audit/audit-log.jsonl` (the same store `/auditlog` reads; `add` writes a `kind:prompt` entry via `audit-log.py`). Do this:

1. **Prefer the interactive stack.** Tell the user to run, in their terminal:
   `python3 docs/ai-forward-pack/scripts/prompt-log.py browse`
   — a stack newest-on-top: **↑/↓** move, **→** expand the highlighted prompt, **←** collapse, **Enter** to reuse (the prompt is copied to the clipboard so they paste it into their next prompt with Cmd/Ctrl+V and edit before sending). A skill cannot type into the CLI input line — clipboard + paste is the reuse mechanism.
2. **If you are in a non-interactive shell,** run `prompt-log.py list`, render the newest-first stack (label · time) yourself, let the user pick a number, then `prompt-log.py show <n>` to expand it and `prompt-log.py get <n> --copy` to place it on the clipboard. Never execute the recalled prompt for them — they edit it first.
3. **Logging is explicit** (no CLI hook auto-captures prompts): log with `prompt-log.py add "<text>"`. The managed block also asks the agent to append substantive user requests automatically.

Be honest about the two limits (no auto-capture of every prompt; reuse is via clipboard-paste, not input injection). Keep it terse — this is a utility, not a workflow.

${input}
