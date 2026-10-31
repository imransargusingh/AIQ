---
applyTo: "**"
---
# Domain & Data Modelling Standard

*Normative guidance for the decision that outranks every other technical decision in a project: **the model**. It governs how `/specify`, `/define-architecture`, `/design-slice` and `/implement` derive the **conceptual domain model** (Domain-Driven Design) and choose the **durable data representation** (dimensional: entities as dimensions, change-over-time as append-only facts). The Testing Strategy governs proof; the Observability Standard governs telemetry; the Specification Standards govern the spec's layers; **this document governs whether the model is right** — and a wrong model is the one mistake that compounds.*

Normative keywords (**MUST**, **SHOULD**, **MAY**, **MUST NOT**) follow RFC 2119.

The governing idea: **code is refactorable; a schema that has taken data is not.** Getting the domain model wrong is therefore the most expensive error available, and it does not present as a modelling defect — it presents months later as an application bug: one quantity with two homes, a field on the DTO but not the wire, a stored flag nothing writes, a balance summed across months. Every one of those is a data-model defect wearing an application-defect costume. So the model is decided **first**, in domain terms, before any surface, endpoint, table, or migration exists.

Two repos running this pack in active development independently arrived at the same directive after production defects. This standard is that directive, made pack-level and evidence-backed. The evidence base is `docs/knowledge/domain-and-data-modelling/` — **cite it rather than re-deriving it**.

---

## 0. When this applies, and who owns it

**Applies:** to any work that introduces or changes a **domain concept, a persisted shape, a measure, or a contract that carries data** — which is nearly all non-trivial work. It is triggered by `/specify` (Part A must carry the conceptual model), `/define-architecture` (the durable representation is an architecture decision), `/design-slice` (the component's data model), `/implement` (the migration), `/investigate` (most defects are model defects), and `/migrate`.

**Owner:** the **Data & Persistence Architect** (hard veto — `persona-cards.md` §C) authors the durable representation in Peer Mode and attacks it in Adversary Mode. The **Patterns Expert** checks the model against established idiom; the **Simplifier** attacks speculative structure; the **Domain Researcher** establishes domain facts; the **Test Architect** demands that every model invariant has a test.

**Proportionality.** A T0 change to an existing, well-modelled area does not re-derive the conceptual model — it *conforms* to it, and a conflict with it is a finding (§4). Everything at T1/T2 that touches a concept or a shape runs the full discipline.

---

## 1. Prime directives

1. **The data model is the highest-priority decision in any task.** It is decided before the API, before the UI, before the tables — and it is decided **in domain terms**, not in table terms.
2. **Model the domain before the storage.** The conceptual model (what things exist, what rules bind them) is a distinct, named artifact. A physical object that cannot be traced to a conceptual statement is unexplainable, and an unexplainable model is one nobody can safely change.
3. **Aggregate boundaries are drawn by invariants, not by relationships.** If two things need not be transactionally consistent, they are not one aggregate.
4. **Declare the grain before the columns.** "One row is exactly one ______." No table, measure, or projection exists without it.
5. **Facts are stored; aggregates are derived.** Two definitions of one quantity is a defect signature, not a design choice.
6. **History is modelled, never overwritten.** Any attribute whose change would alter the meaning of a past record gets a new version, not an update.
7. **The durable representation is chosen for evolvability.** Measures must be able to evolve without rewriting history — which is what makes the dimensional shape the default here.

---

## 2. The conceptual model (DDD) — decided first

**DM1 — Express the top-level model in DDD terms, before anything else.** Every non-trivial change states, in the domain's own language:
- the **bounded context** it lives in, and the **ubiquitous language** of that context (one word, one meaning, used identically in conversation, spec, model, and code — a term that means two things to two stakeholders is a context boundary announcing itself);
- the **entities** (identity persists through attribute change) and the **value objects** (defined wholly by their attributes, immutable, compared by value) — *prefer value objects*: money, date ranges, quantities-with-units and identifiers are value objects, and modelling them as primitives is the primitive-obsession defect the C# Style Guide already names;
- the **aggregates** and, for each, **its root** and **the one invariant it exists to protect**, written as a sentence.

**DM2 — Draw aggregates by Vernon's four rules.** (i) Model **true invariants** within a consistency boundary — everything inside must be consistent *at the end of a transaction*. (ii) Design **small** aggregates — a large object graph produces contention, load cost and slow, brittle tests; "these are all related to the customer" is not an aggregate boundary. (iii) Reference other aggregates **only by identity** — never a direct object reference into another aggregate's interior. (iv) Use **eventual consistency across aggregates** — one aggregate modified per transaction; coordinate the rest with domain events or compensating actions.

> A design that puts fifteen tables under one root because they are all "about the household" has failed rule 2 and will pay for it on every save. This exact mistake was caught in review in a pack repo and the correction was kept visible in the design doc rather than edited away — because *the visible correction is the teaching*.

**DM3 — The aggregate root is the gatekeeper, and the invariant lives in exactly one place.** All outside access goes through the root; no external code mutates an aggregate's interior. This is what makes DM7 (derive-don't-store) enforceable: an invariant with two enforcement sites has two definitions and will drift.

**DM4 — Write the model down at three levels, traceable downward.**

| Level | Answers | Contains | Must not contain |
|---|---|---|---|
| **Conceptual** | What exists and how does it relate? | entities, relationships, cardinality, invariants, ubiquitous language | keys, types, tables, indexes |
| **Logical** | What is the structural shape? | attributes, keys, the chosen shape (dimensional or normalised), grain, additivity, history rules | engine-specific types, storage |
| **Physical** | How is it stored on this engine? | tables, column types, indexes, partitions, constraints | new concepts that appeared here first |

Every physical object traces to a logical one; every logical one to a conceptual statement. **A concept that first appears at the physical level is a finding** — it means the conceptual model was authored by whoever wrote the migration.

---

## 3. The durable representation — dimensional by default

**DM5 — Default to: core entities as dimensions, change-over-time as append-only facts.** Unless the project records a reasoned deviation, the durable representation is dimensional:
- **Dimensions** describe the *things* — the entities from the conceptual model. Keyed by a **surrogate key** per *version*, carrying the natural/business key as an attribute.
- **Facts** record what *happened* or what a measure *was*, at a declared grain, joined to dimensions on the **surrogate** key, and **appended, never updated**. The latest row for a key is the current value; every prior row is the history; the same rows are the audit trail.

This is why the shape is the default: **history and audit come from the primary data instead of from a second schema.** The alternatives all cost a shadow schema — hand-rolled audit tables (which drift and can be bypassed), or SQL:2011 system-versioned temporal tables (which are good, but always materialise a paired history table, record *when* without *who* or *why*, and put pruning on a DBA). And a **new measure is a new fact row or column, not a mutation of existing rows**, so measures evolve without rewriting history.

**DM6 — Say what the layers are, honestly.** An **ODS** and a **star schema** are two different layers doing two different jobs, and conflating them teaches a real error. An ODS (Inmon) is subject-oriented, integrated, **current-valued, volatile and detailed**; a star schema is the historised, non-volatile analytical shape (the *gold* layer in medallion terms). The accurate statement of this standard's stance is: *"one integrated operational store whose durable shape is dimensional"* — we adopt the dimensional **shape** as the operational representation because the requirement arrives through the **audit door**, not the analytics door. Where a project genuinely needs a separate analytical layer, that layer is a *derived projection* of these facts, never a second source of truth.

**DM7 — Derive, don't store.** Facts are stored. Aggregates, roll-ups, current-state pointers, classifications and any quantity computable from other data are **computed**. A quantity that is both stored and computed *will* drift, and the stored copy is usually the one maintained by nothing. Where a derived value is materialised for performance, it **MUST** be (a) labelled a cache, (b) rebuildable from its inputs, and (c) covered by a test proving it equals its derivation. *This is the highest-frequency defect class observed across the pack's repos* — see `continuous-improvement.md` class **DM-A**.

**DM8 — Declare the grain before the columns.** Every fact table, measure and projection carries a grain statement: *"one row in `<table>` is exactly one `<event or state-spell>`, identified by `(<key>)`, recorded when `<the moment that produces the row>`."* The test of a good grain statement: two people given the statement and a real scenario produce the same number of rows. **Never infer a grain from a date rule** — three design drafts died in a pack repo trying to derive "which season does this fixture belong to" from kick-off dates, because rescheduled matches break every such rule. Declare it, put it in the key, and **ask the authority rather than reconstructing it**.

**DM9 — Classify every measure's additivity.** **Additive** (sums across all dimensions), **semi-additive** (sums across all *except time* — balances, headcounts, inventory levels, positions), **non-additive** (ratios, rates, unit prices — recompute from components). Summing a balance across time periods is the standing category error in financial and inventory systems. A measure with no declared class is an unreviewed claim, and it will eventually be summed by someone.

**DM10 — Decide the history rule per attribute, before the attribute ships.** For every dimension attribute, ask: *if this changes, does the meaning of a past fact change?* If yes it is **Type-2** (new version row with an effective interval and an is-current flag) — because a mutable attribute on a current-state row does not merely go stale, it **retroactively rewrites history**. If no, Type-1 (overwrite) is permissible — but Type-1 is *a decision to discard history* and **MUST** be recorded as one. Type 0 (never changes) and Type 3 (a bounded previous-value column) are the remaining options. **SCD type is chosen per attribute, never per table.**

**DM11 — Enforce the append-only and interval invariants; do not assume them.** The shape's correctness rests on invariants that **MUST** be enforced by design and proven by test, not left to discipline:
- facts are never `UPDATE`d or `DELETE`d (enforce by permission or trigger; prove with a test that *attempts* it);
- the current value for a key is a **deterministic function** of the rows, defined once (normally latest `RecordedAt`, tie-broken by surrogate key);
- replaying the rows reproduces current state exactly — **the rebuild test is mandatory whenever rebuildability is claimed** (a pack repo asserted "the projection is rebuildable from the corpus" four times and never once replayed it);
- dimension version intervals for one natural key are contiguous, non-overlapping, and exactly one is current;
- no fact points at a dimension version whose interval does not contain the fact's event time.

**DM12 — Snowflake only when the domain demands the entity.** Kimball's default is flat, wide, denormalised dimensions, and normalising for tidiness is the anti-pattern. Normalise a dimension into its hierarchy **only** when the hierarchy is itself a domain concept with its own identity, invariants and lifecycle — a provider-reference bridge, an independently-versioned org hierarchy, a large sparse dimension. Record the domain justification like any other pattern choice. *Snowflake because the domain model demands the entity, never because normalisation feels tidier.*

**DM13 — Record the deviation, in both directions.** Choosing the dimensional durable representation is a **deviation from the canon** (which reserves star schemas for analytics) and is recorded as an ADR naming the requirement that justifies it and the read-path cost accepted. Equally, choosing *not* to use it — normalised tables plus temporal tables, or event sourcing — is recorded as an ADR. The forbidden option is choosing neither on purpose.

---

## 4. Change discipline — the model is a contract

**DM14 — Conform to the existing model, or surface the conflict.** In an established codebase the conceptual model is authoritative (Rigor Protocol Stage 0). Conform to its language, its aggregate boundaries and its naming. If the work needs a concept the model does not have, or contradicts one it does, **stop and surface the drift** — extend the model deliberately or record a deviation. Adding a second name for an existing concept, or a second home for an existing quantity, is the defect this standard exists to prevent.

**DM15 — Every persisted field has a writer and a reader, both traced.** Before a field ships, trace every reference and classify it: **write** / **CRUD-or-DTO passthrough** / **schema** / **compute**. A field with no *write* path is an unimplemented design (a pack repo shipped declared-but-never-written facets, and a stored readiness flag that nothing set — so the product could structurally never leave demo mode). A field with no *compute* reader is dead weight that round-trip tests will happily prove works (a pack repo stored, entered, round-tripped and tested a compensation figure that nothing computed anything from). **Round-trip tests do not catch either case; the reader trace does.** See `end-to-end-integrity.md` E3.

**DM16 — Migrate by expand-migrate-contract, and never guess in a backfill.** (1) **Expand** — add the new shape, nullable/defaulted, so old and new code both work. (2) **Migrate** — dual-write, then backfill; **rows that cannot be attributed deterministically are surfaced for a human, never inferred**. (3) **Move reads** to the new shape and verify equivalence against the old. (4) **Contract** — drop the old shape in a separate change, only once no reader remains. Each step is independently deployable and reversible. **A migration is not done when it compiles — it is done when the thing that deploys it also applies it**, and when its down/rollback path has been exercised. The Data & Persistence Architect's hard veto applies to any irreversible or destructive migration without a backward-compatible path and a tested rollback.

**DM17 — Name things once.** One concept, one name, taken from the ubiquitous language and recorded in the glossary (`knowledge-visualization.md` V14). Surrogate keys `<Entity>Key`, natural keys `<Entity>Id`; dimensions named for the thing, facts named for the process. Two names for one foreign key, or a derived value stored under the same name as its source, are how "one quantity, two homes" gets in.

---

## 5. The artifact

**DM18 — The model is written down and graph-linked.** The conceptual model lives at `docs/design/conceptual-model.md` (or the spec's Part A for a small project), carries V2 frontmatter, and is linked from every design that realises part of it. It contains: the bounded context and ubiquitous language; an ER or class diagram in **Mermaid** (committed as source — diagrams-as-code, V8); the entities and value objects; each aggregate with its root and its invariant; and the **grain, additivity and history rule** for every fact and dimension it introduces. Material changes to it propagate `review-suggested` to inbound neighbours (V16), because a model change is exactly the kind of change that invalidates its dependents.

---

## 6. Self-verification checklist

- [ ] The **conceptual model** is stated in domain terms — bounded context, ubiquitous language, entities vs value objects (DM1) — and written down, traceable through logical to physical (DM4, DM18).
- [ ] Every **aggregate** names its root and the **one invariant** it protects; boundaries follow Vernon's four rules; nothing is inside a boundary merely because it is related (DM2–DM3).
- [ ] The **durable representation** is chosen and recorded as an ADR — dimensional by default, with the read-path cost accepted, or a reasoned alternative (DM5, DM13).
- [ ] The **ODS / star distinction** is stated honestly; any analytical layer is a derived projection, not a second source of truth (DM6).
- [ ] **Derive-don't-store** holds: no quantity has two homes; every materialised derivation is labelled a cache, is rebuildable, and has an equality test (DM7).
- [ ] Every fact table, measure and projection has a **grain statement**; no grain is inferred from a date rule (DM8).
- [ ] Every measure declares **additive / semi-additive / non-additive** (DM9).
- [ ] Every dimension attribute has a **history rule** decided per attribute; Type-1 choices are recorded as decisions to discard history (DM10).
- [ ] The **append-only and interval invariants** are enforced *and tested* — including a test that attempts a forbidden update, and a **rebuild test** if rebuildability is claimed (DM11).
- [ ] Any **snowflake** carries a domain justification (DM12).
- [ ] The change **conforms to the existing model** or the drift was surfaced (DM14).
- [ ] Every new persisted field has a **traced writer and a traced reader** (write / CRUD / schema / **compute**) (DM15).
- [ ] Schema change follows **expand-migrate-contract**; the backfill never guesses; the deployer applies the migration; rollback has been exercised (DM16).
- [ ] **One concept, one name**, from the glossary (DM17).

---

## 7. References

- **Evidence base:** `docs/knowledge/domain-and-data-modelling/` — the sourced, confidence-labelled research behind every directive here (findings, comparables, the disconfirming case, the observed failure modes). Cite it rather than re-deriving it.
- **Evans** (*Domain-Driven Design*, 2003) — bounded context, ubiquitous language, entities/value objects/aggregates.
- **Vernon** (*Effective Aggregate Design*; *Implementing DDD* ch. 10) — the four aggregate rules.
- **Kimball & Ross** (*The Data Warehouse Toolkit*; Kimball Group technique index) — the four-step method, grain, facts and dimensions, SCD types, the denormalised-dimension default.
- **Inmon** — the ODS definition; the layered architecture the medallion pattern descends from.
- **SQL:2011** period tables + vendor temporal-table docs — the alternative history mechanism and its two-schema cost.
- **In-pack composition:** `agent-body-of-knowledge.md` Part V.3 (stateful changes), `testing-strategy.md` D4/D6 (real-infra + schema/golden-payload proof), `engineering-governance.md` §7 (release/rollback/migration), `layered-optimized-architecture.md` P7 (state lives at the edges), `end-to-end-integrity.md` (the change-surface and reader-trace rules), `continuous-improvement.md` (the defect classes this standard prevents), `knowledge-visualization.md` V8/V14/V16 (diagrams, glossary, propagation).
- **Persona:** the **Data & Persistence Architect** owns this standard and holds the migration hard veto (`persona-cards.md` §C, `persona-audit.md` §4.2/§8.4).
