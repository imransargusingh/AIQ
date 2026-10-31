---
id: aiq-threat-model
title: "AIQ threat model"
type: threat-model
status: draft
owner: "Imran"
phase: "0-1"
tags: [security, stride]
links:
  - { to: aiq-architecture, rel: documents }
  - { to: design-claude-code-skeleton, rel: documents }
review-by: 2027-01-29
summary: >-
  System-level STRIDE pass over AIQ's five trust boundaries. The main assets are the person's
  prompts, code and coaching history; the main threats are other local software reading them, a
  malicious pack, and a lost device.
---

# Threat Model

*System-level pass written during `/define-architecture` (no component designs exist yet). Once
designs exist, section 2 is regenerated with
`python3 docs/ai-forward-pack/scripts/docs-graph.py rollup --heading "Adversarial analysis (STRIDE-lite)" --type design`.*

**Assets:** prompts and agent transcripts (may contain code, secrets, customer data); code excerpts;
derived signals and coaching history (reveal how someone works); feedback; the store key; pack
signing keys.
**Attackers considered:** other software running as the same or another OS user; a malicious or
compromised pack; a network attacker; someone holding a lost or stolen laptop; a curious colleague
or manager (social, not technical: AIQ must give them nothing to ask for).

## 1. System trust-boundary map
| # | Boundary | Less-trusted side | More-trusted side | Owning design |
|---|---|---|---|---|
| B1 | Local API (socket / named pipe) | Any local process, hook scripts, extensions | AIQ core | core-api (to design) |
| B2 | Pack loading and execution | Pack content (first- or third-party) | Core and store | pack-manager (to design) |
| B3 | Core → backend (pack/update pull) | Network, CDN | Core | pack-manager (to design) |
| B4 | Store at rest | Disk, backups, lost device | Decrypted store in the core | store (to design) |
| B5 | Core → person's AI provider (rung judge, ADR-0007) | Provider and its account admin | Core | rung-judge client (to design) |

## 2. Threat register (system level, STRIDE)
| ID | Boundary | STRIDE | Threat | Mitigation | Disposition |
|---|---|---|---|---|---|
| T-B1-1 | B1 | Spoofing | Another local process pretends to be a collector and injects fake events (poisons coaching) | Socket 0600 / pipe ACL for the OS user; per-boot token in a user-only file; schema validation | Mitigate |
| T-B1-2 | B1 | Info disclosure | Another process asks the core for cards, history or evidence | API exposes no read of raw content except to AIQ home with the token; no localhost HTTP | Mitigate |
| T-B1-3 | B1 | DoS | Event flood slows the core or fills disk | Per-source rate limit; bounded queues; drop and count | Mitigate |
| T-B1-4 | B1 | Tampering | Hook script replaced by malware | Hooks live in user config the person owns; same-user malware is out of scope (it can already read everything) | Accept |
| T-B2-1 | B2 | Elevation | A pack runs code that reads the store or calls the network | WASM with no host imports for net/fs/store; CPU/mem limits | Mitigate |
| T-B2-2 | B2 | Tampering | Forged or modified pack | Ed25519 signatures, keys pinned in the binary; keep previous pack on failure | Mitigate |
| T-B2-3 | B2 | Info disclosure | A pack's tips or rules are written to exfiltrate data via a "Try this" link | Try-this actions limited to insert text / open local file / open allow-listed doc URL with no user data in the URL | Mitigate |
| T-B2-4 | B2 | Repudiation | Signing key stolen; malicious pack published | Key in an HSM / offline; two-person release; key rotation via app update | Mitigate (process) |
| T-B3-1 | B3 | Info disclosure | Update checks reveal who uses AIQ and when | Fetch from CDN with no identifiers, no cookies; fixed schedule with jitter | Mitigate |
| T-B3-2 | B3 | Tampering | MITM serves an old vulnerable pack | TLS + signature + monotonic version (no downgrade) | Mitigate |
| T-B4-1 | B4 | Info disclosure | Lost/stolen laptop exposes prompts and code | SQLCipher with key in OS keychain; raw content kept until the person deletes it (ADR-0002), so exposure grows with time: Settings show store size and offer delete-older-than; spool file encrypted or content-free | Mitigate |
| T-B4-2 | B4 | Info disclosure | Store copied into a cloud backup | Encrypted at rest; key not in the backup | Mitigate |
| T-B4-3 | B4 | Tampering | Someone edits the store to fake progress | Only matters to the person themselves (no one else sees it) | Accept |
| T-B5-1 | B5 | Info disclosure | Judge call sends a Task to a provider that had not seen it | Same-provider rule; other providers only by the person's explicit choice | Mitigate |
| T-B5-2 | B5 | Info disclosure | Work-account admin logs and retention capture judge calls | Disclosed at setup; the person can turn judging off; judge prompt carries only the Task, not history | Accept (disclosed) |
| T-B5-3 | B5 | Tampering | AIQ's own hooks observe the judge call and create a feedback loop | Claude judging goes straight to the API (no hooks involved); Copilot CLI judge calls tagged and skipped at ingest | Mitigate |
| T-B5-5 | B5 | Tampering | Prompt injection in Task content steers the judge (e.g. text telling it to rate the Task Factory) | Task content passed as quoted data in a fixed envelope; judge answers narrow facts (F1–F12) to a JSON schema; deterministic rubric places the rung; each fact asked twice and disagreement discards it | Mitigate |
| T-REPO-2 | — | Info disclosure | Reading repo metadata (git log authors, CI runs) for F9 pulls in colleagues' names | Read local git only; store counts, not names; never call hosting APIs | Mitigate |
| T-B5-4 | B5 | Elevation | Judge CLI run with tool access acts on the repo | No tools granted; run in an empty temp working directory | Mitigate |
| TR-1 | B5 | Info disclosure | A cross-surface Task sends content to a provider that never received it | Judge envelope includes only content the judge's provider already received | Mitigate |
| TR-2 | B5 | Info disclosure | The person's own edits or local diff reach the judge | F2 computed on device; own edits and diffs never put in a judge call | Mitigate |
| TR-3 | B2/B5 | Tampering | A pack shapes the judge envelope to exfiltrate data | Core alone builds the envelope; packs supply question text and schema only | Mitigate |
| TR-4 | B5 | Tampering | Malformed judge output is trusted | Schema-failing responses discarded | Mitigate |
| TR-5 | B5 | Tampering | Model-written evidence carries HTML, links or commands into a card | Rendered as plain text only | Mitigate |
| TR-6 | B5 | Elevation | A malicious `claude`/`copilot` on PATH or repo config hijacks judge calls | Judge binary resolved by absolute path at setup; run outside any repo | Mitigate |
| TR-7 | B5 | Info disclosure | AIQ handles provider credentials | AIQ never reads provider OAuth tokens; the person's Anthropic API key lives only in the OS keychain, is sent only to the Anthropic API, and is never logged or exported | Mitigate |
| TR-8 | B5 | DoS (cost) | Judging burns the person's quota | Person-set daily judge budget; lazy/batched re-judging; queue when spent | Mitigate |
| TR-9 | B5 | Repudiation | No record of what was sent where | Per-call send log with version stamps, viewable locally | Mitigate |
| T-SOC-1 | — | Info disclosure (social) | A manager asks the person to "show their AIQ" | No views built for others; share types are fixed and previewed; nothing to export wholesale | Mitigate (design) |
| T-REPO-1 | — | Info disclosure | AIQ hooks in a repo config are seen or run by teammates | Install at user level only (ADR-0005) | Mitigate |

### 2a. Component threat register (generated)
*Generated with `docs-graph.py rollup` from component designs; do not edit by hand.*

| source | Trust boundary | STRIDE threat | Disposition | Control / rationale | Negative test |
|---|---|---|---|---|---|
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | B1 hook client → core (local socket / pipe) | S: another local process or OS user sends frames | mitigate | Socket in a user-only directory (0700) / pipe with a DACL for the current user only; per-boot token in a user-only file; core checks the peer's user id where the OS gives it | Frame without token → closed, nothing stored; another OS user cannot connect (Linux CI with a second user; Windows via spike S1) |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | B1 | T: tampered frames inject fake prompts to trigger cards | accept | Needs the same OS user and the token; such a process can already read everything the person can. Residual: none beyond existing user compromise | — |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | B1 | I: another process reads frames or the token | mitigate | User-only permissions on socket dir, token file, spool and pause file | Permission tests on created files (mode 0600 / DACL) |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | B1 | D: flood the core | mitigate | Bounded channel, frame size cap (256 KB), one frame per connection | Flood test: hook latency stays within budget |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | B1 | E: frame triggers code execution | prevent | Frames are data parsed by serde into typed structs; no eval, no paths from frames are opened | Fuzz frames |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | B2 Claude Code → hook stdin | T: hostile prompt text (e.g. control characters) echoed into the card | mitigate | The card never echoes prompt text; `aiq show` strips terminal control sequences before printing | `Show_PromptWithAnsiEscapes_PrintsSanitised` |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | B2 | E: AIQ alters the agent's behaviour | prevent | Hook never writes `additionalContext`, `decision` or `continue`; output only `systemMessage` on Stop | Contract test: hook stdout is empty or exactly `{"systemMessage": string}` |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | B3 store at rest | I: stolen disk or backup reads prompts | accept (was mitigate) | **Encryption deferred by Imran, 2026-10-31** ([note](../notes/note-20261031-encryption-deferred.md)): plain SQLite in a 0700 directory, files 0600 (Windows: per-user `%LOCALAPPDATA%` ACL). Residual: anyone with the disk, a backup or admin rights reads prompts | `store_file_is_owner_only`, `spool_file_is_owner_only` (file modes) |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | B3 | I: spool holds plain text while the core is down | mitigate + accept | Spool is user-only, ≤ 1 MB, removed on drain. Residual: plain text on disk briefly (the store is also plain text now; see row above) | File-mode test; spool empty after drain |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | B4 settings file (`~/.claude/settings.json`) | T: install corrupts the person's settings | mitigate | Parse, merge only AIQ entries, write to a temp file and rename, keep a backup, refuse on invalid JSON | Golden tests on settings files with other hooks; invalid JSON untouched |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | B4 | R: person cannot tell what AIQ changed | mitigate | `install` shows the diff and asks; `uninstall` restores only AIQ entries | Round-trip test install → uninstall → byte-identical file |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | B4 | E: repo-level hooks run AIQ for teammates | prevent | AIQ never writes to a repo's `.claude/` (ADR-0005) | Test: install never touches the working directory |
| [design-claude-code-skeleton](../design/claude-code-skeleton.md) | B5 local logs | I: prompts leak into logs | prevent | Logs carry ids, counts and codes only; never prompt text (O11) | Log scan test with a canary prompt string |

<!-- rolled up from 1 artifact(s) by docs-graph.py rollup on 2026-10-31 -->


## 3. Accepted-risk register
| Boundary / threat | Accepted by | Rationale | Residual risk | Revisit when |
|---|---|---|---|---|
| T-B1-4 | Pending Imran | Malware running as the same user can already read every file AIQ reads | Full local compromise exposes AIQ data too | If AIQ ever stores data the user's own tools do not already hold |
| T-B5-2 | Imran (ADR-0007) | Judging through the person's own account is the chosen design | Admin of a work account can see judge calls | If an on-device judge becomes good enough |
| T-B4-3 | Pending Imran | Data is private to the person; tampering only fools themselves | None to others | If aggregates or sharing use these values (then sign shares) |
| Phase 1 B1-T: forged frames from the same OS user (design-claude-code-skeleton) | Imran, 2026-10-31 | Needs the same user and the per-boot token; that process can already read everything | None beyond existing user compromise | If frames ever carry other people's data |
| Phase 1 B3-I: plain-text spool while the core is down (design-claude-code-skeleton) | Imran, 2026-10-31 | Encrypting it needs the store key on the hook's hot path (ADR-0005) | Up to 1 MB of recent prompts in a user-only file until the core drains it | If a spike shows keychain reads fit the hook budget |
| Phase 1 B3-I: store not encrypted at rest (design-claude-code-skeleton; supersedes T-B4-1's SQLCipher control for now) | Imran, 2026-10-31 | Encryption deferred; revisit whether AIQ needs it at all ([note](../notes/note-20261031-encryption-deferred.md)) | Anyone with the disk, an unencrypted backup or admin rights can read stored prompts; exposure grows until the person runs `aiq forget --all` | Before any alpha user other than Imran installs AIQ, or a managed-device requirement names encryption at rest |

## 4. Cross-cutting controls
OS user isolation and keychain; SQLCipher; Ed25519 pack signing with pinned keys; WASM sandbox;
signed app updates; user-level-only hook install.

## 5. Gaps and flagged unknowns
- The judge path (B5) gets its full threat review in the rung-model thread (docs/specs/rung-model.md); rows TR-1 to TR-9 come from that review; the T-B5 rows are the system-level cross-check.
- No component designs yet; every boundary needs its own STRIDE-lite in `/design-slice`.
- Windows named-pipe ACL and macOS keychain prompts need a spike.
