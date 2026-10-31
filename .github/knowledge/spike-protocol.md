---
load: skill
skills: [define-architecture, design-slice, investigate]
---
# The Spike Protocol

*Read the code and run a proof-of-concept before you design against an unfamiliar contract. Version 1.0 — operationalizes Body of Knowledge §III.1 (read the source) and §III.2 (verify by execution).*

A spike is a small, time-boxed, throwaway investigation whose only purpose is to **convert an unfamiliar contract from a guess into established knowledge** before any design or implementation depends on it. It is the direct antidote to the two most expensive AI defects (BoK §VIII): **the Confident Guess** (stating a contract from naming intuition) and **the Plausible Hallucination** (inventing an API that "should" exist). The rule is blunt: *you do not get to be plausible about an SDK you have not read or run; you get to be correct about one you have.*

This protocol is invoked at **Rigor Protocol Stage 3 (EVIDENCE)** and is **mandatory** in `/define-architecture` and `/design-slice` whenever the work depends on an API, SDK, MCP server, protocol, or library that is unfamiliar, preview-status, or version-sensitive.

Normative keywords (**MUST**, **SHOULD**, **MAY**) follow RFC 2119.

---

## 1. When a spike is mandatory

The agent **MUST** run a spike before depending on a contract when **any** of these is true:

- The API/SDK/library/protocol is **unfamiliar** — the agent cannot cite, from an authoritative source, its exact semantics, nullability, error model, and concurrency behavior (the BoK Part III dimensions).
- The surface is **preview / beta / recently changed** (e.g., a vendor's preview API, a just-bumped major version). Preview surfaces carry breaking-change risk and **MUST** be confirmed at the call site, not trusted from memory (BoK §VI.1, §III).
- The semantics are **version-sensitive** and the consuming repo's pinned version may differ from the agent's recollection (e.g., a resilience library whose v7→v8 reshaped its API). Check `global.json` / `Directory.Build.props` / `Directory.Packages.props` first, then spike against the *installed* version.
- The contract is **load-bearing**: getting it wrong corrupts data, breaks a trust boundary, or silently violates a delivery guarantee. (Cost-of-error is high — BoK §II.3.)
- An **MCP server or tool** is being designed or consumed: its declared schema, invalid-parameter behavior, and side-effect semantics are a contract that **MUST** be exercised, not assumed (Testing Strategy A2).

The agent **MAY** skip a spike for a contract it can establish authoritatively from the source/docs alone *and* whose cost-of-error is low. When in doubt, spike — a fifteen-minute PoC is cheaper than a design built on a wrong assumption.

---

## 2. The two moves (do both, in this order)

A spike has two complementary halves. Neither alone is sufficient: reading tells you what the authors *intended*; running tells you what the code *does*. Where they disagree, the executed result wins for behavior, and the discrepancy is itself a finding.

### Move 1 — Read the code (the top of the source-of-truth hierarchy)

Before running anything, read the **actual contract**, in this priority (BoK §III.1):

1. **The type signatures and reference source.** Read the real method signature, the XML/doc comments, the nullability annotations, the exception list, the `[Obsolete]`/preview markers — from the decompiled assembly, the published source, or the package's reference repository. This is the compiler-checked truth.
2. **Official samples and conformance/test suites.** The maintainer's own tests are the most honest documentation of intended behavior and edge cases.
3. **Reputable secondary sources / the issue tracker.** For known gotchas and breaking-change notes.
4. **Community Q&A** — for *leads only*, verified against (1).

Record what the source says about each BoK Part III dimension that matters for this use: identity/version, semantics, nullability, boundaries, error model, security model, concurrency, lifecycle, idempotency, cost, lifecycle status.

### Move 2 — Run a minimal proof-of-concept (verify by execution)

Reading establishes the contract; execution confirms it (BoK §III.2). Write the **smallest runnable program** that exercises exactly the behavior you are about to depend on, and **observe the result** — do not predict it. The PoC targets the *installed* version.

Idioms by language (use the lightest one that runs):

- **C# (.NET 10 / C# 14):** a **file-based app** — a single `.cs` file run with `dotnet run app.cs`, no project ceremony. Pin or reference the exact package version the consuming repo uses. (This is the C#-first default for spikes, matching the C# Style Guide and BoK §VII.1.)
- **Python:** a one-file script run with `uv run` against pinned deps, or a `python -c` one-liner for a single call.
- **Rust:** a throwaway `cargo` binary or a `#[test]` exercising the crate API.
- **MCP server/tool:** a minimal client call that (a) sends a valid request and inspects the response schema, and (b) sends a deliberately invalid parameter and confirms the error is deterministic and the server does not pollute stdout (Testing Strategy A2).

The PoC **MUST** exercise the **boundary set**, not just the happy path: the empty/null/max/concurrent/malformed/hostile cases relevant to the contract (BoK §II.1). The single most valuable line in a spike is usually the one that probes what the docs *didn't* say.

**An oracle over a recorded run is itself a contract, and it is spiked the same way.** Two measured failures (DC-178, DC-185): an assertion written from the *intended* state failed a green run on residue the harness's own preparation had recorded; and an oracle that asserted only what *did not* happen passed a run that never ended. So, before an oracle over a recorded run ships: **(1) run it against the harness's own recorded dry run** — the record the harness produces when nothing is under test — and read what it asserts against that; **(2) every "what did not happen" oracle first asserts that the run ended** (an exit record, a final status, a terminal event), because absence over an unfinished run is not evidence of anything. An oracle that has not been run against a real record is Inferred, whatever it asserts.

---

## 3. What a spike produces (the durable part)

The spike *code* is throwaway. The spike *findings* are durable and **MUST** outlive the session. A spike concludes by recording:

- **A confidence-ledger entry** (Rigor Protocol §4) per confirmed contract claim: the claim, the evidence (what the PoC did and what it returned), the source/version (`package@version`, doc URL, `file:line` of the signature read), the disconfirming case tried, and the label **Verified** (read *and* run) / **Inferred** (read only) / **Flagged** (neither resolved the unknown).
- **A contract note** for the design: the established semantics, in the agent's own words, with the surprises called out. This feeds the design doc and, at implementation time, becomes Proof Pack rows (Rules of the Road §3.1) and an inline source citation at the call site.
- **A flagged risk** for anything the spike could *not* resolve. An unresolved contract is reported as a finding, never papered over with a guess (BoK §III.1). A preview surface is flagged as breaking-change risk at the call site even when the spike succeeded.

If the spike *contradicts* the agent's prior assumption (the API throws where it "should" have returned null; the call is not thread-safe; the parameter is seconds not milliseconds), that contradiction is a **high-value finding** and **MUST** be surfaced prominently — it is precisely the defect the spike existed to catch.

---

## 4. Where spikes live, and how they're disposed

- **Location.** Spike code lives under `spikes/<short-name>/` at the repo root (or the repo's established equivalent), clearly separated from production source. It is **never** referenced by production code and **never** ships.
- **Disposal.** A spike is deleted or archived once its findings are recorded in the design artifact and the confidence ledger. The knowledge is kept; the scaffolding is not. (Optionally retain the PoC as a labeled, synthetic integration fixture if it has lasting value — Testing Strategy D6 — but only if it is marked as such and maintained.)
- **Time-box.** A spike is time-boxed (typically minutes to an hour). If the contract cannot be established within the box, that is itself a finding: escalate the unknown as a flagged risk rather than expanding the spike into a project.
- **Not a substitute for tests.** A spike proves *understanding* of a contract; it is not the test suite that proves *your code's* correctness. The behavior the spike confirmed becomes a real test (often a contract test, Testing Strategy D5/D6, or an MCP protocol test, A2) at implementation time.

---

## 5. Wiring into the workflows

| Skill | When the spike fires | What it feeds |
|---|---|---|
| `/define-architecture` | Stage 3, for every unfamiliar SDK/protocol the architecture depends on (especially AI SDKs, MCP, delegated-identity surfaces) | the archetype/tier decisions and the ADRs — *don't choose a transport you haven't exercised* |
| `/design-slice` | Stage 3, for every unfamiliar contract a component will call | the component contracts and the call-site citations in the design doc |
| `/implement` | when implementation reveals a contract the design assumed but never spiked | a just-in-time spike before coding against it, then a real test |
| `/investigate` | when the root-cause hypothesis depends on an unverified contract behavior | the reproduction and the verify-cause-by-data step (Rigor Protocol Stage 3) |

The **Domain Researcher** (collaborative personas P3) owns spike execution in Peer Mode; the **Distributed Systems** and **Security & Identity** architects consume its findings; the **Test Architect** later demands that the spiked behavior became a real test.

---

## 6. In one breath

> If you have not read its signatures and run a minimal program against the installed version, you do not know the contract — you are guessing, and guessing at contracts is the largest source of AI defects. So before you design against any unfamiliar or preview SDK, API, MCP server, or protocol: read the source first, then run the smallest PoC that exercises exactly the behavior and the boundaries you will depend on, and *observe* the result rather than predicting it. Keep the finding, throw away the scaffolding, flag what you couldn't resolve, and turn what you confirmed into a citation and a test.
