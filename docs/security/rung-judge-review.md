---
id: review-rung-judge-security-terms
title: "Rung judge: security and terms review"
type: doc
status: draft
owner: "Imran"
phase: "0-5"
tags: [security, terms, rung-model, judge, review]
links:
  - { to: adr-0007, rel: documents }
  - { to: spec-rung-model, rel: documents }
  - { to: aiq-threat-model, rel: refines }
  - { to: aiq-privacy-review, rel: relates-to }
review-by: 2027-02-01
summary: >-
  Independent review of the rung judge, owed before AIQ goes to anyone other than Imran and before the
  rung-model spec is Accepted. Terms: Imran's own use is consistent with Anthropic's and GitHub's terms
  as read on 2026-11-01; giving AIQ to others with Claude subscription sign-in as the default is not
  clearly allowed, so the Claude default for other people should be their own API key unless Anthropic
  confirms otherwise. Security: 13 findings, none critical; the ones to fix before Phase 5 are secret
  redaction, judge-session clean-up, the judge env-var mismatch, and sealing the Claude call further.
---

# Rung judge: security and terms review

**Date:** 2026-11-01 · **Reviewer:** Claude (independent of the authors of ADR-0007 and the rung-model
spec; did not write them) · **Requested by:** Imran · **Not legal advice.**

## 1. Result first

| # | Finding | Severity | Recommendation | Changes a product decision? |
|---|---|---|---|---|
| T1 | Imran's own use of `claude -p` on his own Claude plan | Low | Keep. Keep the daily judge budget small | No |
| T2 | Giving AIQ to others with Claude **subscription** sign-in as the judge default | **High** | Before AIQ goes to anyone else, default Claude judging to the person's own API key (or their cloud-provider credentials). Keep subscription sign-in only if Anthropic confirms it in writing | **Decided (Imran, 2026-11-02): keep the subscription default for all users; closed** |
| T3 | Imran's own use of `copilot -p` | Low | Keep | No |
| T4 | Giving AIQ to others with `copilot -p` | Low | Keep; setup says work-account (org) policy and audit apply; stop if the org turns the CLI off | No |
| T5 | ADR-0007 still says Copilot judging uses `vscode.lm`; H5 chose `copilot -p` | Low | Amend ADR-0007 to record H5 | No (records H5) |
| S1 | Judge-session copies of task content in `~/.copilot/session-state` survive `aiq forget` | **Medium** | AIQ deletes its own judge session folders after reading the answer, by default, with a setting to keep them | **Decided (Imran, 2026-11-02): delete by default** |
| S2 | No secret redaction anywhere: pasted keys and tokens go into the store, the spool and every judge call | **Medium** | Redact well-known secret shapes at ingest, before store, spool or judge | No (new control) |
| S3 | Judge env-var name differs: docs say `AIQ_JUDGE=1`, code checks `AIQ_INTERNAL` | **Medium** | One name (`AIQ_INTERNAL`, already in code and tests); fix the spec, ADR and threat model; Phase 5 test that every judge spawn sets it | No |
| S4 | The Claude judge call is not fully sealed: user plugins, MCP servers, user `CLAUDE.md` and auto memory still load | **Medium** | Add `--strict-mcp-config --mcp-config '{"mcpServers":{}}'`; check the `system/init` event per release; prepare for `--bare` becoming the `-p` default | No |
| S5 | Prompt injection from task content into the judge | Medium | Existing controls are right. Add a canary MCP/tool test for Copilot, and keep schema validation strict in Rust | No |
| S6 | `copilot -p` takes the whole judge prompt on the command line | Medium (Linux/macOS) · Low (Windows) | Find a stdin or file input for Copilot before Phase 5; until then cap and fail closed | No |
| S7 | Windows named pipe: another OS user can create the pipe name first, and the hook would send it prompts and the token | Low (single-user laptop) · Medium (shared machines) | Hook client checks the pipe server's user SID before writing | No |
| S8 | Hook forgery by same-user software | Low | Keep accepted (Imran, 2026-10-31) | No |
| S9 | Plain-text spool | Low | Keep accepted; covered by S2 redaction | No |
| S10 | Plain SQLite store | Medium (others) · Low (Imran) | Keep deferred for Imran. Before others: S2 redaction first, then decide encryption (it is already on the open list) | Already open |
| S11 | `aiq install copilot` turns on Copilot's `experimental` mode for every Copilot session | Low | Say plainly at install that it turns on *all* Copilot experimental features; re-test each Copilot release whether `STATUS_LINE` works without it | No |
| S12 | Hooks and status line point at a test build under `%TEMP%` on Imran's laptop | Low | Move to a permanent install path (already on the open list) | No |
| S13 | Phase 0 fact-check left judge sessions and outputs on the laptop | Low | Delete the listed judge sessions and `~/.aiq/fact-check` when no longer needed | No |

**Nothing critical.** Nothing here blocks Imran's own use today. Imran decided the two open items on
2026-11-02 (T2: keep subscription; S1: delete by default; see T2 and S1 below). The rest are fixes for the Phase 5 judge build, or doc corrections.

## 2. What was read, and what is inferred

**Read on 2026-11-01 (Verified that the text says this):**
- Anthropic, Claude Code legal and compliance page: https://code.claude.com/docs/en/legal-and-compliance
- Anthropic Consumer Terms, page effective date shown as Oct 8, 2025: https://www.anthropic.com/legal/consumer-terms
- Claude Code headless page: https://code.claude.com/docs/en/headless
- GitHub Terms for Additional Products and Features, effective Aug 27, 2026:
  https://docs.github.com/en/site-policy/github-terms/github-terms-for-additional-products-and-features
- GitHub Terms of Service, effective Apr 27, 2026, sections B and J:
  https://docs.github.com/en/site-policy/github-terms/github-terms-of-service
- GitHub Acceptable Use Policies: https://docs.github.com/en/site-policy/acceptable-use-policies/github-acceptable-use-policies
- Copilot CLI docs: programmatic mode, about, change settings, command reference.
- AIQ code: `crates/aiq/src/hook.rs`, `crates/aiq/src/install_copilot.rs`, `crates/aiq-core/src/{ipc,paths,spool,server}.rs`,
  `crates/aiq-store/src/lib.rs`, `prototypes/phase0-spike/aiq_spike/llm.py`, `prototypes/phase0-spike/fact_check.py`.
- AIQ docs: ADR-0007, `docs/specs/rung-model.md`, `docs/specs/history-import.md`, `docs/security/threat-model.md`,
  `prototypes/design-spikes/results/windows-s1-s3.md`.

**Not read:** Anthropic Commercial Terms and Usage Policy clause by clause; the GitHub Generative AI
Services Terms and Microsoft Product Terms (which govern Copilot Business/Enterprise); any employer
policy that applies to Imran's work accounts. **Not run:** nothing in this review was executed; the
Rust judge does not exist yet (Phase 5). Every "should" below is a recommendation, not a measurement.

## 3. Terms

### T1. Anthropic, Imran's own use — Low
**What the terms say (read):**
- "Advertised usage limits for Pro and Max plans assume ordinary, individual usage of Claude Code and
  the Agent SDK." (legal page)
- The headless page says `claude -p` is "using the Agent SDK via the CLI", for "scripts and CI/CD".
- The legal page does not "prevent an end user from signing in to the unmodified Claude Code binary
  with their own Claude subscription".
- Consumer Terms §3(7) bars access "through automated or non-human means, whether through a bot,
  script, or otherwise", except via an API key or where Anthropic explicitly permits it.

**Reading (Inferred):** Imran running his own unmodified Claude Code on his own plan, from a script he
wrote for himself, is the individual Agent SDK use the legal page describes. Anthropic documents
`claude -p` for scripts, which reads as the explicit permission §3(7) allows for. AIQ never touches the
token. The risk is volume: a background judge that makes many calls is less "ordinary" than a person
typing.

**Recommendation:** keep. Keep the default daily budget small and visible (TR-8 already does this).

### T2. Anthropic, giving AIQ to others with subscription sign-in — High
**What the terms say (read):**
- "Developers building products or services that interact with Claude's capabilities, including those
  using the Agent SDK, should use API key authentication."
- "Anthropic does not permit third-party developers to offer Claude.ai login into their own
  applications, or to route requests through Free, Pro, or Max plan credentials on behalf of their
  users."
- OAuth is "designed to support ordinary use of Claude Code and other native Anthropic applications".
- "Anthropic reserves the right to take measures to enforce these restrictions and may do so without
  prior notice."

**Reading (Inferred):** For anyone other than Imran, AIQ is a product Imran built, it uses the Agent SDK
(through `claude -p`), and it sends prompts AIQ wrote (the judge envelope), not prompts the person
typed. A reviewer at Anthropic could read that as routing requests through the person's plan on their
behalf. The end-user carve-out covers signing in to the binary; it does not clearly cover another
product driving that binary in the background. Imran's position (AIQ works only for the person, on
their machine, with their own sign-in) is a fair argument, and the binary is unmodified and no token
is handled, which meets the hard rules. But the text points the other way on the Agent SDK point, and
the penalty (account action without notice) falls on the user, not on AIQ.

**Recommendation:** before AIQ goes to anyone else, make the person's own Anthropic API key (or their
Bedrock / Agent Platform / Foundry credentials, which the page explicitly allows) the Claude default,
and offer subscription sign-in only if Anthropic confirms it in writing (the page names
https://www.anthropic.com/contact-sales for questions on permitted authentication). ADR-0007 already
says this is a settings change, not a redesign. Imran's own use can stay on his subscription.

**Owner decision (Imran, 2026-11-02):** keep the person's own Claude subscription (`claude -p`) as the
Claude judge default for all users; the matter is closed. Scope, as Imran stated: this applies only to
tasks seen by **Claude Code**. Tasks from Copilot CLI and VS Code are judged by Copilot through the
person's own `copilot -p` (history-import H5) and are not affected. AIQ's target users will almost
exclusively use Copilot, so few judge calls go through this path. The API-key fallback stays opt-in.
The finding is recorded as an accepted risk in the threat model.

### T3. GitHub Copilot, Imran's own use — Low
**What the terms say (read):**
- Programmatic mode is for "scripts, CI/CD pipelines, and automation workflows" (Copilot CLI docs).
- GitHub ToS §J (AI Features) has no restriction on automated use or on other tools calling Copilot.
- The Acceptable Use Policies §4 bar "automated excessive bulk activity" and "undue burden on our
  servers through automated means".
- Copilot Business/Enterprise bought from GitHub are governed by the GitHub Generative AI Services
  Terms; bought through Microsoft, by Microsoft's Product Terms (not read).
- Copilot CLI use consumes AI credits "based on the number of tokens processed".

**Reading (Inferred):** a small daily number of judge calls through the person's own documented CLI is
the use GitHub describes, and is not bulk activity. For a work (Business/Enterprise) account, the
organisation's policies and audit logs apply; ADR-0007 already discloses that.

**Recommendation:** keep. The setup text should also say judge calls use the person's Copilot credits.

### T4. GitHub Copilot, giving AIQ to others — Low
Nothing read bars a tool from running the person's own Copilot CLI on their own sign-in. The real
limits are organisation policy (an admin may turn the CLI or a model off) and the terms for Business /
Enterprise (not read). **Recommendation:** keep; if `copilot -p` fails with a policy error, queue and
say why, never fall back to another provider (already US-5). Read the Generative AI Services Terms
before an alpha user on a business plan.

### T5. ADR-0007 is out of date for Copilot — Low
ADR-0007 still names the VS Code `vscode.lm` API for Copilot judging; history-import H5 (Imran,
2026-11-01) chose `copilot -p`. **Recommendation:** amend ADR-0007 to record H5 and its probe facts.
This review adds a pointer note to ADR-0007; the full amendment belongs with the Phase 5 design.

## 4. Security

The judge path has five steps: hook → core (local pipe / socket) → store (and spool while the core is
down) → judge envelope → provider CLI → schema-checked answer → card. Findings follow that order where
possible.

### S1. Judge sessions keep copies of task content outside AIQ's delete — Medium
**Evidence (read):** history-import spec: every `copilot -p` call saves a session folder, and "AIQ
cleans up its own judge session folders after reading the answer only if the person allows it".
`fact_check.py` records each judge session id in `~/.aiq/judge-sessions.txt` and does not delete the
folder. `aiq forget` (Phase 1) deletes the store and spool only.
**Why it matters:** each judge call writes a second plain-text copy of the task's prompts into
`~/.copilot/session-state`, outside AIQ's store. After `aiq forget`, the person believes their content is
gone, but it is still there. That breaks the privacy rule "raw prompts stay on device until the user
deletes them" in spirit: they cannot delete them from AIQ.
**Recommendation:** delete AIQ's own judge session folders (by the recorded id) as soon as the answer is
read, by default, with a setting to keep them for debugging. `aiq forget` must also delete any that are
left. **Decided (Imran, 2026-11-02): delete by default**, with a setting to keep them; the history-import spec
is updated to match. Claude judge calls already use `--no-session-persistence`.

### S2. No secret redaction — Medium
**Evidence (read):** no redaction code in `crates/` or the specs; prompts are stored as typed (up to 64 KB),
spooled as typed, and the judge envelope carries the task's prompts.
**Why it matters:** people paste API keys, connection strings and tokens into prompts. The provider
already saw them (same-provider rule), so a judge call adds no new party, but it adds copies: in the
store, in the spool, in the provider's retention and a work account's audit log a second time, and in
Copilot's saved judge session (S1). The store is plain SQLite (S10).
**Recommendation:** at ingest, before anything is stored, spooled or sent, replace well-known secret
shapes with a marker (`[secret removed]`): provider key prefixes (`sk-ant-`, `sk-`, `ghp_`, `gho_`,
`github_pat_`, `AKIA`, `xox[bap]-`, `AIza`), private-key blocks, JWTs, and `password=` / connection-string
values. Count removals in `aiq status`. Keep it a deterministic, tested list (a golden set of canary
secrets that must never reach the store, the spool or a judge envelope). This also lowers the risk in
S9 and S10 more cheaply than encryption.

### S3. Judge env-var name mismatch — Medium
**Evidence (read):** `crates/aiq/src/hook.rs` checks `AIQ_INTERNAL`; ADR-0007, rung-model US-5 and TR-10,
and the threat model say `AIQ_JUDGE=1`. The Phase 0 spike sets neither.
**Why it matters:** the surface spike found `copilot -p` fires the person's hooks. If the Phase 5 judge is
built from the spec and sets `AIQ_JUDGE=1`, AIQ's own hooks ingest every judge call as a new task: a
feedback loop, and another copy of task content in the store.
**Recommendation:** keep `AIQ_INTERNAL` (code and the test `hook_internal_env_ignored` already use it) and
correct the three docs (done in this PR). Phase 5 adds a test: every judge spawn sets `AIQ_INTERNAL=1`,
and a judge call with AIQ's hooks installed creates no task. Judge-session ids (US-11) stay as the second
guard. A side effect: any process that sets `AIQ_INTERNAL` silences capture; that is same-user software
and is accepted (S8).

### S4. The Claude judge call is not fully sealed — Medium
**Evidence (read):** the headless page says that without `--bare`, `claude -p` "discovers and loads" hooks,
skills, plugins, MCP servers, auto memory and `CLAUDE.md` "including anything configured in the working
directory or `~/.claude`". ADR-0007 uses an empty temp folder, `--setting-sources user`,
`disableAllHooks`, `--tools ""` and `--no-session-persistence`, but not `--strict-mcp-config`; the Phase 0
spike used `--strict-mcp-config` and `cwd` = the home folder, which the Copilot docs advise against for
their CLI. `--bare` "is the recommended mode for scripted and SDK calls, and will become the default for
`-p` in a future release", and bare mode "never reads OAuth credentials".
**Why it matters:** with tools off, user MCP servers and plugins should have nothing to act with
(Inferred, not tested), but user `CLAUDE.md` and auto memory still enter the judge's context: that does
not leak data (same provider) but makes judgments depend on the person's own instructions. When `--bare`
becomes the default, the subscription path stops signing in.
**Recommendation:** add `--strict-mcp-config --mcp-config '{"mcpServers":{}}'`; run the judge with
`--output-format stream-json` once per Claude Code release and check the `system/init` event lists no
MCP servers, no plugins and no tools (a measured check, not a reading); keep `--system-prompt` (it
replaces the default prompt). If a release turns `--bare` on for `-p`, detect the auth failure, queue,
and tell the person (this also argues for T2's API-key default).

### S5. Prompt injection into the judge — Medium
**Evidence (read):** controls TR-3, TR-4, TR-5, T-B5-5: core-built envelope, task content as quoted data,
narrow facts to a JSON schema, asked twice, deterministic floors and guardrails, plain-text rendering,
no tools, empty working folder. US-7 includes injection gaming cases. `fact_check.py` parses Copilot's
answer leniently (first `{` to last `}`).
**Reading:** with no tools and plain-text output, injection cannot run anything or send data anywhere;
the worst case is a wrong fact, so a wrong rung or tip shown only to the person. That is the right
design. Two gaps: (a) Copilot plugins still load, and MCP servers are disabled only by the names AIQ
finds in three config files, so a server from a plugin or another file stays on; `--available-tools=`
should remove its tools too (Inferred, not tested). (b) The Rust judge must validate against the schema,
not extract a `{…}` span.
**Recommendation:** Phase 5 adds a canary test: a fake MCP server and a plugin that log any call; a judge
call whose task says "call every tool" must log nothing. Strict schema validation in Rust; a failing
answer is discarded (TR-4). Keep the US-7 injection set in the golden set.

### S6. Copilot judge prompt on the command line — Medium (Linux/macOS) · Low (Windows)
**Evidence (read):** `fact_check.py` passes the whole judge prompt (rubric plus task content) as the `-p`
argument. The Copilot docs say piped input is ignored when `-p` is given.
**Why it matters:** on Linux and macOS any local user can read another process's arguments (`ps`,
`/proc/<pid>/cmdline`) unless the system hides them. On Windows other non-admin users usually cannot;
same-user software can. Windows also caps a command line at about 32,000 characters, so long tasks
fail. The Claude path uses stdin and is not affected.
**Recommendation:** before Phase 5, check whether Copilot CLI accepts the prompt from stdin without `-p`
or from a file (`@file` or similar; not checked); use that. Until then: Windows only (the alpha
platform), cap the envelope well below the limit, and fail closed (queue, never truncate silently).

### S7. Windows pipe name can be taken first by another OS user — Low · Medium on shared machines
**Evidence (read):** `ipc.rs` gives the pipe a user-only DACL and the S1 spike showed a second listener on
an existing name is refused. The name is predictable (`aiq-core-<USERNAME>-<hash of path>`).
`connect()` does not check who owns the pipe.
**Why it matters:** another account on the same machine can create that name before the core starts. The
core then fails to start, and the hook client connects to the other account's pipe and sends it prompt
text and the token. On a single-user laptop there is no other account.
**Recommendation:** in `connect()` on Windows, call `GetNamedPipeServerProcessId`, open that process's
token, and refuse unless its user SID equals the caller's (then spool). Cheap, and it closes the case.
Add the negative test to the S1 spike's residual list.

### S8. Hook forgery — Low (accepted)
Frames need the per-boot token from a user-only file; the socket directory is 0700 and the Windows pipe
is user-only (`ipc.rs`, `paths.rs`; the token check is constant-time in `server.rs`). Same-user software
can still forge frames or set `AIQ_INTERNAL`; it can already read everything AIQ reads. Accepted by Imran
on 2026-10-31. **Recommendation:** keep; revisit if frames ever carry other people's data.

### S9. Plain-text spool — Low (accepted)
Owner-only, ≤ 1 MB, removed on drain (`spool.rs`, tests `spool_file_is_owner_only`). On Windows the
protection is the inherited ACL of `%USERPROFILE%`, which also grants SYSTEM and Administrators (per the comment in `paths.rs`;
not re-checked on the laptop). **Recommendation:** keep accepted; S2 removes the
worst content from it.

### S10. Plain SQLite store — Low for Imran · Medium before others
Already decided: encryption deferred (note-20261031); threat model says revisit "before any alpha user
other than Imran installs AIQ". Prompts are kept until the person deletes them, so exposure grows. A
lost laptop with BitLocker on is covered by the disk encryption; an unencrypted backup of the profile is
not. **Recommendation:** for other people, ship S2 redaction first (it removes the highest-value
content), then make the encryption decision, which is already on Imran's open list. Check `aiq forget`
also removes `-wal` and `-shm` (the store code lists them; keep a test).

### S11. Copilot `experimental` mode — Low
**Evidence (read):** `install_copilot.rs` sets `experimental: true` and adds `STATUS_LINE`, after showing a
diff and asking; it records the previous value and uninstall restores exactly that. The Copilot docs do
not say what `experimental` turns on, only that it needs a restart.
**Why it matters (Inferred):** the setting is global. It likely turns on every experimental Copilot
feature in every session, not just the status line, and those features can change between releases
without notice.
**Recommendation:** the install prompt should say in one line that this turns on all Copilot
experimental features, not only AIQ's status line, and that `aiq uninstall copilot` turns it back off.
Re-test each Copilot release whether `STATUS_LINE` works without `experimental`, and drop it when it does.
Offer "no status line" (tips through `aiq show` only) for people who decline.

### S12. Hooks point at a test build in `%TEMP%` — Low
From project memory (not re-checked on the laptop): Imran's Claude Code hooks point at
`%TEMP%\aiq-pr9\...\aiq.exe`. Temp clean-up (Storage Sense) can delete it, and the hooks then fail
silently. `%TEMP%` is per-user, so other users cannot replace it. **Recommendation:** move to a permanent
path (already on the open list).

### S13. Phase 0 fact-check residue — Low
`fact_check.py` wrote judge sessions (ids in `~/.aiq/judge-sessions.txt`) into `~/.copilot/session-state`,
and its outputs into `~/.aiq/fact-check`. Both hold task content. **Recommendation:** delete them when
the fact-check data is no longer needed (Claude can do it through folder access on request).

## 5. Changes made in this PR
- This review.
- Doc corrections for S3: `AIQ_JUDGE=1` → `AIQ_INTERNAL=1` in ADR-0007, rung-model US-5 / TR-10, and the
  threat model.
- Pointer notes in ADR-0007 and the threat model to this review; threat-model rows for S1, S2, S6, S7.
- Imran's decisions (2026-11-02): T2 keep the Claude subscription default (Claude Code tasks only; closed);
  S1 delete judge sessions by default. Recorded here, in ADR-0007, the threat model and history-import.

## 5a. Fixes since the review (2026-11-02)
- **S2 fixed:** `crates/aiq-domain/src/secrets.rs` replaces well-known secret shapes with `[secret removed]`
  in the hook (before the frame is sent or spooled), again in the core's ingest, and in `aiq import`
  (prompts and commands). Canary set in its tests; `aiq status` shows the count since the core started.
  Measured: hook p95 1.3 ms on a 61 KB prompt holding 1,600 secrets (release build, Linux, 40 runs).
- **S7 fixed:** on Windows, `ipc::connect` checks that the pipe's server process runs as the current
  user and refuses otherwise; the hook then spools.
- **S10:** the store deletion test now creates `-wal`, `-shm` and `-journal` first and checks all go.
- **S11 fixed:** `aiq install copilot` says that `experimental` turns on all Copilot experimental
  features, and that uninstall turns it back off. Open: an install option without the status line.
- **S1, S3, S4, S5, S6:** in the rung judge build (thread "Build the Rust rung judge").

## 6. What would change these conclusions
- Anthropic confirms (or rejects) in writing that a locally installed tool may drive the person's own
  `claude -p` on their subscription → T2 drops to Low (or stays High with no exception).
- A new version of the Anthropic legal page or Consumer Terms → re-read before any alpha user.
- Phase 5 canary test shows a plugin or MCP tool reachable from a judge call → S5 rises to High.
- Copilot adds stdin or file input for `-p` → S6 closes.
