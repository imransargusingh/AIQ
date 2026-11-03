# Copilot tool calls after the first turn end (2026-11-03)

**Finding** (DC-020's sweep, `docs/investigations/no-actions/`): AIQ's Copilot import reader closed a prompt's
turn at its first main-line `assistant.turn_end`. Copilot writes one after each model round, so later tool calls
in the same prompt were dropped. On Imran's store, 2 prompts lost 248 tool calls this way (measured read-only).

**Fix:** reader 2 (`COPILOT_READER` in `crates/aiq/src/import.rs`) keeps the turn open to the next typed prompt
or the end of the file, and stamps the stop at the turn's last main-line `assistant.turn_end`. The next
`aiq import` reads every Copilot session again once, so sessions imported before the fix are corrected. The
judge's live Copilot path reads the same function and is fixed with it. Defect class DC-021.

**`measure_recovered.py`** replays reader 1 and reader 2 over every Copilot log and prints counts only: sessions
and prompts that change, and tool calls and sub-agent starts each reader keeps. Run read-only on the laptop with
`python -I measure_recovered.py`. Store-wide numbers: not recorded yet.
