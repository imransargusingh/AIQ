---
applyTo: "**"
---
# Session Worktree Discipline

*Normative guidance for **where a session does its work**. A session isolates into its own git
worktree when another writer might share its checkout; a session may create more worktrees if the
work needs them; and no worktree is ever left behind. `coord-core.py` already keys coordination by
worktree and enforces one session per tree — this document makes isolation **the default response
to a concurrent-writer risk** rather than something you reach for after a collision, and it closes
the half nobody owns: cleanup.*

Normative keywords (**MUST**, **SHOULD**, **MAY**, **MUST NOT**) follow RFC 2119.

The governing idea: **two agents sharing one checkout is not a merge problem, it is a
correctness problem, and it is invisible while it is happening.** A shared working tree has one
index, one HEAD, one set of generated artifacts and one set of build outputs. When two sessions
touch it, `git stash` in one silently reaches into the other's uncommitted work, a regenerated
surface is attributed to whichever session ran `sync` last, and a test run measures a tree that
nobody authored. Nothing fails loudly; the evidence simply stops meaning what it says. The
countermeasure is not care — care does not scale across concurrent sessions and does not survive
a session that crashes mid-edit. The countermeasure is **isolation by default**.

The second half is the half that actually rots: an isolation mechanism people adopt and never
clean up becomes a disk full of half-finished trees, stale git metadata, and — worst — a tree
holding the only copy of some real work that everyone has forgotten. So cleanup is specified
here with the same force as creation, and it is **fail-safe**: it refuses to delete anything that
could still be carrying work.

---

## 0. When this applies

Any session that will **write** to a repository — a skill run, an interactive change, a
long-running agent task. It does not apply to a read-only question, a `--check` run, or a lookup;
creating a worktree to answer "what does this file say?" is the ceremony this pack exists to
avoid. Owner: the **Release / Deployment Engineer** (the path to production and back) with the
**SRE** (resource bounds and orphan accumulation) and **Security & Identity** (an irreversible
delete is a gated action).

---

## 1. The directive

**WT1 — Isolate on concurrent-writer risk, not on session-newness.** Create and enter an own
worktree, on its own branch, before writing — but only when another live agent/session or the
pack's coordination runner might write the same checkout. Staying anyway under that risk is the
exception (WT4); with no such risk, see WT1b.

**WT1b — No concurrent writer: work in place.** A fresh clone, a CI job, a benchmark cell, a
single-agent repo, or a prompt naming the checkout are cases with nothing else writing there —
isolating just moves the deliverable somewhere the requester never opens. Default; no reason
needed. Evidence: class **WT-D** (`docs/lessons/defect-classes.md`).

**WT1c — Done means back where the requester looks.** A change made in a worktree is not done
until it is **integrated into the checkout the requester named**, or handed back **explicitly as
not delivered**.

**WT1a — A new task starts in a new session, and a long session compacts before it turns.** A worktree isolates the *tree*; nothing isolates the *context*. A session that carries a pack refresh, a PR merge and then a product-pivot proposal in one conversation re-reads all three on every request — the profiled shape was a 23-hour session whose main context grew from 159k to 564k tokens across three unrelated tasks with **zero compactions**, because a long-context tier never forces one. So: when the task changes, start a new session (or compact deliberately) *in the same worktree*; the audit log, not the conversation, is how work compounds across sessions (AL5). A harness setting that lets the context grow without bound (`contextTier: long_context` in Copilot CLI, a 1M window in Claude Code) is a per-phase choice (GO19), never a global default — `pack-doctor.py` reports it. `session-profile.py` finding SP-01 is the recurrence signal; class **CTX-A**.

**WT2 — A session MAY create further worktrees when the work genuinely needs them.** Comparing
two revisions side by side, running a long test suite against a baseline while editing, bisecting,
or holding a release branch open are all legitimate. Every additional tree follows the same rules
as the first: its own branch, registered, and cleaned up. Isolation is cheap; *forgotten* isolation
is not.

**WT3 — One session per worktree, one worktree per session at a time.** `coord-core.py` already
enforces the first half (`COORD-WORKTREE-OCCUPIED`: *"two sessions in one tree is how work gets
lost"*). The second half is the discipline this document adds: a session does its writing in one
tree at a time so that "what did this session change?" has a single, answerable location.

**WT4 — Staying in a shared checkout under risk is a recorded exception.** Covers what WT1b does
not: a concurrency risk exists (WT1) but isolation is skipped — tooling that needs the primary
tree, or no worktrees available. Say which, and proceed; an unstated exception is
indistinguishable from having forgotten the rule.

**WT5 — Name the branch and the tree for the work, not for the session.** `feature/audit-duration`
tells the next person what is in there; `session-2026-08-22-a` tells them nothing and guarantees
the tree is never voluntarily cleaned up, because nobody can tell whether it matters. The tool
derives the tree's directory name from the branch for exactly this reason.

---

## 2. Cleanup — the half that rots

**WT6 — A session ends by releasing its worktree.** When the work is committed and pushed, the
session **MUST** either remove its worktree or state plainly that it is being kept and why. "I am
done" without either is how orphans are created, and the cost is paid by someone else, later,
with less context.

**WT7 — Cleanup is fail-safe: never delete anything that might still hold work.** A worktree is
**safe to remove** only when *all* of the following hold, checked in this order and reported
individually:

| Condition | Why it is a hard stop |
|---|---|
| It is **not** the primary checkout | The primary is the reference; removing it is never cleanup |
| It is **not** the current working directory | Deleting the floor you are standing on |
| The tree is **clean** — no modified, staged or untracked files | Untracked is the dangerous one: a new file nobody has committed exists nowhere else |
| It has **no commits absent from the integration branch** | Unmerged work is the only copy |
| Its branch has **0 commits not on the repository's default branch** (`git rev-list --count <default>..<branch>`), and the count is printed | A *pushed* branch passes the row above — every commit exists on its remote-tracking ref — so a frozen tree 21 commits ahead of `main` was labelled "merged" and offered for removal (DC-142). "Merged" means merged into the default branch, never "exists somewhere else"; `--include-unmerged` is the named escape for an abandoned branch |
| Its branch is **not checked out anywhere else** | Someone else is using it |
| Its session is **ended or stale** | A live session's tree is in use |

Anything failing a condition is **reported, never removed** — with the specific reason, so the
human can act. A cleanup tool that deletes on a heuristic will eventually delete the one tree
that mattered, and that single event ends the adoption of the whole practice.

**WT8 — Removal is opt-in, not the default verb.** `cleanup` **MUST** default to reporting a plan
and require an explicit flag to delete. Deleting a directory is irreversible and un-undoable by
git; the pack's standing rule for irreversible actions applies (Rules of the Road §2 — a gate
before, not an apology after). Two properties are part of the rule, because the pack's own
tool broke both: `cleanup` adjudicates **every worktree in the repository** unless narrowed —
that width is what makes the orphan sweep work, so it **MUST** state it in its output and
**MUST** offer `--path <tree>` for the narrower intent (**GO14a**: an action's enforced scope
must match its stated clause). And it **MUST** report what it **measured**, never what it
attempted: a tree that survived the attempt is never counted as removed, and one git
**de-registered but could not delete** is named as **orphaned** — no worktree command will
mention it again.

**WT9 — Prune the metadata as well as the directory.** Deleting a worktree directory by hand
leaves `.git/worktrees/<name>` behind, and git keeps treating that name as taken — so the next
`git worktree add` for the same name fails with a message that describes a state the filesystem
does not show. Cleanup **MUST** run `git worktree prune` so the administrative record matches
reality (E14: read the state back — an exit code is not a result).

**WT10 — Orphans are surfaced routinely, not discovered.** A count of worktrees, how long each has
been idle, and which are safe to remove **SHOULD** be reported at session start, when the count
exceeds a small threshold, or on demand. An orphan discovered by running out of disk is an orphan
that has already cost more than the check would have.

**WT11 — Never remove a worktree to resolve a conflict.** If two sessions have collided, the
remedy is to finish or land one of them, not to delete a tree — deletion at that moment is
maximally likely to destroy the only copy of the work that caused the collision.

---

## 3. The mechanics

The commands live in `coord-core.py`, which already resolves the primary checkout from any
worktree and keys its shared event log by worktree — so worktree lifecycle belongs there rather
than in a parallel tool (one owner per concern):

```bash
# start a session in its own tree (branch, session registration, the cd, and the BASE
# commit it used — which is the INVOKING tree's HEAD, not the primary's)
python3 docs/ai-forward-pack/scripts/coord-core.py worktree new --branch feature/audit-duration --session "<session-id>"

# what exists, who holds it, how long idle, what is safe to remove
python3 docs/ai-forward-pack/scripts/coord-core.py worktree list

# the plan (default: reports, deletes nothing)
python3 docs/ai-forward-pack/scripts/coord-core.py worktree cleanup

# act on the plan — EVERY worktree the plan marked safe, repository-wide
python3 docs/ai-forward-pack/scripts/coord-core.py worktree cleanup --remove

# act on ONE tree only
python3 docs/ai-forward-pack/scripts/coord-core.py worktree cleanup --path ../<repo>-<branch-slug> --remove
```

**WT12 — The tool reports its refusals, not just its actions.** `cleanup` prints every tree it
declined to remove and the condition that stopped it. A cleanup that silently skips is
indistinguishable from a cleanup that found nothing, and the difference is exactly the
information the human needs.

**The join carries the leader's epoch (S3).** When several sessions share one repository under a
designated leader, the join is run as `conductor-join.py --epoch <n>` with the epoch from
`coord leader who`; a lower epoch or an unread `refs/coord/leader` is refused before the merge
(exit 11), never merged and sorted out afterwards (`agent-coordination.md` CO-L).

**Coordinators: the tree comes before the first spawn.** A session that delegates and *then* enters a worktree strands its delegates — class **CTX-Q** in `docs/lessons/defect-classes.md` — so a Coordinator runs `coord worktree new` (WT1) before its first spawn, its brief tells each Sub-Agent it never calls `EnterWorktree`, and the join is fenced as `agent-coordination.md` **CO-L** states. This paragraph points at both; it restates neither.

---

## 4. Self-verification checklist

- [ ] A concurrent-writer risk was checked, not assumed from "this is a new session" (WT1): isolate
      if present, or record why not (WT4); work in place if absent (WT1b).
- [ ] If a worktree was used, the change reached the checkout the requester named, or the hand-back
      named it not delivered (WT1c).
- [ ] Additional worktrees, if any, follow the same lifecycle (WT2).
- [ ] The branch and directory are named for **the work** (WT5).
- [ ] At close, the worktree was **removed or explicitly kept with a reason** (WT6).
- [ ] Nothing was removed that was dirty, unmerged, current, primary, or held by a live
      session — and every refusal was **named** (WT7, WT12).
- [ ] Removal required an explicit flag; the default run only reported (WT8).
- [ ] The cleanup output stated its **scope**, and reported **removed / not removed / orphaned** as measured rather than attempted (WT8).
- [ ] `git worktree prune` ran so metadata matches the filesystem (WT9).
- [ ] Orphan state was surfaced rather than discovered (WT10).
- [ ] No worktree was deleted to settle a collision (WT11).

---

## 5. References

- **`coord-core.py`** — the existing owner of worktree identity (`_worktree_key`), one-session-
  per-tree occupancy (`COORD-WORKTREE-OCCUPIED`), and the primary-checkout resolution that makes
  one shared event log visible from every tree. WT1–WT12 extend it; they do not duplicate it.
- **`end-to-end-integrity.md`** — **E14** (read the state back) is why WT9 prunes rather than
  trusting a delete; **E13** is why WT12 reports refusals.
- **`agent-body-of-knowledge.md`** Part V.3 — code rolls back, state does not. An uncommitted file
  in a deleted worktree is the smallest possible instance of that rule.
- **Rules of the Road** §2 — an irreversible action is gated (WT8).
- **`continuous-improvement.md`** — the orphan-accumulation shape is a resource-leak sibling of
  class **RES-LEAK-TEST**: a durable resource created per unit of work with nothing destroying it,
  invisible on ephemeral CI runners and visible only on a long-lived developer machine.
