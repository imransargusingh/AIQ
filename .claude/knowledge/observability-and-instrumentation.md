---
load: glob
applyTo: "**/*.py,**/*.ts,**/*.tsx,**/*.cs,**/*.go,**/*.rs,**/*.java,**/*.js"
---
# Observability & Instrumentation Standard

*Normative guidance for how code emits telemetry: logs, traces, metrics, and errors. It governs any agent that writes or modifies code in this repository and is the observability companion to the Body of Knowledge and the Testing Strategy — the BoK says failures must be diagnosable; this file defines how that diagnosability is built in.*

Normative keywords (**MUST**, **SHOULD**, **MAY**, **MUST NOT**) follow RFC 2119.

The governing idea: **adopt the OpenTelemetry (OTel) data model as the conceptual default for all telemetry, regardless of the concrete logging or tracing library in use.** OTel is not a logging framework you must adopt wholesale; it is a vendor-neutral data model that existing libraries map into. Whether the concrete sink is `Microsoft.Extensions.Logging`, Serilog, log4net, `slog`, Logback, or stdout, the *shape* of what you emit — a structured record with a normalized severity, a body, semantic-convention attributes, and trace context — is the OTel shape. This makes telemetry portable across services, languages, and backends, and makes logs, traces, and metrics correlatable rather than three disconnected piles.

---

## 0. Authority and adaptation

- **Repo target wins.** Before using any API, exporter, or package, verify the installed version and the project's existing telemetry stack (`Directory.Packages.props`, the relevant `.csproj`, the bootstrap in `Program.cs`). Conform to the established pattern; a competing one is a recorded deviation (BoK Part V.1).
- **The data model is the constant; the implementation is not.** The OTel data model is the conceptual default even when the exporter, collector, or backend is something else and even when no OTel SDK is wired yet. Map your concrete logging into this model; do not invent a bespoke telemetry schema.
- **Unknowns are surfaced, not guessed.** If the installed exporter, the propagation format already in use, or the right log level cannot be established, surface the gap rather than guessing or silently skipping a directive (BoK D2/D3).
- **Ownership.** The **SRE & Systems Diagnostician** owns this lens and enforces it at the design and pre-merge gates. The **Privacy & Data Governance Counsel** owns O11 (no sensitive data in telemetry). The **Test Architect** enforces O12 (load-bearing telemetry is tested). For C#, the **C# Style Guide** governs the code shape of the instrumentation itself.

---

## 1. Prime directives

1. **Telemetry is designed, not sprinkled.** The spans a unit of work opens, the events it logs, the error codes it can emit, and the metrics it reports are part of a component's design and contract — decided up front, not added reactively when something breaks in production.
2. **Correlation is the point.** A log that cannot be tied to the request, trace, and span that produced it is nearly worthless in a distributed system. Every emitted record that runs inside an operation MUST carry that operation's trace context.
3. **Structure over prose.** Telemetry is machine-read first and human-read second. Emit structured, parseable records (key-value / JSON) with named fields — never preformatted, string-interpolated messages that must be parsed back apart.
4. **Errors are addressable.** Every failure path carries a stable, documented error code and the correlation context needed to find its logs. A human-readable message is for humans and may change; the code is for machines and search and does not.
5. **The model is the lingua franca.** Default to the OTel data model and its three signals so telemetry composes across the system, regardless of the concrete library behind any one service.

---

## 2. Agent algorithm

When implementing or modifying code that does observable work (handles a request, runs a job, processes a message, calls a dependency, or can fail):

1. Identify each **unit of work** and ensure it runs inside a **span** (trace). The span's `trace_id`/`span_id` is the correlation key for everything the unit emits.
2. Apply **O1–O6** (the always-on record shape) to every log the unit emits.
3. Apply every directive triggered by the unit's shape (HTTP boundary → O8, O9; cross-service call → O3; failure path → O7; new metric → O10, O13).
4. Treat any **sensitive data** under O11 before it can reach a sink.
5. For anything **load-bearing** — an error code a caller branches on, a span a runbook depends on — write the assertion under O12.
6. Verify installed package versions and the repo's existing wiring before emitting instrumentation code (§0).
7. Run the self-verification checklist in §6.

---

## 3. The record shape (always-on directives)

**O1 — Structured logging only.** Emit structured records with a stable message template and named fields, not interpolated strings. The fields are queryable; a baked string is not.
- C#: `logger.LogInformation("Order {OrderId} shipped to {Region}", orderId, region);` — the template and `{OrderId}`/`{Region}` are preserved as structured fields. **Never** `logger.LogInformation($"Order {orderId} shipped to {region}")` — string interpolation destroys the structure before it is recorded.

**O2 — Every record carries trace context where it exists.** When a record is emitted inside an active operation, it MUST include `trace_id` and `span_id` per the OTel log data model (the fields derive from W3C Trace Context). If `span_id` is present, `trace_id` MUST also be present. This is the single most important property for debugging: it is what lets you pivot from a log line to the full trace.
- C#: when logs are emitted inside an active `Activity` and the OpenTelemetry .NET SDK is wired to `Microsoft.Extensions.Logging`, `TraceId`/`SpanId`/`TraceFlags` are populated automatically. Do not hand-roll a correlation field when the platform supplies it from the active span.

**O3 — Propagate context across boundaries with W3C Trace Context.** Cross-process and cross-service calls MUST propagate context using the W3C `traceparent` (and, where used, `tracestate`) headers — not a bespoke `X-Correlation-ID` / `X-Request-ID` of your own invention. `traceparent` is `version-traceid-parentid-traceflags` (e.g. `00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01`): a 16-byte trace-id (32 hex), an 8-byte span-id (16 hex), and a trace-flags byte whose low bit is the sampling decision. It is the standard replacement for proprietary headers like `X-Cloud-Trace-Context`, `X-Amzn-Trace-Id`, and `X-B3-TraceId`.
- A W3C trace-id and a legacy correlation-id are **not** the same thing: a trace-id links every span in a distributed trace with parent-child hierarchy and a sampling flag; a flat correlation-id ties operations from one user action together but carries no span structure. Prefer the trace-id. If a legacy correlation-id must coexist (e.g. a partner contract), carry it as an attribute alongside — but the trace-id is the primary correlation key.

**O4 — Every unit of work has a correlation identity.** Each inbound request, scheduled job, and message-handler invocation MUST run inside a span so that all of its logs, downstream calls, and errors share one `trace_id`. There are no "orphan" logs without correlation context in a path where context is available.

**O5 — Severity uses the OTel severity model.** Map log levels to OTel's normalized severity (`SeverityNumber` with `SeverityText`), where smaller numbers are less severe and larger are more severe: TRACE (1–4), DEBUG (5–8), INFO (9–12), WARN (13–16), ERROR (17–20), FATAL (21–24). Reserve ERROR and above for actionable conditions; routine expected outcomes are not errors. Consistent severity is what makes cross-service alerting and filtering possible.

**O6 — Attributes follow semantic conventions.** Use OTel **semantic-convention** attribute names for common concepts (`http.request.method`, `url.path`, `server.address`, `db.system`, `messaging.system`, …) rather than ad-hoc keys, so dashboards and queries work across services. Exception data attached to a record MUST follow the OTel **exception** semantic conventions (`exception.type`, `exception.message`, `exception.stacktrace`). Attributes are for structured context; the body is for the human-readable event.

---

## 4. Errors (triggered directives)

**O7 — Errors carry a stable error code.** Every distinct failure condition MUST be assigned a stable, namespaced, machine-readable error code (e.g. `ORD-0007`, `AUTH.TOKEN_EXPIRED`). The code is stable across releases, documented, and the thing both log search and programmatic handling key on. The human-readable message is separate and MAY change without changing the code. Define codes in one registry (an `enum` or a constants table), not as scattered string literals.

**O8 — HTTP error responses use RFC 9457 Problem Details.** An HTTP API MUST return machine-readable errors as `application/problem+json` per RFC 9457 (which obsoletes RFC 7807): `type` (a URI identifying the problem type, ideally linking to docs), `title` (a short, stable, per-type summary), `status` (the HTTP status), `detail` (an explanation specific to this occurrence, focused on how to correct it rather than on debugging), and `instance` (a URI for this occurrence). Carry the trace id as an extension member (e.g. `"traceId": "…"`) so a client-reported error can be tied straight to server-side logs. Validation problems SHOULD use the `errors` extension with a JSON Pointer per item.
- C# (.NET 8+): use the built-in `ProblemDetails`, `AddProblemDetails()`, `IProblemDetailsService`, an `IExceptionHandler`, and `UseExceptionHandler` — do not hand-format error JSON. Map each error code (O7) to a problem `type` URI.

**O9 — Log trace headers at the edge.** At inbound and outbound boundaries, the trace headers (`traceparent`/`tracestate`) SHOULD be logged so a trace can be reconstructed across systems that do not share a backend.

---

## 5. Signals, cost, and safety (triggered directives)

**O10 — Three signals, used deliberately.** Instrument with the right signal, not reflexively with a log: **traces** for causality and latency across operations (instrument spans first — logs hang off them), **metrics** for aggregate health (latency, throughput, error rate, saturation — the RED/USE families), and **logs** for discrete events that need detail. A question best answered by a metric or a span SHOULD NOT be answered by scraping logs.

**O11 — No sensitive data in telemetry.** Telemetry MUST NOT contain secrets, credentials, tokens, full PII, or financial identifiers (card / account numbers). Redact, hash, or omit. This is a hard line owned by the Privacy & Data Governance Counsel; an attribute is not exempt because it is "just for debugging."

**O12 — Load-bearing telemetry is part of the contract and is tested.** Where a span, a key log event, an error code, or a metric is something a consumer, an alert, or a runbook depends on, it is part of the component's contract and MUST be asserted in a test (e.g. "on a downstream timeout, the handler emits error code `ORD-0007` within an active span and returns an RFC 9457 `503` problem document"). A change to emitted telemetry is a contract change and carries a regression gate, exactly as a prompt/schema change does (Testing Strategy A6).

**O13 — Cardinality and cost discipline.** High-cardinality or unbounded values (user ids, request ids, raw URLs) MUST NOT be used as metric label/dimension values — they belong in span/log attributes (or exemplars), not in metric series. Sample traces deliberately; keep logs structured so they are cheap to index. Telemetry that bankrupts the budget gets turned off, which is worse than telemetry designed to be affordable.

---

## 6. Self-verification checklist

Before declaring instrumentation complete, confirm:

- [ ] Every unit of work runs inside a **span**; its `trace_id`/`span_id` is the correlation key (O2, O4).
- [ ] Every log is **structured** (template + named fields), never an interpolated string (O1).
- [ ] Cross-boundary calls propagate **W3C `traceparent`** (not a bespoke correlation header); any legacy id rides as an attribute only (O3).
- [ ] Severity uses the **OTel severity model**; ERROR+ is reserved for actionable conditions (O5).
- [ ] Attributes use **semantic conventions**; exceptions use the exception conventions (O6).
- [ ] Every failure path emits a **stable, documented error code** from the registry (O7).
- [ ] HTTP errors are **RFC 9457 problem documents** carrying the trace id; .NET uses built-in `ProblemDetails`/`IExceptionHandler` (O8).
- [ ] The right **signal** was chosen (trace/metric/log), not a reflexive log (O10).
- [ ] **No secrets/PII/credentials** in any attribute, body, or metric (O11).
- [ ] **Load-bearing** telemetry has a test; telemetry changes are gated as contract changes (O12).
- [ ] No high-cardinality values in **metric labels** (O13).
- [ ] Installed package versions and the repo's existing telemetry wiring were verified before emitting code (§0).

---

## 7. C# / .NET reference (patterns, not pins — verify versions)

- **Logging:** `Microsoft.Extensions.Logging` `ILogger<T>` with message templates (structured). Prefer `LoggerMessage`-source-generated logs on hot paths.
- **Tracing:** `System.Diagnostics.ActivitySource` / `Activity` are .NET's OTel-aligned span primitives; start an `Activity` per unit of work. The OpenTelemetry .NET SDK exports them and auto-correlates logs emitted within an active `Activity`.
- **Wiring:** the OpenTelemetry .NET SDK (`OpenTelemetry.Extensions.Hosting` + instrumentation packages) with an OTLP exporter; register tracing, metrics, and logging providers together so all three signals share resource and trace context.
- **Metrics:** `System.Diagnostics.Metrics.Meter` (`Counter`, `Histogram`, `UpDownCounter`) — the OTel-aligned metrics API.
- **Errors:** define error codes as a stable `enum`/registry; map each to a problem `type` URI; return RFC 9457 documents via `AddProblemDetails()` + `IExceptionHandler` + `UseExceptionHandler` (.NET 8+).
- Apply the **C# Style Guide** to the instrumentation code itself (nullable, `CancellationToken` threading, no `async void`).

---

## 8. References

The directives above synthesize these primary sources; consult them for detail.

- **OpenTelemetry — Logs Data Model.** https://opentelemetry.io/docs/specs/otel/logs/data-model/
- **OpenTelemetry — Semantic Conventions** (general, HTTP, database, exceptions). https://opentelemetry.io/docs/specs/semconv/
- **OpenTelemetry .NET — log/trace correlation.** https://github.com/open-telemetry/opentelemetry-dotnet
- **W3C — Trace Context** (`traceparent` / `tracestate`). https://www.w3.org/TR/trace-context/
- **RFC 9457 — Problem Details for HTTP APIs** (obsoletes RFC 7807). https://www.rfc-editor.org/rfc/rfc9457
- **.NET — Problem Details & `IExceptionHandler`.** https://learn.microsoft.com/aspnet/core/fundamentals/error-handling
