---
id: defect-classes
title: "Defect-class register"
type: doc
status: accepted
owner: "Imran"
tags: [lessons, defect-classes, continuous-improvement]
links:
  - { to: aiq-architecture, rel: relates-to }
review-by: 2027-01-29
summary: >-
  The project's register of defect classes: the recurring shapes of things that go wrong here, what each
  survives, and the control that now fails when the shape recurs. Read at grounding; appended to on every
  defect, correction or falsified assumption.
---

# Defect-class register

*Governed by `continuous-improvement.md` (CI1–CI12). One entry per class, not per bug.*

**Status counts:** controlled 5 · partially-controlled 0 · uncontrolled 0

---

### DC-001 — A classification rule that compares against its own earlier outputs drifts
- **Signature:** a detector keeps a window of "what the request looks like" and adds every accepted item to
  it, including items accepted for a *different* reason (here: a cue-only "try again" joined the word window,
  so the next "try again" matched it as *similar* and scored 1.0 instead of 0.5).
- **Why it survives:** happy-path tests use content prompts only; the mixed case looks plausible in review.
- **Instances:** 2026-10-31, `aiq-domain::reprompt` (first implementation).
- **Sweep:** other windows in the domain: none (Task grouping uses only timestamps).
- **Control:** automated test `observe_cue_only_run_scores_half` (seen red), plus mutation testing on
  `aiq-domain` with 0 survivors. Rung: automated control.

### DC-002 — A test harness ignores pipe semantics, so a correct early exit looks like a failure (or a flake)
- **Signature:** the child stops reading stdin at a cap and exits; the parent's `write_all` races it and gets
  `EPIPE`, sometimes. In production the host (Claude Code) is the writer and may log or surface the error.
- **Why it survives:** passes when the child is slower than the writer, so the first few runs are green.
- **Instances:** 2026-10-31, `e2e_random_bytes_never_print_always_exit_zero` (passed 3 runs, failed 6/6 later).
- **Sweep:** the hook is the only reader with a cap.
- **Control:** the hook drains stdin up to 16 MB after its 256 KB cap (`hook::MAX_DRAIN`); the e2e test writes
  300 KB and unwraps the write, so a regression fails deterministically. Every e2e change is run at least 8×.

### DC-003 — "Reset at midnight" rules that key on the current date re-open budgets when the clock moves back
- **Signature:** a cap counted as "deliveries whose local date equals today" (or an "effective day") lets the
  clock or time zone move backwards and grant a fresh budget, or count deliveries against a day they are not
  stamped with.
- **Why it survives:** example tests move time forward only.
- **Instances:** 2026-10-31, caught in reasoning before code during `/implement` (first candidate rule allowed
  unbounded deliveries while the clock was behind).
- **Control:** single definition `aiq_domain::clock::deliveries_counted_today` (local day ≥ today) and the
  property test `cap_never_exceeded_for_any_clock_moves`; the store returns raw (utc, offset) pairs so no
  second definition exists in SQL.

### DC-004 — A detached child inherits the parent's stdio pipes on Windows, so the host never sees EOF
- **Signature:** a short-lived process (the hook) starts a long-lived one (the core) with null stdio, but
  Windows `CreateProcess` with handle inheritance passes every inheritable handle, including the hook's own
  stdout/stderr pipes from its host. The host's read never ends while the core lives.
- **Why it survives:** Linux closes nothing it doesn't need to (fds 0–2 are replaced by `/dev/null`), so the
  Linux suite is green; the Windows spike's harness did not read the hook's stdout to EOF.
- **Instances:** 2026-10-31, PR #9 Windows CI: `e2e_core_down_prompt_starts_core_once_core_survives_and_drains_spool`
  hung until the 40-minute job timeout.
- **Sweep:** `spawn::core_detached` is the only place AIQ starts a long-lived process (hook and CLI share it).
- **Control:** `core_detached` clears `HANDLE_FLAG_INHERIT` on the std handles before spawning; the e2e test
  reads the hook's stdout to EOF on the Windows CI job, so a regression hangs and fails the job (timeout
  lowered to 25 minutes).

### DC-005 — An address derived from process environment instead of the value passed in
- **Signature:** `Paths::pipe_name` read `AIQ_HOME` from the environment, while every other path came from
  the `Paths` value. A caller holding a `Paths` for another directory reached a different pipe.
- **Why it survives:** on Linux the socket is a file inside the `Paths` directory, so the bug exists only on
  Windows.
- **Instances:** 2026-10-31, PR #9 Windows CI: `e2e_wrong_token_frame_closed_nothing_stored` (NotFound).
- **Control:** the pipe name is a hash of `Paths::runtime`; the e2e test builds `Paths::at(dir)` with no
  environment override, so a regression fails on the Windows job.

### DC-006 — A restore point taken from a state the tool itself already changed
- **Signature:** `aiq install` took a fresh backup on every run. Run again after the binary moved, the
  "backup" held the first install's AIQ hooks, and `uninstall` restored it byte for byte, leaving hooks
  that point at the old path while printing "Removed AIQ's hooks."
- **Why it survives:** every test installed once from one binary. Reinstalling from a new path only
  happens on a real machine, when a build moves out of a temp folder.
- **Instances:** 2026-11-01, Windows laptop check: hooks installed from `%TEMP%\aiq-pr9\...`, and the
  plan to move them to `%LOCALAPPDATA%\AIQ\bin` exposed it. Found in review before it ran.
- **Sweep:** the only other restore point is the spool drain, which renames the file aside and never
  restores it. No other instance.
- **Control:** reinstall over an untouched install keeps the first record's backup; uninstall never
  restores a backup that still holds AIQ entries and strips instead. Tests:
  `e2e_reinstall_from_moved_binary_then_uninstall_restores_original` and
  `e2e_reinstall_after_edit_then_uninstall_strips_all_aiq` (both red without the fix).

### DC-007 — A per-user path that the OS silently virtualizes for some callers
- **Signature:** AIQ's Windows home was `%LOCALAPPDATA%\AIQ`. Hooks started by the Claude desktop app
  (a packaged MSIX app) had those writes redirected to a private copy; hooks started from a terminal used
  the real folder. Same path string, so the same pipe name, but two tokens: terminal hooks were refused
  with `AIQ-API-AUTH` and their events spooled where no core drained them.
- **Why it survives:** every test sets `AIQ_HOME`, and CI never runs a hook from inside a packaged app.
  The path looks identical from both sides; only the file contents differ.
- **Instances:** 2026-11-01, Imran's laptop: core started from the desktop app's Code session; terminal
  `claude` test produced 8 refused connections and no card. Same root cause hid a copied `aiq.exe` earlier.
- **Sweep:** every default path AIQ derives (`Paths::from_env`, install's backup and record, the copy of
  the binary). Settings live in `%USERPROFILE%\.claude`, which is not redirected. No other AppData use.
- **Control:** the Windows default is `%USERPROFILE%\.aiq` via `windows_home`; unit test
  `windows_home_is_in_the_profile_not_appdata` and Windows-only `windows_default_paths_use_the_profile_home`.
  `aiq status` names the home and explains an `AIQ-API-AUTH` count, so a split is visible, not silent.

### DC-008 — Non-ASCII in the CLI's own output turns to mojibake in Windows PowerShell 5.1
- **Signature:** `aiq judge --check-report` printed "52–79%"; Imran's PowerShell 5.1 showed "52ΓÇô79%".
  The binary writes UTF-8, and the console decodes it with the OEM code page. The program can't choose that page.
- **Why it survives:** Linux CI and the cloud terminal are UTF-8, and tests compare Rust strings, never
  what a Windows console shows.
- **Instances:** 2026-11-02, laptop accuracy-check run (en dash in every interval). Sweep found the same in
  `aiq --help` (em dash), the tip feedback line (middle dot) and the judge's consent text (arrows).
- **Sweep:** every non-test, non-comment line of `crates/aiq/src/*.rs` and `aiq-domain/src/check.rs`.
  Tip lines rendered by Claude Code or Copilot are not in scope: those hosts render UTF-8.
- **Control:** `crates/aiq/tests/ascii_output.rs` fails on any non-ASCII character in that code. It was
  seen failing red on the old help text.

### DC-009 — A comparison between two answer vocabularies that can never match
- **Signature:** the accuracy check compared the judge's stored job `unclassified` with the grader's
  `other`. These are two names for "no pack job", so every no-job task counted as a miss: job agreement was
  20 of 30, but it was 30 of 30 wherever the vocabularies overlapped.
- **Why it survives:** the unit tests built both sides from the same vocabulary. Only real data has
  stored `unclassified` rows.
- **Instances:** 2026-11-02, run 01a19ca2 on Imran's laptop.
- **Sweep:** the judge's eval (`judge.rs`, which maps `unclassified` to `other`) already did this. The
  check's `judge_view` did not. No other place compares stored judgments with a model answer.
- **Control:** `judge_view` maps the stored value. Domain test
  `no_job_agrees_with_other_and_records_what_the_grader_picked` and the `judge_view` asserts in `only_unclassified_judgments_of_the_last_version_are_judged_again` cover it, and the report prints what the
  second reader said for no-job tasks.

### DC-010 — An `Option` serialised to JSON loses the difference between "absent" and "null"
- **Signature:** `FeatureRecord.experimental_before` is `Some(Null)` for "the key was absent" and `None` for
  "AIQ did not change it". serde writes both as `null` and reads `null` back as `None`, so after a Copilot
  install on settings without `experimental`, uninstall left `experimental: true` switched on.
- **Why it survives:** the in-memory round-trip test passes; only the path through the record file loses it,
  and the e2e tests started from settings that already had `experimental`.
- **Instances:** 2026-11-02, `aiq install|uninstall copilot` (found in PR #26, carried to current main).
- **Sweep:** other `Option<Value>` / `Option<Option<_>>` fields written to disk: none (`grep Option<Value>`
  in `crates/`: only this one).
- **Control:** the record file now carries `experimental_absent`; unit test
  `feature_record_keeps_absent_through_its_file`. Records written before the fix read as before, so they
  never remove a setting AIQ did not add. Rung: automated control.

### DC-011 — A liveness signal written once and read as "still alive"
- **Signature:** `judge.lock` was written when a run started and never touched again; the run was taken for
  dead once the file was 30 minutes old. With the judge cap raised to 500 a day, runs went longer, the core's
  kicker started a second and third `aiq judge --auto`, each deleted the "stale" lock, and the first run's
  Drop then deleted the second's. 114 tasks were judged twice (measured from judge.log, 2026-11-02).
- **Why it survives:** runs were short while the cap was 50–100; no test held a lock past its stale age.
- **Instances:** 2026-11-02, `judge.rs` `Lock` and `start_background`.
- **Sweep:** other cross-process locks: `core.lock` already uses an OS lock (`File::try_lock`);
  `core-start.lock` likewise (paths.rs). No other file-age liveness check in `crates/` (`grep modified()`:
  only import's source mtime, which is change detection, not liveness).
- **Control:** the judge lock is now an OS lock released by the OS when the run ends or dies, the pattern
  `core.lock` uses. Test `one_run_holds_the_lock_however_long_it_runs_and_frees_it_when_it_ends` ages the
  file an hour and still sees the run hold it. A killed run's leftovers in `judge-work` (prompt text) are
  swept when the next run takes the lock (`sweep_removes_what_an_interrupted_call_left_and_respects_keep`).

### DC-012 — A "subset" rate counted on a wider base than the rate it narrows
- **Signature:** the check report printed kind 14 of 17 and "confident judgments only" kind 14 of 18. A
  subset can't have the larger base. Kind was counted only where the job matched; confident kind also
  counted confident tasks whose job differed, as kind misses.
- **Why it survives:** each count had its own `if`, and the unit test had no confident task with a job miss.
  It shows only when two numbers from the same report are read side by side.
- **Instances:** 2026-11-02, check run 01a19ce8 (`check::agreement`).
- **Sweep:** the other subset pairs in `agreement`: confident job / job and confident rung / rung use the
  same condition plus `!uncertain` (fine). `by_kind` counts every task where the judge named a kind,
  including job misses, on purpose: it answers "when the judge says this kind, how often is it right", not
  "where the job matched".
- **Control:** confident kind now uses the same condition as kind. Test
  `confident_kind_uses_the_same_denominator_as_kind` asserts confident n is never above kind n. Rung:
  automated control.

### DC-013 — A test fixture copied from rendered output instead of the file's bytes
- **Signature:** PR #73 taught `aiq status` to read the laptop swapper's `swapped aiq.exe` lines, and its
  test passed, but on the laptop the line still said "no upgrade recorded". The test line came from the
  console paste; the real file is UTF-16 LE with a BOM (Windows PowerShell 5.1 `Tee-Object -Append`), and
  `read_to_string` rejected it, which `.ok()` turned into "not recorded".
- **Why it survives:** a terminal shows UTF-16 and UTF-8 the same, and the degrade-to-"not recorded" rule
  hides the read error.
- **Instances:** 2026-11-02, `cli.rs` status reading `bin\upgrade-log.txt` (measured on the laptop: starts
  FF FE, 66,974 of 134,022 bytes zero).
- **Sweep:** other files AIQ reads that PowerShell writes: the release `upgrade.ps1` writes its log with
  `Add-Content` (ANSI code page; its lines are ASCII, so read as UTF-8) and `update.json` with
  `Set-Content -Encoding UTF8` (a BOM, already stripped by `update::strip_bom`). `hook-timings` is AIQ's own.
- **Control:** `decode_log` reads UTF-16 LE/BE and UTF-8 with or without a BOM. Test
  `upgrade_logs_are_read_in_the_encodings_powershell_writes` builds the bytes PowerShell writes. Rule: a
  parser of a file another program writes is tested on that program's bytes, not on a pasted line.

### DC-014 — The first key of a JSON map taken as the one that matters
- **Signature:** a sealed `claude -p --model sonnet` judge call recorded its model as `claude-haiku-5-5`.
  Claude Code adds a small helper call on another model, so `modelUsage` lists two models, and
  `serde_json`'s map sorts keys, so `keys().next()` named the helper.
- **Why it survives:** with one model in the map the first key is right; a short probe call lists one model,
  only a real judge-sized call lists two. Nothing reads the recorded model back against the setting.
- **Instances:** 2026-11-03, found by the first `aiq judge --eval` run in the cloud (report showed
  claude-haiku-5-5; replayed by hand: haiku 19 output tokens $0.0001, sonnet 77 tokens $0.017).
  `judge.rs` Claude path, and the same code on Copilot's `modelMetrics`.
- **Sweep:** `grep -rn "keys().next()" crates`: the two judge sites only.
- **Control:** `main_model` picks the entry that cost most, else wrote most; test
  `the_judge_model_is_the_one_that_did_the_work_not_the_first_key`. Copilot's entry shape is not recorded,
  so there it still keeps the first key (marked `assume:` in the code). Rule: never read meaning from a
  JSON map's key order.

### DC-015 — A fact the judge always hedges, read as unknown, demotes the rungs it should support
- **Signature:** golden g1 cases h02, h11, h20: every run said F6 (output checked) yes at 0.5 to 0.55, under
  the 0.6 fact bar, so F6 was unknown; Directed and Delegated both need F6, so one unsure fact took a
  Directed or Delegated task to Assisted and Uncertain.
- **Why it survives:** one bar for every model fact (D10) hides that some facts sit partly off the transcript,
  where an honest model hedges. Unit tests build facts at 0.9; nothing scored placement against expected
  rungs until the judge eval (PR #90).
- **Instances:** 2026-11-03, `aiq judge --eval` replay: rung exact 20/32; 3 of the 12 rung misses were this.
- **Sweep:** replay table of every case's facts vs expected rung: h06 is the same shape at 0.4 (left alone,
  under the hedged bar); no other fact sat yes at 0.5 to 0.6 on a demoted case.
- **Control:** rubric r2 `hedged_review` (F6 yes in every run at 0.5 or more plus a re-prompt in the log);
  test `a_hedged_yes_on_output_checked_counts_when_the_person_came_back_after_the_output`; the replay gate
  pins h02, h11, h20 placements. Rule: a guardrail's fact bar is tuned per fact from eval data, and a lower
  bar needs a log fact to corroborate it.

### DC-016 — Background work scheduled only on idle, starved by steady traffic
- **Signature:** `e2e_copilot_tip_in_chat_off_leaves_it_to_the_status_line` failed once on the Windows
  runner: one prompt's 20 ms send missed, the hook spooled it, and the core drains the spool only when its
  command channel stays idle for 30 s. The test polls `aiq status` every 10 ms, so the idle tick never fired
  and the prompt never arrived within the 10 s wait.
- **Why it survives:** the idle tick looks like "every 30 s", and on a quiet laptop it is. Only steady traffic
  (a poller, a busy session, a test) shows it is "30 s after the last command". A spool is rare, so the gap
  only shows on a slow shared runner.
- **Instances:** 2026-11-03, Windows CI, found by the Copilot capture thread (passed on re-run).
- **Sweep:** `grep -n "recv_timeout(TICK)" crates`: the writer loop is the only idle-scheduled work; its tick
  also prunes and expires stale cards (harmless to delay; left on the tick).
- **Control:** the writer loop drains a non-empty spool before any command; test
  `e2e_spooled_frame_goes_in_at_the_next_command_not_the_idle_tick` (red without the fix). Rule: work that
  must happen soon is triggered by the event that makes it needed, never only by an idle timeout.

### DC-017 — A desktop app that any launcher can open twice
- **Signature:** 2026-11-03 02:11Z, two AIQ home windows open on the owner's laptop; he started neither the
  second nor (knowingly) the first. Each launch of `aiq-home.exe` opened its own window, with its own reads
  of the store and its own update button.
- **Why it survives:** several launchers start AIQ home (install.ps1 first run, upgrade.ps1 `-Relaunch`
  three times, the Start-menu shortcut, dev swap scripts) and each is right on its own. Nothing owned "is one
  already open?", and a single tester rarely double-launches.
- **Instances:** 2026-11-03, laptop, after the 21st dev build install (launcher of the second still being
  traced on the laptop).
- **Sweep:** `grep -rn "aiq-home.exe" packaging crates apps`: every launcher above goes through the exe, so one
  guard in the exe covers them all.
- **Control:** `tauri-plugin-single-instance`, registered first in `main()`: a second launch shows the open
  window and exits, and the open window writes `second_launch cwd=...` to home.log. Test
  `single_instance_guard_is_the_first_plugin`; CI's Windows job launches the exe twice and fails on two
  processes. Rule: a per-user desktop app enforces one instance in the exe, never in each launcher.

### DC-018 — One record reached by two capture paths, counted once per path
- **Signature:** 2026-11-03, Imran's store: a session captured live by the hooks (`event`) and again by
  `aiq import` (`import_event`) showed as two tasks, prompts 2–3 s apart. Both copies were judged (two premium
  requests) and sometimes got different kinds. Measured read-only: 7 sessions held both ways, 805 tasks where
  there were 785; 10 sessions' tasks judged twice.
- **Why it survives:** each path is right on its own and keys its rows its own way (`event_id` vs
  `import:<tool>:<session>:<seq>`), so no key collides. The overlap only exists once a person both runs the
  hooks and imports history, and VS Code's live surface (`vscode`) differs from the file's tool
  (`copilot-cli`), so even a session-id join by surface misses it.
- **Instances:** 2026-11-02 reinstall + import (noted then, not fixed); measured 2026-11-03 by the
  intent-sessions thread.
- **Sweep:** every reader of `import_event`/`import_session` (`grep -rn "FROM import_" crates`): only the task
  list builds tasks; the judge, task detail and kind label read prompts by the task list's member ids. Data-tab
  counts (`imported_sessions`, `imported_events`) count stored rows, which is what they say.
- **Control:** the task list joins the copies (`crates/aiq-home/src/twins.rs`): same session id across the
  tool family, twins (within 30 s with the same text, or within 3 s) are one prompt, the rest are the union. Tests
  `a_session_captured_live_and_imported_is_one_task` and
  `a_session_held_live_and_imported_is_judged_once_with_every_prompt` (red without the fix). Rule: when a
  second capture path writes the same thing, the one read model that builds the unit joins the paths before
  counting; never count per path.

### DC-019 — A device fact that under-reads, labelled "trust this over the text", overrules what the text shows
- **Signature:** golden g1 h16, h17, a04, r03: the agent ran `lychee` + `markdownlint`, `scripts/check_brief.py`,
  `scripts/verify_refresh.py`, `scripts/rerun.sh <test>`. The check detector knew none of them, so the envelope
  said "checks run by: nobody". GPT Sol trusted it (as told), set F6 no at 0.86 to 0.98 and placed rung 1.
- **Why it survives:** the detector is an allow-list tested on the commands its author thought of; Claude Sonnet
  weighed the quoted commands and hid the gap, so only a second model exposed it. The spec's F3 already named
  linters and evals.
- **Instances:** 2026-11-03, Copilot eval on the laptop (68 premium requests): 4 of 6 rung misses.
- **Sweep:** every command in golden g1 (`jq` over `steps[].command`): r04's `pre-commit run` was missed too
  (added); r02's `sha256sum -c` is still not a check (left: the model placed r02 right from the text). Same class elsewhere: any log fact rendered as trusted (`agent edited files`, `sub-agents`,
  `spec named`) is an allow-list too.
- **Control:** fact set fs2 (linters, script names, test-name arguments); test
  `checks_are_read_as_command_lines_not_substrings` holds the four g1 commands and six near-misses; the Copilot
  eval runs on both providers before a fact-set change ships. Rule: a "trusted" device fact must say what it
  could not see, or be tested against every command in the golden set.

### DC-020 — A device fact read from turns AIQ never recorded, shown as "no"
- **Signature:** 78 of 541 judgments on Imran's store (2026-11-03) were made with no agent actions in any turn,
  and the envelope said "agent edited files: no, checks run by: nobody (trust these over the quoted text)". The
  log path treated a turn with no stop row, or a live prompt with no match in the transcript, the same as a turn
  the agent spent doing nothing.
- **Why it survives:** `None` (not recorded) and "read, no tool calls" collapsed into one value; a single
  `actions_known` flag per task hid per-turn gaps; the golden set always has every turn recorded.
- **Instances:** 2 imported tasks with no stop rows and 2 live tasks with no matching transcript prompt among
  the 78 (measured read-only on the laptop, `docs/investigations/no-actions/`). The other 74 really had no tool
  calls inside their prompts' windows, except 2 prompts below.
- **Sweep:** the Copilot import reader closes a turn at the first `assistant.turn_end`; 2 prompts lost 248 tool
  calls that came after it, and 1 prompt's 314 sub-agent calls (and its `subagent.started`) are not counted.
  Same family (an under-read stated as fact); its own change, not this one.
- **Control:** fact set fs3: `log_facts` keeps a yes seen in recorded turns and makes every no unknown when a turn
  is not recorded; the envelope says "not recorded" and how many prompts. Tests
  `a_turn_not_recorded_makes_a_no_unknown_and_never_none`,
  `the_envelope_says_not_recorded_never_none_for_turns_aiq_did_not_record`,
  `an_imported_turn_without_a_stop_row_is_not_recorded_and_one_with_an_empty_stop_is_none`. Rule: a device
  fact is a no only when every input it covers was read.

### DC-021 — A reader that ends a turn at an event the tool writes more than once per turn
- **Signature:** the Copilot import reader closed a prompt's turn at the first main-line `assistant.turn_end`.
  Copilot writes one after each model round, so every tool call after the first round was dropped until the next
  prompt. Measured 2026-11-03 on Imran's store (DC-020's sweep): 2 prompts lost 248 tool calls.
- **Why it survives:** the fixture wrote one `turn_end` per prompt, the shape its author expected; the live reader
  resets at each prompt and never read `turn_end`, so the two paths disagreed silently. And a reader fix alone
  reaches nothing already imported: unchanged files are never read again.
- **Instances:** `read_copilot` since `aiq import` was built; also the judge's live-task path, which reads the
  same function.
- **Sweep:** every reader that ends a unit on a tool's marker (`grep -rn "turn_end\|turn_start" crates`): only
  `read_copilot`. Claude's reader ends at the next prompt or end of file, as does the live reader.
- **Control:** a turn runs to the next prompt or the end of the file; test
  `copilot_reader_keeps_every_round_of_a_prompt_past_its_first_turn_end` (several rounds, a sub-agent's own
  turn end, red on the old reader). `COPILOT_READER` is stored in the settings: a run with a different version
  reads every Copilot session again once (`a_new_copilot_reader_reads_unchanged_sessions_again_once`). Rule: a
  turn ends where the person acts, never at a marker the tool may repeat; any change to what a reader stores
  raises its reader version.

### DC-022 — A tip handed over on a deadline the hook has already passed, recorded as shown
- **Signature:** `e2e_claude_code_card_then_copilot_run_held_by_quiet_period` failed once on the Windows runner
  (PR #112, 2026-11-03): the Claude Code Stop hook printed no card. Reproduced on a loaded 4-core Linux box
  (suite run 150 times beside 16 busy loops): 16 runs missed the card at the first turn end. Hook outcome
  `no-card` 13 (the core's 30 ms window had passed; the card stayed show-ready) and `stop-timeout` 3 (the hook's
  55 ms wait ran out). In 1 of those 3 the core had already recorded the card as shown: the tip was lost, but it
  still counts against the daily cap and starts the quiet period. With the test fixed to retry a safe miss, 10
  of a further 150 loaded runs hit the lost case.
- **Why it survives:** the hook and the core each keep their own clock. The core starts its 30 ms when it reads
  the frame; the hook starts its 55 ms before it connects. A slow connect or a slow reply moves the core's window
  past the hook's deadline, and nothing tells the core the hook gave up. On a quiet laptop both finish in a few
  ms. The tests asserted the card at the first turn end, a timing property, so a safe miss and a lost card
  failed the same way.
- **Instances:** the Claude Code Stop hook and the Copilot CLI agentStop hook (same `TakeCard` path). For
  Copilot CLI the status line still shows a card marked shown, so a lost chat line is softer there.
- **Sweep:** every hook-side wait on a core reply (`grep -n "ipc::request(" crates/aiq/src`): only `TakeCard`
  records state when it replies; ingests send no reply; status-line `TipLine` is the same shape but a missed
  refresh is retried by Copilot about once a second (handled in DC-016's test).
- **Control (tests):** `Env::stop_until_card` in `crates/aiq/tests/e2e_copilot.rs` ends the turn again after a
  miss, as the product does, and fails if a miss left the card anything but show-ready; its failure message
  carries `aiq status` and `hook-timings.log`, so the next CI failure names the hook outcome. A lost card still
  fails.
  Sweep miss (2026-11-08, Windows CI on PR #203): `e2e_reword_three_times_one_card_second_run_held` in
  `crates/aiq/tests/e2e.rs` and `e2e_no_color_card_is_plain_text` still asserted the card at the first Claude Code Stop; that file now has
  its own `Env::stop_until_card`, used by both. Sweep: `grep -n 'expect("a card' crates/aiq/tests`.
- **Control (product):** a tip handed to a turn-end hook is held in the core (`Service::hand_over`) and counts as
  shown only when the hook sends `Shown` after printing it (`card_shown`). A hook that never confirms within
  `HANDOVER_CONFIRM_MS` (5 s), or whose reply the core could not pass back (`hand_back`), leaves the tip ready for
  the next turn end, counted as `AIQ-CARD-UNCONFIRMED`. While a tip is being printed, it counts as shown for the
  budget, both at hand-out and at the decision. No store change. Tests: `a_tip_the_hook_never_confirms_is_ready_
  again_and_never_counted` and three others in `crates/aiq-core/tests/service.rs`;
  `e2e_a_tip_no_hook_says_it_printed_is_not_counted_and_comes_back` (red on the old code). Measured with
  `loaded-repro.sh`, 150 loaded runs: 0 lost tips (was 10). Card-printing Stop hook, 40 unloaded runs: p95
  1.43 ms before, 1.78 ms after. Rule: a hand-over across two processes is complete only when the receiver says
  so; a sender's own record of "sent" is a reservation.

### DC-023 — An unauthenticated GitHub API call on the product's normal path, sharing a per-address limit
- **Signature:** 2026-11-04 00:13Z, Imran's laptop: `aiq update --now` reported "check failed (403) Forbidden".
  Measured by the alpha thread: `api.github.com` answered 403 with `X-RateLimit-Limit 60`, `X-RateLimit-Remaining
  0`, reset 00:58:43Z, for the laptop's address; the repo is public and its release downloads worked. The 60
  unauthenticated calls an hour are counted per public address, so everyone behind one office NAT shares them.
- **Why it survives:** on a home connection or a CI runner the limit is never near, so every test passed. The
  failure needs many people on one address, which no test had.
- **Instances:** `packaging/windows/upgrade.ps1` `Latest-Release` (the only `api.github.com` call in the repo:
  checked with `grep -rn api.github.com` over `*.rs *.ts *.tsx *.ps1 *.yml`). The core and AIQ home start this
  script and make no GitHub call of their own.
- **Control (product):** the tag comes from the `github.com/<repo>/releases/latest` redirect (HEAD, 302 to
  `.../releases/tag/<tag>`) and the assets from `releases/download/<tag>/`, neither under the API limit; the API
  is asked only when the redirect names no tag. A 403/429 from the API records `retry_at` (from
  `X-RateLimit-Reset` or `Retry-After`) and the core checks again then, not in an hour or a day
  (`check_due`, `kick_sleep`).
- **Control (tests):** `packaging/windows/tests/upgrade-network.ps1` on the Windows runner (release.yml) runs the
  script against a local stand-in: redirect path makes no API call; no tag falls back to the API; a rate-limited
  API leaves `retry_at` at the reset. `a_rate_limited_check_waits_for_the_reset_not_the_hour` and
  `the_kicker_wakes_a_minute_after_a_reset_that_comes_before_the_hour` in `crates/aiq-core/src/update.rs`.
- **Rule:** a call the product makes by itself on every install never uses an unauthenticated API with a
  per-address quota when an unmetered route exists; if it must, it reads the quota headers and waits for the reset.

### DC-024 — A second install path that leaves the first path's pending state behind
- **Signature:** 2026-11-04, Imran's laptop: 0.1.1's `aiq update --now` staged 0.1.2 (update.json `state
  waiting`, `staged aiq-0.1.2-windows-x64.zip`, `blocked AIQ home is open`). He then installed 0.1.2 with
  install.cmd. update.json still said waiting, so `aiq update` offered 0.1.2 "when AIQ is idle" and auto-update
  could swap 0.1.2 in over itself.
- **Why it survives:** install.ps1 and upgrade.ps1 both put exes in `.aiq\bin`, but only upgrade.ps1 knew about
  update.json. Each path's tests started from a clean state.
- **Control (product):** install.ps1 drops `available`/`staged`/`blocked`, sets `state current` and removes
  `.aiq\update` when the waiting release is this version or older (`AIQ-INSTALL-CLEARED-WAITING` in
  upgrade-log.txt); a newer waiting release stays.
- **Control (tests):** release.yml step "install.ps1 drops a waiting update of the same version, keeps a newer
  one" on the Windows runner.
- **Rule:** every path that changes the installed version reconciles the state the other paths keep about it.
- **Recurrence (2026-11-04 04:17:03Z, laptop):** before #120 shipped, Home still offered the stale 0.1.2 and
  Imran's **Install now** ran `upgrade.ps1 -Apply` on it: a same-for-same swap that overwrote the `.old` copies
  (the way back to 0.1.1 was lost). A second path, same class: `-Apply` trusted `state waiting` without checking
  the installed version. **Control:** `-Apply` drops a staged release that is not newer than the installed one
  (`AIQ-UPDATE-NOT-NEWER`) before anything is stopped; `-Check`'s up-to-date branch also clears `staged` and
  `blocked`. Test: release.yml auto-update step, "a waiting release that is not newer is dropped" (the `.old`
  copy's hash must not change).

### DC-025 — A process stopped to replace its exe and never started again
- **Signature:** 2026-11-04 04:17:03Z, laptop: `upgrade.ps1 -Apply` (Home's Install now) stopped `aiq.exe` and
  `aiq-home.exe`, swapped, and relaunched only Home (`-Relaunch`). home.log showed `core=false` until a hook
  started the core at 04:22:53Z; Home said "AIQ isn't running" for almost six minutes.
- **Why it survives:** the CI smoke test checked the exes and update.json after the swap, never what was
  running. A hook restarts the core sooner or later, so it looks like a delay, not a fault.
- **Control (product):** `Stop-Aiq` records whether an `aiq.exe` was running; `-Apply` (success and put-back
  paths) and `-Rollback` start `aiq core` hidden again when one was (`AIQ-UPDATE-CORE`). A second core exits at
  once (`CORE_ALREADY_RUNNING`). `Stop-Aiq` now waits for the processes to end before the swap.
- **Control (tests):** release.yml auto-update step starts a core before the idle apply and fails unless an
  `aiq.exe core` from the bin folder is running afterwards.
- **Rule:** whatever an update stops, it starts again, and the test asserts the running set, not only the files.

### DC-026 — One log, two writers, two caps
- **Signature:** 2026-11-04, laptop on 0.1.2: `hook-timings.log` was 15.5 KB / 323 lines, oldest line 2026-11-03
  14:17Z, though the hooks' cap is 128 KiB. The Copilot CLI status-line command appends to the same file with its
  own 32 KiB cap, so `bounded_append` emptied the hooks' log every time it passed 32 KiB. The hook-budget window
  (runs since the last upgrade) lost its runs each time.
- **Why it survives:** each writer is right on its own; the cap is an argument at every call site, and no test
  reads two writers together. A plain truncate also empties the log at every fill, so a short log looks normal.
- **Control (product):** `hook::append_timing` is the only writer of the timing log (hooks and status line);
  a full log rotates to `hook-timings.1.log` (`paths::rotating_append`) and `aiq status` reads both.
- **Control (tests):** `hook::tests::the_timing_log_has_one_writer` fails when another file names the timing log
  and appends (red on the old statusline.rs); `paths::tests::rotating_append_keeps_the_previous_fill`.
- **Sweep:** `judge.log` and `label.log` have several writers, all with their file's one `LOG_MAX`; `hook-errors.log`
  has one writer.
- **Rule:** a shared file has one writer function that owns its cap; callers pass only the line.

### DC-027 — A gate figure that applies an older, coarser rule than the product view it measures
- **Signature:** 2026-11-04, laptop on 0.1.2: 34 hook runs since the install, `aiq status` said "no hook runs since
  then yet" for G1. The figure left out every run in a set-aside folder (7 Copilot runs in foresight, template
  rule), while since PR #87 the task list counts a person's session there (two or more prompts).
- **Why it survives:** PR #87 changed the task list and the judge; the hook figures read the same folder list
  through a separate path (`split_hook_runs`) that nobody re-read. The figure was empty, not wrong-looking.
- **Control (product):** the timing log carries a session tag (`s=`, a hash, never the id); `cli::SetAside` splits
  folders like the task list: `projects` and `aiq-check` leave out every run, other set-aside folders leave out
  only runs whose session has fewer than two prompt runs.
- **Control (tests):** `cli::tests::hook_budget_counts_a_persons_session_in_a_template_folder_like_tasks_do`
  (shaped like the laptop: foresight, the AIQ repo, an aiq-check folder, a pre-0.1.4 line, an automated run).
- **Sweep:** other readers of the folder list: `aiq status`'s folder listing (display only) and `aiq_home::tasks`
  (already per session). None else.
- **Rule:** when a set-aside rule changes, grep every reader of the folder list, not only the one in the change.

### DC-028 — A tool counted as installed because a folder with its name exists
- **Signature:** 2026-11-05, an alpha user on 0.1.4 who never installed Claude Code: setup said "Claude Code ·
  connected". `tools::found_at` set `present` when `~/.claude` existed, so first-run setup ran
  `aiq install claude-code --yes`, created `~/.claude/settings.json` with AIQ's hooks, and "connected" (hooks in the
  file) was then true of a tool that isn't there. Reproduced in a container: an empty `~/.claude` and no `claude`
  program → `aiq install --yes` wrote the file. What created that user's folder is not known (skills, CLAUDE.md or
  another tool's Claude support all can).
- **Why it survives:** on every developer machine the folder and the tool go together, and the tests built a
  "present" tool by making its folder, so the proxy was never tested against the thing it stood for.
- **Control (product):** Claude Code counts as installed only when its `claude` program is on PATH or in its own
  install folder (`~/.local/bin`, `<config>/local`), or its config folder holds session transcripts (`projects/<x>`).
  Setup, Settings and the side panel say "connected" only for hooked **and** installed; hooks without the tool are
  "not installed" and can be removed. `aiq install --new` (core start, Home open) connects a tool installed after
  setup once; tools already seen (so one the person disconnected) are left alone (`tools-seen.json`).
- **Control (tests):** `tools::tests::a_bare_claude_folder_is_not_claude_code` ;
  `e2e_install_with_no_tool_connects_every_tool_found_and_names_the_undo` (bare folder → nothing written; red on 0.1.4, as the container repro showed);
  `e2e_hooks_left_in_a_claude_folder_without_claude_code_are_not_called_connected`;
  `e2e_install_new_connects_a_tool_installed_after_setup_once`; Home e2e "a tool that isn't installed says so".
- **Sweep:** Copilot still counts as present when `~/.copilot` exists; Copilot CLI and VS Code agent mode both
  create it and both read the hook file AIQ writes there, so the folder is the right signal (assume: no other tool
  creates `~/.copilot`; if one does, the same symptom appears for Copilot). `import` and `judge` read tool folders
  only to find transcripts, so a bare folder gives them nothing to do.
- **Rule:** detect a tool by the tool (its program or the files only it writes), never by a folder others can make.

### DC-029 — Background work whose start, progress and failure reason never reach the screen
- **Signature:** 2026-11-05, first outside user on 0.1.4 (Copilot only, 221 imported tasks). Home said "Not judged
  yet" and "AIQ judges your finished tasks in the background" with no progress, no "nothing to do", and no reason.
  Read from the code: analysis ran only from the core's kicker, the core started only from a hook (so nothing ran
  until the first prompt after setup), and a missing or signed-out Copilot CLI was written to `judge.log` only.
- **Why it survives:** every piece was correct on its own (cap, back-off, lock, failure outcome in `judge_call`);
  no view read them together, and the developer's laptop always had a running core and a signed-in tool.
- **Control (product):** `aiq_home::Analysis` (state, waiting, analyzed, today, daily, finish day, blocked tool and
  why) read from the same task list as the chart and from each provider's newest call; Home's card and Getting
  started read only it. Home starts `aiq judge --auto` itself when work waits and no call ran for 10 minutes.
- **Control (tests):** `aiq_home::analysis_tests` (fresh install, used-up share, signed-out tool, give-up, off and
  paused) and the e2e "first-run analysis" test (working, today done, blocked with its one action).
- **Sweep:** other background work with a silent failure: the kind label (`kind_label` calls, shown in Settings),
  the update check (`check_error` shown in Settings > Updates). Both already surface a reason.
- **Rule:** background work ships with a state the person can see: what it is doing, how far, what (if anything)
  they must do, and why it stopped.

### DC-030 — A coverage test that checks a step has some move, not that each failing fact has one
- **Signature:** 2026-11-05, Imran asked how users move from Directed to Delegated. Reading `rung::delegated` against
  the pack showed a doc written in the chat, or a doc, question or run steered one step at a time (F12), had no
  next step, so its kind read "AIQ isn't sure yet". 11 job × step × fact gaps in all (docs F1/F12/F3, questions
  F12/F3, runs F12/F3, every job F9).
- **Why it survives:** `bundled_moves_…` asserted only that each job had *a* move at steps 1 to 3. One move per
  step passed while the other facts of that step's guardrail had none, and step 4 was not checked at all.
- **Control (tests):** `pack::tests::every_guardrail_fact_a_job_can_fail_has_a_move` fails each fact the next
  rung's guardrail reads, for each job and step 1 to 4, on a task that can sit at that step, and asserts a move
  of that step and job names it. Red on bundled-8 with the 11 gaps; green on bundled-9.
- **Sweep:** the guardrails are the only source of moves (`coach::moves_for`), so the test covers every way a
  move is chosen. F9 is unknown until it is read from the log, so `saved_workflow` is guide text only for now.
- **Rule:** test coverage against the rule's own inputs (each fact it reads), never against a count per group.

### DC-031 — A usage guard that runs after the paid work it guards
- **Signature:** 2026-11-05, one-call study: `aiq analyze --eval --runs 8 --cases … --record f` made 272 Claude
  calls ($4.51), then printed "--record needs every case (drop --cases)" and threw the answers away. The check sat
  after the run loop, next to the write it protects.
- **Why it survives:** the guard is correct and tested by reading the code; nobody runs the bad flag mix, so the
  cost of checking late is never seen.
- **Control:** `parse_opts` refuses `--record` with `--cases` before any call;
  `judge_eval::tests::record_with_cases_is_refused_before_any_call`.
- **Sweep:** no other flag check sits after the paid loop in `judge_eval.rs` (read). `aiq label --eval` and
  `aiq analyze --check` were not read for this shape (assume: they parse first; if not, the same loss repeats there).
- **Rule:** validate every argument before the first paid or slow call, never beside the step that uses it.

### DC-032 — An accessibility check that reads colours while a theme change is still fading in
- **Signature:** 2026-11-05, PR #150 CI: "coaching: every page … (dark)" failed once with a color-contrast hit on
  the Settings nav link (`#5cc8c2` on `#ffffff`, 2:1), then passed on a re-run. The e2e `open()` set `data-theme`
  after load and ran axe at once; nav links and buttons fade their background over 100 ms, so axe sometimes read the
  new dark ink on the old light background. Reproduced 5 of 60 runs locally; real dark contrast is fine.
- **Why it survives:** most runs finish the fade before axe reads, so it looks like a one-off flake and a re-run
  turns it green.
- **Control:** `tests/e2e.mjs` `settle()` waits for every running animation and transition to finish after a theme
  switch (in `open()` and the Theme test) before axe runs: 0 of 60 locally, e2e 67/67 three times.
- **Sweep:** `open()` is the only place a test changes `data-theme`. The Theme test's switch by click ran axe
  straight after it, so it now settles too. No other test changes theme mid-page (`emulateMedia` in the Theme test
  runs no axe after it).
- **Rule:** after any change that restyles the page, wait for transitions to finish before measuring colours.

### DC-033 — A best-effort step after the real work, with no error handling, in a script that stops on any error
- **Signature:** 2026-11-05 23:22Z, Imran's laptop, Update from 0.1.7 to 0.1.8: the swap worked, then Microsoft
  Defender's ASR rule C1DB55AB (event 1121, set by IT) refused upgrade.ps1's start of the new `aiq.exe` and
  `aiq-home.exe`. `Start-Core` caught its error; the `-Relaunch` `Start-Process` had no `try`, so under
  `$ErrorActionPreference = 'Stop'` it ended the script with no log line. AIQ home stayed closed.
- **Why it survives:** CI never passed `-Relaunch` and Windows never refused a start on the runner, so the
  unguarded line only ran on the person's machine, and only failed there when Defender stepped in.
- **Sweep:** `Start-Process` in packaging/windows: upgrade.ps1's five `-Relaunch` lines (now `Start-Home`) and its
  core start; install.ps1 lines 137–138 start the import and AIQ home as the last step of a manual install the
  person watches, so an error there is shown to them; left as is.
- **Control (product):** `Start-Again` in upgrade.ps1: never throws, tries again after 1, 2, 4 and 8 s, skips a
  process already up, tries AIQ home through Explorer, and logs one `AIQ-UPDATE-CORE` / `AIQ-UPDATE-HOME` line for
  every outcome.
- **Control (tests):** release.yml auto-update step runs `-Apply -Relaunch -FailLaunches 2` (core started on the
  third try, AIQ home reopened) and `-Rollback -Relaunch -FailLaunches 99` with a hook-like core start (no second
  core; AIQ home refusal logged; exit 0).
- **Rule:** a step that runs after the work is done (restart, reopen, tidy-up) is guarded and logged on every
  path; its failure must never hide the work's outcome or end the script silently.
- **Recurrence (2026-11-07, release build of 0.1.15, main 9cfa689):** `Log`'s own `Add-Content` threw "Stream was
  not readable" right after "Rolled back to AIQ" (the test's stand-in hook was reading the log), and the rollback
  exited non-zero. Same class: the log write is best-effort too. **Control:** `Log` tries 5 times, 100 ms apart, and
  never throws; release.yml runs a `-Apply` with the log held open and wants exit 0. install.ps1's two log lines are
  left: the person watches that run.

### DC-034 — A send that gives up on a timer and drops the frame without knowing whether it went out
- **Signature:** 2026-11-08, Imran's laptop on 0.1.18: a Copilot CLI prompt (04:20:57Z) and a Claude Code prompt
  (03:28:10Z) had no stop event stored, and `aiq status` showed `AIQ-HOOK-STOP-TIMEOUT` 11 with Stop 7 and
  agentStop 15 runs in 24 h. The Stop hook waits 55 ms for connect, write and the core's reply on a helper
  thread. On a timeout it counted the code and returned; the process then exited and ended the helper thread.
  If that thread was still connecting or writing, the turn end never reached the core and was not spooled.
  The Right-now card and live trend then saw the agent as still busy.
- **Why it survives:** on Linux CI a local-socket connect and write finish in microseconds, so the timeout only
  fires after the frame is written; the lost case needs a slow Windows pipe connect (peer check, Defender, a
  loaded machine). The timeout and the "frame stored" cases shared one outcome label, so the log could not
  tell them apart.
- **Sweep:** every `ipc::request` caller in crates/aiq: the prompt and Copilot ingest paths already spool on any
  error, timeout included (a duplicate is ignored by event id); `Shown` counts `AIQ-HOOK-SHOWN-UNSENT` and the
  tip stays deliverable; only the Stop path dropped the frame. Core side: `dispatch` drops a `TakeCard` when its
  1024-deep channel is full, with no counter; noted, not changed (never seen full).
- **Control (product):** `ipc::request_tracked` reports whether the whole frame was written. On a timeout the
  hook spools an unwritten stop (`stop-timeout-spooled`) and keeps `stop-timeout` for a written one, so the
  timing log now measures which case happens.
- **Control (tests):** `ipc::tests::request_tracked_*` (no core: not written; late reply: written and read by the
  core); `hook::tests::a_timed_out_stop_that_never_left_goes_to_the_spool`.
- **Rule:** a sender that gives up on a timer must know whether its message left. If it may not have, keep it
  (spool, retry) before the process ends; an idempotent receiver makes keeping it safe.

### DC-035 — A rule that settles later than the first event it governs, which was already acted on
- **Signature:** 2026-11-08 03:28Z, Imran's laptop: one of Claude's own Remote Control sessions. At its first
  prompt rule RC read the transcript head and found it not yet written (`rc.check projects=false
  settled=false`), so the prompt was stored as the person's. At 03:31Z the head settled as Projects and every
  later event was set aside. The first prompt stayed behind as a one-prompt task with no turn end. Measured
  over 30 days: 6 of 9 Claude Code sessions were such one-prompt, no-stop tasks, and all 6 were analysed
  (14 analysis rows, 12 uncertain).
- **Why it survives:** the re-read on later events (from the 11-02 fix) only governs events after it settles;
  nothing looked back at what was recorded before. Tests used a missing transcript and expected the prompt
  stored at once, which was the defect.
- **Sweep:** the other live gates (`guard` H ≥ 2, folder rules, AIQ check folders) decide from data present at
  the first prompt and already hold a first prompt back (`held`) when unsure; rule RC was the only gate that
  recorded while undecided.
- **Control (product):** `Service::rc_held`: a Claude Code prompt that arrives while rule RC is unsettled
  waits in memory; the session's next event that is not held (its Stop at the latest) records it first, unless
  the head then shows a Projects session, which drops it. Held events are not counted as set aside.
- **Control (tests):** `an_unwritten_transcript_head_is_read_again_until_it_settles_rule_rc` (Projects
  session: 0 prompts kept; terminal session: the prompt is recorded at its Stop, before the stop); e2e
  harness writes a real terminal transcript head.
- **Rule:** a rule that may settle after the first event must not let that event through while undecided:
  hold it until the rule decides, with a bounded release.

### DC-036 — A self-check that switches a feature off and, by doing so, stops measuring itself
(DC-035 is PR #181.)
- **Signature:** 2026-11-08, Imran's laptop, measured read-only. The kind-label self-check reached 6 right of 10
  on 2026-11-04 16:02Z and fell under the 8-in-10 bar. From then on the core asked for no labels, so no new
  label was ever checked, and the check could not recover. Word matching found a kind for 4 tasks in a week,
  so no tip was decided on any surface for four days across 91 coachable prompts. The switch-off logged
  nothing, because the warning fired only when words and labels were both off. The count was all-time, so old
  misses and labels from an older prompt or model would have held the tips forever.
- **Why it survives:** the unit test asserted the behaviour itself ("labels 7 of 10: AIQ stops asking"). No test
  asked whether the check could ever turn the feature back on, and no signal showed that tips had stopped.
- **Sweep:** the word-guess self-check had the same shape (`guess` was only computed while words were on): fixed
  the same way. Home Settings computed one check over words and labels together while the core kept two: now
  both use `check_guesses` per method. The accuracy pause (K6b) reads separate `aiq analyze --check` runs that
  keep running while it pauses, so it does not have this shape. `judge.rs` N3 is an all-time report line, not a
  switch: left as is.
- **Control (product):** while a check holds tips, the core still makes and stores word guesses and still asks for
  labels; only the tip is held. The hold is logged once per core run (`AIQ-COACH-GUESS-LOW`, with the counts).
  `check_guesses` counts only the newest `2 × guess_check_min` checked guesses.
- **Control (tests):** `next_rung::labels_have_their_own_self_check_hold_tips_below_eight_in_ten_and_keep_being_checked`
  (still asks, stores the label, shows no tip, and label tips return after ten newer right labels);
  `next_rung::self_check_holds_tips_at_seven_of_ten_keeps_guessing_and_on_brings_them_back`;
  `coach::tests::self_check_counts_only_the_newest_twenty_so_old_misses_leave`.
- **Rule:** a guard that switches something off must keep measuring the thing it guards, so it can switch it back
  on, and it must say when it switches. A test for such a guard proves the way back, not only the switch-off.

### DC-037 — A stop in the product that only a log line can see
- **Signature:** 2026-11-04 16:02Z to 11-08, Imran's laptop. No tip was decided on any surface for four days (see
  DC-036). Every number that would have shown it existed in the store or the log, but nothing put "tasks in,
  tips out" side by side, so nobody saw it until a person asked why tips had stopped. Of 20 new tasks that reached
  the tip step since 11-04, 15 ended with no kind; that count had to be rebuilt by hand from log lines.
- **Why it survives:** each hold is correct on its own and is unit-tested on its own (cap, quiet, cooldown, accuracy
  pause, self-check). No test or surface asks the end-to-end question: of the tasks that could have had a tip,
  how many did, and where did the rest stop?
- **Sweep:** next-step tips had eleven silent exits (no word match, label failed / unsure / late / never back, label
  or words held, kind held by its state, tips off, a late first prompt). Reprompt tips record a card decision for
  every outcome, so they were already countable. Ready tips that expire were countable (`card_decision`
  `expired`) but shown nowhere.
- **Control (product):** store v11 `tip_path`: every exit of the path writes its step. `aiq status` and Home count new
  tasks → kind known → tip ready → shown → opened, with every reason, and name any working day with no tip shown.
- **Control (tests):** `next_rung::every_new_task_ends_its_tip_path_at_a_counted_step`,
  `next_rung::held_self_checks_are_counted_as_their_own_steps`,
  `next_rung::status_carries_the_funnel_and_a_dry_working_day_warns`,
  `service::a_rescued_first_prompt_is_counted_on_the_tip_path`, `funnel::tests::*` (every step and hold reason has
  words, dry-day rules), the Home e2e "tip funnel" tests.
- **Rule:** a feature whose job is to act on a moment counts, per period, the moments it saw and the ones it acted
  on, and each way a moment was passed over gets its own code. A new early return without a step is a defect.

### DC-038 — A test reads, at one instant, a value the product is allowed to reach later or more than once
- **Signature:** 2026-11-08, two CI flakes. (1) Windows rust job on #185: `e2e_a_tip_no_hook_says_it_printed_is_not_counted_and_comes_back`
  found `AIQ-CARD-UNCONFIRMED 2`, expected 1. The test retries the turn end because a loaded runner can miss the
  hook's 30 ms + 35 ms wait (STOP_WAIT_MS, ADR-0005), but a retry whose hook gave up after the core took the tip
  is a real hand-over and is counted too (`hand_back` or expiry). (2) Linux ui job on main 69ad8fe: the tour test
  read focus the moment `#tourhd` existed; the tour moves focus to Next one animation frame later, by design
  (tour.tsx, so a closing palette can't take focus back), and focus was still on the page.
- **Why it survives:** both pass on a fast machine every time; the window is a frame or a few ms wide, so a local
  run never sees it. The product behaviour is right in both: the tip is not lost (it is ready again), and focus
  does land on Next.
- **Sweep:** e2e.rs asserts `AIQ-CARD-UNCONFIRMED` only here; e2e_copilot.rs already bounds its retries
  (`stop_until_card`). Home's other `requestAnimationFrame` focus moves (App.tsx end of tour, Projects, Data) are
  read with `waitForFunction` or not read right after; the skip-link focus read is native, not deferred.
- **Control (tests):** the unconfirmed count is bounded by the hooks' own outcomes in `hook-timings.log`
  (1 for the gone hook, plus at most 1 per retry that timed out or could not confirm; never 0, never more); the
  tour test waits up to 2 s for focus on Next before asserting it.
- **Rule:** when the product defers a step (a frame, a retry, a window) the test waits for that step with a bound,
  and when a test retries, its exact counts allow what each retry may legitimately add, read from the product's
  own record of the retry, never a looser guess.

### DC-039 — A classifier scored against a different question than the one it is asked
- **Signature:** 2026-11-08, Imran's laptop: the kind label reads only a task's first prompt; its self-check
  scores it against the analysis of the whole task. 6 of 10 right, so label tips stopped on 11-04 for 4 days.
  The offline check before launch (11-03) gave 11 of 12 because it was scored on a different sample, and
  nothing re-ran it. Most misses were problem reports labelled `troubleshoot` whose task went on to a fix.
- **Why it survives:** the label is right about the prompt and wrong about the task, so reading the prompt
  never shows the bug; only the score against later outcomes does, and no test held that score.
- **Sweep:** other first-prompt decisions checked against the whole task: the word guess (4 of 4, too few to
  say) and the Right-now card (reads the live session, not checked against analysis). Copilot labels used
  Copilot's own model pick when no model was set while analysis used AIQ's default: fixed (`copilot_models`).
- **Control (product):** `label::NOT_COACHED` (troubleshoot labels never coach); labels use the analysis models.
- **Control (tests):** `label_replay_gate` (synthetic set, prompt pinned by `PROMPT_VERSION` and text, 8-in-10
  floor, coverage floor, injection), `measured_labels_clear_the_bar` (real measured labels, no text),
  `accept_needs_a_pack_job_a_named_kind_and_both_confidences_at_the_bar`; `aiq label --eval` exits 3 under the bar.
- **Rule:** when a model predicts something checked later against a fuller reading, measure it against that
  reading on real data before it acts, keep the measurement as a replayed fixture, and fail CI when it drops.
- **Recurrence (same day):** the first fix (troubleshoot labels only) was chosen on 10 confident labels and
  read 8 of 10; 60 fresh tasks gave 13 of 29. A rule fitted and scored on the same small sample is a guess.
  Control: `measured.json` now holds both runs (78 rows), so a rule must clear the bar on data it was not
  fitted to; any later rule change adds a fresh laptop run to it before merging.

### DC-040 — A deferred action waits for a condition the person may never reach, and nothing escalates
- **Signature:** 2026-11-08, laptop: the updater installs only "when AIQ is idle", and idle required AIQ home to be
  closed. Imran keeps Home open (PID 44988 since 04:09:55Z), so 0.1.19, 0.1.20 and 0.1.21 were each downloaded and
  replaced in turn over about 3 hours and none installed; 0.1.18 still ran at 07:04Z. `blocked` said "AIQ home is
  open" the whole time; the box offered Update, which was not pressed. A second instance on the same path: a run
  that AIQ home closed itself for exited without reopening Home when another update run held `update.lock`.
- **Why it survives:** every test opens or closes Home on purpose, so "Home stays open for days" is never a case;
  each part is correct alone (the script must not swap exes under a running Home), and the waiting state looks
  like progress (a download, a box, a log line) while nothing ever completes.
- **Sweep:** other waits on a person's state: tips (30-min quiet, cap; already counted by the tip funnel, DC-037),
  the analysis queue (paused shows why and how many wait), Copilot turn end on an open permission prompt (stop
  events thread; a missed confirm keeps the tip, no loss). The update path was the only one with no way forward
  but the person's press. Every exit of upgrade.ps1 after AIQ home closed itself now reaches `Start-Home`.
- **Control (tests):** `update::tests::home_installs_a_waiting_update_when_away_only_under_auto_and_not_every_minute`,
  `an_away_install_records_its_try_and_the_next_open_is_minimised_once`, `updateNow.test.ts` awayInstallDue, the Home
  e2e away-install case (fake clock), release.yml smoke "lock held: AIQ home opened again".
- **Rule:** an action deferred until "the person is not using X" names the longest it may wait and what makes it
  happen then (here: 10 minutes in the background), and every exit of a step that closed something the person had
  open puts it back.

### DC-041 — A freshly installed, unsigned exe started by a script host on a machine with Defender's ransomware rule
- **Signature:** 2026-11-07/08: Windows showed "Risky action blocked … Use advanced protection against ransomware …
  cmd.exe … .aiq\bin\aiq.exe" to an alpha user, and Imran's laptop logged event 1121 (rule C1DB55AB) 17 times for
  AIQ's exes since 11-05: every one a start by cmd.exe or powershell.exe within about an hour of an update.
- **Why it survives:** CI runners and home PCs do not run that IT-set rule, and the block needs a file only minutes
  old. Each release writes new exes, so the window reopens on every update.
- **Sweep:** every place AIQ starts its own exe from a script host: upgrade.ps1 (core and AIQ home after a swap:
  PR #170 starts them through Explorer first), the Copilot status line (cmd.exe; this fix), install.ps1's last
  step (starts the import and AIQ home while the person watches; left). Hooks are started by the tools directly;
  the tip pop-up starts powershell.exe itself, not an AIQ exe.
- **Control (product):** `aiq-statusline.exe`, the same file across releases (U5n); upgrade.ps1 starts through
  Explorer (U5m).
- **Control (tests):** release.yml reuses the previous release's `aiq-statusline.exe` while its version is
  unchanged and fails when `crates/aiq-statusline` changed without a version bump;
  `crates/aiq/tests/statusline_contract.rs` fails if its paths or request differ from the core's.
- **Rule:** on Windows, an AIQ exe that a script host starts again and again must be a file that does not change
  with every release; anything else starts AIQ directly or through Explorer.


### DC-042 — A one-shot decision made from the first input, never revisited when a better input arrives
- **Signature:** 2026-11-08, Imran's laptop: a VS Code task opened with a 15-character prompt. Its kind label came
  back at 0.78 / 0.74, under the 0.8 bar, so the task ended at `label_unsure` and got no tip, although the next
  prompts of the same task said plainly what it was. The tip path read only the first prompt of a task.
- **Why it survives:** each step is right on its own input (a vague prompt should not be labelled with
  confidence), and every test fed a first prompt that already named the task. No test opened a task with
  "hi".
- **Sweep:** other first-prompt-only decisions: the word guess (runs once at the first prompt; a sure word guess
  needs no retry, and an unmatched one falls to the label, which now retries); the Right-now card (reads the live
  session each time, not one-shot); the label's `NOT_COACHED` question rule (a sure answer, not a vague one: left).
- **Control (product):** one second label try per task from the next prompt with at least 4 content words
  (`RETRY_MIN_WORDS`), at most two labels per task (`MAX_LABELS_PER_TASK`), counted as funnel step `label_retry`.
- **Control (tests):** `next_rung::a_hi_then_a_real_ask_labels_the_real_ask_once_and_the_tip_shows` (one tip, one
  decision, no third label), `next_rung::a_real_ask_sent_before_the_first_label_is_back_is_labelled_when_it_comes_back_unsure`,
  `next_rung::the_second_try_is_the_last_and_a_sure_first_label_never_retries`,
  `label::tests::a_second_try_labels_the_later_prompt_once_and_never_a_third_time`.
- **Rule:** a decision that may be made only once per item, from the item's first input, says what happens when
  that input is too thin to decide on, and gives the next input one bounded chance before the item is passed over.

### DC-043 — A rule that sets aside a whole unit because of a marker that is true of only some of its parts
- **Signature:** 2026-11-08 22:59Z, Imran's laptop, measured read-only. Home's Right now card read "Nothing in
  progress" and "Claude Code last activity Yesterday" while he typed to Claude Code all evening, on the laptop and
  from the phone app. Hooks ran (153 runs that day, all sent); the core set every event aside as rule RC
  (`event set aside reason=projects`). Since 11-03, 268 of 269 Claude Code prompts were set aside. Rule RC tested
  one marker on the session's head (the desktop app's `turnOrigin=system` record), and every desktop session has
  it. In the Thatcher session, 33 of 40 typed records were his own (`turnOrigin=human`, `projectsUserTurn=true`;
  29 from the laptop, 4 from iOS) and 7 were relayed briefs. Across his transcripts, 191 of 192 of his typed turns
  carried the marker.
- **Why it survives:** the rule was measured on 14 Projects sessions against terminal and headless ones, never
  against a desktop session typed by hand. Tests fed a head marker and no typed records, so "the whole session is
  Claude's" was the expected result. The card took its last-activity time from counted events only, so it called a
  busy tool idle and never showed the loss.
- **Sweep:** import's rule RC (`import::automated`, `"RC"`) still sets aside a whole imported session: same shape,
  not changed here (past sessions only; a follow-up). The folder rules (`template`, `burst`, `folder`) set aside a
  folder, but H ≥ 2 keeps a session where a person sends a second prompt, and the card now names their activity.
  The `aiq-check` rule is folder-only on purpose (AIQ's own test folders).
- **Control (product):** rule RC decides per turn. A prompt counts when its own transcript record is the person's
  (`projects::turn_at`, `is_own`); a relayed or peer record does not; a prompt whose record is not written yet waits
  and is decided at the next event (its Stop at the latest). A Stop goes with its turn's prompt. The core notes each
  tool's newest set-aside activity (`aside.<surface>`), and the card shows it as "automated, not counted" with the
  reason, so set-aside work is never shown as idle. `rc.turn` log lines give the per-turn verdict and read time.
- **Control (tests):** `a_persons_own_turns_in_a_remote_control_session_count_and_relayed_ones_do_not` (red on main:
  0 of 1 kept), `projects::tests::a_persons_typed_turn_is_told_from_a_relayed_one_by_its_own_record`,
  `set_aside_activity_reaches_the_card`, `rightnow.test.ts` set-aside case, e2e "a tool whose newest activity was set
  aside says so".
- **Sweep, second pass (2026-11-09, 829 transcripts):** sessions with no Projects marker were never read per turn,
  so 1 scheduled run (`turnOrigin=scheduled`, `scheduled-trigger`) and 14 desktop-app turns (`turnOrigin=sdk`) were
  counted as the person's. The per-turn rule now runs in every Claude Code session: only `turnOrigin=human` counts
  (reason `not-typed` otherwise). A missing record outside Projects sessions still counts after 10 s or at the Stop,
  so a slow write never loses a person's turn. Held prompts are re-read every 300 ms (`held_wait`, `recheck_held`):
  on 0.1.27 the first read missed the record 2 of 2 times. Nothing alarmed while 269 prompts were left out over five
  days: `aside.<surface>` now carries today's left-out count, and Home's card (every state) and `aiq status` say so
  when at least 10 and 90% of a tool's prompts today were left out (`AsideNow::mostly_left_out`).
- **Sweep, third pass (2026-11-09 14:12Z, laptop on 0.1.29):** a relayed brief queued during a busy turn ran the
  prompt hook but wrote no record of its own. `turn_at` took the newest typed record, which was Imran's record from
  the turn before, so the brief counted as his task. Each record now matches one prompt: the core keeps the records
  a session's recent prompts matched (`rc_matched`, out-of-order safe), and a record stamped before the hook wins
  over a later one (the core can read the transcript after the next turn is written). Test: step 5 of
  `a_persons_own_turns_in_a_remote_control_session_count_and_relayed_ones_do_not` (red on main: 4 kept, not 3) and
  the `taken` / "core read late" cases in `projects::tests`.
- **Sweep, fourth pass (2026-11-09 20:46Z, 848 transcripts):** a Remote Control session started later is handed an
  older project message again, as a person record (`turnOrigin=human`, `projectsUserTurn`). 3 pairs, always across
  sessions, up to 6.5 h apart and across a core update; one message gave 4 counted turns. No structured field links the
  two records (`turnAttributionKey` and `promptId` are each record's own); the only shared value is the wake envelope's
  `<message id="cmsg_…">`. `textNotTyped` is no signal: a genuine typed Projects turn carries it too. Each project
  message now counts once (`projects::delivered_id`, hashes kept in the `rc.delivered` setting, so a restart keeps
  them; a replay of the same event is not a second delivery). A held prompt past `TURN_WAIT_MS` is re-read every 5 s,
  not ~3 times a second (one brief: 2,256 reads in 12 minutes). Tests:
  `a_project_message_handed_to_a_second_session_counts_once` (red with the rule off),
  `a_held_prompt_past_its_wait_is_re_read_slowly`, `projects::tests::the_delivered_message_is_the_triggering_one…`.
- **Sweep, fifth pass (2026-11-10 14:43Z, laptop on 0.1.32):** a seeded session start counted Imran's message again
  and no `rc.delivered` row was written. The envelope's attributes are `trigger, from, trust, author-id, id, sent-at,
  mention`; `delivered_id` split on `id="` and took `author-id`'s value, which is not a `cmsg_` id, so it found none.
  The tests' envelope was written by hand with `id` before `author-id`, so they passed. Control: `delivered_id` matches
  the whole attribute name, and both tests use the measured attribute order. Lesson: a parser of a measured shape is
  tested against the measured shape (attribute order included), never a hand-written one.
- **Rule:** a rule that leaves out a person's activity must decide at the grain where the marker is true (a turn, not
  a session or a surface), and every left-out activity must still show where the person looks for it, with a count
  that alarms when most of it is left out.

### DC-044 — Explanatory copy that restates a number or a list the app computes elsewhere
- **Signature:** 2026-11-08, audit of the Home tour (`apps/home/src/tour.tsx`) against 0.1.27. The "Tasks, by kind"
  step said "its 14-day trend" after #202 moved the trend to 30 days against 30; the tips step said "Up to 5 a day"
  after tips a day became a setting (5/15/30/Custom) and named only Copilot CLI and VS Code though Claude Code shows
  tips too. Home's "Automated, not counted" section (#205) had no step, and the example data had no growing habit, so
  the habit-change line and chips #202 added never appeared in the tour.
- **Why it survives:** the tour's text is a typed copy of facts that live in the read model and Settings. The tests
  checked the step count and word length, not what the words claim, and the e2e only checked that each spotlight
  found its element.
- **Sweep:** the tour's nine steps (all rewritten). Other copy that restates a window already reads it from the
  snapshot (`Tasks` header `progress_days`, Tips `daily_cap`, Home `shift_days`). Not swept: the Rung guide and the
  landing page.
- **Control:** tour bodies are functions of the snapshot; `tour.test.ts` sets odd values (47, 23, 19) and fails if
  any step types a digit or misses one, and checks the tips step names every tool that shows tips. The e2e asserts the
  example data fills what each step names (a growing habit under the ladder map and on Habits, a set-aside folder
  with Count this folder).

### DC-045 — Two timers started together on the same period race for one lock, and the loser gives up
- **Signature:** 2026-11-09 02:52:28Z, laptop on 0.1.28 with 0.1.29 waiting: AIQ home's check (`check_if_due`) and
  its away install (`install_if_away`) ran from two `setInterval`s of 60 s started in the same mount, so they fire
  within milliseconds. When a check was due the two started 18 ms apart; the check took `update.lock`, the install
  logged "Another AIQ update is running. Nothing changed.", Home reopened, and the recorded try held the next one
  back 30 minutes. Measured by the alpha thread from upgrade-log.txt and home.log.
- **Why it survives:** each path is correct alone and each test runs one of them; the lock is correct; "gave up,
  nothing changed" looks like a designed hold, and the state stays `waiting` with no error.
- **Sweep:** other paths that start upgrade.ps1: the core's kicker (`Check -Apply`, no Home involved, giving up is
  harmless: it retries hourly), Install now and Go back (Home closed itself, so they now wait for the lock too), the
  Settings check (a check: giving up is harmless). Other paired Home timers: the snapshot refresh and the
  starting-core retry only read; neither starts a run.
- **Control (tests):** `update::tests::home_minute_starts_an_away_install_or_a_check_never_both` (one minute's ask
  starts one run), `updateNow.test.ts` minuteAsk (Home sends one ask a minute), release.yml smoke
  "AIQ-UPDATE-LOCK" (an install AIQ home closed itself for waits for a held lock and goes on; one that outlasts the
  wait reopens Home).
- **Rule:** work that takes a shared lock is started from one scheduler, and a run that already closed something
  the person had open waits for the lock (bounded) rather than giving up.

### DC-046 — A funnel stage counted from a different population than the stages above it
- **Signature:** 2026-11-09, Imran's Tips page: "0 of 6 new tasks had a tip ready this week" over bars Tip ready 0,
  Shown 1. New tasks → Kind known → Tip ready counted tasks from `tip_path` (recorded only since #184, 11-08);
  Shown and Opened counted every card first decided in the 7 days. The one shown card (11-03 15:19Z, Copilot CLI)
  belonged to a task with no `tip_path` row. Measured read-only on a copy of the laptop's aiq.db.
- **Why it survives:** each count was right on its own, the code comment even said "Shown can pass Tip found", and
  the fixtures had more ready tips than shown ones, so no test saw a bar grow.
- **Sweep:** `aiq status` prints the same funnel: it now names `tip shown` (same tasks) beside the all-tips counts.
  The drought check uses deliveries on purpose (any tip shown that day clears it). The progress ladder and habit
  counts don't stack stages.
- **Control (tests):** `funnel::tests::shown_and_opened_follow_the_same_tasks_so_they_never_pass_tip_ready` (the
  laptop case), `next_rung.rs` status test (the store links a shown tip to its task through `signal`),
  `funnel.test.ts` (every bar ≤ the one above), and the e2e checks the rendered bars never grow.
- **Rule:** in a funnel, each stage is a subset of the one above, drawn from the same rows; anything else is named
  on its own line, never as a bar.

### DC-047 — One setting offered by two screens, each with its own copy of the choices
- **Signature:** 2026-11-09, Imran's screenshot of first-run setup: "How many tips a day?" offered 1 / 3 / 5 (default)
  and "At most one per session, shown when the agent pauses" after the earlier change moved Settings to
  5 / 15 / 30 / Custom (Imran, 2026-11-07). Setup and Settings each held a literal list; the specs even recorded
  the split ("1, 3 or 5 at setup; 5, 15, 30 … in Settings") instead of flagging it.
- **Why it survives:** the change was asked for on one screen, and the other screen's tests pinned the old buttons
  ("3", "5 (default)"), so they stayed green. DC-044 covered copy that restates a computed fact, not a control.
- **Sweep:** first-run setup against Settings and the core, every step. Tips step: same picker now, rules line from
  the core's budget (one a session, 30-min quiet, 72 h same tip). Tools step: Copilot CLI tips also show in the
  conversation (`copilot_tip_in_chat`, default on). Analysis step already reads `judge_daily_tasks`; Privacy and
  Your history steps match current behaviour. Specs D3 and H9 updated. Not changed: the signed-off mockup
  (`docs/mockups/aiq-home.html`), a historical record.
- **Control:** `apps/home/src/tipsCap.tsx` is the only place the choices (`CAPS`, `MAX_CAP`) and the rules line
  (`TIP_RULES`) live; Settings and Setup both render `TipsCap`. The setup e2e asserts the setup group's buttons are
  exactly 5 / 15 / 30 / Custom, Custom sets 8, and the stale sentence is gone.
- **Rule:** a setting shown on more than one screen is one component with one list of choices; a second screen
  never types its own.

### DC-048 — A reader that renames and deletes a file another process still has open for append
- **Signature:** 2026-11-09, Windows CI on PR #219: `e2e_reword_three_times_one_card_second_run_held` stored 7 of 8
  prompts after 5 s; status showed `AIQ-HOOK-SPOOLED 1`, no "Queued while the core was down" line and no spool file.
  The hook had opened `spool.jsonl` for append; the core's drain renamed it to `spool.draining.jsonl`, read it
  (empty), and deleted it; the hook then wrote its frame into the deleted file. No bad line, no error, nothing left.
  Passed on re-run.
- **Why it survives:** each half is correct alone. Append is safe between writers, and rename-then-read is a safe
  snapshot, but only if no writer holds the file open across the rename. The window is the microseconds between
  the hook's open and its write, so it only shows on a slow shared runner. Measured on the laptop (0.1.31,
  11-03 to 11-09): 66 frames spooled, 67 lines drained, 12 of 12 timed frames drained within 13 s; none lost.
- **Sweep:** every `append(true)` and every `fs::rename` in `crates`. Only the spool is read-then-deleted; the logs
  (`logging.rs`, `paths::rotating_append`, the status-line log) rename to a kept `prev` file, so a late write lands
  in `prev` and survives. Store and judge appends are never renamed.
- **Control:** the hook writes one file per frame in `spool.d/`, under a temporary name, then renames it into
  place (`spool::append_frame`); the core takes only renamed `*.jsonl` files (`spool::take_frames`). Test
  `a_drain_between_the_hooks_open_and_write_loses_nothing` shows the old file losing the frame and the new one
  keeping it. The old single file is still drained, for frames an older hook wrote.
- **Rule:** a file that one process writes and another consumes is handed over whole by an atomic rename; the
  consumer never renames or deletes a file a writer may still hold open.

### DC-049 — A gate that needs two confidences when only one of them decides the outcome
- **Signature:** 2026-11-10, Imran: "something is clearly broken with the logic that determines when tips are
  shown." On a copy of the laptop's aiq.db, 0 of 15 new tasks since 11-08 reached a tip. 11 stopped at "label
  unsure", because the job and the kind both had to reach 0.8. Yet every habit is defined per job, so the kind only
  worded the tip. Sure jobs matched the analysis 17 of 21 times, sure jobs and kinds 9 of 13. The 2 tips that were
  ready were then dropped, because the row (gap on most tasks over 30 days) and the habit (Adopted over 14 days)
  were two definitions of whether the person already does it.
- **Why it survives:** each gate is right alone, and each was tuned on its own measured misses. The kind was in
  the row key, so it looked load-bearing. The tests pinned "both at 0.8", and no test asked what a tip needs.
- **Sweep:** every input to the tip decision against what it decides. The kind was needed for wording only, so it
  now words the tip and no longer stops it. The Adopted state of a habit now feeds the rows (`coach::adopted`),
  so the row and the live drop agree. The self-check answer key needed a sure analysis (`ANSWER_KEY_MIN`). The
  word guess keeps the kind: its neighbours are matched by kind, and the kind is its output. The accuracy pause
  keeps per-kind scopes.
- **Control (tests):** `a_sure_job_with_an_unsure_kind_still_gives_the_jobs_tip`,
  `an_adopted_habit_is_not_coached_and_the_next_move_is`,
  `a_job_row_coaches_across_kinds_too_few_to_coach_alone`, and
  `measured_labels_clear_the_bar_under_the_live_per_job_rule` (the real laptop labels).
- **Rule:** a gate holds only on the inputs its outcome depends on. One fact, such as "already does this", has one
  definition that every reader shares.


### DC-050 — A failure whose cause is written nowhere the person can hand over
- **Signature:** 2026-11-10, the devbox install: 15 of 15 new Copilot CLI tasks ended at "Kind label failed".
  `kind_label` stores only the outcome (`error`), and label.log wrote `label <task> copilot-cli error -/-` with no
  message. The tool's own words were dropped right after the call. The machine is not reachable from here, so the
  cause could not be found without asking the person to run commands.
- **Why it survives:** judge.log kept the message for analysis calls, so the gap looked closed. The label path was
  copied from the judge path without its log line. Tests checked the stored outcome, which was right.
- **Sweep:** every external call AIQ makes and where its failure text goes. Analysis calls go to judge.log (kept).
  Label calls now write `why <task> <message>` to label.log. Update checks keep `check_error` in the updater
  state. Hook errors go to hook-errors.log. A person can now hand all of these over in one scrubbed file
  (design diagnostics-file).
- **Control (tests):** `a_failed_call_writes_the_tools_own_words_to_label_log_and_not_to_the_store` and
  `a_failed_label_carries_the_tools_words_scrubbed`. The second proves the message reaches the diagnostics file,
  scrubbed.
- **Rule:** a failure that happens on someone else's machine is recorded with its cause, and the person can hand it
  over in one step, with nothing private in it.

### DC-051 — A record writer that gives up quietly when another process holds its file
- **Signature:** 2026-11-11, the laptop's 0.1.38 install. The check (04:52:50Z), the install (04:53:53Z) and later
  checks (05:09:01Z, 05:25:01Z) wrote `update.json` but not one line to `upgrade-log.txt`. Measured on the laptop: a
  Git Bash `tail -n 0 -F` on the log (our own watcher, started 04:51:35Z) held the file; an exclusive open failed
  with "being used by another process". `upgrade.ps1`'s `Log` tried 5 times, 100 ms apart, then dropped the line
  with no trace. `install.ps1` had the opposite fault: its `Add-Content` would have thrown and ended the install
  after the exes were copied.
- **Why it survives:** "never throws" was the control for DC-033's held log (0.1.15), and it was tested: the CI
  step held the log and checked the run finished. Nothing checked that the line survived. The state file is
  written another way (temp file, then rename), so the update looked fully recorded.
- **Sweep:** every append-only record a script or AIQ writes. `upgrade.ps1` and `install.ps1` now share the rule.
  The Rust logs (`aiq.log`, `label.log`, `judge.log`, `hook-errors.log`) open with Rust's default share mode and
  are appended by AIQ only; a held file there makes the write fail into `tracing`, unmeasured (no report so far).
- **Control (CI, release.yml):** with the log held, the `AIQ-UPDATE-NOT-NEWER` line must be in
  `upgrade-log-held.txt` with "could not be written: <reason>"; the next run must bring it back into
  `upgrade-log.txt` and remove the held file.
- **Rule:** a record that can't be written goes somewhere with its reason, never nowhere.

### DC-052 — File times copied from a zip, which keeps them without a time zone
- **Signature:** 2026-11-11, the laptop (UTC-8): 0.1.38's files in `.aiq\bin` said modified 11:43–11:49Z, 8 h
  in the future. Measured: the release zip stores DOS times 03:43–03:49 with no zone field (build runner in
  UTC); `Expand-Archive` reads them as local time, and `Copy-Item` keeps them.
- **Why it survives:** CI runs in UTC, where the two readings agree. Nothing compared an installed file's time.
- **Sweep:** `upgrade.ps1 -Apply` and `install.ps1` copy exes, `upgrade.ps1`, `aiq.vsix` and
  `aiq-statusline.exe`; each copy now sets the file's time to the install time. The `.old` copies keep the time
  they were installed with. Nothing in AIQ reads `bin` file times (checked: no `LastWriteTime`/`modified()` on
  them).
- **Control (CI, release.yml):** after install and after `-Apply`, `aiq.exe`, `aiq-home.exe` and `upgrade.ps1`
  must carry a time between the step's start and now. The zip was built minutes earlier, so a kept zip time fails.
- **Rule:** an installer stamps what it installs; it never trusts a time an archive carried.

### DC-053 — A view gains a section and the tour's example data never fills it
- **Signature:** 2026-11-11, audit of the Home tour against 0.1.39. Habits and tips gained "Tips this week" (#216,
  #219), but the tour's example store (`seed::tour`) never seeded tip steps, so on the tour the card read "No new
  tasks this week yet" with five zero bars right above a Tip history full of tips. The milestone note (#230) was
  said nowhere in the tour, and the Milestones intro left out the typical-rung rule the core counts (`MovedUp`), the
  very kind the tour's example shows.
- **Why it survives:** DC-044's control checks what each step's words claim and that each spotlight finds its
  element. A new section on a page the tour visits is neither: nothing asserted it carried data on the example.
- **Sweep:** every empty list in the old `tour.json`: `tip_funnel` (this one), `aside` and `starting` (Right now's
  idle and starting states, which the tour's open task replaces by design), `accuracy_paused` and `droughts`
  (shown only when something is wrong). Steps 1, 7 and 9 also read on the rendered tour. Not covered: Rung guide and
  Your data, which the tour does not visit.
- **Control:** the tips step spotlights Tips this week, and the e2e asserts its "Why tasks had no tip" list on the
  example, so the card can't go back to empty unnoticed.
- **Rule:** a section added to a page the tour visits extends `seed::tour` in the same PR.

### DC-054 — A throttled note never fires on a path or a day its tests never reach
- **Signature:** 2026-11-11, laptop on 0.1.39. Home's Right now card said "Claude Code last activity Yesterday 21:28 ·
  automated, not counted" at 14:22 PST, though this project's Remote Control sessions ran at 16:15Z and 22:22Z.
  `setting_change` had no `aside.claude-code` row from 05:28Z to 22:24Z, and the 22:24Z row said 0 left out today.
  Two causes. (1) `note_aside` resets `at` to `i64::MIN` on a new local day, then computes `now - at`: in the release
  build that wraps negative, so the 5-minute throttle skipped every note after local midnight until the core
  restarted. (2) A relayed brief whose record lands only by its Stop is dropped in `typed_gate`'s held loop with no
  `note_aside` and no "event set aside" line, so the brief that starts each Remote Control session never moved the
  time or the count.
- **Why it survives:** the tests never crossed local midnight with a warm `aside_noted` (a debug build would have
  panicked on the overflow), and the RC tests asserted counts, not the note, on the held-at-Stop path.
- **Sweep:** every `i64::MIN` sentinel in non-test code (only `note_aside`); every place `typed_gate` decides a held
  prompt (own → ingested; unknown outside Projects → ingested as own; everything else → now noted).
- **Control:** `set_aside_activity_is_noted_for_a_brief_decided_at_its_stop_and_after_midnight` fails on either
  hole, in debug and release.
- **Rule:** a sentinel time is subtracted with `saturating_sub`; every branch that leaves an event out notes it.

### DC-055 — A tool that can't run is filed as a task failure, and the task page can't say which
- **Signature:** 2026-11-11, second install (devbox) on 0.1.39. A Copilot CLI task page said "Not analyzed yet" more
  than 3 hours after the task ended. Its diagnostics file: 0 of 214 tasks ever analyzed in 14 days; 2,100 of 2,100
  analysis calls and 60 of 60 label calls failed in about 0.1 s, each answered "GitHub Copilot CLI was not found. Run
  copilot in a terminal to install it". The `copilot` first on that PATH is a stand-in, not the CLI that fires AIQ's
  hooks. Three causes. (1) `find_exe` took the first `copilot` on PATH and never checked it ran. (2) The reply was
  filed as a plain `error`, so Home never showed "Copilot CLI not found", and each task burnt its tries and was given
  up (133 of them). (3) The back-off was indexed by failed calls, and a try makes two, so the first wait was 6 hours,
  never the designed 1. The task page showed one sentence for queued, waiting out a retry, given up and blocked.
- **Why it survives:** the laptop has one `copilot` on PATH, so every measured call ran the real CLI; the queue tests
  built one failed call per try; no test asked what the task page says for a task that isn't analyzed.
- **Sweep:** every `find_exe` caller (Claude Code `claude`: no stand-in seen, left as is; Copilot: now
  `find_copilot`); every outcome the Copilot and Claude Code providers can return (only `missing` and
  `not_signed_in` block a tool, and now neither counts against a task); every reader of the back-off and give-up rules
  (the judge queue, Home's waiting count, the task page: now one `aiq_home::retries`).
- **Control:** `a_failed_try_waits_one_hour_and_a_missing_tool_never_gives_a_task_up`,
  `a_stand_in_copilot_that_says_not_found_is_missing`, `every_copilot_is_a_candidate_and_the_first_that_works_wins`,
  `a_failed_first_try_waits_one_hour_and_the_page_says_when_and_why`, `a_missing_tool_blocks_without_counting_against_the_task`,
  and the e2e "task detail: each analysis state says where the task stands" in three themes.

### DC-056 — Locating a tool by guessing where it lives, when the tool itself tells us
- **Signature:** 2026-11-12, devbox on 0.1.42 (with DC-055's fix). Home said "Analysis can't run: Copilot CLI not
  found" while Copilot CLI ran AIQ's hooks there all day; Copilot turns also showed "AIQ found no log for it". The
  DC-055 fix searched PATH plus guessed install folders (npm, WinGet) for a `copilot` that answers `--version`; the
  real one was in none of them. Every Copilot CLI hook already runs with `COPILOT_LOADER_PID` and `COPILOT_HOOK_DIR`
  set (laptop probe 2026-11-01), naming the exact program and home that work for the person.
- **Why it survives:** the laptop's only `copilot` is first on PATH, so any search order finds it; the DC-055 fix
  was written from the diagnostics file, which recorded the failure but not where the working Copilot was.
- **Sweep:** every place AIQ locates Copilot: analysis and label calls (`find_copilot`: now the hook's program
  first), session logs (`copilot_seen::log_home` for the core), imported history (`import::tool_home`: unchanged,
  `COPILOT_HOME` or `~/.copilot`; the diagnostics file now shows whether that home has session folders).
- **Control:** `a_copilot_cli_hook_records_the_program_and_home_that_run_it`, `copilot_seen` unit tests, and the
  diagnostics file's `copilot` block (hook_saw, on_path, log_home, session folders), which makes the next miss
  visible from one file.
- **Rule:** when the tool runs our code, ask that run where the tool is; search only as a fallback.

### DC-057 — A stored answer shown on one page and left off the page that explains it
- **Signature:** 2026-11-12, laptop. A task opened with "diagnose this error…" showed "Job: Ask or research" and no
  kind on its task page, so it read as unclassified and looked like a missing kind. The store had the kind twice:
  the label said Finding the cause of a problem at 0.99 and both analysis runs said it at 0.92 and 0.91. The Tasks
  list showed it ("Ask or research · Finding the cause of a problem"); the task page never rendered
  `task_type_label`.
- **Why it survives:** the task page was built before kinds existed, and the e2e fixture's task had no kind, so no
  test asked what the page shows when one is named.
- **Sweep:** every Home reader of `task_type_label`: Tasks list (shown), day view and rung mix (shown or "No kind
  yet"), task page (was missing, now a "Kind:" chip). Kind pages are keyed by kind, so they cannot miss it.
- **Control:** e2e "task detail: the kind shows beside the job when the analysis named one" (and no chip when none).
- **Rule:** when a field is added to a judgment, list every page that shows the judgment and check each one.

### DC-058 — Text a hook sends through the person's prompt counted as a typed turn
- **Signature:** 2026-11-12, laptop. A VS Code task showed 5 prompts; 2 began "tacit: before you stop…", sent by a
  Stop hook in the person's own repo to keep the agent going. Over the store, 25 such prompts in 2 sessions, each
  394 characters, 24 of them 1.3 to 4.6 s after the session's Stop. They counted as the person's turns, padded
  prompt counts and could start a task of their own.
- **Why it survives:** the per-turn "who typed it" rule (DC-043) reads Claude Code's transcript record; VS Code
  and Copilot CLI hooks name no transcript and their Stop says nothing of a hook (`stop_hook_active` false on all
  159 VS Code Stops), so nothing looked at those tools' turns.
- **Sweep:** every tool whose hook names no transcript (VS Code, Copilot CLI): now rule FAST. Claude Code keeps
  its record-based rule. Imported history: not covered (import reads the tools' own logs, which mark turns
  differently); left as a next step.
- **Control:** `a_long_prompt_just_after_a_turn_end_was_sent_by_a_hook` (domain bounds) and
  `a_long_prompt_a_turn_end_hook_sends_is_left_out_with_its_stop` (core: the hook prompt and its Stop left out;
  short, pasted and 5-second prompts kept).
- **Rule:** a turn counts as typed only on evidence a person could have typed it; time since the turn end is
  evidence when the tool gives no record.

### DC-059 — A copy of an event reaches the rules before the store drops it
- **Signature:** 2026-11-12, Windows CI on PR #248 (job 130972821379),
  `e2e_reword_three_times_one_card_second_run_held`: 7 of 8 prompts stored after 5 s, `AIQ-HOOK-SPOOLED 2`,
  prompt send p95 51.9 ms against a 20 ms budget. Passed on re-run. Same test and symptom as DC-048 on 11-09.
- **Cause:** a prompt hook whose send runs out spools the frame (DC-034 allows this: "ingest dedupes by event id"),
  but its send thread can still finish writing, so the core gets the event by socket and by spool. The store keeps
  one row, but only at `record`, after rule RC per turn (DC-043) had matched the copy against the transcript. The
  copy's own record was taken, so it took the next prompt's record (or waited and took it later). That prompt then
  found no record and waited 10 s (`TURN_WAIT_MS`), past the test's 5 s. On a real machine the prompt is late and
  held only in memory, and inside a Projects session a stolen record can decide the wrong person's turn.
  Reproduced in `a_prompt_delivered_twice_never_takes_the_next_prompts_transcript_record` (red: 2 of 3 stored).
- **Why it survives:** dedupe lived in one place, the store insert, and every rule added before it (RC per turn,
  rescue, held prompts) assumed it saw each event once. The spool tests replay duplicates, but with no transcript.
- **Measured:** 2 of 535 Windows test jobs since 11-02 lost a prompt this way (11-09, 11-12); 1 of 53 since the
  DC-048 fix (#222). The 11-09 case may have been this class rather than DC-048 (no core log was kept to tell).
- **Sweep:** every caller of `ingest_inner`: spool drain, socket ingest, `take_card`, held re-reads (`recheck_held`,
  `typed_gate`, RC release) and Copilot mid-turn messages. Held events leave `rc_held` before they are ingested
  again, so the check skips only true copies. Not covered: a copy of a first prompt the automation guard holds
  (`held`, not stored yet) still reaches the guard a second time; not measured, left as a next step.
- **Control:** `ingest_inner` drops an event already stored or already waiting, before any rule; the core test above.
- **Rule:** dedupe at the door, before any rule that keeps state, not only where the row is written.

### DC-060 — A long-lived thread keeps the store open, so Windows can't delete it
- **Signature:** 2026-11-12, PR #252 CI on Windows. `e2e_forget_all_deletes_store_and_spool` failed with
  "Could not delete everything: The process cannot access the file because it is being used by another process.
  (os error 32)". The new sync thread opened its own store connection at start and held it for the life of the
  core. `forget --all` stops the writer and then deletes the store, but nothing stopped the sync thread. Linux
  passed because it lets a program delete an open file.
- **Why it survives:** every other store reader in the core and Home opens a connection per call and drops it.
  A new thread followed the "own connection" note without the shutdown half. Only the Windows job runs the
  delete against a live core.
- **Sweep:** `Store::open`/`open_read_only` in `aiq-core`, `aiq-home` and the Home app. Only `sync::run` held a
  connection across calls; the rest are per call (`control.rs`, `update.rs`, `aiq-home` lib/detail/diagnostics).
- **Control (tests):** the Windows `e2e_forget_all_deletes_store_and_spool` (already there; it caught this), and
  `once_the_core_deletes_everything_the_sync_thread_never_opens_the_store_again` in `aiq-core/tests/sync.rs`. That
  test proves a stopped sync thread can't create the store again after the delete.
- **Rule:** a background thread opens the store only for one unit of work. Deleting everything first stops every
  thread that can open the store.
