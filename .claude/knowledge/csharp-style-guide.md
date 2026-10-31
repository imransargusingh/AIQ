---
load: glob
applyTo: "**/*.cs,**/*.csx"
---
# C# Coding Style Guide

*Version 2.0 — Default target platform: the latest stable LTS / C# version (currently .NET 10 (LTS) / C# 14). This is the portable baseline; a consuming repository's own target framework is authoritative where it differs (see that repo's `.github/instructions/csharp-style-guide.instructions.md` / `Directory.Build.props`).*

A specification for writing C# that is legible, intentional, and durable. The guide is opinionated; defaults exist so reviewers can spend energy on design, not formatting.

**Scope and relationship.** This guide governs *how C# is written*. It is one of a coherent set: the **Agent Body of Knowledge** governs how the agent reasons and researches; the **Layered Optimized Architecture (LOA)** governs how AI-integrated systems are designed (its Appendix D is the authoritative .NET idiom map, not duplicated here). Where LOA or the BoK speak, defer to them; this guide is authoritative over C# legibility and the language-level conventions below. Sections 1–4 are the original craft core; sections 5–8 extend it to concurrency, type design, security, and observability; the Enforcement appendix makes the whole guide mechanically checkable.

---

## 1. Self-Documenting Code

The reader is the primary customer. Code should be understandable without comments, debuggers, or tribal knowledge. Comments explain *why*; code explains *what* and *how*.

### 1.1 Names reveal intent, not implementation

A name is a contract. If the name is wrong, the abstraction is wrong.

```csharp
// ✗ Implementation leaks into name
List<Order> GetOrdersFromDb(int customerId)

// ✓ Intent, free of storage detail
IReadOnlyList<Order> OrdersFor(CustomerId customer)
```

Names earn their length. A loop counter may be `i`; a domain concept never is. Avoid the noise words `Manager`, `Helper`, `Processor`, `Util`, `Data`, `Info` — they signal that the author could not name the responsibility.

### 1.2 Prefer expressive types over primitives

Primitive obsession is the single largest source of ambiguous code. A `string customerId` and a `string orderId` are interchangeable to the compiler and disastrous in production.

```csharp
// ✗ Three strings, any order
public Receipt Charge(string customerId, string orderId, string currency, decimal amount)

// ✓ Wrong call site fails to compile
public Receipt Charge(CustomerId customer, OrderId order, Money amount)
```

Use `record struct` or `readonly record struct` for identifiers and value objects. The cost is one declaration; the benefit is compile-time safety and self-naming parameters.

### 1.3 Boolean parameters are a code smell

At the call site, `true` and `false` are anonymous.

```csharp
// ✗ What does true mean here?
user.Save(true);

// ✓ Either split the method...
user.SaveAndFlush();

// ✓ ...or name the intent
user.Save(SaveMode.Immediate);
```

### 1.4 Comments document *why*, never *what*

If a comment paraphrases the code, delete one of them. Comments are reserved for: non-obvious *why*, references to external contracts (RFCs, tickets, regulations), and warnings about constraints that aren't visible in the code.

```csharp
// ✗ Restates the obvious
// Increment the counter
counter++;

// ✓ Captures invisible context
// Retry budget per AWS SDK guidance (see ADR-014); exceeding it
// triggers circuit-breaker fallback rather than caller-visible failure.
const int MaxRetries = 3;
```

### 1.5 Code reads top-down at one level of abstraction

A method should read like a paragraph. Higher-level orchestration sits at the top of a file; lower-level helpers sit below. A single method should not mix `HttpClient.SendAsync` with domain rule evaluation — the reader has to context-switch on every line.

### 1.6 No commented-out or dead code

Commented-out code is not documentation — it is rot with a delay timer. It goes stale silently, misleads the next reader about what the code actually does, and defeats every tool that reasons about the tree (a code-graph builder, an analyzer, a `grep`). **Version control is the archive; the working tree is not.** Deleted code is one `git log` away, so *delete* it — never park it in a comment "in case."

```csharp
// ✗ Rot: which of these is the intended path? Neither the reader nor a tool can tell.
// var result = await _legacyGateway.ChargeAsync(order);
// return result.ToLegacyDto();
var result = await _paymentPipeline.ChargeAsync(order, ct);
return result.ToDto();
```

The same applies to *dead* code that is not commented out — an unreferenced method, an unreachable branch, a field nobody reads, a `using` nobody needs. If it is not called, remove it; the compiler and analyzers (IDE0051 unused member, IDE0052 unread field, CS0219 unused variable, IDE0059 unnecessary assignment) point at most of it.

Commenting a block out **while you work** is fine — it is a transient scratch. It is *not* fine to **end the turn** with it still there: the cleanup is part of finishing the work, not a follow-up (`communication-and-task-discipline.md` CT18a; defect class **HYG-A**).

---

## 2. Terse Methods

A method has exactly one job: the one its name describes. If the name needs `And`, the method needs splitting.

### 2.1 Size

Target ≤10 lines. Methods exceeding 10 lines warrant scrutiny — *especially* when ternaries, pattern matching, switch expressions, and fluent chains are available. The language provides these compression tools precisely so a method body can express its intent in a single readable sweep rather than a paragraph of branching ceremony.

```csharp
// ✗ 14 lines, mostly scaffolding
public string FormatStatus(Order order)
{
    string result;
    if (order.IsCancelled)
    {
        result = "Cancelled";
    }
    else if (order.IsShipped)
    {
        result = "In transit";
    }
    else
    {
        result = "Pending";
    }
    return result;
}

// ✓ Same logic, no scaffolding
public string FormatStatus(Order order) => order switch
{
    { IsCancelled: true } => "Cancelled",
    { IsShipped: true }   => "In transit",
    _                     => "Pending",
};
```

Ternaries earn their place when the choice is genuinely binary and the alternatives are short. Nested ternaries on a single line are acceptable when each branch is a literal or a trivial expression; once they sprawl across lines, switch to a `switch` expression (see §2.3 on stretched expression bodies).

Length is a symptom, not the disease — the real test is whether the method does one thing at one level of abstraction. A 12-line method can still be doing one thing; a 6-line method can be doing three. But on inspection, methods past 10 lines almost always reveal either a missed extraction or a refusal to use the compression the language already provides.

### 2.2 Guard clauses first, happy path last

Nesting is harder to read than sequence. Reject invalid states early and unindent the meaningful logic.

```csharp
// ✗ Arrow-shaped
public Receipt Process(Order order)
{
    if (null != order)
    {
        if (order.Items.Any())
        {
            if (order.Customer.IsActive)
            {
                // actual work, 3 levels deep
            }
        }
    }
}

// ✓ Linear
public Receipt Process(Order order)
{
    ArgumentNullException.ThrowIfNull(order);
    if (!order.Items.Any()) throw new EmptyOrderException(order.Id);
    if (!order.Customer.IsActive) throw new InactiveCustomerException(order.Customer.Id);

    return ChargeAndConfirm(order);
}
```

### 2.3 Expression-bodied members where natural

For one-liners with no branching, expression bodies remove ceremony.

```csharp
public bool IsExpired => ExpiresAt < DateTimeOffset.UtcNow;
public Money Total() => _items.Sum(i => i.Price);
```

Do not stretch expression bodies across multiple lines with nested ternaries — at that point a block body is more honest.

### 2.4 Extract until the name is the only documentation needed

If a block of code needs a `// Calculate shipping discount` comment, that block is a method. The comment becomes the name; the body becomes the implementation.

### 2.5 No flag parameters

A method that branches on a boolean is two methods sharing a body. Split them. If the shared body is substantial, extract it as a private helper that both callers invoke.

### 2.6 Put the constant on the left in equality checks

Write the literal — `null`, `0`, a named constant — on the **left** of an `==` / `!=` comparison. A mistyped single `=` is then rejected by the compiler (you cannot assign to a literal) instead of silently assigning and branching on the result. The protection bites hardest on `bool` comparisons, where `if (isActive = false)` otherwise compiles into a bug; applied uniformly — null checks included — it is one consistent habit, not a special case.

```csharp
// ✗ ordinary order — a slipped '=' on a bool assigns instead of compares
if (order != null) { … }
if (isActive == false) { … }

// ✓ literal on the left — a slipped '=' won't compile
if (null != order) { … }
if (false == isActive) { … }
```

---

## 3. Fluent Code

Fluency is alignment between how code reads and how the operation is conceived. The goal is not method chaining for its own sake — it's matching syntax to mental model.

### 3.1 Prefer LINQ when it expresses intent

LINQ is fluent when the operation *is* a pipeline. It is obfuscation when forced onto operations that aren't.

```csharp
// ✓ The operation is a pipeline; the code reads as one
var topCustomers = orders
    .Where(o => o.PlacedAt >= cutoff)
    .GroupBy(o => o.CustomerId)
    .Select(g => new { Customer = g.Key, Total = g.Sum(o => o.Amount) })
    .OrderByDescending(x => x.Total)
    .Take(10)
    .ToList();

// ✗ Don't force LINQ onto non-pipeline logic
var result = items.Aggregate(new StringBuilder(), (sb, x) => {
    if (x.IsValid) { sb.Append(x.Name); if (x.HasTag) sb.Append("*"); }
    return sb;
}).ToString();
// A foreach with named variables would be clearer here.
```

### 3.2 Builders for complex construction

When an object has many optional dimensions or a meaningful construction order, a builder reads better than a 9-argument constructor or a soup of property initializers.

```csharp
var request = HttpRequest
    .To("https://api.example.com/orders")
    .WithBearerToken(token)
    .WithHeader("X-Trace-Id", traceId)
    .WithJsonBody(payload)
    .WithTimeout(TimeSpan.FromSeconds(30))
    .Build();
```

Builder methods should return the builder (or a new immutable instance). Never mix `void` mutators with chainable methods in the same fluent API — the inconsistency breaks the reader's flow.

### 3.3 Return `this` or a new instance, never both

Mutable fluent APIs return `this`. Immutable fluent APIs return a new instance. Pick one per type and hold the line — the caller should not need to know which kind they're using.

### 3.4 Fluent assertions in tests

Tests are documentation. Fluent assertions read like specifications.

```csharp
result.Should().NotBeNull();
result.Items.Should().HaveCount(3).And.OnlyContain(i => i.IsActive);
result.Total.Should().Be(Money.Usd(149.97m));
```

### 3.5 Beware temporal coupling in fluent chains

If `.Build()` *must* follow `.WithAuth()` or the result silently misbehaves, the API is lying about being fluent. Encode required steps in the type system using staged builders, or simplify the API.

---

## 4. Exception Practices

Exceptions are for *exceptional* conditions — states the caller cannot reasonably anticipate or recover from in normal flow. Expected failure modes (validation, not-found, business rule violations) often belong in return types, not the exception channel.

### 4.1 Throw typed exceptions

Every thrown exception should be specific enough that a catch block targeting it cannot ambiguously catch unrelated failures.

```csharp
// ✗ Caller cannot distinguish failure modes
throw new Exception("Customer not found");

// ✗ Marker types from the BCL with no domain meaning
throw new ApplicationException("Invalid order state");

// ✓ Domain-specific, catchable, carries context
throw new CustomerNotFoundException(customerId);
```

Custom exceptions live in the domain layer alongside the concepts they describe, not in a `Exceptions/` dumping ground.

### 4.2 Custom exception template

```csharp
public sealed class CustomerNotFoundException : Exception
{
    public CustomerId CustomerId { get; }

    public CustomerNotFoundException(CustomerId customerId)
        : base($"Customer '{customerId}' was not found.")
    {
        CustomerId = customerId;
    }

    public CustomerNotFoundException(CustomerId customerId, Exception inner)
        : base($"Customer '{customerId}' was not found.", inner)
    {
        CustomerId = customerId;
    }
}
```

`sealed` by default. Carry the relevant identifiers as typed properties so logging and handlers don't have to parse the message. Always preserve the inner exception when wrapping.

### 4.3 Legibility is part of the type

A typed exception only pays for itself if the name carries the meaning. The reader's eye should reach the exception type and stop — no message parsing, no inheritance chasing, no log archaeology required.

**The type name *is* the failure.** Pattern: `<Subject><Failure>Exception`. Read aloud, the type should sound like a statement of fact about what went wrong.

```csharp
// ✗ Type name conveys nothing; the string message does the work
catch (OrderException ex) when (ex.Message.Contains("shipped")) { ... }
catch (BusinessException ex) { ... }
catch (RepositoryError ex) { ... }

// ✓ Type name is the failure
catch (OrderAlreadyShippedException) { ... }
catch (InsufficientFundsException) { ... }
catch (CustomerNotFoundException) { ... }
```

**Throw sites read like declarations.** The constructor carries the data; the type carries the meaning. A reviewer reading the diff should see the failure mode in one glance, without parsing a format string.

```csharp
// ✗ Reader must decode a string to learn what happened
throw new InvalidOperationException(
    $"Order {orderId} is in state Shipped and cannot be cancelled");

// ✓ The line itself is the failure
throw new OrderAlreadyShippedException(orderId);
```

**Catch sites read as sentences.** Exception filters compose with typed exceptions to produce clauses that map cleanly to a failure-mode table:

```csharp
catch (PaymentDeclinedException ex) when (ex.Reason is DeclineReason.Fraud)        { ... }
catch (PaymentDeclinedException ex) when (ex.Reason is DeclineReason.OverLimit)    { ... }
catch (HttpRequestException ex)     when (ex.StatusCode is HttpStatusCode.NotFound) { ... }
```

Each clause reads as one row of a behavior specification. Anyone scanning the file can enumerate the handled failure modes without entering the bodies.

**Hierarchies stay shallow and match the domain.** Two levels is usually enough — an abstract base per domain area, sealed leaves per failure mode. Callers can catch at the precision they need without either site losing legibility.

```csharp
public abstract class OrderException : Exception { ... }

public sealed class OrderNotFoundException       : OrderException { ... }
public sealed class OrderAlreadyShippedException : OrderException { ... }
public sealed class OrderPaymentDeclinedException : OrderException { ... }
```

A caller handling *any* order failure catches `OrderException`. A caller handling *one* specific failure catches the leaf. Both catch sites still read as English.

**Avoid suffixes that describe the layer, not the failure.** `OrderRepositoryException` tells the reader where the exception came from; `OrderNotFoundException` tells them what's wrong. Prefer the latter — the layer is rarely what the caller cares about, and layer-named exceptions tend to multiply (`OrderServiceException`, `OrderControllerException`) without adding meaning.

### 4.4 Catch narrowly

Bare `catch` and `catch (Exception)` are nearly always wrong. Catch the exception type you can actually handle.

```csharp
// ✗ Catches OutOfMemoryException, ThreadAbortException, programmer bugs...
try { ... } catch { _logger.LogError("oops"); }

// ✓ Catches what this code knows how to react to
try
{
    return await _client.GetCustomerAsync(id);
}
catch (HttpRequestException ex) when (ex.StatusCode == HttpStatusCode.NotFound)
{
    throw new CustomerNotFoundException(id, ex);
}
```

Exception filters (`when`) are preferred over catching-then-rethrowing — they don't unwind the stack on a non-match, which preserves diagnostics.

### 4.5 Preserve the stack trace

`throw ex;` resets the stack trace and erases the scene of the crime. `throw;` preserves it. When wrapping, pass the original as `innerException`.

```csharp
// ✗ Stack trace now points here
catch (SqlException ex) { _logger.LogError(ex, "db error"); throw ex; }

// ✓ Stack trace preserved
catch (SqlException ex) { _logger.LogError(ex, "db error"); throw; }

// ✓ Wrapping while preserving cause
catch (SqlException ex) { throw new OrderRepositoryException(orderId, ex); }
```

### 4.6 Expected failures aren't exceptions

If a failure is part of normal business flow — invalid input, lookup miss, rule violation — return it. Exceptions are expensive, asymmetric, and obscure control flow when used for everyday outcomes.

```csharp
// ✓ Expected outcome, returned
public Result<Receipt, ChargeFailure> Charge(Order order) { ... }

// ✓ Exceptional outcome (network died, DB corrupt), thrown
public async Task<Customer> LoadAsync(CustomerId id)
{
    // If the DB is unreachable, that's exceptional — let it propagate.
}
```

The distinction is whether the caller's *normal* code path needs to handle it. Normal path → return type. Cliff edge → exception.

### 4.7 Never swallow

The only acceptable empty catch is one that explicitly documents why discarding the exception is correct, and even then it should log at debug level so it's recoverable in diagnostics. If silence is genuinely correct, write a comment that justifies it; otherwise, let it propagate.

### 4.8 Async exception ergonomics

`async void` methods cannot be awaited and their exceptions crash the process. Reserve `async void` for event handlers only. Everything else returns `Task` or `Task<T>`. When aggregating, prefer `Task.WhenAll` and inspect `Task.Exception` for the full `AggregateException` rather than only catching the first.

---

## 5. Concurrency & Asynchrony

§4.8 covers the *exception* ergonomics of async. This section covers the correctness rules that cause the most production incidents.

### 5.1 Flow the `CancellationToken`, always

Every async method that calls another async method takes a `CancellationToken` and passes it down. A method that drops the token on the floor cannot be cancelled, and an uncancellable call in a budgeted or request-scoped path is a latent hang.

```csharp
// ✗ Token accepted, then ignored — the inner call can't be cancelled
public async Task<Order> LoadAsync(OrderId id, CancellationToken ct)
    => await _repo.FindAsync(id); // ct dropped

// ✓ Threaded through
public async Task<Order> LoadAsync(OrderId id, CancellationToken ct)
    => await _repo.FindAsync(id, ct);
```

Cancellation is not an error. Let `OperationCanceledException` propagate; do not catch it as a failure and do not log it at error level (see §8.3).

### 5.2 Never block on async

`.Result`, `.Wait()`, and `.GetAwaiter().GetResult()` on a `Task` invite deadlock and hide exceptions inside `AggregateException`. The fix is to be async the whole way up, not to bridge sync-over-async.

```csharp
// ✗ Deadlock risk in any context with a synchronization context; AggregateException masking
var order = LoadAsync(id, ct).Result;

// ✓ Async all the way
var order = await LoadAsync(id, ct);
```

### 5.3 `ConfigureAwait(false)` in library code

Library and infrastructure code that does not need to resume on the original context uses `ConfigureAwait(false)` to avoid capturing it. Application and UI code that *does* need the context omits it. Pick per layer and be consistent.

### 5.4 Prefer the right primitive

Stream with `IAsyncEnumerable<T>`; bound producer/consumer handoff with `System.Threading.Channels`; aggregate with `Task.WhenAll` (and inspect `Task.Exception` for the full `AggregateException`, per §4.8). Reach for `Parallel.ForEachAsync` for bounded concurrent I/O rather than hand-rolling a `Task` swarm. Inject `TimeProvider` for any delay or timeout so it is testable (Appendix Defaults).

---

## 6. Type Design

Several rules elsewhere in this guide are really one idea: *let the type system carry the contract.* This section states the type-level defaults directly.

### 6.1 Records for values, classes for identity

Use `record` / `readonly record struct` for value objects and DTOs — data defined by what it *contains*. Use `class` for entities defined by *identity and behavior*. This generalizes §1.2 (expressive types over primitives): a `readonly record struct CustomerId(Guid Value)` costs one line and buys compile-time safety and self-naming parameters.

### 6.2 Immutable by default

Default to `readonly` fields and `init`-only or get-only properties; introduce mutability only with intent (Appendix Defaults). Immutable types are trivially thread-safe (§5) and cannot drift into invalid states after construction.

### 6.3 Expose read-only contracts at boundaries

Return `IReadOnlyList<T>` / `IReadOnlyDictionary<TKey,TValue>` from public APIs, not `List<T>` (the examples in §1.1 already do this — here it is the rule). The concrete collection is an implementation detail; exposing it invites callers to mutate your internal state.

### 6.4 Nullability is a contract, not a warning

With nullable reference types `enable`d project-wide (Appendix Defaults), `Customer?` and `Customer` are *different contracts*. Honor them: a non-null return type is a promise. Do not launder nulls with `!` (the null-forgiving operator) to silence the compiler — that discards the contract the type was carrying. Validate at the boundary (§7.2) and let the inside of the system trust its own types.

### 6.5 Use the language's modern leverage

C# 14 / .NET 10 features serve the guide's own goals — reach for them when they reduce ceremony: primary constructors (terser dependency capture, as used throughout LOA's samples), collection expressions (`[..]`), `required` members (construction-time invariants without boilerplate constructors), and the `field` keyword (property logic without a backing-field declaration). Adopt them where they *increase* legibility, not for novelty.

---

## 7. Secure Coding

Legible code that leaks a secret or trusts hostile input is not durable. These are the non-negotiable security defaults for production C#. (For AI-integrated trust boundaries — model output, tool calls, prompt injection — defer to LOA P3, P5, P11 and the Confused Deputy anti-pattern.)

### 7.1 Secrets never live in code or logs

No connection string, API key, token, or password in source, configuration committed to the repo, or log output. Use a secret manager / `IConfiguration` provider backed by a vault. A secret in a string literal is a review-blocking defect.

```csharp
// ✗ Secret in source; leaks to every clone and every log of this object
const string ApiKey = "sk-live-9f3a...";

// ✓ Resolved at runtime from a protected source
string apiKey = _config["Payments:ApiKey"]
    ?? throw new ConfigurationMissingException("Payments:ApiKey");
```

### 7.2 Validate at the trust boundary, on the trusted side

Treat all input crossing a trust boundary — request bodies, query strings, file contents, external API responses, model output — as hostile until validated. Validate on the server, never rely on client-side checks. Convert validated primitives into the expressive types of §1.2/§6.1 at the boundary, so the interior of the system works with values that are correct *by type*.

### 7.3 Parameterize every query

Never build SQL, shell commands, LDAP filters, or paths by string concatenation of input. Use parameterized queries / commands. This closes the injection class entirely rather than filtering it case by case.

```csharp
// ✗ SQL injection
var sql = $"SELECT * FROM Orders WHERE CustomerId = '{customerId}'";

// ✓ Parameterized
const string sql = "SELECT * FROM Orders WHERE CustomerId = @id";
cmd.Parameters.AddWithValue("@id", customerId.Value);
```

### 7.4 Safe serialization only

Use `System.Text.Json`. Do not use `BinaryFormatter` (removed/obsolete and a remote-code-execution vector) or deserialize untrusted data into polymorphic types without a vetted, allow-listed binder.

### 7.5 Fail closed

On an unexpected security-relevant condition — failed authorization, an unverifiable token, a validation gap — deny by default. An exception path that falls through to "allow" is the most expensive bug there is.

---

## 8. Observability

Code is operated, not just read. These conventions make a running system diagnosable (and align with LOA's Receipt Ledger / Audit Trail stance).

### 8.1 Structured logging with message templates

Log with `ILogger` message templates and named placeholders — never string interpolation into the message. Templates preserve structured properties for querying; interpolation flattens them to opaque text.

```csharp
// ✗ Interpolated — no queryable properties, and risks logging more than intended
_logger.LogInformation($"Charged {order.Id} for {order.Total}");

// ✓ Templated — Id and Total are first-class structured fields
_logger.LogInformation("Charged {OrderId} for {Total}", order.Id, order.Total);
```

### 8.2 Never log secrets or PII

The corollary to §7.1: log identifiers and amounts, not card numbers, tokens, credentials, or personal data. When in doubt, log a hash or a reference, not the value. Logging is an exfiltration surface.

### 8.3 Distinguish errors from non-errors

Log at the level the event deserves. A cancelled operation (§5.1), an expected not-found, or a handled business-rule rejection is not an error and must not page someone. Reserve `LogError` for conditions a human should act on. Carry correlation/trace context (`Activity`) so a log line can be joined to the request that produced it.

---

## Appendix: Defaults

| Concern | Default |
|---|---|
| Target platform | Latest stable LTS / C# version — currently **.NET 10 (LTS) / C# 14**; bump when a newer LTS ships |
| Nullable reference types | `enable` project-wide |
| Implicit usings | `enable` |
| `var` | Yes, when the type is obvious from the RHS |
| File-scoped namespaces | Yes |
| `sealed` on classes | Default to sealed; unseal only with intent |
| `readonly` on fields | Default to readonly; mutate only with intent |
| `CancellationToken` | Accepted and threaded through every async method |
| Sync-over-async (`.Result`/`.Wait()`) | Forbidden outside startup/`Main` |
| Secrets in source or logs | Forbidden (review-blocking) |
| Method length target | ≤10 lines |
| Cyclomatic complexity | ≤ 8 per method |
| Public API surface | Documented with XML doc comments |
| Commented-out / dead code | Forbidden — delete it (version control is the archive), never park it in a comment |
| Warnings | `TreatWarningsAsErrors` in CI |

---

## Appendix: Enforcement

A style guide enforced by reviewer goodwill drifts; one enforced by tooling holds. Reviewers should spend their attention on design, not on catching formatting or known bug-shapes a machine can catch. Make this guide mechanically checkable.

**`.editorconfig`** is the source of truth for formatting and analyzer severities, committed at the repo root so every IDE and `dotnet format` agree. Encode the Defaults table there: nullable, implicit usings, file-scoped namespaces, `var` preferences, and the analyzer rule severities you treat as errors.

**Analyzers** turn rules into build failures:
- The built-in .NET analyzers (`<AnalysisLevel>latest</AnalysisLevel>`, `<EnforceCodeStyleInBuild>true</EnforceCodeStyleInBuild>`) cover language and style rules.
- Add a curated set (e.g. Roslynator, Meziantou.Analyzer) for the deeper smells — async misuse, allocation, exception handling — that map to §4–§5.
- For AI-integrated code, the LOA conformance analyzers (LOA Appendix G) enforce its criteria C1–C11.
- Treat the unused-code diagnostics as errors so **dead code fails the build**: IDE0051 (unused private member), IDE0052 (unread private field), IDE0059 (unnecessary assignment), CS0219 (unused local). Commented-out code is a review-blocking finding a detector flags (§1.6).

**Project defaults** in `Directory.Build.props` so every project inherits them:

```xml
<PropertyGroup>
  <Nullable>enable</Nullable>
  <ImplicitUsings>enable</ImplicitUsings>
  <LangVersion>latest</LangVersion>
  <TreatWarningsAsErrors>true</TreatWarningsAsErrors>
  <EnforceCodeStyleInBuild>true</EnforceCodeStyleInBuild>
  <AnalysisLevel>latest</AnalysisLevel>
</PropertyGroup>
```

**CI gate.** Run `dotnet format --verify-no-changes` and `dotnet build` (warnings-as-errors) on every pull request. A formatting or analyzer violation fails the build, not the review. What cannot be mechanically enforced — naming intent (§1.1), the right abstraction level (§1.5), whether an exception is truly exceptional (§4.6) — is exactly what reviewers should be free to focus on.
