---
name: searchprompts
description: Search your logged prompts by freeform text and reuse a match — the same arrow-navigable expand/collapse stack as /prompts, pre-filtered to prompts whose label or text contains all your terms. A utility skill backed by the stdlib prompt-log engine.
runs_as: either
---

# Skill: /searchprompts

A **utility skill** (not a Rigor-Protocol workflow): find a past prompt by **freeform text**, then reuse it. It is **/prompts pre-filtered** — you give search terms, and the same interactive **stack (newest on top)** opens over just the prompts whose label or body contains **all** your terms (case-insensitive). **↑/↓** move, **→** expand, **←** collapse, **Enter** reuse (copied to the clipboard to paste-and-edit).

Companion skill: **/prompts** (the full stack, unfiltered). Both are reuse lenses over the same unified **audit log** (`docs/audit/audit-log.jsonl`); the broader timeline/search/change-log/viewer is **/auditlog**.

## Engine
All behavior is in the stdlib script **`docs/ai-forward-pack/scripts/prompt-log.py`** (in this source repo: `pack/scripts/prompt-log.py`). **Unified store:** prompts come from the committed **audit log** `docs/audit/audit-log.jsonl` (the same store `/auditlog` reads); `add` writes a `kind:prompt` entry through `audit-log.py`. Override with `--store` (e.g. a legacy `<repo>/.aiforward/prompts.jsonl`) or `$AIFORWARD_PROMPT_LOG`; the reader adapts to either schema.

## What this skill does
1. **Run the filtered interactive browser** with the user's terms:
   `python3 docs/ai-forward-pack/scripts/prompt-log.py pick <terms...>`
   - matches contain **all** terms; the stack is newest-first.
   - ↑/↓ move · → expand · ← collapse · `/` refine the filter · **Enter** reuse · `q` quit.
2. **If there is no interactive terminal**, the script prints the matching stack as a numbered list (newest first). Render it, let the user pick a number, then `prompt-log.py show <n>` to expand and `prompt-log.py get <n> --copy` to put it on the clipboard.
   - You may also run the non-interactive search directly: `prompt-log.py search <terms...>` (add `--json` for structured output).
3. **Reuse:** the user pastes the copied prompt into their next CLI prompt and edits before executing.

## In both list and interactive views
The list shows **label + timestamp**; when you are *on* a prompt, **→ expands** it to the full text and **← collapses** it back to the label; a compiled twin is labelled `⟲ compiled from <raw id>` (CO-S0) and `--raw <id>` narrows to one raw prompt's compilations.

## Definition of done
- [ ] The matches (label · time) for the user's terms were shown newest-first.
- [ ] The user could expand/collapse and pick one; the chosen prompt is on the clipboard (or printed) for paste-and-edit — never executed for them.
- [ ] If nothing matched, that was stated plainly (no fabricated results).

**Handoff:** none — utility skill. Companion: **/prompts**.
