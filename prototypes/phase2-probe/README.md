# Phase 2 laptop probe

Answers the four open questions behind the Copilot CLI and VS Code slice (P1–P4 in `probe.py`).
Run on the Windows laptop, from the repo root, in **Windows Terminal** (not VS Code's terminal, which
would make the CLI look like VS Code):

```
git pull
python prototypes/phase2-probe/probe.py cli          # Copilot CLI with a throwaway COPILOT_HOME
python prototypes/phase2-probe/probe.py vscode-on    # adds ~/.copilot/hooks/aiq-probe.json
#   VS Code: Copilot Chat, Agent mode, send one short prompt, wait for the reply
python prototypes/phase2-probe/probe.py vscode-off   # removes that file
python prototypes/phase2-probe/probe.py report       # writes results/laptop-probe.json
```

`cli` never touches the real Copilot config. `vscode-on` adds one file and nothing else; `vscode-off`
removes it. The log in `%TEMP%\aiq-phase2-probe.jsonl` holds no prompt text (lengths only); the report
also drops session ids.
