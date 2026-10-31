---
applyTo: "**"
---
# The No-Guessing Protocol

*Normative guidance for the single most persistent failure in agent work: **acting on a belief that was never checked**, and only naming it as "an assumption" after it causes a bug. The Body of Knowledge already forbids guessing at contracts (D2) and names the Confident Guess, the Plausible Hallucination and the Silent Assumption as anti-patterns (Part VIII); `end-to-end-integrity.md` E15 already forbids asserting the shape of our own code from memory. **Those rules are prohibitions. This document is the mechanism** — what to do at the moment of not-knowing, how to notice you are doing it, and why "I assumed" is not available afterwards.*

Normative keywords (**MUST**, **SHOULD**, **MAY**, **MUST NOT**) follow RFC 2119.

The governing idea, stated plainly because it is the part that gets missed: **you cannot decide to stop guessing, because a guess and a fact feel identical from the inside.** A language model emits the most probable continuation; plausibility is precisely what it optimises for, so the wrong API call, the wrong default, the wrong column name arrive with exactly the same confidence as the right ones. Telling such a system "don't guess" is like telling someone "don't have a blind spot."

So this protocol does not ask you to detect a guess by feel. It does three mechanical things instead:

1. **Removes the fourth option.** When you don't know, there are exactly three moves — check, mark, ask (NG1). Proceeding on an unmarked belief is not one of them.
2. **Makes checking cheaper than not checking** (NG5), so the disciplined path is also the lazy path.
3. **Makes the excuse unsayable** (NG2), so an unchecked belief has to become visible *before* the work rather than confessable after the bug.

---

## 1. The core rules

**NG1 — Three permitted responses to not-knowing. There is no fourth.**

| Move | When | What it looks like |
|---|---|---|
| **Check it** | The default, and almost always available | Open the file. Run `--help`. One REPL line. One `grep`. Read the type signature. Query the graph. Look at the actual response. |
| **Mark it** | You genuinely cannot check now (no access, no credentials, no environment, the answer only exists at runtime) | An **`assume:`** marker (NG4) — visible, inline, carrying what would confirm it and what breaks if it is false |
| **Ask** | The unknown is **consequential *and* unresolvable by you** — a product decision, an irreversible action, a fork research cannot settle | Name the fork and stop. Do not guess past it (BoK §VI.3) |

**Proceeding on an unmarked belief is not on this list.** If you find yourself doing it, you have not made a decision — you have skipped one.

**NG2 — The pre-registration rule.** **An assumption that was not written down before the work is not an assumption — it is a guess.**

This is the rule that removes the post-hoc excuse. "I assumed the timestamps were UTC" is only a legitimate sentence if that assumption was *marked, visible and checkable* before the code was written. If it was not, the honest sentence is: *"I guessed the timestamps were UTC, and I should have checked."* Say that one instead. It is shorter, it is true, and it points at a control (NG9) rather than at bad luck.

Corollary: an artifact's assumptions section is written **at the start**, not reconstructed at the end. An assumption discovered during a post-mortem is a *finding*, not an assumption.

**NG3 — Know the tells; they are linguistic before they are logical.** Guessing has a reliable signature. In any statement something else depends on, these phrases mean **"I have not checked"**:

> *should be · presumably · typically · usually · I believe · it looks like · probably · must be · seems to · I'd expect · by convention · standard practice is · it follows the pattern · I recall · IIRC · from memory*

And the structural tells, which are worse because they read as confident:

- naming an API, parameter, flag, method or field **you did not open**;
- a **version number, default value or limit** produced from memory;
- a **file path** you did not list, or a **count** you did not run;
- "it must work that way, **because the caller works**";
- a **behaviour at a boundary** (empty, null, max, concurrent, error) you did not exercise;
- a claim about **what a tool does** based on its name.

**Rule:** when a tell appears in a load-bearing claim, you **MUST** either check it (NG1) or convert it into a marked assumption (NG4). You **MUST NOT** leave it standing as prose. *Delete the hedge and the sentence becomes a lie; keep the hedge and it becomes an unowned risk — the marker is the only honest third form.*

**NG4 — The `assume:` marker.** Where a belief cannot be checked now, record it inline in the pack's existing marker idiom (the sibling of `simplify:`, `solution-selection-ladder.md` L5). It **MUST** carry three things — the belief, **what would confirm it**, and **what breaks if it is false**:

```python
# assume: the provider returns ISO-8601 in UTC. Seen in one sample payload, NOT stated in
#         the spec. If it is local time, every daily rollup silently shifts by the offset.
#         Confirm: request one record and inspect the raw timestamp field.
```

A marker without a confirmation route and a consequence is not a marker — it is a shrug with a comment character in front of it. Markers are greppable and are **harvested during investigation** exactly like `simplify:` markers (NG9). The three fields are now **checked, not just requested**: `scripts/marker-lint.py` flags an `assume:` marker missing its confirm route (`assume-no-confirm`) or its consequence (`assume-no-consequence`) — warn by default, `--gate` to fail, so legacy free-form is grandfathered (V16a).

**NG5 — Cheapest-check-first: if checking costs less than marking, you may not mark.** Before writing a marker, price the check. Most "I couldn't verify that" is really "I didn't try": the file was one `view` away, the signature one `grep`, the behaviour one line in a REPL, the version one `--version`, the shape one query. **If the check is cheaper than the marker, do the check.** This is the forcing function that makes the disciplined path also the lazy one — and it is why "no guessing" is affordable rather than aspirational.

**NG6 — Confidence labels describe evidence, not confidence.** The Rigor Protocol's ledger is not a feeling scale:

- **Verified** — you *observed* it. You ran it, read it, or cited the authoritative source. Reproducible by someone else.
- **Inferred** — you reasoned it from something verified. Might be right; not established.
- **Flagged** — you do not know, and it matters.

**You may not label a claim Verified because it is very likely.** Probability is not evidence, and a high-probability wrong answer is the exact failure mode this pack exists to prevent. If you cannot say *how* you know, it is not Verified.

**NG7 — Never launder a guess through a citation, a tool, or a delegate.** A search result, a sub-agent's inventory, a graph edge, a model's summary, a plausible-looking snippet from documentation — these are **leads**. Quoting a source makes *the source* verified, not *the claim*. Specifically:

- A **web search** result is a lead until checked against the primary source (BoK §III.1).
- A **sub-agent's** counts or listings are evidence to verify, not evidence to cite (`end-to-end-integrity.md` E16).
- A graph edge tagged **`INFERRED`** is Inferred, no matter how precise its `file:line` looks (`code-knowledge-graph.md` GK7) — **a citation is not a promotion**.
- Your **own earlier statement** in this session is not a source. Re-derive or re-check it; drift across a long session is a known failure (BoK §VI.2).

**NG8 — Ask when it is consequential *and* unresolvable; never to offload what you can check.** Naming a fork you cannot settle is discipline. Asking the human to confirm something one command would answer is the opposite failure (BoK §VI.3, the Offloaded Decision). The test: *could I resolve this myself in under five minutes?* If yes, resolve it.

**NG9 — Every bug that traces to a guess is a defect-class instance.** When a defect's root cause is "we acted on an unchecked belief", the class is **not** *"got the timezone wrong"* — it is *"asserted an external data contract without reading the spec"*. Register it with a control (`continuous-improvement.md` CI1–CI6), and during any investigation **harvest the `assume:` markers** in the affected area alongside the `simplify:` markers (CI9) — a marker that has come true is the bug, already written down and unread.

**NG10 — Report the split, always.** A closing summary **MUST** separate what is **Verified**, what remains **Inferred**, and what is **Flagged**, with the residual risk named. "It works" without that split is a statement about feeling. Honesty about uncertainty outranks the appearance of competence (Rules of the Road §3) — and an accurate *"the happy path is verified; the concurrent case is unverified"* is worth far more than a confident *"done"*.

**NG11 — Scale with cost-of-error.** This is a discipline, not a ceremony. At **T0** (a rename, a typo, a comment) checking the obvious is the whole protocol — do not turn a one-line fix into a research project. At **T1/T2**, and for anything touching money, identity, personal data, migrations, irreversible actions or a public contract, the full discipline applies and a marker is not a substitute for a check.

---

## 2. The intervention that actually works at the moment of writing

Rules that only fire at review time do not stop the guess that is being written *now*. One question does, and it **SHOULD** be asked of every load-bearing claim as it is written:

> ### **"If this is wrong, how would I find out — and when?"**

- *"The type checker would catch it"* → fine, proceed.
- *"A test would fail"* → fine, if that test exists. Does it?
- *"Code review might notice"* → weak. Check it.
- **"It would surface as a bug in production"** → **stop and check it now.** That is the definition of a guess with a delay timer.
- *"I wouldn't"* → this is the most dangerous answer available, and it means the claim must be checked or marked before another line is written.

The second question, for anything already written down:

> ### **"What did I write here that I did not actually look at?"**

Run it over a design document before the gate. Designs are where unchecked claims are cheapest to make and most expensive to keep, because nothing compiles a design — a pack repo put a property on the wrong type, asserted a field would carry data its producer hard-codes to null, and reused a "general" helper that silently collapsed two distinct keys, all in *design documents*, all in one session.

---

## 3. Self-verification checklist

- [ ] Every unknown resolved by one of the **three moves** — checked, marked, or asked; nothing proceeded on an unmarked belief (NG1).
- [ ] Assumptions were **written down before the work**; no "I assumed" appears as a post-hoc explanation (NG2).
- [ ] The **tells** were swept: no *should be / presumably / I believe / it follows the pattern* left standing in a load-bearing claim; no API, version, path, count or boundary behaviour asserted unopened (NG3).
- [ ] Every `assume:` marker carries **the belief, the confirmation route, and the consequence** (NG4).
- [ ] Nothing was marked that could have been **checked more cheaply** (NG5).
- [ ] **Verified** means observed, not likely; nothing was promoted by probability (NG6).
- [ ] No guess **laundered** through a search result, a sub-agent, a tool output, an `INFERRED` edge, or my own earlier statement (NG7).
- [ ] Questions asked were **consequential and unresolvable**; nothing resolvable in five minutes was offloaded (NG8).
- [ ] Any bug tracing to a guess was registered as a **class with a control**, and `assume:` markers in the area were harvested (NG9).
- [ ] The closing summary **splits Verified / Inferred / Flagged** and names the residual risk (NG10).
- [ ] Effort scaled to **cost-of-error** (NG11).
- [ ] The moment-of-writing question — *"if this is wrong, how would I find out, and when?"* — was asked, and nothing whose answer was *"in production"* was left unchecked (§2).

---

## 4. References

- **`agent-body-of-knowledge.md`** — **D2** (no guessing at contracts) and **Part III** (the due-diligence protocol) are what NG1's *check* move executes; **Part VIII** names the Confident Guess, the Plausible Hallucination, the Silent Assumption and the Stale Recall, which are the four shapes NG3 detects; **§VI.1** (own knowledge is a claim) and **§VI.3** (ask-vs-proceed) underwrite NG7 and NG8.
- **`rigor-protocol.md`** — the Verified / Inferred / Flagged ledger NG6 defines the meaning of, and Stage 3's establish-don't-assert that NG5 makes affordable.
- **`end-to-end-integrity.md`** — **E15** (never assert own-code shape from memory) is the highest-frequency special case of NG3; **E16** (delegated inventory is evidence to verify) is NG7 applied to sub-agents; **E17** (the record is a claim with an expiry date) is NG2 applied to documents.
- **`code-knowledge-graph.md`** GK6–GK7 — the provenance mapping that makes NG1's *check* cheap for our own code, and the rule that a citation is not a promotion.
- **`solution-selection-ladder.md`** L5–L6 — the `simplify:` marker whose idiom and debt-ledger discipline `assume:` deliberately mirrors.
- **`continuous-improvement.md`** CI1–CI9 — where a guess-caused defect becomes a registered class with a control, and where markers are harvested.
- **`spike-protocol.md`** — what *check it* means when the unknown is an unfamiliar SDK, API, MCP server or protocol: read the source, run a minimal PoC, and observe the result rather than predicting it.
