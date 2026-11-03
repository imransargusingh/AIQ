# Copilot tool calls after the first turn end (2026-11-03)

**Finding** (DC-020's sweep, `docs/investigations/no-actions/`): AIQ's Copilot import reader closed a prompt's
turn at its first main-line `assistant.turn_end`. Copilot writes one after each model round, so later tool calls
in the same prompt were dropped. On Imran's store, 2 prompts lost 248 tool calls this way (measured read-only).

**Fix:** reader 2 (`COPILOT_READER` in `crates/aiq/src/import.rs`) keeps the turn open to the next typed prompt
or the end of the file, and stamps the stop at the turn's last main-line `assistant.turn_end`. The next
`aiq import` reads every Copilot session again once, so sessions imported before the fix are corrected. The
judge's live Copilot path reads the same function and is fixed with it. Defect class DC-021.

**`measure_recovered.py`** replays reader 1 and reader 2 over every Copilot log and prints counts only.
Run read-only on Imran's laptop 2026-11-03 ~14:50Z (`python -I`, exit 0, 164 s), all logs including sessions
that import later sets aside as automated:

| Count | Reader 1 | Reader 2 |
|---|---:|---:|
| Main-line tool calls | 3,391 | 85,817 |
| Sub-agent starts | 4 | 411 |

6,532 sessions, 306 change; 7,730 prompts, 1,408 gain; 10 go from no actions to some. Reader 1 kept about 4% of
the tool calls. Most prompts that gain already had some actions, so a yes/no device fact changes for few of them;
anything that counts or names the tools changes for many.
