# Muse and the AI-Forward Pack

Muse Code (qualified here: 1.4.7, Windows 11, 2026-10-13) reads the repository's
`AGENTS.md` as project rules. **AGENTS.md takes precedence**: when both files exist
Muse ignores `CLAUDE.md` in that directory (observed at startup:
"rules file at CLAUDE.md is ignored this session because AGENTS.md takes
precedence"). Therefore the Muse pointer lives in the `AGENTS.md` managed block,
never only in `CLAUDE.md`. This file is only the **Muse path map** so you do not
follow Claude-only or Copilot-only locations. Do not paste knowledge docs here —
that would attach them on every turn (defect class CTX-B).

## Skills

Project skills live in `.agents/skills/<name>/SKILL.md` — the surface shared
with Codex and Antigravity, generated from `pack/commands/` by
`tools/sync-pack.ps1` (source repo) or `pack-apply.py` (consuming repo). Muse
discovers them as project skills: `muse skills list --workspace <repo>
--trust-workspace` reports all 29 pack skills at `.agents/skills/`, and
`muse skills validate .agents/skills/<name>` reports the pack's `SKILL.md`
shape `compatible` (profile `agent-skills-common-subset`; the pack's extra
`runs_as:` frontmatter field is tolerated as an unknown field).

Invoke a skill by loading it: `read_skill` with the skill id, or name it in the
request ("run specify on …"). The pack's `/name` notation denotes that skill;
it is not a registered Muse slash command. When `/name` reaches the model as
text, resolve the named pack skill and read its `SKILL.md`. Read stage files
from `reference/` beside the selected skill at the stage, never by re-invoking
the skill. Local extra skills under `.agents/skills/` are allowed and do not
substitute for a missing pack skill.

## Ground in the constitution

Read the foundation at the start of a substantive task, unless already in
context:

- `.claude/knowledge/agent-body-of-knowledge.md`
- `.claude/knowledge/agent-rules-of-the-road.md`
- `.claude/knowledge/agent-persona-catalog.md`
- `.claude/knowledge/layered-optimized-architecture.md`
- `.claude/knowledge/engineering-governance.md`

Then read the relevant standards named by `AGENTS.md` and the selected skill.
Resolve a skill's `knowledge/<name>.md` and any citation of
`.github/instructions/<name>.instructions.md` to `.claude/knowledge/<name>.md`
from the repository root. These are shared knowledge files, not Claude-only
instructions. Knowledge load scopes still govern: the `load: always` set is
what the `AGENTS.md` managed block summarises; `load: skill`, `load:
reference`, and `load: glob` docs are read on demand with `read_file` at the
stage that names them. A path reference is an instruction to read the file,
not evidence that its contents are in context.

## Personas & subagents

Persona definitions live in `.claude/agents/<name>.md` (23 lenses). Muse has
no project agent-definition directory equivalent to `.claude/agents/`; its
registered subagent types are the host's own (this qualification observed
`general-purpose` and `workflow-subagent`). Convene the council two ways:

1. **Spawned lens (preferred when delegation is warranted):** spawn a
   `general-purpose` subagent whose objective carries the persona — read the
   persona file, paste its mandate, mode (Peer to author, Adversary to
   review), knowledge lens, and output contract into the spawn objective, and
   name the persona in the task label. The persona card is self-sufficient:
   do not re-read `AGENTS.md` or the catalog to orient (class CTX-G).
2. **Inline turn (default for standard turns):** enact the peer/adversary
   perspectives inline with explicit PASS/BLOCK ratings.

Author in Peer Mode, review in Adversary Mode; the author never clears its
own hard veto. Muse also accepts an ephemeral `--agents <JSON>` overlay at
launch (`muse --help`); its definition schema is **observed-only** — it is
not the pack's persona mechanism until a live spawn through it is qualified.

## Hooks & coordination identity

Muse exposes no lifecycle-hook configuration in its CLI surface
(`muse --help`, `muse skills --help`, `muse mcp --help` inspected
2026-10-13): there is no Muse equivalent of `.claude/settings.json` hooks,
`.github/hooks/`, `.grok/hooks/`, or `.agents/hooks.json`, and the pack's hook
scripts accept `--host claude|copilot|grok|agy|codex` — no `muse`
host. Per the pack's rule (support does not imply identical events; unwired
beats guessed, PK-07), every hook control is **unsupported** natively on Muse
and is discharged by an explicit in-skill step instead:

| Control | Muse status | Manual step |
|---|---|---|
| session-start audit marker (AL4a) | unsupported | At a skill's grounding, run its `audit-log.py start` step as the skill directs; the closing entry's duration is then measured, not modelled. |
| re-read guard (CTX-D) | unsupported | Do not re-read a file already in context; the surface rule is in `CLAUDE.md`'s addendum for Claude Code — apply the same discipline here. |
| mail doorbell (CO-S2) | unsupported | Before dispatch and at stop, check the inbox explicitly (`coord mail` / `coord board` as `agent-coordination.md` directs). A stop is a message (CO-S2). |
| heartbeat / liveness | unsupported | Progress is reported in the turn (todo list), not to the coordination ledger; do not fabricate heartbeat rows. |
| owner-review Stop gate | unsupported | Before ending a turn, confirm no decision request you sent is unresolved; if one is, say so and name its fallback instead of stopping silently. |
| git-identity guard (PK-07) | unsupported | Never set `git config user.name/user.email`; the rule stands as prose on this host. |

Coordination identity: use the current Muse session id (named in the
session's own context and log path) wherever pack scripts ask for a session
(`--session <id>`). Do not invent an `AGENT_SESSION` value; where a script
requires that environment variable and none is set, record the capability as
not recorded rather than guessing an identity. Cross-session messaging, when
needed, is the host's own `muse session-message` surface, separate from the
pack's `coord mail` store; do not confuse the two.

Worktree discipline (WT1–WT12) is tool-agnostic and applies unchanged; Muse
additionally offers `-w/--worktree` at launch, which is a host convenience,
not a substitute for the WT1 concurrent-writer test.

## Scripts

Follow script paths under `docs/ai-forward-pack/scripts/`. On Windows use
`python` or `py -3`; on Linux/macOS use `python3`. `pack-doctor.py`'s
`python interpreter` check names the working form for the current machine.
Shell commands run through PowerShell on Windows: a gate's exit status is
never read behind a pipe (CT27).

## MCP

`muse mcp` logs in to a streamable-HTTP entry under `mcpServers` in the
host's `settings.json`. The repo-root `.mcp.json` is the Claude/Copilot form;
do not assume Muse reads it — configure servers in Muse's own settings
surface. Never copy credentials into the repo to make an MCP server resolve.

## Install, update, and diagnose

`pack-apply.py` deploys this surface on fresh installs and updates (it writes
`docs/ai-forward-pack/muse-surface.md` and keeps the shared `.agents/skills/`
tree); in the pack source repo, `pwsh tools/sync-pack.ps1` generates the same
surface. Run:

```sh
python3 docs/ai-forward-pack/scripts/pack-doctor.py
```

The **Muse repository readiness** check validates the installed pack skill
inventory (via the shared `codex-skills.json` manifest), companion files,
constitution, this guide, and scripts. It checks files, not a running Muse
catalog or whether an instruction was followed. If a skill is absent in a
live session after updating, restart Muse, confirm workspace trust
(`--trust-workspace`), and ask Muse to name the resolved skill path
(`muse skills inspect <name>`) before treating it as runtime confirmation.

## Context budget (measured 2026-10-17, Muse 1.4.10)

Muse assembles a different static prefix from Copilot, so Copilot's
`prefix_tokens` baseline does not describe it. Measured components: the
skills catalog is capped at 32 KB (32768 bytes). A 52-skill load (30 pack +
21 bundled + 1 plugin) costs 27215 startup bytes, 12963 of them the pack's,
so nothing is compacted; the 30 `.claude/skills` duplicates are shadowed by
`.agents/skills`. Muse 1.4.7 compacted 11 descriptions at the same count, so
re-check after a Muse upgrade. `AGENTS.md` in this repo is 29570 bytes (the
managed block is most of it, including the Muse pointer above). The ratcheted record is `muse_profile` in
`pack/context-budget.json`; re-measure with `muse skills list
--trust-workspace --json` (per-skill `context_cost`) after any catalog or
block change rather than reusing these numbers.

## Qualification (CO12 — status per capability, 2026-10-13)

| Capability | Status | Evidence |
|---|---|---|
| `AGENTS.md` loaded as project rules | enforced | Live session + `muse init --dry-run` wording; CLAUDE.md-shadow warning observed in `muse exec`. |
| Skill discovery (30/30 pack skills) | enforced | `muse skills list --json` (1.4.10, 2026-10-17): 30 project skills, all `.agents/skills/`; live session catalog. |
| Skill package shape | enforced | `muse skills validate .agents/skills/specify`: valid, `compatible`. |
| Skill invocation by name/description | enforced | Live session: project skills loaded via `read_skill` (implement, extendaibundle). |
| Knowledge path resolution | observed-only | Files present and read via `read_file` this session; no host loader exists to enforce scoping. |
| Persona council | observed-only | Spawn tool accepts persona-carrying objectives; no native persona registry. `--agents` overlay: help-verified, schema unqualified. |
| Lifecycle hooks (all six controls) | unsupported | No hook surface in CLI help; manual steps above. |
| MCP via repo `.mcp.json` | unsupported | `muse mcp` targets `mcpServers` in host settings; repo file not read (Inferred from help text). |
| Coordination ledgers (`coord-*`) | observed-only | Scripts run under `python` on Windows; no doorbell/heartbeat fires without hooks. |
