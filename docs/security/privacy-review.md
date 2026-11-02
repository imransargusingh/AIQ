---
id: aiq-privacy-review
title: "AIQ privacy review"
type: privacy-review
status: draft
owner: "Imran"
phase: "0-1"
tags: [privacy, linddun]
links:
  - { to: design-coaching-from-rungs, rel: documents }
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
| Credentials | Store key, the person's optional Anthropic API key (Claude Code's own sign-in is never read) | OS keychain | API key to Anthropic API only | Encryption; judging | Until removed | core |

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

| source | Threat | Disposition |
|---|---|---|
| [design-aiq-home-app](../design/aiq-home-app.md) | Disclosure | The app shows the person's own prompts only on Your data, on their own machine; nothing is sent |
| [design-aiq-home-app](../design/aiq-home-app.md) | Linkability | No identifiers leave the machine |
| [design-aiq-home-app](../design/aiq-home-app.md) | Unawareness | Your data says where the data lives and that it stays until deleted |
| [design-aiq-home-app](../design/aiq-home-app.md) | Non-compliance | Delete everything removes store, spool and content (existing `forget`) |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | Prompt text (may hold code, customer data, secrets) → store | D: disclosure on a shared or lost machine | mitigate + accept | User-only files; no network. Encryption at rest deferred (Imran, 2026-10-31) | Kept until the person deletes it (ADR-0002); `aiq forget --all` deletes the store and spool. Delete-one and older-than arrive in Phase 6 |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | Prompt text → card | D: card text visible on screen-share | mitigate | Card never quotes prompt text; only counts | n/a |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | Coaching history (how someone works) | L/I: could profile the person if shared | mitigate | Nothing leaves the device; no sync, no telemetry in Phase 1 (US-6) | Same as above |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | Capture while paused | U: person unaware capture continues | prevent | Pause flag checked in the hook before sending; `aiq status` shows paused | `aiq pause` / `aiq resume` |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | Installed hooks | U: person unaware AIQ is installed | mitigate | Explicit `install` with diff and consent; card shows how to pause | `aiq uninstall` |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | Tool calls and outputs | D: code and file contents | prevent (not captured) | Phase 1 subscribes only to UserPromptSubmit and Stop | n/a |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | Logs | D: prompt text in logs | prevent | O11 log scan test | Logs rotate at 5 MB × 3 files, local only |
| [design-coaching-from-rungs](../design/coaching-from-rungs.md) | Linkability / Identifiability | Habits and task types reveal working patterns | **Mitigate**: user-only store; no team view (never); no aggregate in this slice |
| [design-coaching-from-rungs](../design/coaching-from-rungs.md) | Non-repudiation | Feedback rows prove what the person said | **Accept**: private to them; deletable with `forget` |
| [design-coaching-from-rungs](../design/coaching-from-rungs.md) | Detectability | A tip on screen shows a colleague looking over the shoulder that AIQ runs | **Accept**: same as Phase 2; Pause is one action |
| [design-coaching-from-rungs](../design/coaching-from-rungs.md) | Disclosure | Tip text visible in a shared terminal recording | **Mitigate**: no prompt text, no rung, no score in tips (Part C rules) |
| [design-coaching-from-rungs](../design/coaching-from-rungs.md) | Unawareness | The person doesn't know imports feed habits | **Mitigate**: Habits page footnote "Includes tasks AIQ imported from your history" when any are |
| [design-coaching-from-rungs](../design/coaching-from-rungs.md) | Non-compliance (retention, rights) | New tables must be covered by delete and export | **Mitigate**: `forget --all`, `forget --imported` (imported judgments' observations disappear with them) and export cover all three |

<!-- rolled up from 3 artifact(s) by docs-graph.py rollup on 2026-11-02 -->


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
| Plain-text spool of recent prompts while the core is down (design-claude-code-skeleton) | Imran, 2026-10-31 | Bounded (1 MB), user-only, emptied on drain; encrypting needs the key on the hot path |
| Prompts stored unencrypted at rest in Phase 1 (design-claude-code-skeleton) | Imran, 2026-10-31 | Encryption deferred; files are user-only; revisit whether it is needed ([note](../notes/note-20261031-encryption-deferred.md)) |

## 7. Gaps and flagged unknowns
- Works-council / legal review not started.
- Third-party privacy review before launch (brief) not scheduled.
