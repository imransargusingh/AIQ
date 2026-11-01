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
  signed-in Claude Code (`claude -p`, unmodified binary) for Claude, and their Copilot sign-in via VS
  Code's language model API. AIQ never handles a token. No on-device judge, no AIQ servers in the path.
---

# ADR-0007: The rung judge runs through the person's own AI account

- **Status:** Accepted (Imran, 2026-10-31, decision card in the rung-model thread); Claude credential path revised by Imran 2026-11-01
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
  - Claude (tasks seen by Claude Code): the core runs the **person's own, unmodified Claude Code** in
    headless mode (`claude -p`), on whatever sign-in that Claude Code already has (their Claude
    subscription, their own API key, or their cloud provider). Claude Code does the sign-in; **AIQ
    never reads, stores or passes a token** (Imran, 2026-11-01, superseding the API-key ruling of
    2026-10-31). Setup needs nothing extra when Claude Code is already signed in.
    - **Isolated call** (verified with Claude Code 2.1.316 on Linux, 2026-11-01): run in an empty temp
      folder; `--setting-sources user`, so no project `CLAUDE.md` or project hooks load;
      `--settings '{"disableAllHooks": true}'` (a control run without it fired the hook; with it, the
      hook did not fire); `--tools ""` (no tools); `--no-session-persistence`; `--output-format json`
      with `--json-schema` (returned `structured_output`); a fixed `--model`. The env var `AIQ_JUDGE=1`
      makes AIQ's own hook client drop anything it sees from the call, as a second guard.
      `--bare` is not used, because it never reads the subscription sign-in.
    - **Fallback, off by default:** the person's own Anthropic API key in the OS keychain, called
      directly on the Messages API. It is for people who don't use Claude Code's sign-in or who prefer
      API billing. With neither, Claude-seen Tasks get signals-only judging.
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
  setup. Uses the person's quota: on a Claude subscription, judge calls count toward its usage limits, which
  Anthropic says "assume ordinary, individual usage of Claude Code and the Agent SDK", so the daily
  judging budget must stay small. Depends on Claude Code CLI flags that can change (`--json-schema`,
  `--no-session-persistence`, `disableAllHooks`); pin and test them per Claude Code release.
- **Follow-ups / new risks:**
  - **Terms check (done 2026-10-31, reading only; not legal advice):**
    - **Anthropic (re-read 2026-11-01; a reading, not legal advice).** The page
      (https://code.claude.com/docs/en/legal-and-compliance) says both of these:
      - *For:* "Nor does it prevent an end user from signing in to the unmodified Claude Code binary
        with their own Claude subscription". AIQ runs the person's unmodified binary on their own
        sign-in, and never offers a Claude login of its own or touches the token ("developers may not
        collect, store, or intermediate Claude.ai credentials or session tokens").
      - *Against:* "Anthropic does not permit third-party developers to … route requests through Free,
        Pro, or Max plan credentials on behalf of their users", and developers building products
        "should use API key authentication". AIQ's judge prompts are AIQ's, not the person's, so a
        reviewer could read `claude -p` from AIQ as that.
    - **Owner ruling (Imran, 2026-11-01):** use the person's own `claude -p`. Imran's position: AIQ is
      first-party to the person who builds and installs it, runs only on their machine, and works only
      for them. The API key stays as an opt-in fallback.
    - **Open, owed before AIQ is given to anyone else:** the independent review should check this
      reading, or Anthropic should confirm it (the page's contact link). If the reading fails, flip
      the default to the API-key fallback. That is a settings change, not a redesign.
    - **GitHub: no blocker found.** `vscode.lm` is GitHub's sanctioned route for extensions to use the
      person's Copilot models with consent, under the Copilot extensibility acceptable use policy;
      Copilot CLI programmatic mode is documented. Confirm against the current Copilot product terms
      before alpha.
  - Spike: run each call path end to end; measure the 60 s p95 budget; confirm `disableAllHooks` via
    `--settings` also suppresses hooks from the person's user-level `~/.claude/settings.json` (verified
    only for hooks in the same `--settings` source); confirm judge calls are not observed by AIQ's
    own hooks.
  - VS Code consent must be triggered by a user action (first-run setup).

## Evidence
https://code.claude.com/docs/en/headless · https://code.claude.com/docs/en/legal-and-compliance (re-read 2026-11-01) · `claude --help` 2.1.316 · https://docs.github.com/en/copilot/how-tos/copilot-cli/automate-copilot-cli/run-cli-programmatically ·
https://code.visualstudio.com/api/extension-guides/ai/language-model (read 2026-10-31, not run).
