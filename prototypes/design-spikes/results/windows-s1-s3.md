# Design spikes S1, S3, S4b — Windows results

Run on 2026-10-31, on one Windows 11 Enterprise laptop (10.0.26300), Entra-joined user account.
All numbers below were measured on this run. Lines marked **Inferred** are reasoning, not measurement.
User name and SID are redacted to `<user>` / `<sid>`.

## Summary

| Spike | Question | Result |
|---|---|---|
| 0 Toolchain | Can we build Rust here? | Yes. rustup installed for the user: cargo 1.99.1, rustc 1.99.1, host `x86_64-pc-windows-msvc`. MSVC from VS 2022 Enterprise 17.14, VS 2026 Enterprise 18.11 and Build Tools 2019 16.11 (`link.exe` is not on PATH; cargo finds it through vswhere). |
| S1 | Does the default pipe DACL leak? | **Yes.** The default DACL grants **Everyone (WD)** and **Anonymous (AN)** `FR` (generic read). The user-only DACL is protected (`D:P`) and holds **one ACE: the current user**. Same-user connect works. Creating a second pipe with the same name fails with `ERROR_ACCESS_DENIED` (5). |
| S1 cross-account | Can another local account connect to the user-only pipe? | **Not tested.** No second local account or password was available. |
| S3a | Hook cost, direct exe | p95 **26.2–34.2 ms** with the core up, **43.0–88.7 ms** with the core down (spool path). Meets the 50 ms Stop budget with the core up. **Misses the 20 ms hand-off budget**: p50 is 22–27 ms. |
| S3a | Hook cost via Git Bash | p95 **452 ms** (core up) and **663 ms** (core down), max 2.2 s. About 9–25× over the 50 ms budget. |
| S3b | Detached core survives `claude -p` | **Yes.** The heartbeat kept updating after claude exited. **But breakaway was refused** under `claude -p` launched from the Claude desktop app (the calling shell was already in a job). From an interactive terminal session, `spawned with breakaway`. |
| S3b | Card shown | **Yes**, in both modes. Stream: a `system/informational` line, `"Stop says: …"`. Interactive: `⎿  Stop says: AIQ spike: compiled hook client ran (event …).` under the reply. |
| S3c | SQLCipher (vendored OpenSSL) builds | **No.** No native Perl on PATH. Git for Windows' Cygwin perl fails at OpenSSL `Configure` (`Can't locate Locale/Maketext/Simple.pm`). |
| S4b | Nested sessions: env and session_id | The inner hook sees the same `CLAUDE_*` names as the outer one, **plus `CLAUDE_CODE_EXECPATH` and `CLAUDE_EFFORT`**. The payload `session_id` values **differ** (outer `01117f98…`, inner `f77f9941…`). |

## 0. Toolchain

```
cargo 1.99.1 (5f94df478 2026-08-27)
rustc 1.99.1 (58cf8f9c3 2026-10-13)
where link.exe  -> not found on PATH
vswhere -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64:
  C:\Program Files\Microsoft Visual Studio\2022\Enterprise
  C:\Program Files (x86)\Microsoft Visual Studio\2019\BuildTools
  C:\Program Files\Microsoft Visual Studio\18\Enterprise
where perl -> not found
```

rustup was installed for the current user only (`rustup-init.exe -y --default-toolchain stable --default-host x86_64-pc-windows-msvc --profile minimal`, from static.rust-lang.org over HTTPS). The `s1-s3-windows` release build took 12.2 s.

## 1. S1 — user-only named pipe

Verbatim output (`aiq-spike-win.exe s1`):

```
default_create: Ok("ok")
default_sddl: Ok("O:<sid>D:(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;<sid>)(A;;FR;;;WD)(A;;FR;;;AN)")
user_only_create: Ok("ok")
current_user_sid: Ok("<sid>")
user_only_sddl: Ok("O:<sid>D:P(A;;FA;;;<sid>)")
same_user_connect: Ok(())
server_received: Some("hello")
second_create_same_name: Err(Os { code: 5, kind: PermissionDenied, message: "Access is denied." })
```

`<sid>` is an `S-1-12-1-…` SID, which marks an Entra ID (Azure AD) user.

### What each ACE grants

**Default DACL**: `D:` has no `P`, so it is not protected and can inherit.

| ACE | Trustee | Rights |
|---|---|---|
| `(A;;FA;;;SY)` | LocalSystem | `FA` = FILE_ALL_ACCESS (full) |
| `(A;;FA;;;BA)` | Built-in Administrators | full |
| `(A;;FA;;;<sid>)` | the current user (also the owner, `O:`) | full |
| `(A;;FR;;;WD)` | **Everyone** | `FR` = FILE_GENERIC_READ |
| `(A;;FR;;;AN)` | **Anonymous Logon** | FILE_GENERIC_READ |

So **yes: the default DACL gives Everyone and Anonymous read access.** **Inferred** from the access mask: a reader can open the pipe for read, take a free instance, and read whatever the server writes to it. It cannot write requests, because FILE_WRITE_DATA is not granted. For a core that answers on the same pipe, that is an information leak and an instance-squatting DoS.

**User-only DACL** (`D:P(A;;FA;;;<sid>)`): protected (`P`, so nothing is inherited), and **one ACE: the current user, full access**. SYSTEM, Administrators, Everyone and Anonymous have no entry. The spike asks for `GA` (GENERIC_ALL); Windows maps that to `FA` for the pipe. **Inferred:** an administrator can still take ownership with SeTakeOwnershipPrivilege. That is outside a user-level threat model.

**Squatting**: a second listener on an existing name fails with error 5. **Inferred:** this matches FILE_FLAG_FIRST_PIPE_INSTANCE, which the spike comment says the code relies on. The consequence: if another process creates the name first, the core fails to start instead of silently sharing the name.

**Cross-account connect: not tested.** No other local account or password was available.

## 2. S3a — hook speed

Each sample is one whole-process hook run (spawn + 186-byte stdin payload + exit), n = 50 after one warm-up.

### Run 1: `AIQ_BASH` unset

```
bench core_up=true via_bash=false: {"n":50,"p50_ms":27.1,"p95_ms":34.2,"max_ms":69.0}
bench core_up=true via_bash=true: {"n":50,"p50_ms":239.4,"p95_ms":337.0,"max_ms":344.8}
core_connections_received: Some("51")
bench core_up=false via_bash=false: {"n":50,"p50_ms":39.3,"p95_ms":88.7,"max_ms":98.8}
bench core_up=false via_bash=true: {"n":50,"p50_ms":211.9,"p95_ms":319.1,"max_ms":389.2}
```

**Caveat:** with `AIQ_BASH` unset, `bash` resolved to `C:\WINDOWS\system32\bash.exe`, which is **WSL**, not Git Bash. `core_connections_received: 51` is 1 warm-up plus 50 direct runs, so the WSL runs never reached the core: WSL cannot see the `C:/…` path. **Those `via_bash=true` rows measure WSL start-up plus a failing command, not the hook.** Ignore them.

### Run 2: `AIQ_BASH="C:\Program Files\Git\bin\bash.exe"`

```
bench core_up=true via_bash=false: {"n":50,"p50_ms":22.3,"p95_ms":26.2,"max_ms":28.4}
bench core_up=true via_bash=true: {"n":50,"p50_ms":363.6,"p95_ms":452.2,"max_ms":926.7}
core_connections_received: Some("102")
bench core_up=false via_bash=false: {"n":50,"p50_ms":24.7,"p95_ms":43.0,"max_ms":63.4}
bench core_up=false via_bash=true: {"n":50,"p50_ms":386.7,"p95_ms":663.0,"max_ms":2216.8}
```

Here 102 = 51 direct + 51 via Git Bash, so every Git Bash run reached the core.

### Against the budgets

| Path | p95 | Stop budget 50 ms | Hand-off budget 20 ms |
|---|---|---|---|
| direct, core up | 26.2 / 34.2 ms | pass | **fail** (p50 22.3 / 27.1 ms) |
| direct, core down (spool) | 43.0 / 88.7 ms | pass in run 2 / **fail** in run 1 | fail |
| Git Bash, core up | 452.2 ms | **fail** (9×) | fail |
| Git Bash, core down | 663.0 ms | **fail** (13×) | fail |

**Inferred:** the run-to-run spread on the direct path (34 vs 26 ms with the core up, 89 vs 43 ms with it down) looks like endpoint-security scanning of process creation on this managed machine. That is not measured. A per-run Defender trace would confirm it. Either way, the hook command must call the exe directly. Any `bash -c` wrapper costs about 0.35–0.65 s per event.

## 3. S3b — detached core survives the hook

Stop hook in a temp settings file (passed with `--settings`, plus `--setting-sources user` so the project's own hooks stay out of the run). The command was run from this repo's checkout, which Claude Code already trusts:

```json
{"hooks":{"Stop":[{"hooks":[{"type":"command","timeout":5,
  "command":"\"C:/Users/<user>/AppData/Local/Temp/aiq-spikes/prototypes/design-spikes/s1-s3-windows/target/release/aiq-spike-win.exe\" hook --spawn --card"}]}]}}
```

### `claude -p … --output-format stream-json --verbose` (CLI 2.1.312)

- claude exit 0, 10.1 s wall.
- `%TEMP%\aiq-spike-spawn.txt`: `Ok("spawned WITHOUT breakaway (job refused it)")`
- Heartbeat `<unix-secs> <pid>`, read 14 s and 18 s after claude exited:
  - `1793473672 39500`
  - `1793473676 39500`
  
  **The core survived claude's exit.** It later ended on its own 60 s timer (`core 60`).
- Card in the stream: **yes**:
  `{"type":"system","subtype":"informational","content":"Stop says: AIQ spike: compiled hook client ran (event 01a1935c-3bba-74ae-a607-3e84a3d4c8e7).","level":"notice",…}`
- `IsProcessInJob`: the PowerShell that launched claude (inside the Claude desktop app) was **already in a job**, and so was the core. So this run cannot tell whether claude's own job or the outer desktop-app job refused breakaway. What it does show: claude does not kill the job on exit, so the core outlived it.

### Interactive `claude --settings <file> --setting-sources user "Reply with the single word: ok"` (terminal tab; the `claude` in the user's profile runs through the Agency wrapper, CLI 2.1.316)

Exact text under the reply:

```
● ok
  ⎿  Stop says: AIQ spike: compiled hook client ran (event 01a1935f-555d-76b7-b0a9-92cf05dfd852).
```

- `%TEMP%\aiq-spike-spawn.txt`: `Ok("spawned with breakaway")`
- That core (pid 69312) ran its full 60 s: spawn file written 12:11:01, last heartbeat 60 s later.
- The prompt was passed as the initial positional argument (the session stays interactive). The TUI could not be typed into from this harness.

## 4. S3c — SQLCipher build

`cargo run --release` in `s3-sqlcipher` (`rusqlite 0.41`, `bundled-sqlcipher-vendored-openssl`).

**Attempt 1 (PATH as is): failed after 65.3 s.** First real error:

```
warning: openssl-sys@0.9.117: configuring OpenSSL build: Command 'perl' not found. Is perl installed?
error: failed to run custom build command for `openssl-sys v0.9.117`
  Error configuring OpenSSL build:
      Command 'perl' not found. Is perl installed?
```

`where perl`: not found. **No native Windows Perl is installed.**

**Attempt 2 (`PERL=C:\Program Files\Git\usr\bin\perl.exe`, Git for Windows' perl 5.42.3 `x86_64-cygwin-thread-multi`): failed after 5.7 s.** First real error:

```
Can't locate Locale/Maketext/Simple.pm in @INC (you may need to install the Locale::Maketext::Simple module) (@INC entries checked: … /usr/lib/perl5/site_perl /usr/share/perl5/site_perl …)
BEGIN failed--compilation aborted at /usr/share/perl5/core_perl/Params/Check.pm line 6.
Compilation failed in require at /usr/share/perl5/core_perl/IPC/Cmd.pm line 59.
Compilation failed in require at …/OpenSSL/config.pm line 19.
BEGIN failed--compilation aborted at ./Configure line 23.
```

No program output: the build never finished. Perl was not installed, as instructed. **Inferred:** Strawberry Perl, or a system OpenSSL via `OPENSSL_DIR`/vcpkg without the vendored feature, would get past this. Neither was tried.

## 5. S4b — nested sessions

UserPromptSubmit hook: `bash '<scratch>/s4b-hook.sh'`. It appends the payload's `session_id` and the output of `aiq-spike-win.exe env` (names only) to a file. The outer `claude -p` got `--allowedTools "Bash(claude:*)"` (least privilege, instead of skipping permissions) and was told to run `claude -p 'say hi' --settings <same file> --setting-sources user`. Inner reply: `Hi! 👋 What do you want to work on in AIQ today?`

| | Outer hook | Inner hook |
|---|---|---|
| payload `session_id` | `01117f98-bc7b-4af6-aa09-38f574ed51a1` | `f77f9941-3dcb-4401-b3e8-b4995287053c` |
| match | **no** | |
| extra `CLAUDE_*` names | — | `CLAUDE_CODE_EXECPATH`, `CLAUDE_EFFORT` |

Names seen by both hooks:

```
CLAUDECODE
CLAUDE_AGENT_SDK_MCP_NO_PREFIX
CLAUDE_AGENT_SDK_VERSION
CLAUDE_CODE_ACCOUNT_UUID
CLAUDE_CODE_CHILD_SESSION
CLAUDE_CODE_DESKTOP_APP_VERSION
CLAUDE_CODE_DISABLE_CRON
CLAUDE_CODE_DISABLE_TERMINAL_TITLE
CLAUDE_CODE_EAGER_FLUSH
CLAUDE_CODE_EMIT_STARTUP_TIMING
CLAUDE_CODE_EMIT_TOOL_USE_SUMMARIES
CLAUDE_CODE_ENABLE_ASK_USER_QUESTION_TOOL
CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING
CLAUDE_CODE_ENTRYPOINT
CLAUDE_CODE_HOST_SESSION_ID
CLAUDE_CODE_MESSAGING_SOCKET
CLAUDE_CODE_MESSAGING_TOKEN
CLAUDE_CODE_OAUTH_SCOPES
CLAUDE_CODE_ORGANIZATION_UUID
CLAUDE_CODE_SDK_HAS_HOST_AUTH_REFRESH
CLAUDE_CODE_SESSION_ATTENDED
CLAUDE_CODE_SESSION_ID
CLAUDE_CODE_SIMPLE
CLAUDE_CODE_STARTUP_FAILURE_RESULTS
CLAUDE_CODE_TERMINAL_MCP_TOOLS
CLAUDE_CODE_USER_EMAIL
CLAUDE_PID
CLAUDE_PREVIEW_CLASSIFIER_FLOOR
CLAUDE_PROJECT_DIR
```

**Caveat:** the outer claude was itself started from a Claude desktop-app session. So the "outer" hook already inherits desktop-app variables (`CLAUDECODE`, `CLAUDE_CODE_CHILD_SESSION`, `CLAUDE_CODE_HOST_SESSION_ID`, `CLAUDE_CODE_DESKTOP_APP_VERSION`, …). This is really depth 2 vs depth 3. Only names were recorded, not values, so this run cannot say whether `CLAUDE_CODE_SESSION_ID` tracks the payload's `session_id`. **For the design: use the payload `session_id` as the identity. Nesting cannot be detected from variable presence alone in this setting.**
