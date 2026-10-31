---
load: skill
skills: [forensicreview, define-architecture]
---
# Engineering Governance — the SDLC lenses the craft docs don't cover

*Version 1.0. A reference companion to the Agent Knowledge Pack.*

The Body of Knowledge governs *reasoning*, the Style Guide governs *C#*, and LOA
governs *AI-integrated architecture*. This document governs the **software-development
lifecycle concerns** that sit around the code: the non-functional requirements,
governance, and operational obligations a change must satisfy before it is truly
"done" (Rules of the Road D1). It is **reference, not auto-ingested** — pull the
relevant section in when a change touches that concern, proportional to the tier
(Rules of the Road §0.2). Normative keywords follow RFC 2119.

> **How to use this.** During Phase 1 (Frame) and Phase 4 (Specify), walk the
> checklist below and mark each lens *applies / does-not-apply / flagged*. A lens
> that applies but has no answer in the spec is a gap, not a silence.

---

## 1. Requirements traceability

Every shipped behaviour traces back to a spec statement, and every spec statement
forward to a test (Test Architect, Persona Catalog §2). The **Proof Pack** (Rules
of the Road §3.1) is the join table. For T2 work, maintain the chain
`requirement → spec → test → Proof Pack row`; an orphan in either direction is a
finding — untested behaviour, or behaviour no one asked for (YAGNI; Tech Lead).

## 2. Quality attributes (the non-functional spec)

Functional correctness is necessary, not sufficient. For T1/T2 work, the spec
**MUST** state targets (or explicit non-goals) for the attributes the change
touches:

- **Performance & latency** — see §6 budgets.
- **Reliability & availability** — failure modes, retries, timeouts, degradation.
- **Security** — see §3 and the Security & Identity Architect.
- **Privacy & data protection** — see §4.
- **Accessibility** — see §5.
- **Maintainability** — the Style Guide and LOA are the standing answer.
- **Observability** — see §8.

Unstated quality attributes default to "no worse than today" — and that default
**MUST** be verified, not assumed.

## 3. Threat modeling (lightweight STRIDE)

For any change crossing a trust boundary — new input, new endpoint, new
subprocess, new MCP tool, new stored artifact — run a quick **STRIDE** pass
(Spoofing, Tampering, Repudiation, Information disclosure, Denial of service,
Elevation of privilege) and record the result in the spec. This is mandatory for
T2 and is the Security & Identity Architect's standing agenda (hard veto). For
this repo specifically:

- **Subprocess / agent surface** — the Copilot SDK agent and CLI invocation are a
  command and prompt-injection surface; model output that triggers a side effect
  **MUST** pass a verifier or human gate (LOA P3/P5).
- **MCP server** — every exposed tool is an external entry point; enumerate
  authZ, input validation, and resource bounds per tool.

## 4. Privacy & data governance

This product operates **on behalf of a signed-in leader** over M365 / Work IQ
data — the most sensitive class of data in the system. Therefore:

- **Delegated identity is a hard boundary.** All data access stays strictly inside
  the signed-in user's permissions, sensitivity labels, and audit scope. Never
  broaden scope or cache another principal's data. (Security & Identity Architect.)
- **Data at rest** — observation bundles, assessment snapshots, and artifacts are
  encrypted (DPAPI CurrentUser) when `CoachData:EncryptObservationsAtRest=true`.
  New persisted personal data **MUST** route through the encrypted store, not a
  bare file write.
- **Minimization & retention** — collect only what the assessment needs; state a
  retention/exclusion posture for any new personal data field.
- **Logs & telemetry** — personal data and secrets **MUST NOT** appear in logs,
  traces, or exception messages (Style Guide §7; §8 below).

## 5. Accessibility

UI lives in `CoachApp.Components` and is hosted by MAUI Blazor today and Blazor
Server tomorrow. New components **MUST** be operable by keyboard, expose
accessible names/roles, maintain a logical focus order and adequate contrast, and
not convey state by colour alone. The components test suite includes an
accessibility smoke check (bUnit) — extend it, do not skip it.

## 6. Performance budgets

Assign a budget to the resources a change consumes before writing it (LOA P-budget):

- **Model calls** — every call has a token/time budget and a timeout. Real Work IQ
  / M365 Copilot queries take minutes; honour the configured `TimeoutMs` /
  `OverallTimeoutMs` / heartbeat rather than inventing shorter ones.
- **UI thread** — no synchronous I/O on the UI thread; stream long results.
- **Allocations / I/O** — name the expected order of magnitude; a regression
  against it is a finding even when tests are green.

## 7. Release, rollback & data migration

- **Reversibility.** Every change states how it is rolled back. Irreversible
  actions are a Part 2 stop condition and a Security hard-veto trigger.
- **Schema / persisted-format changes** are migrations: provide a forward path,
  preserve or upgrade existing on-disk artifacts (run logs, assessments,
  snapshots), and never make a change that silently corrupts data written by a
  prior version. Versioned formats get an explicit version bump and a read path
  for the old version.
- **Feature gating.** Risky behaviour ships behind configuration that defaults to
  the safe state.

## 8. Observability & operations (SRE lens)

For load-bearing or long-running paths (the run lifecycle, agent invocation, MCP):

- **Structured logging** with correlation (run id / session id), no personal data
  or secrets, consistent levels.
- **Telemetry** — emit the signals needed to answer "is it healthy?" and "why did
  this run fail?" without attaching a debugger.
- **Failure visibility** — terminal run states (Failed/Cancelled) are recorded
  durably; a crash between two persisted events must not lose the last event.
- **Resource bounds** — timeouts, cancellation threaded through every async path,
  bounded retries with no non-idempotent replay. (Distributed Systems hard veto.)

## 9. Supply chain & licensing

- **Dependencies** are added only via central version management
  (`Directory.Packages.props`); a new dependency is justified against owning the
  code (maintenance, license, transitive cost) per BoK Part III adopt-or-not.
- **License compatibility** is checked before adoption; copyleft or unknown
  licenses are a stop-and-ask.
- **Pinned, deterministic builds** — do not float versions; transitive pinning is
  on. New external tools (MCP servers, subprocesses) are part of the trust
  boundary (§3).

## 10. Incident readiness

A change to a load-bearing path states, in one line, how an operator would detect,
diagnose, and mitigate its most likely failure in production. If that line cannot
be written, the design is not observable enough (§8) — that is the SRE
Diagnostician's standing finding.

---

## Governance checklist (walk this at Specify time)

| Lens | Applies? | Answer in spec / Proof Pack |
|---|---|---|
| Requirements traceability (§1) | | |
| Quality attributes (§2) | | |
| Threat model — STRIDE (§3) | | |
| Privacy & data governance (§4) | | |
| Accessibility (§5) | | |
| Performance budget (§6) | | |
| Release / rollback / migration (§7) | | |
| Observability & ops (§8) | | |
| Supply chain & licensing (§9) | | |
| Incident readiness (§10) | | |
| Responsible-AI stance — the committed policy and the PII/secret scrub (`responsible-ai-policy.md`; `scrub.py`) | | |

A row marked *applies* with an empty answer is a gap to close or a risk to flag —
never a silence.
