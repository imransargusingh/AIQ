---
id: proof-claude-code-skeleton
title: "Proof Pack: Phase 1 walking skeleton on Claude Code"
type: proof-pack
status: draft
owner: "Imran"
phase: "1"
tags: [proof-pack, collector, core, coaching, claude-code]
links:
  - { to: design-claude-code-skeleton, rel: implements }
  - { to: spec-coaching-loop, rel: implements }
  - { to: note-20261031-encryption-deferred, rel: relates-to }
review-by: 2027-01-31
summary: >-
  Evidence for the first end-to-end slice: the `aiq` binary (hook client, core, CLI) turns three rewordings
  of one request in Claude Code into one card at the next Stop, holds a second card in the same session,
  and fails open within budget when the core is down. Linux measured; Windows CI and the laptop run pending.
---

# Proof Pack: Phase 1 walking skeleton on Claude Code

- **Change:** branch `claude/project-thread-j3myua` (this PR)
- **Spec / design:** [coaching-loop](../specs/coaching-loop.md) · [claude-code-skeleton](../design/claude-code-skeleton.md)
- **Tier:** T2 (new trust boundaries, persistence, concurrency)
- **Author / date:** Claude via `/implement`, owner Imran, 2026-10-31
- **Run of record:** Linux container, Rust 1.97.0, `cargo test --workspace` = **134 passed, 0 failed**
  (run 3× for the suite, 8× for the e2e file after one flake was fixed; see DC-002). Windows: clippy
  clean via a cross-target check; build, tests and the timing run on Windows are **not yet observed**.

## Claims & evidence

### C1. Rewording a request three times gives one card at the next Stop; the card never quotes the prompt
- **Evidence:** `e2e_reword_three_times_one_card_second_run_held` drives the real binary with Claude Code
  payloads (4 prompts, then Stop) against a real core; Stop prints exactly one
  `{"systemMessage": "AIQ tip: you've reworded this request 3 times without adding new details.\n…"}`.
  Manual smoke run printed the same line (Linux, debug build).
- **Oracle:** asserts the card's first line, `Try this:`, `aiq pause`, and that none of the four prompt
  texts appear in it. Domain tests pin the threshold: `observe_two_reprompts_no_signal` (2 → none),
  `observe_three_reprompts_one_signal_with_evidence` (3 → one signal, evidence = 4 event ids).
- **Red observed before green:** yes for the detector: `observe_cue_only_run_scores_half` failed on the first
  implementation (a repeated "try again" counted as similar), fixed by keeping cue-only prompts out of the
  request window (DC-001). Mutation testing (below) shows the detector tests fail when the logic is wrong.
- **Confidence:** Verified (Linux).
- **Residual risk:** detector thresholds (0.6 / 4 words / 3 / 0.75) are uncalibrated pack data (design
  `assume:`); real-data week decides.

### C2. A second qualifying run in the same session is held, also across a core restart
- **Evidence:** same e2e test: second run's Stop prints nothing; `aiq status` shows `1 shown (ok)` and
  `1 hold (session_budget)`. `restart_mid_session_budget_survives`, `second_ready_card_before_stop_held_one_at_a_time`.
- **Oracle:** budget is derived from `card_delivery` rows, and `second_delivery_same_session_conflicts_even_bypassing_loop`
  proves the unique index rejects a second delivery even without the decision loop.
  `e2e_concurrent_stops_deliver_once`: 8 parallel Stop hooks → at most one card, and never a second later.
- **Red observed:** not separately (tests written with the code); mutation run kills every `trigger.rs` mutant.
- **Confidence:** Verified.
- **Residual risk:** none known for one machine. Cross-surface sessions are Phase 2.

### C3. The hook fails open within budget and never changes Claude Code's behaviour
- **Evidence:** `e2e_core_down_stop_exits_zero_within_50ms_p95_and_spools` (50 Stop runs, core down: exit 0,
  no output, 50 spool lines, no token in the spool). Release build, Linux, whole process, 50 runs each:
  **core down p50 2.2 / p95 3.0 ms; core up p50 2.8 / p95 3.8 ms** (budget 50 ms).
  `e2e_random_bytes_never_print_always_exit_zero` (0 B to 300 KB of random bytes, both events: exit 0,
  empty stdout and stderr). Every Stop output is checked by `assert_hook_stdout_contract` (empty, or one
  line that is an object with exactly the `systemMessage` key). `UserPromptSubmit` output is asserted empty
  on every call in the e2e suite.
- **Oracle:** the stdout contract would fail on any `additionalContext`, `decision`, `continue` or stray
  print; the timing assertion fails above 50 ms p95.
- **Red observed:** the random-bytes test went red (broken pipe) when stdin was larger than the 256 KB cap;
  fixed by draining the rest of stdin (DC-002).
- **Confidence:** Verified on Linux. **Windows: Inferred** from spike S3a (26–34 ms p95, core up) until the
  laptop run; the shipped exe is larger than the spike's (4.2 MB on Linux release) and process start is most
  of the Windows cost.
- **Residual risk:** Windows Defender scanning of a larger exe; the Windows CI job widens the budget to 200 ms
  (shared runners), so the laptop run is the Windows measurement of record.

### C4. At-least-once delivery with idempotent ingest (architecture gate: spool-replay property test)
- **Evidence:** `spool_replay_any_interleaving_duplication_truncation_same_result` (proptest, 48 cases):
  any shuffle, duplication and partial-line corruption of the spool gives the same event rows, signal count
  and decisions as one clean delivery. `spool_replayed_twice_one_row_one_signal`, `insert_event_replay_stores_one_row`.
  `e2e_core_down_prompt_starts_core_once_core_survives_and_drains_spool`: a prompt hook with the core down
  starts a detached core, which survives the hook and drains the spool.
- **Oracle:** compares full outcomes, not just "no panic"; drain sorts by `event_id` and de-duplicates.
- **Confidence:** Verified (Linux). Windows detached start: Inferred from S3b.
- **Residual risk:** a spool over 1 MB drops oldest lines (`Spool_OverLimit` → `append_over_limit_drops_oldest_keeps_newest`).

### C5. Nothing is captured or shown while paused; the person can delete everything
- **Evidence:** `e2e_paused_nothing_captured_no_card`, `paused_nothing_captured` (service),
  `e2e_forget_all_deletes_store_and_spool`, `e2e_forget_all_needs_confirmation`.
- **Oracle:** asserts zero stored events while paused and that the store and spool files are gone.
- **Confidence:** Verified.

### C6. The store keeps history as append-only facts and derives current state
- **Evidence:** `fact_tables_reject_update_and_delete` (6 tables × update/delete → `AIQ-STORE-APPEND-ONLY`),
  `content_allows_delete_forbids_update`, `failing_migration_leaves_version_and_data_unchanged`,
  `rebuild_equals_live_working_set`, `rebuild_after_signal_does_not_fire_again`, `ready_card_expires_after_30_minutes`.
- **Confidence:** Verified.

### C7. The daily cap holds for any clock or time-zone movement
- **Evidence:** `cap_never_exceeded_for_any_clock_moves` (proptest over random clock jumps and offsets
  −12 h…+14 h), `timezone_move_back_does_not_reopen_used_day`, `daily_cap_reached_holds_then_resets_next_local_day`,
  `daily_cap_zero_never_shows`.
- **Red observed:** the first rule considered (count deliveries on an "effective day") was shown wrong by
  reasoning before code (unbounded deliveries while the clock is behind); the property test encodes the
  invariant that rule breaks (DC-003).
- **Confidence:** Verified.

### C8. Local boundaries are guarded (STRIDE B1, B2, B4, B5)
- **Evidence:** `e2e_wrong_token_frame_closed_nothing_stored`, `authorize_wrong_or_missing_token_counted_no_request`,
  `channel_full_drops_ingest_answers_take_card_null`, `read_line_capped_rejects_over_long_frame`,
  `store_file_is_owner_only`, `spool_file_is_owner_only`, `render_show_prompt_with_ansi_prints_sanitised`,
  `e2e_logs_never_contain_prompt_text` (canary), `e2e_install_uninstall_round_trip_byte_identical`
  (other hooks kept, exec form with absolute `command` + `args`, never writes the working directory),
  `e2e_install_invalid_json_refused_untouched`, `e2e_uninstall_after_user_edit_strips_only_aiq`,
  `e2e_copilot_shaped_payload_dropped_and_counted`.
- **Confidence:** Verified on Linux. Windows pipe DACL: Verified for the same user by spike S1; the code path
  (`ipc::listen`, SDDL `D:P(A;;GA;;;<sid>)`) compiles for Windows but has **not run** yet.
- **Residual risk:** no peer-uid check on the Unix socket (0700 dir + token instead). Cross-account denial on
  Windows is inferred from the DACL.

### C9. Behaviour is measurable by default
- **Evidence:** `aiq status` shows tips today vs cap, events by kind, signals, cards by state and reason, store
  size, hook p50/p95 per event (in-process), spool backlog, hook and core error counters. Decisions are logged
  as JSON with `trace_id` (`e2e_logs_never_contain_prompt_text` asserts both).
- **Confidence:** Verified.
- **Gap:** the hook log records in-process time; whole-process time comes from the e2e test and the laptop run.

## Mutation testing
`cargo mutants -p aiq-domain`: first run 141 mutants, **21 missed** (getters, `FakeClock`, window bound,
idle-gap subtraction masked by small test timestamps, open-task count, bare `@`/`/`, reason strings).
Six tests added. Second run: **133 caught, 7 unviable, 1 timeout (an infinite-loop mutant, counted as
caught), 0 missed.** Not run on store/core/bin (integration-heavy; their failure paths are covered by the
tests named above).

## Test coverage of the boundary set (design D1)

| Boundary | Test |
|---|---|
| 2 vs 3 re-prompts | `observe_two_reprompts_no_signal`, `observe_three_reprompts_one_signal_with_evidence` |
| Jaccard 0.59 / 0.60 | `observe_jaccard_boundary_060_counts_059_does_not` |
| 4 vs 5 new words | `observe_four_new_words_still_reprompt_five_is_new_info` |
| New file mention / pasted block reset | `observe_new_file_mention_resets`, `observe_pasted_block_resets`, `observe_known_file_mention_is_not_new` |
| Cue-only run | `observe_cue_only_run_scores_half`, `observe_mixed_similar_and_cue_score` |
| Empty prompt | `observe_empty_prompt_ignored` |
| 10,000 prompts in one Task | `observe_ten_thousand_prompts_one_task_bounded` |
| Every trigger-table row, cap 0, midnight | `decide_*` (9 tests), `daily_cap_*`, `cap_never_exceeded_for_any_clock_moves` |
| Malformed / hostile stdin and frames | `e2e_random_bytes_*`, `authorize_*`, `read_line_capped_*` |
| Oversized prompt | `to_event_oversized_prompt_truncates_to_64kb` |
| Payload drift (missing `prompt`) | `to_event_missing_prompt_keeps_event_without_text`, `ingest_missing_prompt_text_counted_no_detection` |
| Concurrency | `e2e_concurrent_stops_deliver_once`, single writer + unique index |

**Not covered (flagged):** disk-full integration test (design listed it; not built); a second OS user
attempting to connect (no second account in CI); golden payloads recorded from the Windows laptop (payload
shapes in the tests follow spike S4's recorded key lists, written by hand).

## Change surfaces (E7) and writer / compute reader per field (E8)

| Surface | Done | Writer → compute reader |
|---|---|---|
| Store | `schema_v1.sql` | `event`, `content` → core ingest → detector rebuild, `aiq show` |
| Domain | `aiq-domain` | `signal` → detector → trigger engine, `aiq show`, `aiq status` |
| Service | `aiq-core::service` | `card_decision` → trigger engine / expiry → take-card, `aiq show`, `aiq status` |
| Wire | `aiq.event.v1`, frames, `aiq.card.v1`, `{"systemMessage"}` | `card_delivery` → take-card → budget, `aiq status` |
| Client type | `aiq-contracts` (one crate, both ends) | `setting_change` → `aiq pause/resume/config` → trigger engine, ingest |
| UI | card text; `aiq status` / `show` | `payload_meta` → ingest → **no compute reader yet** beyond being stored (Flagged: `aiq status` reads counters, not meta) |

## Deviations
Listed in the design under "Implementation notes (2026-10-31)": encryption deferred (owner decision),
Task derived once in the domain (no SQL view), `decision_id` key, cue-only window rule, cap rule,
spool sort/dedup, core-side `wait_ms`, hook timing log, frames schema / peer-uid / SIGTERM drain not built.

## Gate record
`GATE implement · 2026-10-31 · Rust Developer ⇄ Test Architect (author: Claude, one session; not independent) · verdict: pass on Linux, Windows pending`
- **Test Architect (hard):** triggered directives D0–D4, D6, D7 applied; D7's `TestKey` is moot (no key);
  D3 enforced by `domain_dependencies_only_contracts`. Open: disk-full test, Windows run. Not cleared by the
  author for Windows.
- **SRE:** stable codes for every failure path; status answers how long / how often / how big / did it fail.
- **Security & Identity:** boundaries B1–B5 have negative tests on Linux; Windows pipe code unexecuted.
- **Privacy:** encryption at rest deferred and recorded as an accepted risk (owner).

| | |
|---|---|
| **Completed** | `aiq` binary: hook client, core, CLI (install, uninstall, status, show, pause, resume, config, forget); store v1; re-prompt detector; trigger engine; spool; CI workflow |
| **Remaining** | Windows CI green; laptop dogfood (install, real Claude Code reword ×3, `aiq status`, Stop p95 ≤ 50 ms); disk-full test; Phases 2–6 |
| **Best next action** | Run CI, then the Windows human check on Imran's laptop with the CI-built `aiq.exe` |
