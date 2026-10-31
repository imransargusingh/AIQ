# Local delivery checkpoints

Owner: @ahutanu. Use a **verified Python ≥3.10** interpreter, or `uv run --no-config --no-project --python ">=3.10" docs/ai-forward-pack/scripts/delivery.py --help`; source contributors use `pack/scripts/delivery.py`. Verify a native interpreter's `--version` before use; do not assume `python3` exists or is new enough. The uv form isolates invocation from the target project's dependencies/environment. This stdlib helper only selects stages and checks routing/checkpoint integrity. It never runs skills, arbitrary shell, models, releases or host permissions, and never certifies semantic correctness or authenticates a human.

## Recover the first question before work starts

If grounding discovers a consequential decision before a dispatchable contract exists,
create the recovery pointer **before asking**. Reuse the original raw audit entry and
its nondispatchable draft compilation; do not manufacture an accepted contract:

```text
delivery.py prestart --task <id> --audit-root <audit-parent> --compiled-id <draft-al-id> --question <exact-question> --evidence <question-source>
delivery.py status --task <id>
```

Carry the original `--repo` and `--state-root` choices exactly as for ordinary
checkpoints. Status reports `phase: prestart`, the original raw request, draft,
question and evidence; no stage is completed and no approval is assumed. Give that
same task id in the stop message. The agent owns these existing audit pointers and
local paths, not the customer.

A fresh `/deliver resume <id>` reads status and the original raw/draft records first.
Inspect the actual scoped human answer and preserve its original evidence. Use the
existing compiler to finish a **new** settled compilation linked to the same raw id,
keeping earlier audit entries unchanged. A human may explicitly revise criteria at that
question; retain both versions and the bound original answer rather than silently
weakening the contract. Then convert this same task:

```text
delivery.py start --task <id> --facts <facts.json> --audit-root <audit-parent> --compiled-id <settled-al-id> --decision-evidence <original-answer-source>
```

The recovery record remains attached to the accepted task. If the actual human answer
**explicitly** changes completion conditions or exclusions, preserve that original answer
and author a `--scope-change <change.json>` receipt. Bind it to the same task, raw id,
draft compilation and exact question, with `before` and `after` criteria and the original
answer evidence; pass it with `start --decision-evidence`. Record `source: human-message`,
`decision: approved` and the actual actor only after checking the original source's meaning
and authority. The receipt contains these exact fields:

```json
{"task":"<same-task>","raw_id":"<original-raw-id>","draft_id":"<draft-compilation-id>","question":"<exact-question>","before":{"done_when":["<original-criterion>"],"not_in_scope":["<original-exclusion>"]},"after":{"done_when":["<settled-criterion>"],"not_in_scope":["<settled-exclusion>"]},"source":"human-message","actor":"<actual-human>","evidence":"<original-answer-source>","decision":"approved"}
```

Pass this file with `start --scope-change <change.json>` and include its original
answer in `--decision-evidence`. The helper checks the exact before/after binding
and rechecks the captured receipt/answer evidence on reuse, not human authenticity.
This is an explicit same-task reconciliation, not a blanket permission to
replace criteria merely because an evidence file is nonempty. Keep the original raw/draft
history alongside the accepted settled contract. Missing, denied, mismatched or changed
receipts refuse. An unchanged-scope answer needs no scope-change receipt.

A different raw request,
missing/changed evidence or drift does not become permission to reset the pointer or
reuse old approvals. An answer source is not authenticated by this helper: compare
its meaning and authority against the original question before conversion. Do not
flip `dispatchable`, rerun valid completed work, or send the customer to `/compile`.
Ordinary `start` still accepts an already settled contract directly; only a pending
pre-start pointer needs decision evidence. Accepted tasks cannot be overwritten.

## Start and continue

Use a lowercase task slug. Preserve raw and compiled prompts through the existing compiler/audit scripts. `start` takes a **finished dispatchable compilation audit id**, reuses the compiler's schema/provenance gate, and stores both records. An in-hand accepted compilation is adopted, not compiled again.

```text
delivery.py start --task <id> --facts <facts.json> --audit-root <audit-parent> --compiled-id <al-id> [--input <file> ...]
delivery.py status --task <id>
delivery.py complete --task <id> --stage <current> --actor <author-id> --evidence <proof-file> [--evidence <file> ...]
```

Every example is a helper invocation under the interpreter above. `--audit-root` is the directory **containing** `audit/audit-log.jsonl`, as in `prompt-compile.py`, not that log itself. Use the existing opted-in audit root when present; otherwise an explicit local root. For a new local compilation, log raw first with `audit-log.py --root <audit-parent> append --kind prompt --prompt-file <raw> ...`, then compiler `skeleton --from-audit <raw-id> --audit-root <audit-parent> ...` and `finish ... --audit-root <audit-parent>`. Do not use the compiler's prose default as permission to bootstrap `docs/audit`. Its reference resolution is rooted at the audit parent's parent: ground references yourself and inspect any unresolved compiler reference rather than assuming arbitrary external audit roots preserve resolution.

Git checkpoints remain private to this worktree's Git directory at `ai-forward/delivery/<id>.json`. Plain projects require **no Git initialization**: `--repo <project-root>` (default current directory) identifies the project, and checkpoints live beneath the local state home (`XDG_STATE_HOME`, otherwise `~/.local/state`), at `ai-forward/delivery/<identity-hash>/<id>.json`. A plain project's subdirectory is not automatically its root; the agent carries the original root when resuming. All checkpoint verbs accept `--state-root <dedicated-local-area>`; use the same choice on every invocation. The helper keys explicit areas by project identity, rejects a symlink area or the project/its ancestor, and prints `identity` and `local_area` in state. **The agent chooses/carries this local plumbing; do not send the user off to initialize Git or configure state.** Never put product code in the dedicated state area.

Keep facts, compiler audit, receipts and proof in `local_area` or an explicitly chosen external evidence area. That narrow area is excluded from workspace hashing so creating a human-reply receipt does not invalidate the pause; registered inputs/evidence remain separately hash-checked even there. Git tracked/nonignored files, HEAD and branch are fingerprinted automatically from the root, including when invoked in a subdirectory. Plain projects fingerprint files without following directory links, excluding only local state, common runtime directories (`.git`, `node_modules`, `__pycache__`, `.venv`, `venv`) and the exact regular duration markers described below; `.gitignore` is not interpreted for plain projects. Register ignored/runtime/external load-bearing files with `--input`. Unrelated nonignored product edits conservatively block reuse; there is no broad ignore switch or semantic drift waiver. Regular workspace files include their content hash and POSIX user/group/other executable bits; Windows uses a neutral executable field because it has no equivalent POSIX execute permission. This same boundary is used by resume, recursive submodules and out-of-scope repair comparisons; timestamps and unrelated permission metadata are not fingerprinted. Git discovery and queries discard inherited repository-local redirection variables so `--repo` selects the intended project, while credentials and global configuration remain available. Earlier helpers' content-only workspace fingerprints do not establish this stronger boundary: preserve refused checkpoints and revalidate their original contract/evidence rather than editing the integrity hash.

Session-start duration markers are ephemeral bookkeeping, not product edits. The installer and checkpoint boundary recognize only regular duration stores whose exact root or descendant suffix is `docs/audit/.run-starts.json`, `.agents/log/audit/.run-starts.json` or their `.tmp` companions. This covers a real session payload rooted in a project subdirectory without moving the audit opt-in root. Symlink components, lookalike names and entire log directories are not exempt; durable child audit and coordination records remain visible. If a task needs marker bytes as an input, register them with `--input`; that separate input check still rejects changes. Git projects retain their own ignore and explicit tracking decisions.

Own authoring-stage edits, including any authorized audit writes, are captured at `complete`; finish those writes before checkpointing. **Verification is non-authoring:** both `complete --stage verify` and every pause while verification is current compare the final observation with, and retain, the unchanged independently reviewed workspace fingerprint. A permission pause cannot rebind a changed product, and a completion-only guard is insufficient. Keep fresh verification proof, closure reports and receipts in the excluded local area or an external evidence area. A new nonignored in-project proof file also changes that fingerprint; naming it as evidence is not a drift waiver. If a correction is needed, preserve the reviewed version and use the affected authored stage with scoped repair and fresh independent review. If edits already occurred, preserve them and reconcile the drift explicitly before proceeding; do not reset the record or adopt them through a permission pause. Read/validate status **before** resuming work. New sessions must not use `complete` as a way to adopt unexplained drift. Registered JSON is parsed and hashed from the same captured regular-file bytes. Routing facts may then receive the compiled-tier safety floor, but their registered source version is preserved. Changed facts, receipts, repair authorization or closure files cannot acquire a digest that blesses another interpretation. Nonempty evidence files are hashed, not semantically judged; inspect the actual observations/results against the stage's exit checklist before recording them.

Atomic replace plus an exclusive task writer lock protects cooperative concurrent updates. A crash may leave `<id>.lock`; do not infer completion or automatically break it. The lock records its writer's pid and creation time, and the busy error names both (FR-097). Verify that pid is gone, inspect last durable checkpoint/evidence, and have the operator remove only that stale local lock. Checkpoints are local and are not transported to a different checkout/session store automatically.

## Startup observations before resume

Read the startup report as well as checkpoint status. `FAILED` or `NOT CHECKED` is an
advisory observation, not an approval or automatic all-task veto. Decide whether the
check is relevant to the original outcome and whether its failure invalidates earlier
evidence. Explain the affected prerequisite, repair or obtain it within authorized
scope, and revalidate only affected work. Preserve valid completed work and the
original decision history; do not silently ignore a relevant failure, invent a new
human gate for every notice, or rerun the full lifecycle to regain context. A zero
hook exit and a valid fingerprint do not establish present environment readiness.

## Gates and decision receipts

Before executing a selected skill's independently reviewed plan or design, use the
existing `pause --kind hard-veto --authority reviewer` against that current stage,
with its original plan evidence and actual author. Continue only after an actual
independent ruling and bound `resume`. This is agent-owned sequencing, not another
skill command the customer must type. Reuse valid existing clearance; do not invent
an optimization stage simply to repeat `/implement` planning. If selected, its own
pre-execution veto remains applicable even when the helper has no automatic gate.
A late approval does not prove a pre-execution gate occurred. If execution already
missed it, retain the missed-gate evidence and obtain explicit scoped reconciliation;
never backdate a ruling, relabel the author or declare historical compliance.


Completing `investigate` always records a human repair-review gate. Completing implementation/documentation at T1/T2, or UI/migration/coordination work, records an independent review gate before outcome closure. Genuinely T0 code/docs without UI/coordination may self-check; a routine feature/fix is T1+, not trivial just because its diff is small. This exemption does not clear any actual veto recorded via `pause --kind hard-veto`, or waive a selected skill's applicable preimplementation review. Explicit preauthorization can clear repair-review without a new question **only** after reading the original human instruction and verifying the diagnosed phases remain within it. The helper never mines raw prose for consent.

Answered `decision_requests` in an already dispatchable compilation are allowed; missing, blank, non-text or `unanswered` answers still refuse. The current compiler's native `finish` marks **any** nonempty request list nondispatchable even when answered. This helper does not rewrite its records or waive `dispatchable`: obtain a new finished, human-settled contract through the existing compiler flow; never flip the flag or pretend the inherited producer limitation is repaired. Stored answer text is not authenticated human consent.

```text
delivery.py pause --task <id> --kind decision|permission|hard-veto|release --authority human|reviewer --question <specific-question> [--actor <actual-stage-author> ...] [--evidence <partial-work-proof> ...]
delivery.py resume --task <id> --receipt <receipt.json>
```

Human decisions/permission/release require `human`; hard-veto clearance requires `reviewer`. Send the CO-S2 stop message with task id, original goal, completed proof, exact question, authority and resume action. No polling or repeated stage execution while awaiting an answer. For a gate encountered during active work, `pause --actor <actual-stage-author> --evidence <partial-work-proof>` captures current edits without marking the stage complete; repeat `--actor` for all actual authors. Evidence describes partial changes and tests not yet executed. Partial evidence or any hard veto requires nonblank author identities, including for ignored/external work with an unchanged workspace hash. The checkpoint retains stage/actor/evidence records in `partial` across approvals and stage completion, and binds them into each new gate. Like `complete`, authoring-stage pauses are only for known own edits after validation, never for adopting unexplained fresh-session drift. A pause during `verify` cannot capture new workspace edits, even with partial evidence and author identities. To resume with no gate (e.g. interrupted work), run `status` and continue its `next` stage; no invented approval receipt.

After observing the actual authorized decision, capture its original evidence (human message/tool-permission event, or independent review report), and create a receipt using **the current gate**:

```json
{"task":"<id>","gate":"<current-gate-id>","binding":"<current-binding>","authority":"human","actor":"<actual-operator>","source":"human-message","decision":"approved","evidence":"<original-decision-file>"}
```

Reviewer source is `reviewer-report`; its actor cannot be any completed stage's author or any recorded partial-work authors, even after a permission pause/resume or later stage completion. Use actual stable identities; renaming or omitting an author is not clearance. Copying an old receipt, labeling model output as human, using an ACK/exit status, choosing a default, or claiming approval without the original decision is forbidden. A blocked/denied verdict remains paused. The helper checks source labels, bindings, hashes and nominal actor separation, **not authenticity or the decision's scope**. The agent/host must verify original source, approved phases/actions and applicable authority. A helper receipt never grants host tool permission, clears another required veto, or authorizes release. Reuse the pack's real reviewer/ruling and host permission standards.

`status` and `resume` fail closed on identity, raw/contract, registered input/workspace, or evidence drift. Preserve the record and explain the invalidated boundary. The narrowly authorized reviewer remediation below is the only same-task tree rebind; other drift requires human reconciliation/revalidation, not an edited integrity hash or silently reset completion list. Valid unrelated completed stages stay evidence-backed and are not repeated merely to regain context.

### Repair a hard veto without abandoning the task

**Before any correction**, validate `status`, inspect the independent BLOCK report and the original human authority. Do not clear the rejected artifact merely to permit editing. Capture two separate records against the current gate:

```json
{"task":"<id>","gate":"<current-gate-id>","binding":"<current-binding>","authority":"reviewer","actor":"<independent-reviewer>","source":"reviewer-report","decision":"blocked","evidence":"<original-BLOCK-report>"}
```

```json
{"task":"<id>","gate":"<current-gate-id>","binding":"<current-binding>","actor":"<operator>","source":"human-message","decision":"approved","stage":"implement","paths":["src/app.py"],"evidence":"<original-human-repair-authorization>"}
```

The human evidence may be explicit prior authorization **only after** verifying it covers the diagnosed correction, exact file scope and unchanged original criteria. Otherwise ask the human. Labels are not authentication or semantic scope validation; this receipt grants no host/tool permission, release authority or broader product mandate.

```text
delivery.py repair --task <id> --stage <affected-stage> --review <BLOCK-receipt.json> --authorization <repair-scope.json> --actor <actual-repair-author> [--actor <coauthor> ...] --path src/app.py [--path <exact-file> ...]
# Perform only the authorized correction and real verification.
delivery.py recheck --task <id> --evidence <fresh-repair-proof> [--evidence <file> ...]
delivery.py resume --task <id> --receipt <fresh-independent-approval.json>
```

`repair` requires the unchanged checkpoint tree; it cannot retroactively adopt unexplained edits. It preserves the task/contract, BLOCK evidence, old gate/binding, prior snapshot, affected-stage proof and every recorded author in `repairs`/`partial`. Only the current authored stage or the latest completed stage can be repaired; `verify` and human `repair-review` are not authored correction stages. A completed affected stage is reopened, retaining all unrelated completed work. A midstage veto never claims completion.

When repair reopens an authored stage after a verification pause, displaced verification authors remain historical provenance, not completed verification. The helper accepts them only through the matching retained repair transition; they remain excluded from independent clearance. Invalid unrelated future-stage records still refuse. Load and save apply the same structural guard: a refused transition preserves checkpoint bytes instead of persisting a state that cannot be reloaded. A valid-integrity schema-2 checkpoint from the earlier helper can be read with the corrected helper without deleting authors or rewriting its integrity. Workspace, contract and evidence checks still apply; refreshing in-project tools is separately observable drift, not permission to expand an active product-repair scope.

`recheck` compares every non-scoped file and Git index entry with the repair's baseline and rejects HEAD/branch changes, unrelated edits, filesystem aliases, reparse points, hardlinks and special-file substitutions. Existing scoped leaves must remain single-link regular files; an explicitly scoped missing leaf may be created or deleted, but any created object must pass the same check before re-review. It records fresh evidence, restores a reopened completed-stage record and rotates the still-active reviewer gate to the corrected snapshot/history. An active repair cannot be approved before `recheck`; old receipts and original/partial/repair authors cannot clear it. A fresh independent BLOCK can start another bound `repair` cycle; prior history remains as `superseded`. A midstage repair returns to the same unfinished stage after clearance; a completed-stage repair returns to verification. The original criteria still govern closure.

Use exact root-relative, slash-separated regular product-file paths, including new/deleted files where fingerprinted. No directory-wide scope, state/Git metadata, ignored/excluded runtime inputs or submodule-internal paths are a drift waiver. Registered input/evidence hashes remain immutable: if a correction would invalidate those boundaries, reconcile explicitly rather than use this transition. While scoped edits are not yet checkpointed, ordinary `status`/`resume` still refuse tree drift; `recheck` is the explicit bounded adoption path. A new session inspects the durable repair scope and original evidence before using it.

### Git submodule input boundary

Each Git gitlink records its index object/stage plus checkout HEAD, branch, indexed content and tracked/nonignored worktree files recursively, with a **16-level nesting bound** (deeper layouts refuse). `snapshot.submodules` exposes these dispositions. An initialized unchanged checkout remains reusable; tracked, staged, untracked nonignored, checked-out revision and nested-submodule changes invalidate `status`/`resume`. Ignored/runtime inputs still require explicit `--input` registration.

An absent/uninitialized checkout is recorded as `checkout: "unavailable"`, not inspected content or proven dependency correctness. The helper never fetches or initializes it. Treat unavailable load-bearing dependencies as a real blocker; if initialization is authorized, do it before grounding the task. Availability changes invalidate existing context. Filesystem symlinks are recorded rather than traversed; symlink ancestors of a gitlink refuse. A checkpoint made by the old helper with unrecorded gitlink content cannot establish this stronger boundary and will refuse reuse; preserve its evidence and reconcile, never recompute its integrity hash to pretend it was checked.

Checkpoints now use version 2 with explicit partial authors. Version 1 cannot prove who authored in-progress work and is refused unchanged, not automatically migrated or cleared. Preserve its contract/progress/evidence and reconcile with the human/reviewer before establishing a new accepted record; never invent missing identities or silently rerun valid completed work.

## Closure

After real-path verification, `complete --stage verify` also requires `--closure <report.json>`:

```json
{"criteria":[{"criterion":"<exact original done_when>","observed":"<actual-path observation>","evidence":["<proof-file>"]}],"changes":[],"tests":[],"skips":[],"limits":[],"remaining_gates":[]}
```

List original criteria in their original order, each with nonempty observation and real evidence. Record actual changed paths, commands/results, reasoned skips and limits. Nonempty remaining gates cannot be declared closed. A matching structure is not proof the observation is true; apply CT25a/end-to-end-integrity and existing Proof Pack/reviewer checks, then hand back this map in chat. Audit via `audit-log.py` with only observed watcher signals and the existing compiled-from linkage; honor AL0.2 opt-in roots. No unsolicited graph bootstrap, default adoption, Git identity change or commit.

## Forgetting a task

```text
delivery.py forget --task <id> [--state-root <dir>]
```

`forget` deletes one task's checkpoint under the writer lock, and nothing else (FR-107). For a plain
project it also removes the per-project digest directory once it is empty. A missing task is
refused (`unknown task`). Use it only when the human asks for the task to be dropped: the
checkpoint is the only record of in-progress authors, gates and evidence, and the committed audit
log keeps the prompt. A plain project's state lives outside the project (`$XDG_STATE_HOME` or
`~/.local/state/ai-forward/delivery/`), so deleting the project does not delete it; this is the
deletion path.

## Host reachability

The pack installer copies complete skill directories to `.claude/skills`, `.agents/skills` and `.grok/skills`, and Copilot wrappers to `.github/prompts`. These resources therefore travel with the skill. Claude Code and VS Code support `/deliver`; Copilot CLI supports `/skills` discovery followed by `/deliver`, or requesting a named skill as `Use the /deliver skill ...` (not a custom shell command). `resume <id>` is interpreted by this skill, not a new host-native command. No `allowed-tools` preapproval is added.

Official invocation references: [Claude Code skills](https://code.claude.com/docs/en/skills), [Copilot CLI skills](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-skills), [VS Code skills](https://code.visualstudio.com/docs/agent-customization/agent-skills). Documentation compatibility is not live host qualification; deterministic helper tests do not measure model compliance.

The maintenance case `pack/evals/cases/deliver-feature-01.json` is a T1 feature artifact test, not a live workflow qualification. Supported `cmd-exit` assertions check the reviewer's LF-normalized verifier digest before and after exercising the actual application import. The boundary command must also emit its exact positive completion line after every check; an exit zero before that line is refused. Supported `files-absent` assertions check the declared product-document exclusions while allowing ordinary test caches and proof in the existing dedicated `.git/ai-forward/delivery/` local area. This narrow pathname allowance is not permission to put product code or product documentation there, and does not semantically validate proof. Case/verifier ownership remains outside the worker workspace. A correct deterministic file writer can still pass: this is finite artifact conformance, not evidence of red-to-green stage execution, authentic independent review, permission handling or native host compliance. Those claims require a separately observed bounded customer trajectory. Unknown assertion types fail closed; a new label is not an implemented check. The oracle is not a malicious-code sandbox or protection against modifying the reviewer-owned interpreter/case.
