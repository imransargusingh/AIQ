---
load: skill
skills: [define-architecture, collectknowledge]
---
# AI Commercial, Cost & Billing Models

*Base knowledge for choosing and architecting **how an AI feature is paid for** — whose account runs the inference, whose data it touches, and how the cost reaches the customer. It governs how `/specify` frames the commercial model, how `/define-architecture` and `/design-slice` realize it, and how the **AI Systems Engineer** (owner of inference cost), the **Privacy & Data Governance** lens (owner of whose-data), and the **UX & Accessibility** lens (owner of the cost surface) reason about it. It composes **down** into the Layered-Optimized Architecture (`layered-optimized-architecture.md`) — the commercial model is a first-class architecture input alongside the capability-tier allocation — and into the UI standards. It invents no new LOA pattern; it names which existing ones each model leans on.*

Normative keywords (**MUST**, **SHOULD**, **MAY**, **MUST NOT**) follow RFC 2119.

The governing idea: **"which AI capability tier" and "who pays for it, in whose account, over whose data" are two orthogonal architecture decisions, and the second is as load-bearing as the first.** The same chatbot built three ways — the customer's own Anthropic key, a metered system account, or an absorbed subscription — is three different systems in its identity model, its data-governance posture, its margin risk, and its UX. Decide the commercial model **before** the architecture sets, because it constrains the tier you may use, the data boundary you must honor, and the surfaces you must build.

---

## 0. When this applies

Any product whose AI component has a **cost** (essentially every model-backed feature). The commercial model is a **product decision** (`/specify`) and an **architecture decision** (`/define-architecture`) — name it explicitly; "we'll figure out billing later" is the anti-pattern this doc exists to kill, because it silently fixes the identity model, the data boundary, and the margin exposure.

---

## 1. The models

Three primary models (the ones most teams choose between), plus two common variants. Each differs along five axes: **who holds the provider account**, **whose data/history the inference touches**, **who carries the cost/margin risk**, **onboarding friction**, and **data-governance posture**.

### M1 — Bring Your Own subscription / key (BYO / BYOK)
The **customer holds their own provider account** (Anthropic / OpenAI / Azure OpenAI / Google) and supplies a key or connects via OAuth; the app only orchestrates. AI usage lands **in the customer's account, against their context/history, under their DPA**.
- **Cost/margin:** the app carries **no inference cost** → high, predictable gross margin; the app charges for its value-add (UX, orchestration, integration).
- **Data:** strongest isolation — the customer is the account owner, holds the DPA/SCCs directly with the provider, controls subprocessors, retention, and residency. The app is a *facilitator/processor*, not a subprocessor of the model traffic.
- **Friction:** highest onboarding (the customer must have an account + key); typically a developer/technical or enterprise/regulated audience.
- **Ceiling:** the app cannot mark up usage, cannot guarantee a model/tier the customer's account doesn't allow, and inherits the customer's rate limits.

### M2 — Bill pass-through (metered system account)
The app uses a **system account** to call the provider, **meters usage per customer, and passes the cost on** (optionally with a markup). The customer needs no provider account.
- **Cost/margin:** the app fronts the inference cost and **recovers it per-use** (+markup = the margin); volatility is passed to the customer, so the app's risk is mostly *float and mispricing*, not absorption.
- **Data:** customer data flows **through the app's provider relationship** — the app is a **processor**, the provider its **subprocessor**; the customer's DPA is with the *app*. The app **MUST** configure the commercial-API tier's **zero-retention / no-training** settings and disclose residency.
- **Friction:** low (no key); flexible pay-as-you-go.
- **Load-bearing requirement:** the **meter is the billing source of truth** — it MUST be accurate, auditable, and reconcilable against the provider's own usage (§3).

### M3 — Absorbed / standard subscription
The AI cost is **masked under the app's revenue model** — a flat SaaS fee with "included" AI; the customer is **unaware of the per-call cost**. The app uses its own system account and **eats the inference bill**, averaging heavy and light users.
- **Cost/margin:** simplest customer experience and predictable revenue, but the app **absorbs the cost risk** — a heavy/outlier user can turn a subscription margin-negative. Margin protection (quotas, fair-use, tier discipline) is **existential**, not optional.
- **Data:** same processor/subprocessor posture and obligations as M2 (the app's account, the app's DPA, the app must configure zero-retention).
- **Friction:** lowest; familiar SaaS. Best for consumer / general B2B where usage is bounded and the AI is one feature among many.

### Variants (very common in 2025)
- **M4 — Credits / hybrid metering:** the customer buys **credits** (or gets N/month with a plan) spent on AI actions. Aligns variable cost to revenue with an easy upsell path; needs clear consumption reporting. A consumer-friendly skin over M2/M3.
- **M5 — Offer both (BYOK *or* bundled):** the dominant new-app pattern — a low-cost/free tier on **BYOK** (M1) and a premium tier on **bundled** inference (M2/M3). Lets the technical user bring their key and the mainstream user pay for convenience.

| Model | Provider account | Data isolation | App's cost risk | Friction | Ideal for |
|---|---|---|---|---|---|
| **M1 BYO/BYOK** | customer's | strongest (customer's DPA) | none | high | dev tools, regulated/enterprise, privacy-critical |
| **M2 Pass-through** | app's (system) | medium (app = processor) | float/mispricing | low | prosumer, usage-variable B2B |
| **M3 Absorbed** | app's (system) | medium (app = processor) | **highest** (outliers) | lowest | consumer, general B2B SaaS |
| **M4 Credits/hybrid** | app's | medium | medium | medium | creative/consumer, upsell-led |
| **M5 Both (BYOK+bundled)** | either | per-tier | per-tier | mixed | broad-audience apps |

---

## 2. Architecture patterns (per model)

**AC1 — Name the model in the architecture, and the identity boundary it fixes.** The architecture **MUST** state the commercial model and, with it, **whose credential runs each inference** (LOA P11, Least-Privilege Delegated Identity):
- **M1 BYO** → the inference runs **as the customer**, on the customer's key/token. Keys are **secrets** — never logged, never in the client bundle, stored encrypted, scoped, revocable (Security & Identity lens; LOA P11). Prefer **OAuth / delegated connect** over a pasted raw key where the provider supports it (the Work IQ / Entra-delegated pattern is the same shape: act as the user, at least privilege, never broaden scope).
- **M2/M3** → the inference runs on a **system account**. That account is a high-value secret and a **shared trust boundary**: per-customer data MUST be isolated in logs/caches (no co-mingling), and the system credential MUST be least-privilege and rot[a]table.

**AC2 — Metering is the Receipt Ledger, and for M2 it is the billing source of truth.** Both providers return token usage on every response; capture it at the call boundary. This is **exactly LOA Pattern 4.3 (Receipt Ledger)** — which already records `model`, `tier`, tokens, and **`CostUsd`** per call — extended with the **acting principal** and the **customer/tenant id** (LOA P10 Audit, P11). For **M2 pass-through**, the ledger is the **invoice's source of truth** and **MUST** be: accurate (provider-reported tokens, not estimated), append-only/auditable (LOA 6.3 Audit Trail), idempotent under retries (a retried call is not double-billed — LOA 5.3 Idempotent Action), and **reconcilable** against the provider's own usage export. Aggregate per billing period and report to the billing system (e.g. Stripe metered usage records).

**AC3 — Quota & budget enforcement is Token Budget Throttle, and for M3 it is margin protection.** Every model **MUST** bound spend (LOA Pattern 2.4, Token Budget Throttle) — but the *purpose* differs: M1 protects the *customer's* account from runaway cost; M2 enforces the *plan's* metered limits; **M3 protects the app's margin** (a fair-use / quota ceiling is the only thing standing between an absorbed subscription and an outlier user who makes it loss-making). Enforce **soft** (warn) then **hard** (deny / degrade) quotas; degrade gracefully (LOA 6.5) — drop to a cheaper tier (LOA P1) before denying.

**AC4 — Tier allocation and the commercial model are coupled.** The capability-tier decision (LOA T0–T4, the cost surface) and the commercial model multiply: **margin = price − (Σ tier cost per call × volume)**. So:
- **M3 absorbed** makes **Determinism-at-the-Floor (LOA P2)** and **cheapest-sufficient-tier (P1)** *financial* imperatives — every call routed to a model when a T0 function would do is direct margin loss; **Cascade (1.2)**, **Semantic Cache (2.1)**, **Prompt Prefix Cache (2.2)**, and **Distillation (2.3)** are margin levers, not just latency ones.
- **M1 BYO** loosens the app's tier-cost pressure (the customer pays) but **constrains** the choice to what the customer's account/plan permits.
- **Provider Portability (LOA 6.6)** enables cost arbitrage and BYOK-across-vendors (M5).

**AC5 — Use the platform billing rail; don't hand-roll money.** Metering → aggregation → invoicing **SHOULD** route through an established billing platform (Stripe metered billing / usage records, or the equivalent) rather than bespoke money code. The app's responsibility is the **accurate meter** (AC2); the platform's is the invoice. (Patterns Expert: don't reinvent billing; Security: money is a hard trust boundary.)

---

## 3. The data-governance impact (the part teams underestimate)

The commercial model **fixes the data boundary**, and the difference is load-bearing — this is the **Privacy & Data Governance** lens's standing concern on any AI feature, co-held with **Security & Identity** (the *who runs as whom* of AC1).

**AC6 — State the data posture with the model.** The spec/architecture **MUST** record, per the chosen model: **whose provider account** the data flows through, **who holds the DPA**, whether the **commercial-API tier's zero-retention / no-training-by-default** is configured and *verified*, the **residency** guarantee, and the **subprocessor** disclosure.
- **M1 BYO:** the customer holds the DPA directly; the app is a facilitator; isolation is strongest; data, context, and history live **in the customer's account** under their terms. The privacy posture is largely **transferred** to the customer (a *named* transfer, not an assumption — LINDDUN discipline).
- **M2/M3 (system account):** the customer's DPA is with the **app**; the **provider is the app's subprocessor**; the app **MUST** (a) configure the **commercial/API tier** (zero-retention, no-training-by-default — *not* the consumer tier, which may train and offers no DPA), (b) prevent cross-customer co-mingling in its own logs/caches, and (c) disclose residency and subprocessors. The **highest-risk** model for data is M3/absorbed, because the customer has the **least direct control** and trust rests entirely on the app's configuration.

**AC7 — The no-training guarantee is tier-specific and MUST be verified, not assumed.** Zero-retention / no-training-by-default applies to **commercial API / enterprise** traffic, **not** consumer/web tiers. Routing customer data through a consumer-tier account (or an un-configured API account) is a governance defect the Privacy lens **vetoes**. Verify the setting against the provider's current terms (Spike/Domain-Researcher discipline — terms change).

---

## 4. The UX/UI impact

Each model needs **distinct user-facing surfaces**; the cost model is not invisible to UX even when it is "absorbed." This is the **UX & Accessibility** lens's concern, composing the UI standards (`ui-interaction-design.md`) and the archetype catalog.

**AC8 — Build the surfaces the model requires.**
- **M1 BYO** needs a **secure "connect your AI account" onboarding** — OAuth connect where available, else a **masked key-entry field** (never echoed, validated on entry, stored as a secret), with clear copy on *why* and *what scope*; a **status/connection** surface ("connected to your Anthropic account"); and a pointer that **usage and history appear in the customer's own provider dashboard**. Archetype: a **Settings/Connection surface** (catalog **F2**), with the irreversible/secret-handling care F2 prescribes.
- **M2 pass-through / M4 credits** need **cost transparency as a trust surface**: a **usage meter** (tokens/credits/$ consumed this period), a **per-period breakdown**, **soft-then-hard quota** states (approaching limit → at limit → overage handling), and an honest **markup/disclosure**. Archetype: a **metering/usage dashboard** (catalog **B3 Telemetry Bento** shape) + the billing settings (F2). Show the running cost; never surprise-bill.
- **M3 absorbed** hides per-call cost **but still needs fair-use UX**: a **"you've used X of your monthly allowance"** indicator, a **graceful quota wall** with a clear upgrade path (not a dead end), and degraded-mode messaging when dropped to a cheaper tier (AC3). The cost is masked; the **limit is not** — a silent hard stop with no explanation is a UX failure.

**AC9 — Cost/uncertainty honesty composes the AI-UX standard.** The cost surface is a **Trust builder** in the Shape-of-AI sense (`ui-interaction-design.md` U13–U15): disclose what the AI run costs or consumes, don't hide quota mechanics, and make the wrong-answer/regenerate affordance aware that *regeneration costs* (in M2/M4 a regenerate spends again — say so). For technical/expert surfaces, pair with the numerical-legibility rules in `technical-ui-design.md` (TQ2) for the usage numbers.

---

## 5. The decision framework

**AC10 — Choose by audience, data sensitivity, and margin tolerance.**
- **Regulated / enterprise / privacy-critical, or a technical audience** → **M1 BYO** (data isolation + the app carries no inference cost + the customer controls compliance).
- **Usage-variable prosumer / B2B where customers accept pay-as-you-go** → **M2 pass-through** (recover cost + markup; the meter is the contract).
- **Consumer / general B2B where AI is one bounded feature** → **M3 absorbed** (or **M4 credits**) — simplest UX, *but* mandate quotas + tier discipline for margin.
- **Broad audience** → **M5 both** (BYOK free/low tier + bundled premium).
The trade is **margin vs. friction vs. data-control vs. trust**: M1 maximizes margin and data-control at the cost of friction; M3 minimizes friction at the cost of margin risk and data-control. Record the choice and its rationale as an **ADR** (it is a load-bearing, hard-to-reverse decision).

---

## 6. Self-verification checklist

- [ ] The commercial model (M1–M5) is **named** in the spec and architecture, with an ADR for the choice (AC1, AC10).
- [ ] The **identity boundary** is fixed: BYO runs as the customer (keys as secrets, OAuth preferred); system-account models isolate per-customer data and run least-privilege (AC1; LOA P11).
- [ ] **Metering** uses the Receipt-Ledger shape with cost + acting principal + tenant; for M2 it is accurate, auditable, idempotent, and **reconcilable** with the provider (AC2; LOA 4.3/6.3/5.3).
- [ ] **Quota/budget** enforced (Token Budget Throttle); for M3 it actively **protects margin** with graceful degradation (AC3; LOA 2.4/6.5).
- [ ] **Tier discipline** treated as a margin lever where the app absorbs cost — cheapest-sufficient tier, cache, cascade, distill (AC4; LOA P1/P2).
- [ ] Billing routed through a **platform rail**, not hand-rolled (AC5).
- [ ] **Data posture** recorded: whose account/DPA, **zero-retention/no-training verified at the commercial-API tier**, residency, subprocessors; consumer tiers are not used for customer data (AC6–AC7; Privacy veto).
- [ ] The model's **UX surfaces** are built — BYO connect, usage meter + quota states, or absorbed fair-use wall + upgrade path — with cost honesty (AC8–AC9; UX & Accessibility).

## 7. References

- **Monetization models** (2025): BYOK vs usage-based pass-through vs absorbed vs credits/hybrid; the AI **gross-margin compression** problem and cost-absorption risk; the dominance of **hybrid (BYOK + bundled)** and **transparent usage reporting**. *(Sourced — AI app pricing/monetization surveys, 2025.)*
- **Metering & billing:** token usage from the provider response; per-customer usage logs; **Stripe metered billing / usage records**; markup logic; soft/hard quotas and rate limits. *(Sourced — Stripe metered-billing docs; OpenAI/Anthropic pricing.)*
- **Data governance per model:** DPA/subprocessor/residency/retention by model; **zero-retention & no-training-by-default apply to commercial/API tiers, not consumer**; system-account co-mingling risk. *(Sourced — provider DPA/data-usage terms; verify on use, terms change.)*
- **The pack's own authorities this composes with:** `layered-optimized-architecture.md` (Tiers; P1/P2/P10/P11; Patterns 2.1–2.4, 4.3, 5.3, 6.3, 6.5, 6.6; Appendix H cost model) — *referenced, not duplicated*; `ui-interaction-design.md` (U13–U15 AI-UX trust) + `ui-archetype-catalog.md` (F2 settings/connection, B3 metering dashboard) + `technical-ui-design.md` (TQ2 numeric legibility); the **AI Systems Engineer** (inference cost), **Privacy & Data Governance** (whose-data), and **Security & Identity** (delegated identity, secrets) lenses.
