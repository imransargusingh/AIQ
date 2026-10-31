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
| T-B5-3 | B5 | Tampering | AIQ's own hooks observe the judge call and create a feedback loop | Judge calls tagged and skipped at ingest; hooks disabled for the call | Mitigate |
| T-B5-5 | B5 | Tampering | Prompt injection in Task content steers the judge (e.g. text telling it to rate the Task Factory) | Task content passed as quoted data in a fixed envelope; judge answers narrow facts (F1–F12) to a JSON schema; deterministic rubric places the rung; each fact asked twice and disagreement discards it | Mitigate |
| T-REPO-2 | — | Info disclosure | Reading repo metadata (git log authors, CI runs) for F9 pulls in colleagues' names | Read local git only; store counts, not names; never call hosting APIs | Mitigate |
| T-B5-4 | B5 | Elevation | Judge CLI run with tool access acts on the repo | No tools granted; run in an empty temp working directory | Mitigate |
| T-SOC-1 | — | Info disclosure (social) | A manager asks the person to "show their AIQ" | No views built for others; share types are fixed and previewed; nothing to export wholesale | Mitigate (design) |
| T-REPO-1 | — | Info disclosure | AIQ hooks in a repo config are seen or run by teammates | Install at user level only (ADR-0005) | Mitigate |

## 3. Accepted-risk register
| Boundary / threat | Accepted by | Rationale | Residual risk | Revisit when |
|---|---|---|---|---|
| T-B1-4 | Pending Imran | Malware running as the same user can already read every file AIQ reads | Full local compromise exposes AIQ data too | If AIQ ever stores data the user's own tools do not already hold |
| T-B5-2 | Imran (ADR-0007) | Judging through the person's own account is the chosen design | Admin of a work account can see judge calls | If an on-device judge becomes good enough |
| Provider terms (ADR-0007) | Imran, 2026-10-31 | Owner confirmed AIQ may call models under the person's own credentials | Anthropic's published Claude Code terms restrict third-party use of Free/Pro/Max sign-in; an account action or a forced switch to API keys is possible | If Anthropic or GitHub changes or enforces terms; before public release |
| T-B4-3 | Pending Imran | Data is private to the person; tampering only fools themselves | None to others | If aggregates or sharing use these values (then sign shares) |

## 4. Cross-cutting controls
OS user isolation and keychain; SQLCipher; Ed25519 pack signing with pinned keys; WASM sandbox;
signed app updates; user-level-only hook install.

## 5. Gaps and flagged unknowns
- The judge path (B5) gets its full threat review in the rung-model thread (docs/specs/rung-model.md); the T-B5 rows here are the system-level cross-check.
- No component designs yet; every boundary needs its own STRIDE-lite in `/design-slice`.
- Key recovery pending (ADR-0003).
- Windows named-pipe ACL and macOS keychain prompts need a spike.
