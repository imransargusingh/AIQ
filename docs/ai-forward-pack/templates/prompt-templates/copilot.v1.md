---
harness: copilot
version: 1
current: true
forbids: ["EnterWorktree", "ExitWorktree"]
---
Start the audit marker for session {{session}} and skill {{skill}} before grounding, using the installed audit-log.py start command and the current platform's Python interpreter.
{{goal_state}}
{{trace}}
{{references}}
{{assumptions}}
{{decision_requests}}
{{contract_slot}}
Use the assigned absolute working directory and coordination identity. Use python on Windows and python3 on POSIX for the installed coordination scripts. Do not change the admitted model, trust, or permission profile. Return evidence to the Owner; transport completion is not acceptance.
{{provenance}}
