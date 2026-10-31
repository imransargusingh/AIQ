---
load: always
---
# Instrumentation over Inference

*Normative guidance for a standing bias: **when you want to know how something behaves, measure it — do not reason about it.** `observability-and-instrumentation.md` governs *how* telemetry is shaped (the OpenTelemetry data model, structured logs, traces, metrics, error codes). **This document governs whether the measurement exists at all** — it makes instrumentation a delivery obligation and a gate, so that a question about a running system has an answer that is read rather than derived.*

Normative keywords (**MUST**, **SHOULD**, **MAY**, **MUST NOT**) follow RFC 2119.

The governing idea: **an uninstrumented system does not become unknowable — it becomes a system you reason about.** That is worse, because reasoning produces an answer with no error bar and no way to be surprised. The team ends up confidently discussing behaviour nobody has observed, and every such discussion is an unmarked guess wearing a technical vocabulary.

The pack already forbids guessing at *contracts* (`no-guessing-protocol.md`) and forbids asserting the shape of *our own code* from memory (`end-to-end-integrity.md` E15). This document closes the third gap, which is the one that survives both: **the behaviour of the thing after it is deployed** — how long it took, how much it cost, how often it ran, which branch it took, whether anyone used it. Those are not answerable by reading the source. They are answerable only if someone made them measurable *before* the question was asked.

**The worked instance, from this repository.** The audit log recorded a single `datetime` per entry and no duration. When a back-test needed per-prompt execution times, none existed — 750 committed entries and not one measured elapsed time. The session ids spanned days of human-paced work, so even the available timestamps could not be differenced into a duration. The result: a whole class of question was answerable only by **modeling**, and the report had to label every time and token figure *Inferred*. The instrumentation would have cost one field. Its absence cost the ability to measure anything, permanently and retroactively — **you cannot backfill a measurement nobody took.**

---

## 0. When this applies

Every feature, script, skill, pipeline and deployed surface — and the agent's own work. It is not tier-scaled: a T0 change does not need a dashboard, but it does need to not *remove* a measurement, and any new behaviour with a cost or a failure mode carries IO1. The **Owner** is the **SRE & Systems Diagnostician** (who already owns observability and design-time performance budgets), with the **Test Architect** holding the veto that a correctness or performance claim without a measurement path is not verifiable.

---

## 1. The principle

**IO1 — Instrument at delivery, not after. A feature is not done until its behaviour is measurable by default.** "Measurable by default" means the measurement is emitted by the normal running of the thing, with no extra flag, no re-run, no attaching a debugger, and no asking someone to reproduce it. Instrumentation added later can only answer questions asked later; it can never answer the question you have *now* about what already happened.

**IO2 — Name the questions before you build.** For any new capability, write down the questions its operator will have to answer within a month — *how long does it take? how often does it run? how much does it cost? which path did it take? did it fail, and where? is anyone using it?* — and confirm each one has an emitting source. A question with no emitting source is an **instrumentation gap**, and it is a finding at design time, not a discovery at incident time. The **structured home** for these is the Proof Pack's *operator questions* table (opt-in for a new capability, `templates/proof-pack.template.md`).

**IO3 — Never infer a deployed system's behaviour from its source.** Reading the code tells you what it *would* do; it does not tell you what it *did*, on real inputs, at real volume, with real failures. This is E15 pointed at runtime: **read the telemetry, or label the claim Inferred.** The tells are the same family as NG3 and are worth listing because they are said fluently and constantly:

> *"it should be fast" · "that path is probably rare" · "it typically takes about…" · "users mostly…" · "that error is unlikely" · "it's handling roughly N" · "the bottleneck is obviously…"*

Every one of those is a measurement claim with no measurement behind it. Check it, or mark it.

**IO4 — Absence of instrumentation is a finding, not a neutral state.** "We can't tell" is a defect report about the system, not a property of the universe. Record it, and record what it prevents.

**IO5 — Instrument the agent's own work too, and make it default-on.** The pack's own artifacts are subject to this: the audit log records *what happened*, and it MUST also record **how long it took**. The mechanism is deliberately **not** a flag someone has to remember — a skill marks its start at grounding (`audit-log.py start --session <id>`, the Audit Mandate AL4a) and the closing `append` picks the stamp up **automatically**, recording `started_at` and `duration_seconds` with nothing extra passed. *An opt-in measurement is not "measurable by default" (IO1); it is a measurement that will be forgotten.* Without this, every future question about the cost of a class of work is unanswerable — which is exactly what happened here.

**IO6 — Instrument the cost axes, not only the correctness ones.** Latency, spend/tokens, volume and failure rate are the four that make optimization possible at all. `ci-and-test-efficiency.md` CE1–CE2 makes the same point from the other end: **profile before optimizing, because the bottleneck is reliably not the suspect.** You cannot profile what does not emit.

---

## 2. Honesty when the measurement is missing

**IO7 — A modeled number is labelled, and the gap is named.** Where a measurement genuinely does not exist yet, you MAY model — but the output **MUST** carry the confidence label **Inferred**, **MUST** state the model so it can be attacked, and **MUST** name the instrumentation gap that forced it. A modeled figure presented as a measurement is the RIG-E error ("it works, therefore it conforms") applied to numbers, and it is the most persuasive way to be wrong (NG6, NG7).

**IO8 — Degrade to "not recorded", never to a wrong number.** A measurement path that cannot produce a trustworthy value **MUST** emit nothing (or an explicit "not recorded") rather than a plausible one. A precise, wrong number is worse than a blank, because a blank prompts a question and a wrong number ends one. *Worked example:* the audit log's duration capture returns no duration when the start stamp is absent, unparseable, or later than the end stamp (clock skew) — it refuses to emit a negative or invented elapsed time.

**IO9 — Fix the instrumentation when you hit the gap; do not just model around it.** The moment a question is unanswerable, that is the cheapest it will ever be to close the gap — you have the motive, the context, and the exact shape of the question in front of you. Modeling around it and moving on **guarantees** the next person hits the same wall with less context. Close it, then model only what remains.

---

## 3. The gate

**IO10 — The delivery gate: no feature ships uninstrumented.** A change that adds or alters a capability with a runtime cost, an external dependency, a failure mode, or a user-visible outcome **MUST NOT** be declared done until all four hold:

1. **The questions from IO2 are listed**, and each has a named emitting source (a metric, a span, a structured log field, a recorded duration, a persisted counter).
2. **The emission happens on the normal path** — not behind a debug flag, not only on error, not only in a test.
3. **The measurement has been observed at least once**, on a real run, with the value read back (E14 — an exit code is not a result).
4. **Anything still unmeasurable is recorded** as a named instrumentation gap with what it prevents, so it is a tracked debt rather than a silent one.

An unmet item is a gap the reviewer catches, exactly like an unmet Testing-Strategy trigger. **This gate is satisfied by an emitting source, never by an intention to add one.**

**IO11 — Instrumentation is itself verified.** A measurement nobody has read back is not a measurement — it is a line of code that is believed to emit. Load-bearing telemetry carries a test (`observability-and-instrumentation.md` O12), and a new measurement path is **observed producing a correct value and correctly declining to produce a wrong one** before it is trusted (CI6, red-first).

**IO12 — Instrument deliberately, not maximally.** This is not licence to emit everything: high-cardinality values stay out of metric labels (O13), context and log volume have real cost, and telemetry that bankrupts a budget gets switched off — which is strictly worse than telemetry designed to be affordable. Measure what answers a named question from IO2; drop the rest.

---

**IO13 — What you attach to every call is a measured budget, not an accumulation.** Anything loaded unconditionally — an instruction set, a tool schema, a system preamble — is the **static prefix** of every model call: re-read on every turn, billed on every turn, and subtracted from the window before the user has said anything. It is the purest form of the failure this document is about, because *nothing reports it*: each addition looks free at the moment it is written, and the total is knowable only by measuring it.

- **Declare the scope, per item.** Every knowledge doc carries a `load:` scope — `always` (attached to every request), `glob` (attached to matching files), `skill` (read on demand), `reference` (consulted, never attached). The test for `always` is sharp: **a doc is always-on only if an agent could violate it without knowing it exists.** You cannot look up the no-guessing protocol when it becomes relevant, because not knowing it is relevant *is* the failure. You can look up an archetype catalog — you know when you are building a UI.
- **Ratchet the total; do not ceiling it.** `context-budget.py gate` fails when the always-on set grows past its **recorded baseline** without that baseline moving, and when any doc declares no scope. An undeclared doc is an unbudgeted doc. **Growth is allowed — silent growth is not**, so the fix is one acknowledged line in a diff a reviewer can question. A fixed ceiling is the wrong instrument for accumulation: it stays quiet through the whole build-up and then red-lights an ordinary paragraph, which teaches people to raise the ceiling reflexively — the exact habit the gate exists to break (class **PACK-R**). Keep an absolute backstop behind the ratchet, but **derive it** from the smallest model window you still delegate to, so passing it is a decision about capability rather than a formatting preference.
- **A sub-agent inherits its lens, not the world.** Every persona declares the `knowledge:` it actually needs. An agent with no declared lens carries a main-thread-sized prefix into every delegated run, which is what turns cheap delegation into the *only* thing that fails — the smallest model has the smallest window, so it breaks first while costing least, and cost telemetry cannot see it.
- **Measure the whole prefix, not the part you wrote.** The knowledge docs are one component of the static prefix. The managed blocks (`AGENTS.md`, `CLAUDE.md`), the host's own system prompt, the tool definitions, the skill roster and the auto-memory index are the rest — and on a host that loads both `AGENTS.md` and `CLAUDE.md` the managed block is counted twice. A profiled Copilot session had a ~113k-token prefix while the knowledge-only budget reported ~44k, and the same tool run with default discovery from an installed repo found the *vendored* copy and reported 461 tokens, green (class **CTX-B**, the PACK-P shape again). `context-budget.py prefix` measures every component it can see and names the ones it cannot; `session-profile.py` reads the real prefix back from the harness store (SP-02/SP-03).
- **Preflight before a fan-out.** One failure at a context ceiling predicts every sibling in the wave: the prefix is identical for all of them. Check once (`context-budget.py preflight --window <N>`) rather than rediscovering it N times (class **PACK-S**).
- **Report the estimate as an estimate.** Token figures derived from character counts are labelled Inferred wherever they appear, and the ratio used is stated. A budget you can gate on does not need to be exact; it needs to be honest about which it is (NG6, IO6).

**IO14 — Reasoning is a billed quantity you mostly cannot read; measure the proxies and label the rest.** No API setting returns a model's raw chain of thought: OpenAI's `reasoning.summary` returns a summary at best and the reasoning item itself is `encrypted_content`; Anthropic's `display` takes `summarized`, `omitted` or `updates`, defaults to `omitted` on the newest models, and bills the full thinking tokens in every case. Copilot CLI exposes no summary control (its OpenTelemetry export carries `gen_ai.request.reasoning.level` and `gen_ai.usage.reasoning.output_tokens`, not text); Claude Code's `showThinkingSummaries` changes what the UI shows (measured: no change: with the setting on and off, the transcript thinking block on Fable 5.1 was empty (signature only) while 102 and 129 thinking tokens were billed - the setting changes the display only where the API returns summarized thinking). So a profile reports **reasoning visibility** (SP-17: text on disk as a share of billed reasoning) and reads the signals that *are* recorded — reasoning tokens per step, time-to-first-token, the per-request effort level, and the intent line on every shell call (CT26, SP-18). When visibility is under 10%, every judgement derived from reasoning text is **Inferred**, and the drift indicators (fan-out, re-reads, cap firings, goal-state presence) carry the finding instead.

## 4. How this composes

| Standard | Its question | This document's question |
|---|---|---|
| `no-guessing-protocol.md` | do you know this *contract*? | do you know this *behaviour*? |
| `end-to-end-integrity.md` E15 | did you read *our code*? | did you read *its telemetry*? |
| `observability-and-instrumentation.md` | is the telemetry *well-shaped*? | does the telemetry *exist*? |
| `ci-and-test-efficiency.md` CE1 | profile before optimizing | you cannot profile what does not emit |
| `execution-graph-optimization.md` GO3/GO18 | is the bottleneck measured? | record planned vs **actual** |
| `testing-strategy.md` | is correctness proven? | is behaviour observable? |

The through-line: **the pack's whole posture is that a claim must be established rather than asserted.** Instrumentation is what makes that posture affordable for the one class of claim you cannot establish by reading — what the system actually did.

---

## 5. Self-verification checklist

- [ ] The questions an operator will ask within a month are **written down**, each with a named emitting source (IO2).
- [ ] Every such measurement is emitted on the **normal path**, not behind a flag or only on error (IO10.2).
- [ ] At least one real run has been observed and the value **read back** (IO10.3, E14).
- [ ] No claim about deployed behaviour rests on reading the source; the **tells** in IO3 were swept.
- [ ] **Cost axes** — latency, spend/tokens, volume, failure rate — are instrumented, not just correctness (IO6).
- [ ] The agent's own run recorded its **duration** automatically via the AL4a start marker — not opt-in, not forgotten (IO5).
- [ ] Any modeled figure is labelled **Inferred**, states its model, and **names the instrumentation gap** that forced it (IO7).
- [ ] Every measurement path **degrades to "not recorded"** rather than to a plausible wrong value (IO8).
- [ ] Gaps hit during the work were **closed**, not modeled around (IO9).
- [ ] New measurement paths were **observed working and observed declining** to emit a wrong value (IO11, CI6).
- [ ] Nothing high-cardinality entered a metric label; emission is deliberate, not maximal (IO12, O13).

---

## 6. References

- **`observability-and-instrumentation.md`** — the shape of what IO1 requires to exist: the OTel data model, trace correlation, semantic conventions, stable error codes, O12 (load-bearing telemetry is tested), O13 (cardinality and cost).
- **`no-guessing-protocol.md`** — NG1's three moves (check / mark / ask) applied to behaviour; NG3's tell-list is the model for IO3; NG6 is why IO7 labels a model as Inferred.
- **`end-to-end-integrity.md`** — **E15** (never assert own-code shape from memory) is the compile-time sibling of IO3; **E14** (an exit code is not a result — read the state back) is IO10.3.
- **`ci-and-test-efficiency.md`** — CE1–CE3: profile before optimizing, and the measured lesson that the bottleneck is reliably not the suspect.
- **`execution-graph-optimization.md`** — GO3 (measured, or labelled Inferred) and GO18 (record planned vs actual); the back-test that had to model time and tokens is IO1's worked failure.
- **`continuous-improvement.md`** — CI6's control ladder: an instrumentation gap closed by a control outranks one closed by a note.
- **`scripts/audit-log.py`** — `start --session` (at grounding) + automatic pickup in `append` are the pack's own IO5 implementation, added after this exact gap was hit and then made **default-on** because an opt-in flag is a measurement waiting to be forgotten.
- **`audit-and-change-log.md`** — **AL4a** (mark the start as the first action) and **AL5** (append as the last); together they make every skill run self-timing.
