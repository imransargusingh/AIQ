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
  In the alpha the rung judge is a frontier model reached through the person's own AI tool and
  sign-in (Claude Code, Copilot CLI, VS Code's language model API). No on-device judge, no AIQ
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
- The judge is a **T3** call made by the core through the **person's own tool and sign-in**, sent to
  the same provider that already saw the Task unless the person chooses otherwise:
  - Claude Code: `claude -p --output-format json --json-schema <rubric schema>` with no tools granted
    and AIQ's hooks disabled for that call (via `--settings`), using the person's login.
  - Copilot CLI: `copilot -p "<prompt>" -s` with no tools granted.
  - VS Code: the Language Model API (`vscode.lm`), after the consent dialog, from the AIQ extension.
- **Facts first:** the model answers narrow facts (F1–F12 in the rung-model spec) to a JSON schema,
  each asked twice for consistency; deterministic per-job rubric rules (T0) place the rung. Task content
  is passed as quoted data. Judgments are versioned by fact set + rubric + judge version, never edited.
- **Judge calls are tagged** (marker in the prompt and session) and the core never ingests them as
  Tasks.
- **No account or offline:** judgments queue; they are never rerouted to another provider.
- Judging uses the person's quota; budgets per day are set in settings.
- Every call is recorded locally (P10): provider, model, rubric version, tier, Task ref, outcome.
- **Fallback:** if a tool's terms do not allow this use, the person's own API key for that provider.

## Alternatives considered
- **On-device T2 model:** rejected for alpha by Imran; may return if the golden set shows it is enough
  and cheaper.
- **AIQ-operated cloud model:** rejected; puts AIQ servers in the data path, against D4.

## Consequences
- **Positive:** frontier-quality judgments with no model to ship; no AIQ servers see data; data goes
  only to a provider that already saw it.
- **Negative / accepted trade-offs:** judge calls appear in the tool's own session history and in a
  work account's admin usage/audit logs, and that account's retention applies; this is disclosed at
  setup. Uses the person's quota. Depends on CLI flags that can change.
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
    - **Owner ruling (Imran, 2026-10-31):** it is OK for AIQ to call the models under the person's own
      credentials. Recorded as an accepted risk (threat model, accepted-risk register). Keep the
      person's own API key as a supported fallback so the Claude path can switch without redesign if
      Anthropic objects.
    - **GitHub: no blocker found.** `vscode.lm` is GitHub's sanctioned route for extensions to use the
      person's Copilot models with consent, under the Copilot extensibility acceptable use policy;
      Copilot CLI programmatic mode is documented. Confirm against the current Copilot product terms
      before alpha.
  - Spike: run each call path end to end; measure the 60 s p95 budget; confirm `--settings` disables
    AIQ's hooks for judge calls.
  - VS Code consent must be triggered by a user action (first-run setup).

## Evidence
https://code.claude.com/docs/en/headless · https://docs.github.com/en/copilot/how-tos/copilot-cli/automate-copilot-cli/run-cli-programmatically ·
https://code.visualstudio.com/api/extension-guides/ai/language-model (read 2026-10-31, not run).
