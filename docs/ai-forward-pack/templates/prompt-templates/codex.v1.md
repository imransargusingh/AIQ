---
harness: codex
version: 1
current: true
forbids: ["EnterWorktree"]
---
Save the brief between the markers as <brief-file>, then run (one line, the brief read from the file):
codex exec --json -o <last-message-file> --output-schema <schema-file> --worktree -C <dir> "$(cat <brief-file>)"
--- brief ---
python3 docs/ai-forward-pack/scripts/audit-log.py start --session {{session}} --skill {{skill}}
{{goal_state}}
{{trace}}
{{references}}
{{assumptions}}
{{decision_requests}}
{{contract_slot}}
Rules: absolute paths only; a multi-line program is a file, then a run; a gate's exit status is never behind a pipe.
{{provenance}}
--- end brief ---
