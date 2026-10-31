---
load: always
---
# Responsible AI Policy

*AI-Forward's committed Responsible-AI stance. This is a **crosswalk, not a new control set**: it states the principles the pack commits to and maps each one to the **existing** persona, template, gate, or standard that already enforces it. It anchors on the **Microsoft Responsible AI Standard** (principles) and the **NIST AI Risk Management Framework** (lifecycle functions), the two references the field pairs for a credible, auditable policy. Owner: the Privacy & Data Governance and Security & Identity lenses' human counterpart.*

---

## 1. Stance

AI-Forward is a **methodology pack** that helps humans direct AI coding agents responsibly. Its Responsible-AI posture is built into the methodology, not bolted on: **a human stays accountable** for priorities, approvals, and final changes (the Rules of the Road gates); **verification is never self-certified** (BoK D3); and **personal/work data and security are governed by hard-veto lenses**. This policy makes that posture explicit and traceable.

> **RAI-0 — Human accountability.** Every consequential or irreversible action **MUST** pass a human gate; an agent never self-approves its own hard veto (Rules of the Road §2; BoK D3; `persona-audit.md` §8). This is the load-bearing commitment all others rest on.

## 2. Principles (Microsoft RAI) → how AI-Forward enforces them

| MS RAI principle | AI-Forward commitment | Enforced by (existing artifact) |
|---|---|---|
| **Fairness** | Inclusive, accessible interfaces; bias surfaced in spec acceptance criteria | UX & Accessibility lens (WCAG 2.2 AA veto); `ui-interaction-design.md` U16; Product Strategist (testable criteria) |
| **Reliability & safety** | Correctness demonstrated, not asserted; failure modes dispositioned; tested red-first | BoK D1; Testing Strategy; `/design-slice` failure-mode analysis; Test Architect (hard veto) |
| **Privacy & security** | Least privilege, trust-boundary analysis, no secrets/PII in code, logs, or memory | Security & Identity persona (STRIDE, hard veto); Privacy & Data Governance persona (LINDDUN, hard veto); `engineering-governance.md` §3–4; `scrub.py`; Observability O11 |
| **Inclusiveness** | Accessibility is a floor, not a feature; multiple platforms/media considered | UX & Accessibility + UX Researcher/IA lenses; `specification-standards.md` (the three layers) |
| **Transparency** | Show-your-work: assumptions, sources, confidence labels, and a Proof Pack | BoK §III show-your-work; Rigor Protocol confidence ledger; Rules of the Road §3 Proof Pack |
| **Accountability** | Decisions recorded; deviations documented; ownership + freshness on every artifact | Deviation Protocol (BoK Part IX); ADRs; decision notes (V17); `owner`/`review-by` (V13); Project Memory |

## 3. Lifecycle controls (NIST AI RMF) → how AI-Forward realizes them

| NIST function | What it asks | AI-Forward practice |
|---|---|---|
| **Govern** | Roles, policies, oversight, culture | This policy + the persona operating standard (`persona-audit.md` §8); the human-in-the-loop gates; `engineering-governance.md` |
| **Map** | Identify context, intended use, stakeholders, harms | `/specify` (problem, personas, non-goals, comparables); `/collectknowledge` (sourced domain evidence); threat-model + privacy-review scoping |
| **Measure** | Metrics for reliability, fairness, privacy, security | Testing Strategy (mutation/eval thresholds); `/design-slice` failure-mode/STRIDE/LINDDUN tables; Observability Standard (telemetry, error codes) |
| **Manage** | Mitigate, monitor, respond, improve | Disposition every failure mode/threat (prevent/detect/mitigate/recover/accept); `/investigate` (root-cause + repair); freshness gate (V13); change-impact propagation (V16) |

## 4. Data handling & the PII/secret scrub

- **No secrets or PII in committed artifacts** — code, logs, telemetry, or project memory (Observability O11; Security persona; Project Memory M7).
- **`scrub.py`** is the in-pack **first-pass** redactor for the obvious cases (emails, common secret shapes) in Markdown. It is explicitly **not CI-grade**: regex recall is limited.
- **Real enforcement is transferred to CI** — use **gitleaks**/**detect-secrets**/**TruffleHog** for secrets and **Microsoft Presidio** for PII, and **git-filter-repo**/**BFG** to purge anything already committed. The RAI commitment is that these are *named and used*, not that the regex pass suffices.

## 5. Human oversight, incident response, and limits

- **Oversight:** consequential actions are gated (RAI-0); the pack is "a productivity tool for humans, not a replacement for engineers, reviewers, or decision-makers."
- **Incident/redress:** a defect or harmful output is handled by `/investigate` (verified root cause → failure-class generalization → phased repair, stopping for human review); a contract/security regression is a hard-veto gate.
- **Limits (honest scope):** this policy governs *how the pack guides work*. It does not certify any downstream product as compliant — the consuming project applies these lenses to its own system, and regulatory obligations (EU AI Act, GDPR) are the consuming team's to meet, with the Privacy persona as the standing guide.

## 6. Self-verification checklist

- [ ] Consequential/irreversible actions pass a human gate; no self-cleared hard veto (RAI-0).
- [ ] Each MS RAI principle traces to an enforcing lens/standard (§2) — no principle is decoration.
- [ ] The NIST Map/Measure/Manage practices ran for the work (§3).
- [ ] No secret/PII in code, logs, telemetry, or memory; `scrub.py` run; CI secret/PII scanning named (§4).
- [ ] Decisions/deviations recorded; ownership + freshness set (§2 Accountability).

## 7. References

- **Microsoft Responsible AI Standard (v2)** — the principle set — https://www.microsoft.com/en-us/ai/responsible-ai
- **NIST AI Risk Management Framework (AI RMF 1.0)** — the lifecycle functions — https://www.nist.gov/itl/ai-risk-management-framework
- **`engineering-governance.md`** §3–5, **`persona-audit.md`** §8, **Rules of the Road** §2–3 — the in-pack enforcement this policy maps to.
- **`project-memory-and-obsidian.md`** (M7) + **`scrub.py`** — the no-PII-in-memory control.
- **Microsoft Presidio**, **gitleaks** — the CI-grade tools real enforcement is transferred to.
