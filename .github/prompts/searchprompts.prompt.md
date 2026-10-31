---
mode: agent
description: Search your logged prompts by freeform text and reuse a match — the same arrow-navigable expand/collapse stack as /prompts, pre-filtered to prompts containing all your terms. Utility skill over the stdlib prompt-log engine.
---
You are running the **/searchprompts** utility skill (not a Rigor-Protocol round-table). It finds a past prompt by freeform text and lets the user reuse it. It is **/prompts pre-filtered**.

The engine is the stdlib script `docs/ai-forward-pack/scripts/prompt-log.py` — a reuse lens over the committed **audit log** `docs/audit/audit-log.jsonl` (the same store `/auditlog` reads). Take the user's search terms and:

1. **Prefer the interactive filtered stack.** Tell the user to run:
   `python3 docs/ai-forward-pack/scripts/prompt-log.py pick <their terms>`
   — opens the stack over just the prompts whose label or text contains **all** the terms (case-insensitive, newest first): **↑/↓** move, **→** expand a match to read it in full, **←** collapse, **Enter** to reuse (copied to clipboard for paste-and-edit).
2. **If you are in a non-interactive shell,** run `prompt-log.py search <terms>` (add `--json` if you need structure), render the matches (label · time) newest-first, let the user pick a number, then `prompt-log.py show <n>` to expand and `prompt-log.py get <n> --copy` to copy it. Never execute the recalled prompt for them.
3. If nothing matches, **say so plainly** — do not fabricate results.

The list always shows label + timestamp; → expands the one you are on, ← collapses it. Keep it terse.

${input}
