---
skill: execute-with-coordination
part: copilot
---

# Copilot coordination profile

Copilot uses the existing ACP runner, leader designation, immutable controls and
Owner-review fences. It does not introduce another coordination service. An Owner
may run the same commands from Copilot or another qualified harness. Qualifying a
Copilot worker does not qualify a Copilot Owner; both need independent live evidence.

## Scope and design

The user-approved extension adds Copilot and Windows to the bounded coordination
runtime. It preserves `spec-coordination-runtime-v2` AC2 through AC8: finite execution,
same-session mailbox input, exact once-only permission decisions, no replay after
possible dispatch, honest attachment semantics, and evidence before review readiness.
The upstream graph is `spec-coordination-runtime-v2` -> `spec-acp-coordination` and
`spec-multi-harness-launch-and-monitor`.

The Run and Worker aggregates, immutable manifest, control-record grain, additive
attempt/byte counts, non-additive wall time, and append-only history remain unchanged.
There is no migration or duplicated ownership store. New Copilot qualification adds
an `effective_model` attestation; its reader checks the explicit argv model. The
transport calls the native model setter before the first prompt, and the runner
checks actual assistant-message and usage model evidence before readiness.
Existing non-Copilot qualification records remain compatible.

Surfaces: native executable/profile -> prepared worker -> ACP handshake -> native
hooks and coordination identity -> mailbox/permission controls -> evidence reader ->
status -> Owner review and explicit join. Each surface must preserve the same worker
identity, assigned checkout, selected model, current leader epoch, and bounds.

Patterns: existing Adapter for ACP; bounded Producer/Consumer for stdio and mailbox;
existing compare-and-swap leader fencing; native owned-process lifecycle. Rejected:
another broker, terminal keystroke injection, global trust relaxation, or treating a
saved session as a verified live terminal.

## Profile admission

Use native `copilot --acp --model <explicit-model>`. Automatic/default model selection,
implicit resume and broad approval-bypass flags are not this qualified profile.
The qualification's `effective_model` must equal the pinned argv model. A successful
`session/set_model` must precede any prompt; unsupported or failed setters refuse
dispatch. After completion, both native `assistant.message.model` and
`session.usage_checkpoint.promptCacheBreakState` must report only the admitted model.
`models.currentModelId` and the possibly empty `modelCacheState` are not inference
evidence. Missing or mismatched actual-model evidence blocks readiness.
Model-family policy belongs to the operator's run, not to the pack globally. For a
GPT-only run, pin every Owner, worker and native subagent to a named GPT model; do not
use automatic routing.

The runner requires a committed native repository model policy before preparing
Copilot and checks it again in each actual worker checkout. Its fingerprint always
includes that policy. The measured exact-ID policy for the GPT-only fixture was:

```text
fallback: gpt-5.4
gpt-5.4
```

This belongs in the consuming repository's `.github/allowed_models.txt`, not in the
pack's global defaults. The native parser refused session creation without the
fallback directive. The runner pins `COPILOT_MODEL` from argv; the qualification
also bound its native policy and excluded provider overrides. Other runs may choose
other explicit models; this is not a product-wide GPT-only restriction.

Copilot 1.0.88-0 was observed advertising ACP v1, `loadSession`, and the `copilot-login`
authentication method. It did **not** advertise `additionalDirectories`. Therefore
Copilot does not inherit the Codex-only operational-file-root grant. A missing
capability is refused, not replaced with a broader directory grant.

The current client does not qualify arbitrary Copilot terminal attachment. Mailbox
input goes to the worker session owned by the running transport. Loading saved history
and attaching to an independently running terminal remain distinct capabilities.

## Lifecycle and enforcement

Launch with an explicit `AGENT_SESSION` and `AGENT_HOST=copilot`. A child native agent
must not write heartbeat or decision receipts as its parent. Native session/subagent
start, successful tool completion, and stop hooks carry the corresponding identity.
Hooks cannot export variables into the parent process; the launcher establishes them.

Copilot loads both `.github` and compatible `.claude` hook configurations. The
Claude-form copies must not emit duplicate coordination records when the active host
is Copilot. Native lease enforcement is an explicit project opt-in emitted by
`coord hook --config --host copilot`; installing the pack does not silently grant
permissions or enable an ownership guard.

The measured Copilot ACP surface did not execute the repository-hook configuration
alone. The qualified Windows path explicitly loads the emitted plugin. From the
primary checkout:

```sh
python3 docs/ai-forward-pack/scripts/coord-core.py plugin --emit .git/coord-copilot-plugin --host copilot
```

Use the emitted bundle's absolute path with `--plugin-dir` in the worker argv. Bind
its manifest, hook definitions and launchers, plus the installed hook scripts they
reference. Re-emit and requalify after updating those scripts. Emitting is not
installing: the command changes no user-level settings or trust. Keeping this local
bundle under the primary `.git` prevents its machine-local launcher paths from being
committed. A linked worktree's `.git` is a file, so do not emit there.

The plugin carries start, tool, mail, heartbeat and stop hooks. Native GPT
`apply_patch` is a freeform patch; the PascalCase plugin envelope calls it `Edit`.
Both shapes use one strict target parser. The plugin process directory is not the
tool directory: the hook accepts the payload directory only after proving it belongs
to the same Git common repository, and then resolves targets against that checkout.
Successful ownership checks return neutral `{}`, preserving native permission
prompts; only a real refusal returns a permission decision.

Historical Copilot 1.0.80 plugin evidence remains historical. It cannot satisfy a
current executable/profile qualification. A native hook refusal is a cooperative
edit boundary, not a shell sandbox, and requested stop refusal alone is not proof
that the host honored it. The final runner decision fence remains independent.

## Windows boundary

Windows uses a gated Job Object before the target executable may run. A containment
failure refuses release. Termination targets the owned job, never all processes with
the executable's name and never an inferred process tree. POSIX retains process-group
containment and its existing selector transport. Windows stdio uses bounded threaded
reads/writes with a deadline and owned-process cleanup.

Windows receipt/control reads open each ancestor without following reparse points
and hold handles that deny deletion/rename through the read. Junctions, non-regular
files, replaced identities, oversized records and concurrent mutations are rejected.
Private control directories use an inheritable current-user/SYSTEM protected DACL;
POSIX mode bits are not misreported as Windows access control. Windows mailbox locks
use bounded native byte-range locking. Control records retain exclusive atomic
publication and hash-chain validation. Hostile code with the same OS identity is
outside the local filesystem authority boundary.

## Failure modes and proof obligations

| Failure or threat | Control and falsifying input |
|---|---|
| Wrong/default model or changed profile | Pin argv, qualify effective model, compare native selected model; missing/mismatched model blocks before prompt. |
| Duplicate hook or parent/child identity confusion | Explicit host routing and child identity; one action must not produce two records or credit the parent. |
| Permission spoofing or stale leader | Exact offered once-option and live epoch; forged option/request/epoch refuses. |
| Prompt replay after ambiguous EOF | Existing prompt-start fence; post-dispatch EOF never retries. |
| Blocked stdin, stderr flood or hung callback | Time/byte/turn limits; cancel reaps owned descendants without hiding cleanup failure. |
| Failed Job assignment or child-start race | Assign before gate release; a failed assignment never starts the target. |
| Reparse, rename or record replacement | Pinned non-reparse handles and identity/checksum checks; junction and same-path replacement negatives. |
| Empty receipt or unresolved decision | Independent bounded receipt read and strict decision projection; transport success never overrides either. |
| Private action leakage | Retain raw bodies only in private local controls; public telemetry uses identity, digest, counts and state. |

Testing union: D0, D1, D2, D3, D4, D5-provider, D6, D7, A1, A3, A4 and A6.
Real filesystem/process/Git tests cover the deterministic boundary. Live GPT-only
canaries separately establish native contract fidelity. No exact model prose is an
acceptance oracle. CLI states and error codes retain the existing text/JSON interface;
there is no new graphical surface or accessibility convention.

## Execution and release evidence

Order: repair primary registration -> contract spike and independent design review ->
parallel hook and transport implementation, with runner/control-file integration ->
joined offline proof -> separate worker and Owner live qualification -> source/install
sync, full bundle gates and independent release review. Only the independent authored
surfaces run concurrently; no source file has two authors. The bounded worklist is the
termination variant; exhausted time/call budgets report remaining work, never weaken
a gate. No duration or speedup estimate is a measured result.

Design gate: 2026-09-21, independent Security & Identity review PASS; independent
Test Architect review permits implementation with red-first and separate live-role
proof conditions. Neither verdict is a release approval.

### Correction: advertised model was not the inference model

Eight initial attempts supplied `--model gpt-5.4` and received advertised GPT metadata,
but native assistant/usage records showed `claude-opus-4.8`. They violated the run's
GPT-only restriction and remain **INVALID**, not qualification evidence. Testing
stopped on discovery. Reading the installed implementation established that fresh
ACP creation initialized the advertised model independently of the real session.
The native setter, exact-ID policy and actual-inference postcondition are the
controls derived from that failure. Later successes do not erase the invalid runs.

### Recorded Windows qualification, 2026-09-21

The sanitized record is `pack/evals/fixtures/copilot-windows-qualification.json`.
It omits machine paths, native prompt bodies and reasoning, and retains source
trace digests, actual model identities, outcomes and limitations.

| Claim | Observed evidence | Boundary |
|---|---|---|
| Native instructions and permitted writes work | GPT-only plugin profile read the instruction marker and wrote the requested fixture; a separate exact write required one `allow_once`. | This explicitly loaded plugin profile, not repository-hook-only ACP. |
| Lease refusal reaches the host | The actual native patch was refused with the holder's reason and the leased bytes stayed unchanged. | A malformed-input refusal is not counted as this proof. |
| Native permission denial holds | One exact request was denied; its target file was absent. | Neutral ownership success does not preapprove the operation. |
| Active cancellation cleans up | Cancellation followed a real authorized marker write; outcome `cancelled`, `cleanup_error: null`. | Usage may be absent after cancellation; this is never readiness evidence. |
| Copilot can hold Owner and coordinator | The current Copilot Owner prepared/pinned/launched an isolated GPT worker, inspected and approved its native request once, read model/receipt evidence, reviewed the file and fast-forward integrated its commit. | Structural readiness remained separate from semantic review and integration. |
| Cross-harness handoff works | Native Codex `gpt-5.5` returned the requested read-only acknowledgement through a bounded Windows Job launch. | Not full Windows Codex write-hook/trust qualification. |
| Open Owner decisions prevent premature completion | A seeded request caused native Stop refusal. An attempted out-of-scope ledger edit received no approval; the bounded attempt expired and never became ready. | The completed-transport `RUN-DECISION-OPEN` branch is separately proven by real-Git offline integration; expiry is not mislabeled as that branch. |

### Defect classes and controls

| Class | Sweep and derivation | Preventive control |
|---|---|---|
| RIG-D / RIG-E: requested configuration mistaken for execution | Compared CLI intent, ACP metadata, native setter and actual inference events. | Pre-inference native policy plus setter; actual assistant and usage model gate; recorded invalid runs fail its regression test. |
| HOST-A: one envelope assumed across models and hook surfaces | Compared native batch, single-call and PascalCase plugin input; GPT patches and Claude JSON edits differ. | Shared strict parser, captured payload fixtures, alias/move/traversal negatives. |
| WT-A: process directory mistaken for tool directory | Replayed the same patch from plugin and tool directories; leases were invisible in the former. | Same-Git-repository validation before accepting payload cwd; actual-holder refusal oracle. |
| E2E-E: an ownership check silently grants permission | The first permitted edit had zero native permission requests. | Neutral Copilot success and explicit allow-once/denial proofs. |
| PLAT-A: native process/filesystem contracts assumed portable | Compared Windows closed-pipe/NUL input, junctions/symlinks, LF bytes and POSIX locking. | Job-before-release, closed-pipe EOF test, Windows threaded wire, non-reparse pinned reads, protected DACL, junction-based negative tests. |

These are profile-scoped observations, not a promise about every installed harness,
model, permission mode or future version. Requalification remains mandatory when the
bound profile changes. Public API/source coverage gaps remain visible in the generated
reference rather than being replaced with invented documentation.
