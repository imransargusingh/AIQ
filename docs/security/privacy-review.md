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
  - { to: design-claude-code-skeleton, rel: documents }
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

### 2a. Component privacy register (generated)
*Generated with `docs-graph.py rollup` from component designs; do not edit by hand.*

| source | Data flow / category | LINDDUN finding | Disposition | Control / rationale | Retention and rights path |
|---|---|---|---|---|---|
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | Prompt text (may hold code, customer data, secrets) → store | D: disclosure on a shared or lost machine | mitigate | Encrypted store; user-only files; no network | Kept until the person deletes it (ADR-0002); `aiq forget --all` deletes the store, spool and keychain entry. Delete-one and older-than arrive in Phase 6 |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | Prompt text → card | D: card text visible on screen-share | mitigate | Card never quotes prompt text; only counts | n/a |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | Coaching history (how someone works) | L/I: could profile the person if shared | mitigate | Nothing leaves the device; no sync, no telemetry in Phase 1 (US-6) | Same as above |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | Capture while paused | U: person unaware capture continues | prevent | Pause flag checked in the hook before sending; `aiq status` shows paused | `aiq pause` / `aiq resume` |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | Installed hooks | U: person unaware AIQ is installed | mitigate | Explicit `install` with diff and consent; card shows how to pause | `aiq uninstall` |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | Tool calls and outputs | D: code and file contents | prevent (not captured) | Phase 1 subscribes only to UserPromptSubmit and Stop | n/a |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | Logs | D: prompt text in logs | prevent | O11 log scan test | Logs rotate at 5 MB × 3 files, local only |

<!-- rolled up from 1 artifact(s) by docs-graph.py rollup on 2026-10-31 -->


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
| Plain-text spool of recent prompts while the core is down (design-claude-code-skeleton) | Pending Imran | Bounded (1 MB), user-only, emptied on drain; encrypting needs the key on the hot path |

## 7. Gaps and flagged unknowns
- Works-council / legal review not started.
- Third-party privacy review before launch (brief) not scheduled.
