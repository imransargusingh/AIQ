---
id: adr-0007
title: "ADR-0007: The rung judge runs through the person's own AI account"
type: adr
status: accepted
owner: "Imran"
phase: "0-1"
tags: [architecture, ai, privacy]
links:
  - { to: aiq-architecture, rel: refines }
  - { to: spec-rung-model, rel: implements }
review-by: 2027-04-29
summary: >-
  In the alpha the rung judge is a frontier model reached through the person's own account: their own
  Anthropic API key for Claude, and their Copilot sign-in via VS Code's language model API or Copilot CLI. No on-device judge, no AIQ
  servers in the path.
---

# ADR-0007: The rung judge runs through the person's own AI account

- **Status:** Accepted (Imran, 2026-10-31, decision card in the rung-model thread)
- **Date:** 2026-10-31
- **Deciders:** Imran
- **Context spec/architecture:** docs/architecture.md, docs/specs/rung-model.md (PR #3)

## Context
The rung model judges each closed Task's current and potential rung (slow path, never deciding
in-the-moment cards). Options were a small on-device model (T2), an AIQ-operated cloud model, or the
person's own AI account.

## Decision
- The judge is a **T3** call made by the core through the **person's own account**, sent to
  the same provider that already saw the Task unless the person chooses otherwise:
  - Claude (tasks seen by Claude Code): the core calls the Anthropic Messages API directly with the
    **person's own Anthropic API key**, stored in the OS keychain and billed to them. Not their
    Pro/Max sign-in (Imran, 2026-10-31, revised). No `claude -p`, so AIQ's own hooks never see the
    call. Without a key, Claude-seen Tasks get signals-only judging.
  - Copilot (tasks seen by VS Code or Copilot CLI): the VS Code Language Model API (`vscode.lm`) from
    the AIQ extension, after the consent dialog, on the person's Copilot sign-in. Copilot-seen Tasks
    queue while VS Code is not running.
- **How the rung is placed (Imran, 2026-10-31, rung-model Q8/Q9):** the model places the rung. Facts
  that can be verified from logs or the repo (T0) set a floor for Orchestrated and Factory, and
  guardrails (e.g. pack "don't delegate" rules) can lower it. The model also answers narrow facts
  (F1–F12) to a JSON schema, asked twice for consistency. Task content is passed as quoted data. Judgments are versioned by fact set + rubric + judge version, never edited.
- **Judge calls are tagged** (marker in the prompt and session) and the core never ingests them as
  Tasks.
- **No account or offline:** judgments queue; they are never rerouted to another provider.
- Judging uses the person's quota; budgets per day are set in settings.
- Every call is recorded locally (P10): provider, model, rubric version, tier, Task ref, outcome.

## Alternatives considered
- **On-device T2 model:** rejected for alpha by Imran; may return if the golden set shows it is enough
  and cheaper.
- **AIQ-operated cloud model:** rejected; puts AIQ servers in the data path, against D4.

## Consequences
- **Positive:** frontier-quality judgments with no model to ship; no AIQ servers see data; data goes
  only to a provider that already saw it.
- **Negative / accepted trade-offs:** judge calls appear in the tool's own session history and in a
  work account's admin usage/audit logs, and that account's retention applies; this is disclosed at
  setup. Uses the person's quota. Depends on CLI flags that can change (Copilot CLI). Claude judging needs people to create and pay for an API key.
- **Follow-ups / new risks:**
  - **Terms check (done 2026-10-31, reading only; not legal advice):**
    - **Anthropic: blocking for consumer sign-in.** Claude Code's legal page says OAuth sign-in "is
      intended exclusively for purchasers of Claude Free, Pro, Max, Team, and Enterprise subscription
      plans and is designed to support ordinary use of Claude Code and other native Anthropic
      applications", and that developers building products, including with the Agent SDK, "should use
      API key authentication… Anthropic does not permit third-party developers to… route requests
      through Free, Pro, or Max plan credentials on behalf of their users"
      (https://code.claude.com/docs/en/legal-and-compliance). AIQ calling `claude -p` on the person's
      subscription could read as that.
    - **Owner ruling (Imran, 2026-10-31, revised):** Claude judging uses each person's own Anthropic
      API key, which the terms explicitly allow when usage is billed to the key owner. No subscription
      sign-in is used, so no accepted risk remains on the Anthropic side.
    - **GitHub: no blocker found.** `vscode.lm` is GitHub's sanctioned route for extensions to use the
      person's Copilot models with consent, under the Copilot extensibility acceptable use policy;
      Copilot CLI programmatic mode is documented. Confirm against the current Copilot product terms
      before alpha.
  - Spike: run each call path end to end; measure the 60 s p95 budget; confirm Copilot CLI judge calls are not
    observed by AIQ's own hooks.
  - VS Code consent must be triggered by a user action (first-run setup).

## Evidence
https://code.claude.com/docs/en/headless · https://docs.github.com/en/copilot/how-tos/copilot-cli/automate-copilot-cli/run-cli-programmatically ·
https://code.visualstudio.com/api/extension-guides/ai/language-model (read 2026-10-31, not run).
