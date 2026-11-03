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
  tool family, twins within 10 s with the same text are one prompt, the rest are the union. Tests
  `a_session_captured_live_and_imported_is_one_task` and
  `a_session_held_live_and_imported_is_judged_once_with_every_prompt` (red without the fix). Rule: when a
  second capture path writes the same thing, the one read model that builds the unit joins the paths before
  counting; never count per path.
