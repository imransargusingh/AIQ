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
