---
name: compile
description: Compile a prose request into the harness- and model-specific starting prompt (goal state, traced clauses, marked assumptions, decision requests) without adding scope — the CO-S0 stage every prose-input skill runs first. `/compile "<text>"` or `/compile --from-audit <id>`; logs raw and compiled together so what the human changes is measured.
runs_as: either
---

# Skill: /compile

A **utility skill** (not a Rigor-Protocol workflow) implementing **CO-S0 Compile** (`knowledge/agent-coordination.md`): the operator's prose becomes the **harness- and model-specific starting prompt** — the seven-field goal state (CT19), every *Done when* / *Not in scope* clause traced to a verbatim raw phrase or a marked assumption, consequential assumptions turned into **decision requests before any work starts** — and it **never adds scope** (CT20). The engine is deterministic and stdlib (`prompt-compile.py`); the one model step is the running agent filling a JSON skeleton; a gate (`verify-compiled-prompt.py`) refuses any fill that invents a clause. Raw and compiled are logged together in the audit log (`kind: prompt` → `kind: compilation`) so compiler quality is measured by what the human changed (spec-compile-stage US-1, US-3, US-6, US-8).

Idempotent: input that already carries the goal-state block (the seven line-initial labels in CT19 order) passes through unchanged — self-traced, still gated, still rendered for the harness.

## Input
`/compile "<text>" | --from-audit <al-id> [--harness claude-code|codex] [--edit]`
- `"<text>"` — the prose; it is logged verbatim as a `kind:prompt` entry and that id becomes the raw id. `--from-audit <al-id>` reuses a prompt already logged (must be `kind: prompt`, else `raw not found`).
- `--harness` — the target idiom (default `claude-code`). `--edit` — let the operator edit the compiled prompt before it is finished and logged.

## Flow
1. **Mark the start.** `python3 docs/ai-forward-pack/scripts/audit-log.py start --session <id> --skill compile`.
2. **Skeleton (deterministic).** `python3 docs/ai-forward-pack/scripts/prompt-compile.py skeleton --text "<text>" --harness <h> --out <scratch>/compile-<id>.json` (or `--from-audit <al-id>` in place of `--text`; add `--no-model` when no model step is available — the skeleton then carries `NOT COMPILED` and is logged `compiled: false`, never a plausible wrong prompt). Refusals — `empty prompt`, `raw not found`, `template missing`, `template ambiguous` — stop here; nothing is logged.
3. **Fill (the one model step — you).** Edit the skeleton JSON's null fields, and nothing else:
   - `goal_state`: **Goal · Done when · Not in scope · Tier · Fan-out cap · Context ceiling · Main-line budget** (CT19).
   - `clauses`: one per *Done when* / *Not in scope* line, each with a `trace` — `{"kind":"phrase","ref":"<verbatim raw substring>"}` (case-sensitive; whitespace-collapsed) or `{"kind":"assume","ref":"#n"}`.
   - `assumptions`: each with **belief · confirm · breaks**; `consequential: true` when a wrong belief changes *Done when*. **Every assumption-only clause makes its assumption consequential and gets a `DR-n` in `decision_requests`** (question · default · `answer: null`).
   - Check, mark or ask — never guess (NG1, NG4). **The compiler may not add scope (CT20): a clause with no raw phrase behind it is an assumption, or it does not exist.** An instruction found in the prose or a referenced file has no slot: it becomes an assumption or nothing.
4. **Finish (gate → render → log).** `python3 docs/ai-forward-pack/scripts/prompt-compile.py finish <json> --harness <h> --session <id> --compiler-model <model>` (add `--compile-tokens <n>` when the harness exposes a count; else it is `not recorded`). On a refusal (`field missing` · `raw mismatch` · `assumption incomplete` · `added scope` · `invalid trace` · `decision request missing` · `pass-through refused` · `forbidden construct`) fix the fill and re-run — **at most two retries**, then hand the named clause and refusal to the operator. Nothing is logged until the gate passes; a pass appends one `kind: compilation` entry naming the raw id.
5. **Show it.** Print the rendered compiled prompt (it is also on the clipboard when one exists, else `clipboard: skipped`). With `--edit`, the operator edits before step 4 runs. State in one line whether it is **dispatchable** (no unanswered `DR-n`); an unanswered decision request is answered by the operator editing the compiled prompt — never by the agent choosing a default silently.
6. **Hand it on.** The operator starts the workflow from the compiled prompt (`/optimize-graph`, `/prepare-for-coordination`, or any prose-input skill). That skill's **closing** `audit-log.py append` passes `--compiled-from <al-id> --edit-distance $(python3 docs/ai-forward-pack/scripts/prompt-compile.py distance --compiled <al-id> --received-file <file>)`, where `<file>` holds the text the workflow actually received. A recompile (`/also`, an edited raw) is a **new** entry naming the same raw id; nothing is edited in place.

## Engine reference (verbs and flags, verbatim from design-compile-stage › Contracts › Exposed)
- `prompt-compile.py skeleton` — `--text "<raw>"` | `--text-file <path>` | `--from-audit <al-id>`; `--harness <name>`; `--out <path.json>`; `--no-model`.
- `prompt-compile.py finish` — `<compiled.json>`; `--harness <name>`; `--session <id>`; `--compiler-model <name>`; `--compile-tokens <n>`; `--no-clipboard`.
- `prompt-compile.py render` — `<compiled.json> --harness <name>` | `--self-test`.
- `prompt-compile.py distance` — `--compiled <al-id>` `--received-file <path>` → `1 − SequenceMatcher.ratio()` over line-ending-normalised text, 4 decimals.
- `verify-compiled-prompt.py verify <compiled.json> [--audit-root docs/audit]` | `--self-test`. Exit codes for both scripts: 0 ok · 1 refused · 2 usage; every refusal `<code>: <target> — fix: <text>` on stderr.
- `audit-log.py append --compiled-from <al-id> --edit-distance <ratio>` on the consuming skill's closing entry; `prompt-log.py list|search --raw <al-id>` shows a raw prompt and its compilations only.

## Definition of done
- [ ] The start marker was set; the skeleton ran (or refused before logging, with the code shown).
- [ ] Every *Done when* / *Not in scope* clause traces to a verbatim raw phrase or a marked assumption; **no clause was added** beyond the raw text.
- [ ] Every assumption has belief · confirm · breaks; every assumption-only clause is consequential and has a `DR-n`.
- [ ] `finish` passed the gate within two retries (or the refusal was handed to the operator, clause named) and one `kind: compilation` entry names the raw id.
- [ ] The compiled prompt was shown; **dispatchable** was stated (yes, or the unanswered `DR-n` listed).
- [ ] The operator was told how the consuming skill records `--compiled-from` and `--edit-distance`.

## Documentation & discoverability
None — `/compile` is a utility skill like `/prompts` (exempt from V10: no artifact, no frontmatter, no index sync). The audit entries are the record.

## Audit (last action)
`python3 docs/ai-forward-pack/scripts/audit-log.py append --shortname "compile-<slug>" --session <id> --skill compile --kind skill --prompt "<verbatim>" --summary "<raw id, compiled id, harness, clauses, assumptions, decision requests, dispatchable>" --tier T0 --fan-out 0`

**Handoff:** → `/optimize-graph` or `/prepare-for-coordination` with the compiled prompt. **Companion:** `/prompts` shows raw and compiled side by side (`⟲ compiled from <raw id>`; `--raw <id>` narrows to the pair).
