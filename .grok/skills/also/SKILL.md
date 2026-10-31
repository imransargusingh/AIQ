---
name: also
description: Append a late addition to the prior prompt without derailing the work already in flight. Use when — mid-turn or just after — you think of something you meant to include — `/also <addition>` captures it now, lets the current reasoning and work finish undisturbed, and folds the addition in afterwards, either as refined context for the remaining work or as a task appended to the end of the current turn.
runs_as: either
---

# Skill: /also

A turn-control utility for the thing that happens to everyone: you send a prompt, the agent starts working, and *then* you remember the bit you meant to add. Adding it as a normal message mid-turn **derails the reasoning already under way** — the agent re-plans around the interruption and loses the thread. `/also <addition>` solves that: it says *"add this to the prior prompt, but don't stop what you're doing — consider it once the current reasoning and work are complete."* The addition is captured immediately, the in-flight work runs to its natural finish undisturbed, and only then is the addition folded in.

> **Where it sits.** Not a workflow — a **meta / turn-control** utility, the deliberate counterpart to a *stop* (`communication-and-task-discipline.md` CT21). A stop **ends** the current track; `/also` explicitly does **not** — it **extends** it, deferred. It composes with the two-step front matter (CT19–CT24): the addition is folded in against the turn's stated **goal state**, after the current work meets its **done-when**.

## Grounding (first action)
Identify the **work in flight**: the current turn's goal state (Goal / Done when / Not in scope / Tier / Fan-out cap / Main-line budget, CT19) and its active tasks (the session todo list, if one is in use); a compiled prompt in flight (CO-S0) *is* that goal state. This is what must **not** be disturbed. **Three cases, and the third is the one that bites:**

- **A goal state is in flight** — the addition inherits its bounds. Normal path.
- **Nothing is in flight** — `/also` arrived on a fresh turn, so it degrades to "treat this as an addition to the immediately prior prompt" and is handled normally, **writing the goal state as any turn would**.
- **Work is in flight with no goal state** — then there is nothing to inherit, and this is the measured failure: an addition to an unbounded turn *inherits unboundedness*. **Write the CT19 block for the combined remaining work before integrating** (step 4). Measured: one such `/also` turn became 81 main requests, 6 sub-agents, 83 minutes and 13,411 AIU — the session's most expensive turn (class **CTX-N**, `session-profile.py` SP-20).

## Input
The `<addition>` — the text the user wishes they had included in the prior prompt. It may be extra **context** (a constraint, a preference, a clarification) or an extra **task**.

## Flow

1. **Capture, don't pivot.** Record the `<addition>` **verbatim** as a *deferred addendum* attached to the current turn (a `pending` row in the session todo store, or an explicit "Addendum captured" note when no store is in use). Acknowledge in **one line**. Do **not** re-open, re-plan, or re-reason the current work now — the non-derailment is the entire point of the skill.
2. **Classify (note only, act later).** Decide which kind the addition is, and say so briefly:
   - **(a) Refined context** — it changes *how* the remaining or next work should be done (a constraint, a preference, a correction of scope). It will be **applied to work not yet finished**, not used to redo work already correct.
   - **(b) Appended task** — a new unit of work to perform **after** the current work completes.
3. **Continue the in-flight work, unchanged.** Run the current reasoning and tasks to their natural completion or checkpoint exactly as planned. The addendum waits.
4. **Integrate at the checkpoint.** When the current work meets its done-when: re-read the goal state (CT19), then fold the addendum in —
   - **(a)** apply the refined context to the remaining / next steps (and note anything already done that it would have changed, as a finding — do not silently redo settled work);
   - **(b)** append the task, plan just *that* added work with the turn's execution graph (`/optimize-graph` scoped to the addition, CT24 — not a whole re-plan), and complete it.
5. **Guard the magnitude, not only the direction.** The direction guard is below; this is the one that was missing. Before integrating, size the addition against the in-flight turn's **Tier**, **Fan-out cap** and **Main-line budget**. If its own plan exceeds any of them, **raise the tier explicitly and say so in one line** — *"this addition takes the turn from T0 to T1; fan-out cap 0 → 2"* — rather than absorbing it silently — and **recompile** (`/compile --from-audit <raw id>`, CO-S0): the addendum carries the new compiled prompt id and its `pending` row references it. A raise is a decision the user can see and stop; a silent absorption is how the measured turn above happened. If the addition deserves its own goal state, say that instead of growing this turn.

6. **Guard against derailment and silent scope inflation.** `/also` is the user **extending** the goal, so the addition is user-authored (it clears the CT20 "autonomy is latitude in the *how*, not the *what*" bar because *the user* set the new *what*). But if the addition **contradicts or materially reverses** the in-flight goal rather than extending it, do **not** silently pivot: bring the current work to a safe checkpoint, then **surface the conflict and the choice** rather than resolving it unilaterally. An extension is absorbed; a reversal is raised.
7. **Autopilot.** Under autopilot, integrate and complete the addendum after the current work without pausing — unless it opens a **consequential or irreversible** new direction, which is surfaced for a decision (the standing gate, Rules of the Road §2).

## Output
No committed artifact. The visible result is: a one-line capture acknowledgement, the current work finishing undisturbed, and the addendum handled at the end (its context applied, or its task done). Like `/auditlog` and `/prompts`, `/also` is **exempt from the Discoverability Mandate** (it creates no artifact — no frontmatter, no index sync). The captured addition is not itself logged; the substantive work it feeds logs per its own skill / the interactive-turn rule (AL5b).

## Definition of done (exit gate)
- [ ] The addition was **captured verbatim** and acknowledged in one line, **without re-planning** the work in flight.
- [ ] It was **classified** as refined context (a) or an appended task (b).
- [ ] The current reasoning and work **finished undisturbed** — no mid-stream derailment.
- [ ] The addendum was **integrated after** the current work: context applied to unfinished/next steps, or the task appended and completed.
- [ ] A genuine **contradiction** of the in-flight goal (not a mere extension) was **surfaced**, not silently actioned.

## Documentation & discoverability
None — `/also` is a reader/controller of the turn, not an artifact producer (exempt from V10, like the other utility skills).

**Handoff:** → back into the in-flight work (immediately); then → the addendum, handled once that work is complete.
