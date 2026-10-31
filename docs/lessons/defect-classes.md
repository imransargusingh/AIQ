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

**Status counts:** controlled 3 · partially-controlled 0 · uncontrolled 0

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
