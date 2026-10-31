---
harness: claude-code
version: 1
current: true
forbids: ["EnterWorktree", "ExitWorktree"]
---
python3 docs/ai-forward-pack/scripts/audit-log.py start --session {{session}} --skill {{skill}}
{{goal_state}}
{{trace}}
{{references}}
{{assumptions}}
{{decision_requests}}
{{contract_slot}}
Rules: absolute paths only; a multi-line program is a file, then a run; a gate's exit status is never behind a pipe.
{{provenance}}
