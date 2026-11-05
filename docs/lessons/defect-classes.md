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

### DC-029 — A coverage test that checks a step has some move, not that each failing fact has one
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
