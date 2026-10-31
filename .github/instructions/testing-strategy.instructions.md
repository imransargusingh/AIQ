---
applyTo: "**/tests/**,**/test/**,**/*.test.*,**/*.spec.*,**/test_*.py,**/*_test.go"
---
# Testing Strategy for AI Coding Agents

*Normative guidance for deciding what to test, how to test it, and what evidence is strong enough to claim correctness.*

This file governs any agent that writes or modifies code in this repository. It is the testing companion to the Body of Knowledge: the BoK says correctness must be demonstrated; this file defines the test selection and quality bar used to demonstrate it.

> **Execution economics is a companion, not this file's job.** This file governs *what* to test and *what counts as proof*. **How that verification is executed cheaply and quickly without weakening the gate** — runner economics, rings of integration, build-once, profiling the suite's real bottleneck, and the cost-budget control — is governed by `ci-and-test-efficiency.md` (CE1–CE26). The two compose: same coverage, fewer minutes and dollars. D0 determinism (no wall-clock, no sleeps) is the shared floor — the TEST-CLOCK defect class lives on both sides.

Normative keywords (**MUST**, **SHOULD**, **MAY**, **MUST NOT**) follow RFC 2119.

---

## 0. Authority and adaptation

- **Repo target wins.** The consuming repository's pinned target framework and installed test stack are authoritative — verify them in `Directory.Packages.props` and the relevant `.csproj` before using any API or template. Absent a repo pin, default to the **latest stable** SDK/language (currently **.NET 10 / C# 14**). A representative stack: xUnit, FluentAssertions, NSubstitute, AutoFixture, bUnit, Verify.Xunit, NetArchTest.Rules, and coverlet.
- **Templates are patterns, not pins.** PactNet, Testcontainers, FsCheck, WireMock.Net, NJsonSchema, Stryker.NET, and SDK APIs change across major versions. Do not assume an example compiles as written. If a directive needs a package not already installed, follow the dependency-adoption protocol and central package management rules before adding it.
- **Thresholds are tunable defaults.** Mutation score >=80%, eval pass-rate >=90%, Stryker 70/75/85, and judge-human agreement of 75-90% are starting points. Repo-defined thresholds override them; raise thresholds for safety-critical, data, money, or irreversible paths and relax only with a recorded rationale for low-stakes tooling.
- **Unknowns are surfaced.** If the package version, API surface, or appropriate threshold cannot be established, the agent MUST surface the gap instead of guessing or silently skipping a directive.

---

## 1. Prime directives

1. **Coverage is a floor, never a target.** The agent MUST NOT write tests whose purpose is to raise a line-coverage number. Line coverage proves execution, not verification. Low coverage is a warning; high coverage is not a proof.
2. **Mutation resistance is the real signal for deterministic logic.** Domain and business-rule suites MUST be strong enough to catch flipped booleans, off-by-one errors, deleted assertions, and equivalent injected faults. A test suite that survives simple mutants is theater.
3. **The mock is a debt instrument.** Prefer real implementations, real filesystem temp directories, Testcontainers, in-process fakes, or recorded/replayed dependencies before hand-rolled substitutes. Every boundary mock/substitute/fake MUST be paired with a contract, schema, or fidelity test that validates its assumptions.
4. **Determinism lives at structure; probability lives at content.** For AI-composed systems, split deterministic tests (schema, routing, parsing, tool choice, argument shape) from probabilistic tests (semantic quality). Never exact-match generated natural-language content.
5. **Prompt and schema changes are contract changes.** Prompt files, system messages, tool descriptions, payload schemas, and skill markdown that shapes model behavior MUST be treated as versioned contracts with regression protection.

---

## 2. Agent algorithm

When implementing or modifying code:

1. Identify the interaction model by matching the code shape against the trigger table in section 3.
2. Apply **D0 Test Hygiene & Determinism** to every test, unconditionally.
3. Apply every directive linked to every matching trigger. Use the union of directives, not the first match.
4. Verify installed package versions and local test patterns before emitting code.
5. Use repo-defined thresholds where present; otherwise use section 0 defaults and tune by risk.
6. Write the mandated tests before declaring the implementation complete.
7. Run the self-verification checklist in section 9 against the produced tests.
8. If a MUST cannot be satisfied, report the gap as unverified residual risk; do not replace it with a weaker test and call it done.

---

## 3. Trigger table

| Trigger | Signal in the code being written | Interaction model | Required directives |
|---|---|---|---|
| T1 | Pure functions, domain entities, calculations, business rules, no I/O | Deterministic logic | D1 |
| T2 | Parsers, serializers, validators, math, or wide/unbounded input domains | Deterministic logic | D2 |
| T3 | New namespace/layer, project reference, dependency direction, composition contract | Structural | D3 |
| T4 | Database, cache, queue, broker, filesystem, or persistence behavior | Integration | D4 |
| T5 | Code consumes another service's HTTP/gRPC API | Distributed consumer | D5-consumer |
| T6 | Code exposes an API or tool other consumers call | Distributed provider | D5-provider |
| T7 | Code produces or consumes event/message/payload schemas | Distributed payload | D6 |
| T8 | A substitute/mock/stub/fake is written at a boundary | Any | D7 |
| T9 | Deterministic wrapper around an AI call: prompt assembly, parsing, routing | AI-composed | A1 |
| T10 | MCP server, MCP client, or tool schema/description | AI-composed | A2 |
| T11 | LLM output is consumed as JSON or typed structured data | AI-composed | A3 |
| T12 | Multi-step agent or tool-calling workflow | AI-composed | A4 |
| T13 | Value depends on semantic quality of generated content | AI-composed | A5 |
| T14 | Prompt, system message, tool description, or skill markdown is added/edited | AI-composed | A6 |

---

## 4. D0 -- Test Hygiene & Determinism

D0 applies to every test the agent writes.

**Structure and intent**

- Tests MUST follow Arrange-Act-Assert with phases visually separable.
- Test names MUST read `Method_State_ExpectedOutcome`.
- Each test SHOULD verify one behavior. Split unrelated assertions rather than creating assertion roulette.
- Non-obvious expected values MUST be named constants, builders, or comments that explain why the value is expected.

**Focal-method guarantee**

- Every test MUST invoke the real method or observable behavior under test.
- Every test MUST contain at least one meaningful assertion. Assertion-free tests, `Assert.True(true)`, and `Assert.NotNull` on never-null values are forbidden.

**Determinism and isolation**

- Tests MUST NOT depend on wall-clock `DateTime.Now` / `DateTimeOffset.Now`, unseeded randomness, `Guid.NewGuid()` in assertions, current culture, machine-local state, or developer credentials.
- Tests MUST NOT use `Thread.Sleep` or fixed delays to wait for async work. Await the operation, poll with a bounded timeout, or use a scheduler/fake clock.
- Tests MUST be order-independent, hermetic, parallel-safe unless the project explicitly disables parallelism for a known platform reason, and responsible for their own setup/teardown.
- Tests MUST NOT call third-party network services. Use a local fake, WireMock, Testcontainers, recorded payloads, or a protocol test boundary.
- Flaky tests are defects. The agent MUST fix the root cause rather than adding retries or silently skipping the test.

---

## 5. Deterministic, structural, and distributed directives

### D1 -- Unit + Mutation

- Unit tests MUST assert behavior and boundary cases, not private structure or incidental implementation.
- Domain/business logic tests MUST be mutation-resistant: a flipped branch, off-by-one, deleted guard, or removed assertion should fail a test.
- Where Stryker.NET is configured or proportionate to the risk, run it against the changed namespace/file and address surviving mutants. If mutation tooling is not wired, record mutation confidence as inferred or unverified in the Proof Pack.

### D2 -- Property-Based

- Wide input domains MUST be tested with invariants/laws when examples cannot cover the meaningful space: round trips, idempotency, monotonicity, commutativity, range bounds, and "never throws on valid input."
- A shrunk failing counterexample MUST become a permanent `[Fact]` regression test.

### D3 -- Architecture

- Dependency direction and layer boundaries MUST be encoded as architecture tests when a change adds a layer, project reference, or cross-layer contract.
- Existing architecture and composition contract tests are load-bearing and MUST remain green.

### D4 -- Real-Infra Integration

- Behavior that depends on a real engine MUST be tested against the real engine or a high-fidelity equivalent. Do not mock SQL semantics, broker delivery, filesystem atomicity, or serialization behavior.
- For filesystem persistence in this repo, use the `TempDirectory` fixture pattern and real files/directories. Do not mock `System.IO`.
- In-memory providers MAY be used only for trivial wiring checks and MUST NOT be represented as proof of real persistence semantics.

### D5-consumer -- Consumer Contract

- When code consumes an external HTTP/gRPC API, write a consumer contract for the exact request/response shape depended on. Use matchers for flexible values and exact strings only where semantics require exactness.
- If Pact or equivalent tooling is not present, either add it through the dependency protocol or record the missing contract test as residual risk.

### D5-provider -- Provider Verification

- When code exposes an API/tool consumed by others, provider verification MUST run against published consumer contracts where such contracts exist.
- Deployments of provider contracts SHOULD be gated with `can-i-deploy` or an equivalent compatibility gate. Provider responses SHOULD also be checked against the published OpenAPI/schema to detect bidirectional drift.

### D6 -- Schema + Golden Payload

- Message and payload contracts MUST have schemas where practical: JSON Schema, OpenAPI, AsyncAPI, Avro, or a typed equivalent enforced by tests.
- Realistic, PII-scrubbed production/staging payloads SHOULD be committed as versioned fixtures when policy permits. Synthetic fixtures MUST be labeled as synthetic and must cover known boundary cases.
- Backward-compatibility fixtures MUST keep passing unless the breaking change is deliberate and documented.
- Deserializers SHOULD have Postel-style tests for unknown extra fields and missing optional fields when forward compatibility is part of the contract.

### D7 -- Mock Fidelity Pairing

- Apply this fidelity hierarchy before creating a boundary substitute: real implementation -> Testcontainers/real local engine -> in-process fake -> recorded/replayed WireMock -> hand-rolled substitute -> auto-generated mock.
- Any step down the hierarchy MUST be justified by test scope, cost, or determinism.
- Every boundary substitute/mock/fake MUST be paired with a D5 contract, D6 schema/golden-payload test, or equivalent fidelity test that checks the substitute's assumptions against reality.

---

## 6. AI-composed system directives

### A1 -- Adapter Boundary

- Every model call MUST sit behind an adapter that validates structure before returning to downstream code.
- Prompt assembly, parsing, routing, and retry/error mapping are deterministic wrapper code and MUST be unit-tested with canned model outputs.
- Prompt assembly MUST be snapshot/golden-file tested with Verify or the repository's established snapshot tool.

### A2 -- MCP Protocol + Semantic

- MCP protocol behavior is deterministic and MUST be tested every commit: declared schema conformance, deterministic invalid-parameter errors, and no stdout pollution.
- Semantic tool usability is probabilistic and SHOULD be tested pre-release with real model prompts that exercise tool selection and argument shape.
- If multiple models misuse a tool, treat the tool description/schema as suspect before treating the test as flaky.

### A3 -- Structured Output Contract

- LLM structured output MUST be validated at the boundary before use.
- Every malformed structured output observed in development or production SHOULD become an adversarial fixture and deterministic regression test.

### A4 -- Tool Call Trace

- Multi-step agent workflows MUST assert on execution trace structure: tools called, order, and argument keys/types.
- Tests MUST NOT assert exact model-generated argument values unless the value is deterministic by construction; use A5 for semantic quality.

### A5 -- Eval Suite

- Generated-content quality MUST be evaluated against a golden dataset that covers common, edge, and adversarial cases.
- Code-based evals SHOULD run at commit cadence for deterministic dimensions: schema, required elements, forbidden content, latency, token budget, and hallucinated identifiers.
- LLM-as-judge evals, when used, MUST use analytic rubrics and threshold pass rates rather than exact output matches. The judge SHOULD be calibrated against human labels before it is trusted.

### A6 -- Prompt Version Gate

- Changing a prompt, system message, tool description, skill instruction, or payload schema MUST run regression protection against old and new behavior.
- Previously passing golden cases MUST NOT regress without an explicit recorded decision. Track per-case deltas, not just aggregate pass rate.

---

## 7. Quality stack reference

| ID | Layer | Cadence | Main triggers |
|---|---|---|---|
| L0 | Test hygiene and determinism | Every test | All |
| L1 | Unit + mutation resistance | Every commit / Proof Pack | T1 |
| L2 | Property-based invariants | Every commit when applicable | T2 |
| L3 | Architecture tests | Every commit | T3 |
| L4 | Real-infra integration | PR gate / relevant suite | T4 |
| L5 | Consumer/provider contract | Commit + deploy gate | T5, T6 |
| L6 | Schema + golden payload | Every commit | T7 |
| L7 | MCP protocol | Every commit | T10 |
| L8 | Tool-call trace | Commit / pre-release | T12 |
| L9 | Structured output contract | Every commit | T11 |
| L10 | Code-based eval | Every commit when harness exists | T13 |
| L11 | LLM-as-judge eval | Pre-release + sampled production review | T13, T14 |

---

## 8. Anti-patterns the agent must refuse

- **Coverage theater:** tests written only to increase a percentage.
- **No focal call:** tests that never invoke the behavior under test.
- **Assertion-free or assertion-lite tests:** no meaningful assertion, `Assert.True(true)`, or `Assert.NotNull` on values that cannot be null.
- **Assertion roulette:** many unrelated assertions in one test.
- **Magic expected values:** unexplained literals that hide the reason for the expected result.
- **Hallucinated test APIs:** packages, overloads, attributes, or methods not present in the installed version.
- **Conditional assertions:** `if`, `for`, or `try` logic that controls whether assertions run.
- **Mock fiction:** a substitute at a boundary with no contract/schema/fidelity test.
- **Implementation mirror tests:** tests that duplicate current implementation rather than asserting the spec.
- **Probabilistic exact match:** exact-string assertions against generated natural language or exact model-chosen values.
- **Prompt/schema drift without a gate:** changing prompts, schemas, tool descriptions, or skill instructions without regression protection.

---

## 9. Self-verification checklist

For each test:

- Does it invoke the focal method or observable behavior?
- Does it contain at least one meaningful assertion?
- Does it verify one behavior, or are unrelated concepts split?
- Does the name read `Method_State_ExpectedOutcome`?
- Are non-obvious expected values named or explained?
- Is it deterministic: no wall-clock, unseeded randomness, sleeps, culture dependence, environment dependence, or real third-party network?
- Is it isolated and safe to run independently?
- Are all APIs and package symbols verified against installed versions?
- Are assertions straight-line rather than conditionally gated?

For the suite:

- Were all matching triggers in section 3 applied as a union?
- Is every boundary substitute paired with a fidelity, contract, or schema test?
- Are deterministic AI structure checks separated from probabilistic content evaluation?
- Are mutation/eval thresholds wired or explicitly recorded as unverified residual risk?
- Are all unsatisfied MUST directives surfaced rather than silently skipped?
