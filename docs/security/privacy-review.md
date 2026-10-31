---
id: aiq-privacy-review
title: "AIQ privacy review"
type: privacy-review
status: draft
owner: "Imran"
phase: "0-1"
tags: [privacy, linddun]
links:
  - { to: aiq-architecture, rel: documents }
review-by: 2027-01-29
summary: >-
  System-level privacy pass: what personal data AIQ holds, where it lives and flows, how long, and
  how the person exercises control. Everything stays on the device except judge calls to a provider
  that already saw the Task.
---

# Privacy Review

*System-level pass written during `/define-architecture`. Section 2 is regenerated from component
designs with `python3 docs/ai-forward-pack/scripts/docs-graph.py rollup --heading "Privacy analysis (LINDDUN-lite)" --type design` once they exist.*

## 1. Personal-data inventory
| Category | Examples | Where stored | Where it flows | Purpose | Retention | Owning design |
|---|---|---|---|---|---|---|
| Raw AI interaction content | Prompts, agent replies, tool inputs/outputs, code excerpts | Encrypted store, `content` table | Judge calls, only to the provider that already saw it (ADR-0007) | Evidence links; rung judgement | Until the person deletes it (ADR-0002) | store, judge client |
| Derived signals | Re-prompt counts, Task/Job, durations | Encrypted store, facts | Nowhere | Detection, habits | Until deleted | store |
| Coaching history | Cards shown, feedback, habit states, rung judgments | Encrypted store, facts | Nowhere | Coaching, progress | Until deleted | store |
| Repo metadata | Distinct author **count**, CI run counts, workflow file present | Encrypted store | Nowhere | Rung floor facts (F9) | Until deleted | git reader |
| Settings | Daily cap (default 5), role preset, quiet hours | Encrypted store | Nowhere | Behaviour | Until changed | core |
| Credentials | Store key, the person's Anthropic API key | OS keychain | API key to Anthropic API only | Encryption; judging | Until removed | core |

## 2. Findings (system level, LINDDUN-lite)
| Category | Finding | Control |
|---|---|---|
| Linking | Activity across three surfaces is linked into one profile | That is the purpose; profile never leaves the device |
| Identifying | Repo metadata could name colleagues | Store counts only, never names |
| Non-repudiation | Judge calls appear in the provider's own logs and a work account's admin logs | Disclosed at setup; judging can be turned off |
| Detecting | A colleague could see AIQ hooks in a repo | Hooks install at user level only (ADR-0005) |
| Disclosure | Unlimited retention grows exposure over time | Encryption at rest; store size shown; delete-older-than |
| Unawareness | The person may not know what is captured | Capture log; opt-in per surface; pause everywhere |
| Non-compliance | Monitoring tools can need works-council review in some regions (brief, Risks) | Engage legal before any rollout beyond volunteers |

## 3. Data-subject rights paths
Access: capture log and AIQ home. Export: Settings → Export. Delete: one item, older-than, all.
Rectify: feedback on guidance (rung placement itself is not edited, per the rung-model spec).
Object: pause per surface, turn judging off, uninstall removes the store and keychain entries.

## 4. Telemetry alignment
No remote telemetry. Local health counters only.

## 5. Transfers and processors
The person's own AI provider (Anthropic via their API key; GitHub Copilot via VS Code) for judge
calls only. AIQ operates no processor that sees user data. The pack CDN receives no identifiers.

## 6. Accepted privacy risks
| Risk | Accepted by | Rationale |
|---|---|---|
| Lost machine loses history (ADR-0003) | Imran, 2026-10-31 | Simplicity for alpha |
| Judge calls visible to work-account admins (ADR-0007) | Imran, 2026-10-31 | Provider already saw the content; disclosed |

## 7. Gaps and flagged unknowns
- Works-council / legal review not started.
- Third-party privacy review before launch (brief) not scheduled.
