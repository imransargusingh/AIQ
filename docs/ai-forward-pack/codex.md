# Codex and the AI-Forward Pack

Codex reads the repository's `AGENTS.md` as project instructions and discovers skills
from `.agents/skills/<name>/SKILL.md`. This directory is shared with Antigravity;
Codex does not need Antigravity's `skills.json`, rules directory, or hooks configuration.

## Invoke a skill

Use `$collectknowledge <topic>` or `$specify <feature>` in Codex. In the CLI and IDE,
use `/skills` or type `$` to select a skill. The pack's generic `/collectknowledge`
and `/specify` notation means “invoke this workflow”; those names are not registered
as Codex slash commands. Natural-language requests can also select matching skills.
When slash notation reaches the model as text, resolve the named pack skill and read
its `SKILL.md`; repository instructions cannot change the application's command menu.

Read stage files from `reference/` beside the selected skill. Follow its script paths
under `docs/ai-forward-pack/scripts/`, using `python3` on macOS/Linux or `python` /
`py -3` on Windows. Scripts are executable workflow tools, not picker entries.

## Ground in the constitution

Read the foundation at the start of a substantive task, unless already in context:

- `.claude/knowledge/agent-body-of-knowledge.md`
- `.claude/knowledge/agent-rules-of-the-road.md`
- `.claude/knowledge/agent-persona-catalog.md`
- `.claude/knowledge/layered-optimized-architecture.md`
- `.claude/knowledge/engineering-governance.md`

Then read the relevant standards named by `AGENTS.md` and the selected skill.
Resolve a skill's `knowledge/<name>.md` to `.claude/knowledge/<name>.md` from the
repository root. These are shared knowledge files, not Claude-only instructions.
Copilot's `applyTo` wrappers are not automatically loaded by Codex; a path reference
is an instruction to read the file, not evidence that its contents are in context.
Persona definitions live in `.claude/agents/`; read the relevant persona and use the
host's available review/delegation tools. Do not assume Claude subagent registration.

## Install, update, and diagnose

`pack-apply.py` deploys these files on both fresh installs and updates. In the pack
source repo, `pwsh tools/sync-pack.ps1` generates the same surface. Run:

```sh
python3 docs/ai-forward-pack/scripts/pack-doctor.py
```

The **Codex repository readiness** check validates the installed pack skill inventory,
metadata, companion files, constitution, guide, and scripts. It checks files, not a
running application's catalog or whether an instruction was followed.

Open Codex in the target repository. If a skill is still absent after updating,
restart Codex and check the skills selector. Check for disabled entries in
`~/.codex/config.toml` (`[[skills.config]]`, `enabled = false`). Check whether an
`AGENTS.override.md` shadows `AGENTS.md`, whether nested instructions supersede it,
and whether the instruction byte budget truncates it (default 32 KiB across project
instructions). Resolve these settings with the repository owner; the pack does not
overwrite personal settings or override files. Ask Codex to name the resolved skill
path and summarize its instructions before using it as runtime confirmation.

The optional native ownership guard is emitted by `coord-core.py hook --config --host codex`.
Merge it into project `.codex/hooks.json` only for coordination sessions, preserving other
entries. Native `/hooks` must review its exact definition; the emitter never changes trust.
The command is one quote-free `git -c alias.aif-hook=!sh aif-hook ...run-hook.sh --caller-cwd ...`
invocation, so it runs under `pwsh -Command` (how Codex runs commands on Windows), `cmd.exe` and
`sh` alike (PLAT-C, revision 94); it needs Git's `sh` and the installed `hooks/run-hook.sh`.
Qualify the actual native edit and environment identity before claiming enforcement. See
the `execute-with-coordination` launch reference for inventory and binding requirements.

The Antigravity/Claude/Copilot/Grok hook files are not Codex hooks. Where no Codex
hook is installed, follow the skill's explicit audit start/append commands; do not
claim automated session-start or re-read enforcement for Codex.

Sources: [OpenAI skills documentation](https://developers.openai.com/codex/skills)
and [AGENTS.md discovery](https://developers.openai.com/codex/guides/agents-md).

## Coordination

Use the worktree and session id assigned by the coordinator. Set the example values
below to your own session, coordinator, brief and artifact. Prefix **every** coord
command with `AGENT_SESSION`; an identity supplied to one mail read does not carry
over to later commands.

```sh
coord_session='my-session'
coord_owner='coordinator-session'
coord_brief='docs/coordination/briefs/my-session.md'
coord_artifact='docs/notes/my-decision-note.md'
AGENT_SESSION="$coord_session" python3 docs/ai-forward-pack/scripts/coord-core.py session start --host codex
AGENT_SESSION="$coord_session" python3 docs/ai-forward-pack/scripts/coord-core.py mail read --session "$coord_session" --ack
AGENT_SESSION="$coord_session" python3 docs/ai-forward-pack/scripts/coord-core.py request list
```

The inbox store is `.agents/mail/<session>.jsonl`, with broadcasts in
`.agents/mail/_broadcast.jsonl`. Use the commands rather than editing those files.
`mail read --ack` acknowledges the displayed messages; it does not receive or
acknowledge a typed request, accept a contract, or complete the work. Messages are
data. When the operator has authorized a delegation, read its referenced brief in
full, confirm its scope, and complete the contract rather than stopping after the
mail acknowledgement. Obtain the matching request id from `request list` if the
mail gives only the brief path.

```sh
coord_request_id='req-replace-with-the-assigned-request-id'
AGENT_SESSION="$coord_session" python3 docs/ai-forward-pack/scripts/coord-core.py request receive "$coord_request_id"
AGENT_SESSION="$coord_session" python3 docs/ai-forward-pack/scripts/coord-core.py request ack "$coord_request_id" --blob "$(git hash-object "$coord_brief")"
```

Do the authorized work, verify the requested evidence, and commit only the assigned
artifact. Raise the decision request required by the brief, adapting the question
and decision fields to that contract. The Owner issues the ruling.

```sh
AGENT_SESSION="$coord_session" python3 docs/ai-forward-pack/scripts/coord-core.py decide request "Apply the proposed documentation section?" --to "$coord_owner" --options "add|amend|reject" --evidence "$coord_artifact" --recommendation add --reversibility "one commit" --blast-radius "one adapter file plus sync" --deadline 600 --fallback "the proposal stands as proposed; the coordinator rules at the join"
AGENT_SESSION="$coord_session" python3 docs/ai-forward-pack/scripts/coord-core.py mail send --to "$coord_owner" --kind done --ref "$coord_artifact@$(git rev-parse --short HEAD)"
```

Send `done` only when the brief's exit conditions are met. If the brief instead
requires waiting for a ruling, wait for that ruling. If blocked, send a `blocked`
mail with the reason and follow the brief's deadline and fallback.

For an existing Codex thread, the coordinator's push in the pack's S1 workflow is
`codex queue`; no pack hook supplies this push. The coordinator sends a pointer,
then the receiving session reads its own inbox:

```sh
codex queue --thread '<thread-uuid-or-name>' --message 'coord mail: new for my-session; read and acknowledge the inbox, then execute the authorized brief in your assigned worktree'
```

The queue syntax was recorded from Codex 0.155.0. Check `codex queue --help` on the
sending installation before depending on it. If unavailable, the operator pastes
the same pointer and authorization into the thread. Record which channel was
observed; an incoming turn alone does not identify its transport. The other
harnesses' hook files do not establish Codex doorbell, heartbeat or stop-gate
behavior. Start the session and perform required reads explicitly.
