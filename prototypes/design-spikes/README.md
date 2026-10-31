# Design spikes for the Phase 1 Claude Code slice

Spikes named in `docs/design/claude-code-skeleton.md`. These were run in the Linux cloud container on
2026-10-31: Rust 1.97.0, keyring 4.2.0, Claude Code 2.1.316. Everything below was measured unless it
is marked otherwise.

## S2: Linux keychain with no Secret Service (`s2-keyring/`)

`cargo run` inside `s2-keyring/` calls `keyring::Entry` with the v1 API (the default feature). It
reports the store status, then sets, gets and deletes a 32-byte secret.

| Case | How it was run | Result | Time |
|---|---|---|---|
| A. No session bus (headless, CI) | `env -u DBUS_SESSION_BUS_ADDRESS` | `store_status` = `PlatformFailure(Zbus(Connection … NotFound))`; `Entry::new` = `NoDefaultStore` | 1 ms |
| B. Session bus, no Secret Service | `dbus-run-session` | `ServiceUnknown: org.freedesktop.secrets`; `Entry::new` = `NoDefaultStore` | 33 ms |
| C. gnome-keyring, unlocked | `run_c.sh` | set / get (32 bytes) / delete all `Ok` | 73 ms |
| E. gnome-keyring started but locked, no collection | `run_e.sh` | `store_status` Ok; `set_secret` = `NoStorageAccess(NoResult)`; get = `NoEntry` | 90 ms |

**What this shows:** the crate never hung and never panicked. Each failure came back fast as a
distinct error. That fits the design, where the core refuses to start with `AIQ-STORE-KEY`, the hooks
spool, and `aiq status` explains why. There is no unencrypted fallback.

- On a desktop session, a locked keyring will probably show the desktop's unlock prompt. That is
  *Inferred*; it was not tested here because the container has no GUI.
- The key is only read by the core, never by the hook, so a prompt like that cannot slow Claude Code.
- CI uses the test-only `TestKey`.
- The kernel keyutils store, an optional crate feature, is rejected. Its keys do not survive a
  reboot, which would mean losing all history.

## S4: Claude Code hook payloads (`s4-claude-hooks/`)

The probe ran `claude -p "Reply with the single word: ok" --settings settings.json --output-format
stream-json --verbose`. It used an async `UserPromptSubmit` hook and a sync `Stop` hook, and each
dumped its stdin to a file.

- **`UserPromptSubmit` keys:** `cwd`, `hook_event_name`, `permission_mode`, `prompt`, `prompt_id`,
  `session_id`, `transcript_path`.
- **`Stop` keys:** `background_tasks`, `cwd`, `effort`, `hook_event_name`, `last_assistant_message`,
  `permission_mode`, `prompt_id`, `session_crons`, `session_id`, `stop_hook_active` (false),
  `transcript_path`.
- **`prompt_id` is the same on both hooks of one turn.** The core can therefore tell whether the
  turn's prompt event has arrived yet.
- **`Stop` carries `last_assistant_message`,** which is the agent's reply text. The design does not
  store it: Phase 1 keeps only `stop_hook_active` and `prompt_id`.
- **A `Stop` hook that prints nothing** gives `hook_response` with `output: ""`, `exit_code: 0` and
  `outcome: success`. Nothing is added to the stream. The interactive terminal was not checked for
  the empty case; the laptop test checked only the case with text.
- **Nested `claude -p` shares the parent session.** The nested run inherited the parent session's
  `session_id` through the environment (`CLAUDE_CODE_SESSION_ID` and `CLAUDE_CODE_CHILD_SESSION` are
  set). This was observed in this cloud environment. Whether a local nested `claude -p` does the same
  is *Inferred* and still open. Either way, an agent that runs `claude -p` can add prompts to the
  person's session.
- **Not tested here:** whether Copilot CLI also runs hooks from the user-level
  `~/.claude/settings.json`. Copilot CLI is not installed in this container, so this moves to the
  Copilot CLI revisit on Imran's laptop.
