# Hooks — the controls that run at the tool seam

## Supported platforms and safe refresh

AI-Forward and consuming repositories support **Copilot CLI, Claude Code, Codex CLI,
Grok Build and Antigravity (`agy`) on Windows and macOS**. Hook prerequisites are
**Git and compatible Python (3.10+) or uv**; no Node dependency is introduced.
Startup entries for Claude, Grok and Copilot first locate the installed launcher from
a plain child directory, then run it through Git's shell. Their fixed, single-quoted
resolver is read by sh or PowerShell; Copilot retains separate platform fields.
Other default events and native ownership retain the existing quote-free launcher.
Antigravity retains its measured Git root/`.agents` command contract, including its
Windows cmd.exe quote restriction; arbitrary plain child cwd is not qualified there.
Codex ownership keeps the caller directory through `--caller-cwd`.
Skill discovery and each host's native trust/approval requirements are unchanged.
Codex lifecycle hooks remain opt-in; support does not imply identical native events.

Source sync and downstream `pack-apply` now share the same Claude settings merger.
It replaces only complete known shipped commands, not commands merely mentioning a
pack path. Custom wrappers, extra handlers, matchers, timeouts, permissions and existing
ownership opt-ins survive. Malformed nested settings and symlink targets are refused
without overwrite. A valid UTF-8 BOM is removed during the authorized refresh, and
repeated refreshes are idempotent. New installs never enable ownership implicitly.
Existing Codex hook files, Grok ownership files and Agy ownership bundles receive the
same exact-command migration; absent opt-ins stay absent. Re-review changed native
hook definitions in the harness when it requests approval.

The optional `pack-doctor.py` diagnoses invalid configuration encoding and old
unsupported commands, then executes a fixed benign hook-help probe through the
launcher. In serial use, absent optional coordination is not an installation failure;
explicitly activated but missing or invalid coordination is still diagnosed. On a
uv-only setup, use `uv run --no-config --no-project --python ">=3.10"` to invoke it.
When native Python is unavailable, uv must be on the hook process's PATH. A caller
may explicitly invoke a uv executable by path for diagnostics; setting `UV_PATH`
does not make this persisted launcher discover it. A successful probe verifies
execution on this machine, not native event or authorization enforcement. Update the JSON and scripts together; a JSON-only copy
can point at a launcher that is not installed.

Native ownership is an explicit project opt-in. `coord hook --config --host
claude|codex|copilot|grok|agy` emits a reviewable entry; it changes no settings or trust.
Once enabled, a session working in a linked worktree that writes into the primary checkout is
denied by name (`COORD-PRIMARY-WRITE`, PRIM-A) on every host; in-place work in the primary is not.
Grok can keep it in a separate `.grok/hooks/coord-ownership.json`. Agy uses a local
`ownership-guard` named section; both sync and installer refresh source-managed names
while preserving project-owned names, and reject malformed current JSON without overwrite.
Fresh pack installations do not enable ownership guards automatically.
The Codex entry is the quote-free launcher form (PLAT-C), because Codex on Windows runs hook
commands through `pwsh -Command` (measured 2026-09-24, codex 0.156): `git -c alias.aif-hook=!sh
aif-hook docs/ai-forward-pack/hooks/run-hook.sh --caller-cwd ../scripts/coord-core.py hook --host
codex`. `--caller-cwd` keeps the directory Codex called from, which Codex patch paths are relative to.

The separate Codex Stop entry requires its own native review; approving apply_patch
does not approve Stop. Stop facts in the existing coordination log record the script's
requested refusal, allowed closure or bounded loop guard, with no request bodies.
They do not alone prove the host honored that response. Grok1.0.34 advertises blocking
Stop metadata although its current web documentation calls Stop passive; qualify the
actual host. Regardless of native continuation limits, the runner refuses final readiness
for open or unreadable worker Owner decisions after verifying independent work receipts.

Agy ownership success returns no permission override. Its `TargetFile` must be absolute:
the observed hook cwd is the workspace's `.agents` directory. Relative targets fail closed.
The native file-edit mode and the ownership guard are separate policy layers.

Prose that says "check whether you already have it" is a memoir (`continuous-improvement.md` CI6). The
hooks here are the same rule as a **control**: a host runs them at a fixed lifecycle point regardless
of what the model decides.

**One process per host event (B15, revision 101).** Every surface registers ONE command per event,
`run-hook.sh dispatch.py --host <host> --event <event>`. `dispatch.py` runs that event's hooks, in the
order the table in its `_PLAN` names, inside one Python process, on the one stdin payload. Each hook keeps
its contract: exit 2 from any hook (a blocking guard) is the dispatcher's exit 2, with the hook's reason
on stderr; a hook that raises is fail-open; JSON objects from two hooks merge into one object. Measured on
Windows (2026-10-14): one hook launch cost ~500 ms (git, sh, a `python3` Store-alias probe of ~200 ms,
the interpreter probe, then the hook), and pack-on Grok cells ran four per tool call - a median 11 s per
read against ~0 pack-off. Byte-code caches go to the OS temp dir, never into the working tree. A refresh
retires the per-hook commands and Antigravity sections revision 100 shipped (`pack-apply.py`
`RETIRED_HOOK_SCRIPTS`, `RETIRED_AGY_SECTIONS`); custom wrappers and project-owned sections survive.

**Coordination off means the coordination hooks cost nothing.** The doorbell, heartbeat and owner review
gate return at once unless a session identity resolves AND the repository's coordination record holds
something - a ledger `.agents/log/<session>.jsonl`, an inbox `.agents/mail/*.jsonl` or `.agents/requests.jsonl`
(`coord-core.py coordination_active`). The `.agents/log/` directory alone is not enough: the audit log's
local fallback (AL0.2) creates `.agents/log/audit/` in repositories that never coordinate.

| File | Host | Deploys to | Purpose |
|---|---|---|---|
| `dispatch.py` | Claude Code, Copilot CLI, Grok Build, Antigravity | `docs/ai-forward-pack/hooks/dispatch.py` | The per-event dispatcher (B15): every pack hook command in the four configs below is `dispatch.py --host <host> --event <event>`. `ROUTES` is the one table of which hooks run on which host event. Unknown host or event: exit 0, nothing run. |
| `reread-guard.py` | Claude Code, Copilot CLI, Grok Build, Antigravity | `docs/ai-forward-pack/hooks/reread-guard.py` | Counts identical reads per turn; on the third, and on any paged tool output viewed whole, adds a warning to the model's context. Warns, never blocks (a real third read exists). Fail-open on every error path. `--host claude\|copilot\|grok\|agy`. |
| `copilot.ai-forward-hooks.json` | Copilot CLI | `.github/hooks/ai-forward.json` | Copilot's hook config (`version: 1`, camelCase events, `bash`/`powershell` per platform, `timeoutSec`). Loaded from the repo automatically. Personal alternative: `~/.copilot/hooks/`. |
| `grok.ai-forward-hooks.json` | Grok Build | `.grok/hooks/ai-forward.json` | Grok's hook config (PascalCase events, one dispatcher command per event, unmatched; the re-read guard counts only `read_file`/`Read`). Project hooks require folder trust (`/hooks-trust` or `--trust`). |
| `agy.ai-forward-hooks.json` | Antigravity (`agy`) | `.agents/hooks.json` | Antigravity hook config: one named section, `ai-forward`, with one dispatcher command per event — `PreToolUse` (matcher `view_file`: the re-read guard), `PreInvocation` (session-start, then the doorbell), `PostToolUse` under a `matcher: ""` + `hooks` wrapper (heartbeat), and `Stop` (heartbeat, then the owner review gate). Revisions up to 100 shipped five per-hook sections (`reread-guard`, `session-start`, `mail-doorbell`, `heartbeat`, `owner-review-gate`); a refresh removes them. Shape per event follows the host's docs: tool events need the wrapper, lifecycle events take handlers directly. **Correction to revision 83:** `enabled` defaults to true, so the missing flag was not why nothing fired in the S1 session; the causes were the bare `PostToolUse` handler (loaded, never fired), the doorbell's string `injectSteps` (rejected by protojson), and a session that hung inside its ack command before any `Stop`. Headless probes on 2026-09-20 showed `PreInvocation` (run-start marker) and `Stop` (heartbeat rows) firing once the folder was a registered project (`--new-project` or `--add-dir`); a plain `agy -p` in an unregistered folder loaded only the user-level file. |
| `run-hook.sh` | Antigravity (`agy`), Codex | `docs/ai-forward-pack/hooks/run-hook.sh` | The Antigravity hook launcher (PLAT-C). Antigravity runs hook commands through **cmd.exe on Windows**, from `<repo>/.agents`, and its Go launcher escapes an inner `"` as `\"` (measured 2026-09-24, agy 1.2.10). So every agy command is one quote-free `git` invocation that cmd.exe and sh parse alike: `git -c alias.aif-hook=!sh aif-hook docs/ai-forward-pack/hooks/run-hook.sh <hook>.py --host agy ...`. git runs the `!sh` alias through its own sh from the top of the working tree, passing stdin and the exit status through; this script resolves `python3`, falling back to `python`, and execs the named hook. Before this, every agy hook failed on Windows, and agy marked each tool step `ERROR` even when the tool succeeded. Verified live on Windows: tool steps `DONE`, `heartbeat` and `owner-review-stop` rows written. **Codex** (PLAT-C, revision 94): Codex on Windows runs hook commands through `pwsh -Command` (codex 0.156), so its ownership entry uses the same launcher with `--caller-cwd`: the launcher `cd`s back to `$GIT_PREFIX` (the directory git was called from; Codex patch paths are relative to it) and names the hook relative to that directory, because Git for Windows' sh hands `python.exe` an absolute `/c/...` argument holding `'`, a backtick or `;` unconverted. Tested under `pwsh -Command`, `cmd /c` and `sh -c` from a two-level subdirectory of a checkout whose path holds shell metacharacters; not yet observed in a live Codex session. |
| `coord_identity.py` | Claude Code, Copilot CLI, Grok Build, Antigravity | `docs/ai-forward-pack/hooks/coord_identity.py` | Shared identity validator for the hook layer: parent `$AGENT_SESSION` must match `[A-Za-z0-9_.-]` in the bounded hook length, and Copilot child events derive one canonical id from the native session/agent pair only when both parts are valid and bounded. Invalid ids are refused silently with no writes; ids are never truncated into collisions. |
| `session-start.py` | Claude Code, Copilot CLI, Grok Build, Antigravity | `docs/ai-forward-pack/hooks/session-start.py` | Runs on session start (`SessionStart`/`SubagentStart`, Copilot `sessionStart`, or `PreInvocation` invocationNum 1): records the audit start marker at the session seam (`audit-log.py start`, keyed to `$AGENT_SESSION` when the harness environment carries it, else a harness slot an `append` uses only when it has no marker of its own — `duration_source: session-start-hook`). The marker store is the checkout's git dir (`<git-dir>/ai-forward/run-starts.json`, B15), so the hook leaves the working tree unchanged; outside git it stays beside the audit log. Copilot child events never reuse the parent's `$AGENT_SESSION`; if a stable native child id is absent, Copilot `subagentStart` is a no-op rather than guessing from `agentName`. Then runs the repo-declared checks in `.agents/session-checks.json` (`{"checks": [{"name", "argv", "remedy"}]}`, `{python}` = this interpreter; at most 5 in 10 s) and reports each failure with its remedy (Claude: stdout, the model's context; other hosts: stderr) — only a declaration approved on this machine runs (FR-098, below) — e.g. a mutation tool's `--check-clean` (MUT-A). Otherwise prints nothing; exits 0 on every path. Closes DC-190. |
| `mail-doorbell.py` | Claude Code, Copilot CLI, Grok Build, Antigravity | `docs/ai-forward-pack/hooks/mail-doorbell.py` | The message layer's doorbell (design-message-layer section 8): folds the caller's inbox (`$AGENT_SESSION`) and, when unacknowledged mail newer than five minutes exists, emits a **count and a pointer, never a body** in the host's shape - Claude/Grok `additionalContext` at `PreToolUse`/`UserPromptSubmit`, Antigravity `injectSteps` at `PreInvocation`, Copilot `additionalContext` at `preToolUse` and `decision: block` + `reason` at `agentStop`/`subagentStop` (only when count > 0 and `stop_hook_active` is unset). Copilot child events derive the same canonical child identity as the other hooks instead of reusing the parent's `$AGENT_SESSION`. When Copilot also loads `.claude/settings.json`, the Claude-form invocation is skipped to avoid duplicate doorbells. The inbox is truth; the doorbell is a hint. Exits 0 and prints nothing on every other path. Per-harness execution status: `.agents/harness-status.json` (`verified` only when executed on this machine). |
| `owner-review-gate.py` | Claude Code, Grok Build, Copilot CLI (Antigravity: unsupported) | `docs/ai-forward-pack/hooks/owner-review-gate.py` | The Owner review gate (design-owner-review, D6): at the **stop seam** it resolves the session to `$AGENT_SESSION`, reads P1's request store through P1's reader, and refuses the stop when that session still holds an unresolved **decision request it sent** — Claude/Grok `Stop`/`SubagentStop`: **exit 2** with one reason line on stderr (count + ids, never a body); Copilot `agentStop`/`subagentStop`: `{"decision":"block","reason":…}` with exit 0. Copilot child events derive the same canonical child identity as the other hooks instead of reusing the parent's `$AGENT_SESSION`; duplicate Claude-form invocations are skipped when Copilot also loads `.claude/settings.json`. Exits 0 and prints nothing on every path it cannot evaluate (no `AGENT_SESSION`, non-JSON stdin, `stop_hook_active`, no store, malformed store, `COORD_ROOT` outside the repo). Per-host status below. |
| `heartbeat.py` | Claude Code, Copilot CLI, Grok Build, Antigravity | `docs/ai-forward-pack/hooks/heartbeat.py` | The progress heartbeat (spec-liveness-and-track, D7): on every tool call (`PostToolUse` on Claude Code and Antigravity, Claude-format `PreToolUse` on Grok, Copilot `postToolUse`) it adds one call and any file a write-class tool named to a machine-local accumulator in the git common dir; at most once per 100 s (D13's renew cadence) - or on the stop-class event (`Stop` / `agentStop` / `subagentStop`) - it writes ONE `kind: heartbeat` row with the deltas (calls, distinct files, tokens `not recorded`) into `$AGENT_SESSION`'s ledger via `coord-core.py heartbeat_tick`. A zero-delta row on Stop is deliberate: `coord track` renders it `stalled`, never `live`. A beat from the session holding `refs/coord/leader` renews the designation (F-1). Copilot child events derive the same canonical child identity as the other hooks instead of reusing the parent's `$AGENT_SESSION`; duplicate Claude-form invocations are skipped when Copilot also loads `.claude/settings.json`. Counts, never paths; prints nothing; exits 0 on every path. Claude Code executed here against the documented contract; Grok, Antigravity and Copilot are **observed-only** until a live session shows the event fire (CO12). |
| `git-identity-guard.py` | Claude Code, Copilot CLI, Grok Build (Antigravity: unsupported) | `docs/ai-forward-pack/hooks/git-identity-guard.py` | PK-07's mechanical control: at `PreToolUse`, unmatched (it does not filter by tool name — those differ per host and are not all verified), it inspects whatever command-like field the call carries (`command`/`cmd`/`script`/`shellCommand`/`commandLine`) and refuses only a git-identity-**SET** invocation (`git config [--global\|--local\|--system] user.name/email <value>`, or an inline `-c user.name=...`/`-c user.email=...`) — never a bare read (`git config user.name` alone). Claude/Grok: exit 2 + reason on stderr blocks the call. Copilot: exit 2 denies on `preToolUse` (reread-guard.py's own documented contract). Antigravity has a `PreToolUse` event (reread-guard's `view_file` matcher proves it fires) but this pack has never observed its PreToolUse **deny** shape (only `allow` is verified) — left unwired rather than guessed; PK-07 stays prose-only for agy (AL0.2). |
| `claude-code.settings.hooks.json` | Claude Code | merge into `.claude/settings.json` | The `hooks` object: one unmatched dispatcher command each for `PreToolUse` (re-read guard, doorbell, git-identity guard), `UserPromptSubmit`, `SessionStart`, `SubagentStart`, `PostToolUse` (heartbeat), `Stop` and `SubagentStop`. Committed project settings run in sub-agents too. |

`.agents/session-checks.json` is a project-owned, portable declaration: installation neither creates
nor overwrites it. Startup discovery chooses the nearest declaration from the session directory up
to the Git root or the installed project root identified by `docs/ai-forward-pack/hooks/`, including
plain projects without `.git`; it never walks past that installed root to execute parent commands.
Declared argv runs with the existing same-user project trust, not in a sandbox or as a blocking gate.
**It runs only once approved on this machine (FR-098).** A cloned declaration is committed config
that runs commands, so a new or changed file is reported `NOT RUN` with the command that approves it:
`python docs/ai-forward-pack/hooks/session-start.py --approve-checks <path>` prints every argv and
records the file's sha256 in `$XDG_CONFIG_HOME/ai-forward/session-checks-approved.json` (default
`~/.config`), outside every repository. Any change to the bytes needs a new approval.

**Stop-class events per host (the owner review gate).** A host is `enforced` only once a live session
has shown the hook refuse a stop here (CO12); until then it is `observed-only`, and a host with no
stop event is `unsupported`.

| Host | Event | Block form | Status (2026-09-19) |
|---|---|---|---|
| Claude Code | `Stop`, `SubagentStop` | exit 2, reason on stderr | **enforced** — 2026-09-19, Claude Code 2.1.278: a headless `claude -p` session in a linked worktree with an open decision request had its stop refused; the model reported the refusal and touched nothing (Ruling 2 in `docs/notes/rulings.md`) |
| Grok Build | `Stop` (Claude-format hooks) | exit 2, reason on stderr | observed-only — the S1 Grok session reported "refused stop: not seen" while its decision request was open |
| Copilot CLI | `agentStop`, `subagentStop` | exit 0 + `{"decision":"block","reason"}` | observed-only |
| Antigravity | `Stop` (handlers directly under the key; payload `executionNum`, `terminationReason`, `fullyIdle`) | exit 0 + `{"decision":"continue"}` on stdout, at most twice per stop sequence, reason on stderr | **enforced** — 2026-09-20, 1.2.7: a headless session with an open decision request reported "Termination was blocked … has an unresolved decision request that must be ruled or expired before stopping" twice, then stopped (Ruling 5) |

**Codex push.** `codex queue --thread <id> --message <pointer>` **verified** — 2026-09-20, codex-cli 0.155.1: two pointers queued by the coordinator arrived in the operator's Codex thread as user-role turns and are quoted verbatim in `note-20260920-s1-codex-coordination-surface`; the first ("run mail read --ack") produced exactly that and a stop, the second ("then execute the brief") completed the track — the pointer must name the action after the read.

**Heartbeat per host.** Claude Code **enforced** — 2026-09-19, 2.1.278: host-fired `PostToolUse` and `Stop` rows in the session ledger, the first Stop carrying `calls: 3` for a three-call turn, in an interactive session and again headless. Grok Build **enforced** — 2026-09-20, Grok Build 1.0.34: a host-fired `PreToolUse` heartbeat row with `"host": "grok"` in `.agents/log/s1-grok.jsonl` during the S1 three-harness test (`note-20260920-s1-grok-hooks-surface-freshness`). Antigravity **enforced** — 2026-09-20, 1.2.7: headless probes wrote `Stop` rows with `calls 2` once the `PostToolUse` handler took the documented `matcher`/`hooks` form (the S1 session's rows were absent because the handler was in the direct form). Copilot CLI stays observed-only. `coord mail dispatch --harness claude-code` **verified** the same day (`.agents/harness-status.json`, machine-local).

**Contracts these are written to** (established from the hosts' own documentation and a captured event
stream, not assumed — class RIG-D): Claude Code hooks receive `{"hook_event_name","session_id",
"tool_name","tool_input"}` on stdin, and exit 0 with `hookSpecificOutput.systemMessage` to warn (exit 2
would block). Copilot CLI hooks receive `{"sessionId","toolName","toolArgs"}` and return
`{"additionalContext": …}`; on `preToolUse` any non-zero exit other than 2 **denies** the call, so the
guard exits 0 on every path including its own failures.

**Interpreter (Windows and macOS).** Every hook command resolves the interpreter **at run time**:
`py=$($first -c 'import sys;print(sys.executable)' 2>/dev/null); [ -x "$py" ] || py=$($second -c …); "$py" <script>`.
`python3` is asked first where it is real Python (Linux, macOS). On Windows (`OS=Windows_NT`) `python` is
asked first: python.org ships no `python3.exe`, and the `python3` on PATH is a Store alias that exits 9009
without printing a path after ~200 ms (measured 2026-10-14) - a cost paid on every hook call. Nothing machine-specific is
written into the tracked config (class PLAT-B), and no one edits the command per machine (class
PLAT-A). Cost: one extra interpreter start (~25 ms) per hook. That form needs a POSIX `sh`, so only the
Grok Build commands carry it inline. The Claude Code, Copilot CLI (both arms) and Antigravity commands,
and the ownership entries `coord-core.py hook --config` emits for Claude, Codex, Copilot and Antigravity,
are one quote-free launcher invocation, `git -c alias.aif-hook=!sh aif-hook docs/ai-forward-pack/hooks/run-hook.sh <hook>.py ...`,
which `run-hook.sh` resolves as above. The reason is that Antigravity runs hooks through cmd.exe and
Copilot through PowerShell on Windows, and Copilot also reads `.claude/settings.json` (PLAT-A, revision 95).
Git runs the alias through its own `sh` from the top of the tree. Only a compatible (3.10+) interpreter is
accepted; when neither `python3` nor `python` qualifies, `run-hook.sh` falls back to an isolated
uv-managed runtime. The bootstrap does not add Python to PATH or grant host permissions. A file-level
setup pass therefore does not prove native event execution; verify each host's trust
and command approval separately.

**Plain-project startup.** Claude, Grok and Copilot startup entries contain one fixed
upward search for `docs/ai-forward-pack/hooks/run-hook.sh`, bounded to 64 ancestors and
stopping at a Git boundary, ahead of the startup dispatcher. The resolver operates on relative paths, passes the JSON
payload untouched on stdin and delegates interpreter selection to the installed
launcher. It is not a payload-controlled `eval`, a copied child-directory shim or a
new global dependency. A direct `session-start.py` subprocess and a configured hook
command are different tests; qualify the exact persisted command under its native
shell. Existing Antigravity cmd.exe and Codex caller-cwd commands are unchanged.

Duration markers remain keyed to the payload directory. Only regular known
`.run-starts.json` and `.run-starts.json.tmp` files under `docs/audit/` or
`.agents/log/audit/` inside the project are ephemeral checkpoint bookkeeping, including
child directories. Durable records, marker-shaped symlinks, explicit registered inputs
and product edits are not exempted. Startup-check messages are advisory: the agent
must assess whether a reported failure prevents this outcome rather than converting
all warnings into a new customer gate.

Measured origin: the profiled TheTerrace session viewed `public.html` four times in three minutes,
a 43 KB paged output whole twice, and one sub-agent read the same mockup six times — none of it errored.
