---
id: session-contract-<workstream>
title: "Session contract - <workstream>"
type: doc
status: draft
owner: "@<owner>"
tags: [collaboration, contracts, ownership, worktrees]
links:
  - { to: architecture, rel: relates-to }
review-by: "<yyyy-mm-dd>"
summary: >-
  Agreement for concurrent sessions working one repository: roles, seams, file ownership, shared
  artifacts, change protocol, and merge policy. Claims are advisory unless enforced by hooks,
  pre-commit, merge drivers, or another fenced resource.
---

# Session contract - <workstream>

Two or more sessions are working this repository at once, in separate worktrees.

| Session | Agent/tool | Worktree/branch | Accountable for |
|---|---|---|---|
| <core> | <Claude Code / Copilot / other> | `<path>` / `<branch>` | <responsibility> |
| <design> | <Claude Code / Copilot / other> | `<path>` / `<branch>` | <responsibility> |

This document is the readable agreement. The enforcement floor is still `coord`: sessions register
with `coord session start`, claims are recorded with `coord claim`, and derived/register artifacts use
their configured merge drivers.

---

## 1. The seam

State the one sentence that divides the work.

```text
<producer> -> <contract> -> <consumer>
```

Example: `Core produces view models; Design renders them.`

Neither side reaches across the seam. If one side needs a value or behavior the contract does not
carry, it requests an additive contract change instead of re-deriving or guessing.

---

## 2. File ownership

Ownership means: **you edit it; the other session proposes changes to it.** Reading is always allowed.

### <Session/role A> owns

| Path | Why |
|---|---|
| `<path-or-glob>` | <reason> |

### <Session/role B> owns

| Path | Why |
|---|---|
| `<path-or-glob>` | <reason> |

### Shared, and therefore rule-bound

| Path | Rule |
|---|---|
| `docs/audit/*.jsonl` | Append only, through `audit-log.py`; preserve both sides and re-issue colliding unpublished ids. |
| `docs/docs-index.js` | Derived; never hand-merge. Regenerate with `docs-graph.py derive`. |
| `<shared-path>` | <rule> |

---

## 3. Contracts

| Contract | Shape | Producer | Consumer | Change rule |
|---|---|---|---|---|
| `<contract>` | <record/interface/artifact shape> | <role> | <role> | Additive first; removals/renames require agreement. |

**Invariants neither session may break alone:**

1. <Invariant + test/control>
2. <Invariant + test/control>

---

## 4. Changing the seam

1. **Write it down first** - amend this contract in the same commit as the seam change.
2. **Make it additive where possible** - optional fields/defaults before renames/removals.
3. **Land it before depending on it** - two branches assuming each other's unmerged changes turn a merge into a rewrite.

---

## 5. Reducing merge pain

- Register every writing session: `coord session start`.
- Claim files before editing: `coord claim --wi <work-item> --path <path-or-glob>`.
- Rebase or fast-forward from `origin/main` before each stretch of work.
- Land small. A day-long branch touching twenty files is not isolation; it is a merge deferred.
- Never hand-merge derived files. Regenerate them after the merge.
- For append-only JSONL, union by content and re-issue colliding unpublished ids; never dedupe by id alone.
- Announce a restructure before starting it.

---

## 6. Open requests

| Request | From | To | Why | Contract/artifact | Status |
|---|---|---|---|---|---|
| <request> | <role> | <role> | <reason> | `<contract>` | proposed |

---

## 7. Session responses

### <Session/role> response - <date>

Accept / amend / reject the contract here. List current claims so another session can read intent
without depending on chat history.

| Claimed path | Work item | Status |
|---|---|---|
| `<path-or-glob>` | `<wi>` | active |
