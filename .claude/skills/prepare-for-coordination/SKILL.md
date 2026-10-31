---
name: prepare-for-coordination
description: Turn the coordination layer on, then derive from the repo's own specs, architecture and artifact classes the optimal division of work across sessions or sub-agents - maximising parallelism while minimising contention - and emit the plan as a committed md + html pair.
runs_as: Coordinator
---

# Skill: /prepare-for-coordination

Produce the **coordination plan**: the document that says who owns what, what must stay serial, which artifacts need no coordination at all, and what each track costs. It is consumed two ways — by `/execute-with-coordination`, which spawns one sub-agent per track in its own worktree, or by a human spinning up distinct sessions and assigning each one a track, possibly across different harnesses.

**The finding that reorders everything: contention is a property of the artifact, not of the task.** The instinct is to divide by task and give each session a lane. The engine carries the counter-evidence beside its class registry: *the six busiest files in the reference repo are all generated, so a uniform lease aims at 13/60 and misses 58/60.* Most contention is not two people editing one document — it is two sessions regenerating one derived file. **Classification removes most of the problem and needs no coordination at all**, which is why Stage 1 comes before any division of work and is the highest-ratio thing in this skill. A plan that divides tasks over an unclassified repo has solved the fifth that was easy.

**Second: parallelism is a cost multiplier, not a saving** (GO6 — the production orchestrator-worker shape reports roughly 15× token usage). The lexicographic objective is **(1) completeness and rigor · (2) token cost · (3) speed**. So this skill's default answer is *fewer tracks than you asked for*, and it must say so when that is the honest answer. What worktrees genuinely buy is **isolation** (a long-running or destructive job out of the tree you are writing in), **long-running machine time**, and **context hygiene** (WT1a, class CTX-A) — none of which is speed.

**Spine:** the Rigor Protocol, weighted to **Stage 3 EVIDENCE** (the classes and the dependency edges are read from the repo, never recalled) and **Stage 4 DISCONFIRM** (the Simplifier deletes tracks; the Test Architect refuses a track with no exit evidence). **Authority:** `knowledge/session-worktree-discipline.md` (WT1–WT12), `knowledge/execution-graph-optimization.md` (GO5–GO7, GO16–GO19), `knowledge/end-to-end-integrity.md` (E7 surface list), `knowledge/communication-and-task-discipline.md` (CT19 goal state), `knowledge/domain-and-data-modelling.md` (the aggregate boundaries that make good track boundaries). **Mode:** Peer Mode to author, Adversary Mode at the gate. **Lead:** the **Orchestrator**, composing the **Enterprise Architect** (module and bounded-context boundaries), the **Tech Lead** (can the team hold this), the **Simplifier** (soft veto: delete every track that does not earn its multiplier) and the **Test Architect** (hard veto: a track with no exit evidence is not a track).

## Grounding (first action)
`audit-log.py start --session <id>` (IO1). Then read, in this order, and cite what you read:
1. `coord doctor` and `pack-doctor.py --json` — **measure the layer's state; never assume it**. An uninstalled layer reports "0 decisions, nothing claimed", which is indistinguishable from a working layer that saw no traffic (class **CTX-H**).
2. `docs/lessons/defect-classes.md` — the **CTX-\*** and **WT** classes.
3. The intent: `docs/specs/`, `docs/adr/`, `docs/architecture*`, `docs/bounded-contexts.yaml` if present, and the docs graph (`docs-graph.py`). **Traverse the graph; do not grep for structure** (V15).
4. `git worktree list` and `coord session list` — what already exists, and who holds it.

## Input
A scope: a spec, an architecture doc, a milestone, or prose describing the work to divide. Optional: `--tracks N` (a ceiling you want tested, not obeyed), `--harness claude|copilot|both`, `--horizon <phase>`. No input means: the whole of the repo's declared, not-yet-built intent.

## Cast
- **Peers:** Orchestrator (lead), Enterprise Architect (module/context boundaries), Data & Persistence Architect (the aggregate boundary is usually the right track boundary), SRE (long-running and destructive work — the case worktrees actually exist for).
- **Adversaries:** **The Simplifier** (soft veto — every track must justify the 15× multiplier; the default is one session), **Test Architect** (hard veto — a track with no exit evidence and no owner is not a track), **Tech Lead** (casting vote on track count).

## Flow

**Stage 0 — Interdict the rush.** Consume the compiled prompt when one is in hand (CO-S0, `knowledge/agent-coordination.md`; `/compile`): its goal state is the turn's goal state and its Not-in-scope is the interdiction — derive nothing from raw prose that a compiled prompt already fixed. Do not draw tracks yet. The first move is *classification*, and the second is reading the intent. A division of work drawn before the artifact classes are known will allocate humans to problems a merge driver solves for free.

**Stage 1 — Turn the layer on (do this first; it is the best ratio in the skill).**
```
python3 docs/ai-forward-pack/scripts/coord-core.py classify init
python3 docs/ai-forward-pack/scripts/coord-core.py install
python3 docs/ai-forward-pack/scripts/coord-core.py doctor      # read the state back (E14)
```
`classify init` writes `.agents/artifacts.yml` from what this repo has, **running every regenerate command before writing it**. A refusal is the control working: a *wrong* regenerate command resolves every merge silently and leaves the artifact permanently stale while reporting as handled. Then extend it by hand with this repo's own generated and append-only artifacts, **under the same rule — run the command first**. A `derived` entry needs a regenerate command; a `register` entry is append-only and union-merges; everything else stays `authored`, and you do not enumerate it. `install` is **per clone, in the primary checkout** — and *per clone is not per worktree*. A linked worktree shares `.git/config` and `.git/hooks` with its parent, so every tree this plan creates **inherits** the registration; running `coord install` inside one overwrites the repository's with a path that dies with the tree, and is refused. The plan says **install once, then `coord doctor` in each tree** to read the inherited state back.

**Stage 2 — Read the intent, end to end.** Build the **surface list** (E7) the work must reach: store → model → service → projection/wire → client type → UI → compute reader. Name the bounded contexts and the aggregates. This is the material the track boundaries are cut from — a track that splits an aggregate will generate seam requests forever.

**Stage 3 — Classify the contention (EVIDENCE).** For every artifact the work will touch, record its class and *why it is that class*. Then:
- `derived` and `register` → **no coordination needed**. Say so explicitly; this is the finding.
- `authored` → the only real contention. **One owner per file.**
**A plan row that names a control's trigger quotes the ADR line, never a paraphrase** (DC-189): a row restated an ADR's trigger tuple in its own words, the node built the row, and the gate it produced disagreed with the ADR that defined it. Copy the sentence and cite `ADR-NNNN §<gate>`; the paraphrase is where the width changes (GO14a).

Any file two tracks would both author is either a missing seam or a wrong boundary. Fix the boundary; do not schedule around it. One owner is not the whole check: for every shared surface, list each **other** track's guard over it and show it **jointly satisfiable** with what the owner may write, and require every scan-shaped guard to state its **root, recursion, token set and allowlist** — widening reddens at the join, narrowing stays green (GO14a).

**Stage 4 — Build the DAG (INTERROGATE).** Real dependencies only — delete incidental ordering. Apply **GO5 independence**: no data edge, **no decision edge**, no shared exclusive resource; all three, or the tracks are not independent. Apply the **coupling test**: tightly-coupled work is cheaper and more reliable in one coherent session. **Every edge points one way** — downstream rebases, upstream never asks. A cycle is a boundary error, not a scheduling problem.

**Stage 5 — Name the serial spine.** What must **not** be parallel, and why. The recurring answer: **until the interfaces are fixed, every track's result changes every other track's shape**, which fails GO5(b) outright. Also serial by default: any change to a shared schema or vocabulary; two sessions in the same layer; and the first commit after the class registry lands — verify the driver is effective before two sessions rely on it.

**Stage 6 — Cost it honestly.** State the multiplier. Per track, the reason it is worth paying: isolation, machine-time parallelism, context hygiene, or genuine independence. **If the honest answer is one session, say one session** — a plan that recommends fewer tracks than asked is the Simplifier working.

**Stage 7 — Assign.** Per track: **owner · authored paths owned · depends-on · seam requests expected · tier · fan-out cap · budget (tool calls, tokens, wall) · exit evidence · target harness · doorbell status (`pack-doctor`'s `doorbells` line) · deadline · fallback · termination condition**. On harness: name what the track *needs* from a delegation mechanism, then record what each available harness is **qualified** to do — `enforced` / `observed-only` / `unsupported` — from what you can actually verify here. **Never advertise a harness mode you have not exercised**; an unproven mechanism is `unsupported`. For S3 the first row of the order of operations is `coord leader pin <coordinator-session>` and every track row carries that epoch (CO-L). Before dispatch, refuse a compiled prompt whose `dispatchable` is false or whose text still carries an unanswered `DR-n` line — stop with `decision request unanswered: DR-n` (CO-S0). Every track row names **who rules**: the Owner session a track's `coord decide request --to <owner-session>` addresses, and the register it rules into (`docs/notes/rulings.md`, class `register`). A plan whose tracks can raise a decision request but name no Owner session has no termination variant for that request (D5, D6).

**Stage 8 — DISCONFIRM.** The Simplifier deletes every track that does not clear its multiplier and proposes the merged alternative. The Test Architect blocks any track with no exit evidence. The Tech Lead casts the deciding vote on count. Record struck tracks with their reason.

**Stage 9 — CONVERGE and emit.** Write **both** files, same content, one canonical:
- `docs/coordination/<plan-id>.md` — canonical, V2 frontmatter, **machine-readable**: `/execute-with-coordination` parses it. Use the schema below exactly.
- `docs/coordination/<plan-id>.html` — the readable view for the human assigning sessions. Self-contained, dependency-free, theme-aware (`knowledge/ui-design-craft.md`).

### Plan schema (the md is the contract — keep these headings and columns verbatim)

```markdown
---
id: coordination-<slug>
title: "Coordination plan - <scope>"
type: plan
status: proposed
owner: "@<owner>"
tags: [coordination, worktrees, parallelism]
links:
  - { to: <spec or architecture node>, rel: implements }
review-by: "<date>"
summary: >-
  <one sentence>
---

## Layer state
| check | result | meaning |            <- from `coord doctor`, measured, not assumed

## Artifact classes
| path / pattern | class | mechanism | coordination needed |

## Tracks
| track | owns (authored) | depends on | tier | fan-out cap | budget | exit evidence | harness |

## Serial spine
| item | why it cannot be parallel | who owns it |

## Seams
| from -> to | the request | resolved by |

## Struck tracks
| track | why it was not worth its multiplier |

## Order of operations
| # | action | cost | why now |
```

Close with the status table (Completed / Remaining / Best next action).

## Definition of done (exit gate)
- [ ] `coord doctor` was **run** and its output is in the plan — the layer's state is measured, not assumed.
- [ ] `.agents/artifacts.yml` exists, every `derived` command was executed before it was written, and `coord doctor` reads back clean.
- [ ] Every artifact the work touches carries a class and a reason; `derived`/`register` are explicitly marked *no coordination needed*.
- [ ] Every shared surface carries each other track's guard over it, shown **jointly satisfiable** with what its owner may write, and every scan-shaped guard states its root, recursion, token set and allowlist (GO14a).
- [ ] Every row that names a control's trigger **quotes** the ADR line with its citation; no trigger tuple is restated (DC-189).
- [ ] Every dependency edge points one way; no cycles; incidental ordering deleted.
- [ ] The serial spine is named, with the reason each item fails GO5.
- [ ] Every track has an owner, owned authored paths, a tier, a fan-out cap, a budget and **exit evidence** — the Test Architect's veto is cleared by a reviewer, not the author.
- [ ] The 15× multiplier is stated and each track's justification names isolation, machine time, context hygiene, or genuine independence.
- [ ] Struck tracks are listed with reasons.
- [ ] Harness capability is recorded per track as enforced / observed-only / unsupported, from what was verified here.
- [ ] The plan says the layer is installed **once, in the primary checkout**, and that each worktree **inherits** it — never that a worktree needs one of its own.
- [ ] Both `.md` and `.html` are written and carry the same content; the md follows the schema above.
- [ ] Status table emitted.

## Documentation & discoverability (last action)
The plan carries V2 frontmatter and links to the spec or architecture node it implements. Run `python3 docs/ai-forward-pack/scripts/docs-graph.py derive`.

**Audit (last action).** `python3 docs/ai-forward-pack/scripts/audit-log.py append --shortname "coordination-<slug>" --session "<id>" --skill prepare-for-coordination --kind skill --prompt "<verbatim>" --summary "<tracks, serial spine, struck>" --artifact docs/coordination/<plan-id>.md --goal "<goal>" --done-when "<done when>" --tier T1 --fan-out 0`.

**Handoff:** → `/execute-with-coordination` (runs the plan) · → `/optimize-graph` (a single track's internal shape) · → `/session-profiler` (measure whether the division actually paid).
