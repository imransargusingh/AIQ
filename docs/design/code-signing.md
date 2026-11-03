---
id: design-code-signing
title: "Design: code-signing the Windows build"
type: design
status: deferred
owner: "Imran"
phase: "alpha"
tags: [design, alpha, release, signing, smartscreen, install, auto-update, security]
links:
  - { to: design-private-alpha, rel: depends-on }
  - { to: design-auto-update, rel: depends-on }
review-by: 2027-05-02
summary: >-
  DEFERRED (Imran, 2026-11-03): AIQ is an internal tool, so no commercial signing for now. Revisit only
  if the G3 install on a managed laptop shows the unsigned files blocked rather than warned. The options
  below (Azure Artifact Signing first) stay as the record of what to do then.
---

# Design: code-signing the Windows build

Started without waiting, under Imran's standing rule: it is cloud-only, and it buys nothing and creates
no account. The alpha welcome note (PR #81) tells testers that `aiq.exe` and `aiq-home.exe` are not
signed. Most alpha testers use managed work laptops, so this is a real install risk.

## Decision (2026-11-03): deferred

Imran: "why do we need to sign the build? This is an internal tool." So: **no signing for now.** Nothing is
bought and no account is made. The alpha ships unsigned and the welcome note keeps its SmartScreen
click-through text.

**Revisit trigger:** the G3 install on a managed laptop shows the unsigned exes or `install.ps1`
**blocked** (app-control rule, Group Policy script policy, Smart App Control), not just warned. Then the
recommendation below is the starting point, and the US/Canada and legal-name question goes back to Imran.

## Recommendation if the trigger fires

**Recommendation: Azure Artifact Signing (formerly Trusted Signing), Basic tier, started when the trigger fires.**

- It costs about **$9.99 a month** (about $120 a year) and needs no USB token. It signs from GitHub
  Actions. *Source: Microsoft Learn "Code signing options", updated 2026-08-29.*
- The slow part is the identity check: **1 to 20 business days**. *Source: Artifact Signing quickstart.*
  Signing would land 1 to 4 weeks after the trigger fires.
- Signing does **not** remove the SmartScreen warning on day one. Reputation builds per certificate
  over downloads (EV lost its instant pass in 2024). What signing does do: the warning names a
  publisher, the reputation carries over from release to release, and company rules that allow
  "signed by a known publisher" can let it through. *Source: Microsoft Learn; Eric Lawrence, "Best
  practices for SmartScreen AppRep" (2024-11-15).*
- Meanwhile, keep the welcome note's click-through text. Gate G3 (a run on a managed laptop) tells us
  whether unsigned is merely annoying or actually blocked. If it is blocked, signing becomes the alpha's
  first task, as private-alpha A2 already says.

**Imran: the one question is whether you can enrol as an individual.** Individual enrolment is open only
to people in the **US or Canada**. The certificate then shows **your legal name** as the publisher, taken
from an Azure billing account of type Individual. If either is a problem (location, or your name on a
side project while you work at Microsoft), the fallback is an OV certificate from a CA (below), which
also shows your name, or forming a legal entity. *Source: Artifact Signing quickstart, identity
validation section.*

## Options compared

| Option | Cost | Lead time | SmartScreen | What it needs from Imran | Fit |
|---|---|---|---|---|---|
| **Azure Artifact Signing (Basic)** | ~$9.99/month, 5,000 signatures | 1–20 business days identity check | Builds reputation over time; publisher shown | Azure subscription, billing account type Individual, government ID, portal steps (below) | **Recommended** |
| **OV certificate from a CA** (DigiCert, Sectigo, SSL.com) | $150–300/year | Days (CA checks ID); key must be on a hardware token or the CA's cloud HSM | Same as Artifact Signing | Buy cert, ID check, then either a token (cannot sign on a GitHub runner) or the CA's paid cloud signing | Fallback if not eligible for Artifact Signing |
| **EV certificate** | $400+/year | As OV, stricter checks | **Same as OV since 2024**, no instant pass | As OV, usually a registered business | Not worth it |
| **SignPath Foundation** (free for open source) | Free | Their review | OV-level | An **OSI-approved licence** on every component, project already released, a public code-signing policy, manual approval per release | **Not eligible today**: AIQ has no licence (privacy page says none chosen) |
| **Stay unsigned**, document the click-through | Free | None | "Unknown publisher" warning; Microsoft's page: "enterprises may block entirely" | Nothing | Fine for the first testers only if G3 shows no block |
| Self-signed | Free | None | "Strong block for public users" | — | Rejected |

Costs, SmartScreen behaviour and the EV change are from Microsoft Learn "Code signing options"
(2026-08-29). SignPath terms are from signpath.org/terms. Certificates from a CA now last at most **460
days** (from 2026-03-01), so an OV certificate is renewed yearly (DigiCert blog).

## What I checked and what I assumed

**Read (documentation, 2026-11-02):** Microsoft Learn code-signing options page; Artifact Signing
quickstart and FAQ; `azure/artifact-signing-action` README; PowerShell `about_Execution_Policies`;
SignPath Foundation terms; DigiCert on the 460-day limit; Lawrence on SmartScreen AppRep.

**Read (this repo, main 444dcda):** `.github/workflows/release.yml`, `packaging/windows/install.ps1`,
`packaging/windows/upgrade.ps1`, `packaging/windows/README.txt`, `docs/design/private-alpha.md`,
`docs/design/auto-update.md`.

**Assumed, not checked** (each says what would confirm it and what breaks if wrong):

- `assume:` Artifact Signing can sign `install.ps1` and `upgrade.ps1`. The FAQ says it signs "all file
  types that SignTool supports" and does not name `.ps1`. Confirm: the first signed CI run, then
  `Get-AuthenticodeSignature` on the scripts. If wrong: only the exes are signed, and a laptop whose
  Group Policy requires signed scripts still cannot run `install.ps1`.
- `assume:` the alpha laptops do not enforce a Group Policy script policy. The docs say a Group Policy
  execution policy **overrides** `-ExecutionPolicy Bypass`. Confirm: G3. If wrong: the README's
  install command fails on those laptops, signed exes or not, until the scripts are signed.
- `assume:` Explorer's "Extract all" passes the download mark to the unzipped exes, and `Copy-Item` in
  `install.ps1` keeps it, so SmartScreen checks `aiq-home.exe` when the installer opens it. Confirm:
  G3, or `Get-Item -Stream Zone.Identifier` on the installed exe. If wrong: SmartScreen never
  appears, and the only real risk is a company app-control rule.
- `assume:` auto-update downloads (`Invoke-WebRequest` in `upgrade.ps1`) carry no download mark, so
  updates do not raise SmartScreen. The PowerShell docs say these tools "may not mark" files.
- Unknown: what rules Microsoft-managed laptops apply to new apps. Only G3 on such a laptop answers it.

## Recommended option: the steps

### Imran's steps (need money, an account or an ID; Claude does none of them)

1. Confirm eligibility: in the US or Canada, and happy for your legal name to be the publisher.
2. In an Azure subscription you own, check the billing account is type **Individual**, with the name
   and address on your government ID.
3. Register the `Microsoft.CodeSigning` resource provider.
4. Create an Artifact Signing account, **Basic** tier. Billing starts here and is not prorated.
5. Give yourself the **Artifact Signing Identity Verifier** role. Create a **Public** identity
   validation (portal only). Wait 1–20 business days. Upload documents if asked (three tries).
6. Create a **Public Trust** certificate profile on that identity. Check "Certificate subject preview".
7. Create an app registration (or user-assigned managed identity) with a **federated credential** for
   GitHub environment `release` on `imransargusingh/AIQ`. Give it the **Artifact Signing Certificate
   Profile Signer** role on the profile.
8. In the repo, add environment `release` with yourself as **required reviewer**, and these values:
   `AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, `AZURE_SUBSCRIPTION_ID` (secrets), and `SIGNING_ENDPOINT`,
   `SIGNING_ACCOUNT`, `SIGNING_PROFILE` (variables). No password or key goes into GitHub.

Then tell the thread. The workflow change below is cloud work.

### Change to `.github/workflows/release.yml`

Only the `windows` job changes. Pull-request runs stay unsigned: forks get no secrets, and a PR run
must not use the signing identity.

1. Job permissions: add `id-token: write` (OIDC for `azure/login`), keep `contents: read`.
2. Job environment: `release` on tag and manual draft runs only, so every signature waits for
   Imran's approval. `assume:` an expression such as
   `environment: ${{ github.event_name != 'pull_request' && 'release' || '' }}` is accepted; confirm on
   the first run. If not, split signing into its own job that downloads the built files, signs them and
   re-uploads them before the zip step.
3. After "Build aiq-home.exe" and **before "Zip and checksum"**, add, with
   `if: github.event_name != 'pull_request'`:
   - `azure/login@v3` with the three IDs;
   - `azure/artifact-signing-action@v2` with `endpoint`, `signing-account-name`,
     `certificate-profile-name` from the variables, `files` listing `target/release/aiq.exe`,
     `apps/home/src-tauri/target/release/aiq-home.exe`, `packaging/windows/install.ps1`,
     `packaging/windows/upgrade.ps1`, `file-digest: SHA256`,
     `timestamp-rfc3161: http://timestamp.acs.microsoft.com`, `timestamp-digest: SHA256`.
   Timestamping keeps the signature valid after the certificate expires.
4. Order matters: signing changes the file bytes, so it must come before the zip and `SHA256SUMS`.
   Signing after would make every checksum wrong.
5. In the first smoke test, on signed runs, fail unless `Get-AuthenticodeSignature` reads `Valid` for
   both installed exes, with the expected publisher name.
6. `runs-on: windows-latest` already meets the action's "Windows runners only" rule. `assume:`
   windows-latest is windows-2022 or windows-2025, the two the README lists. Pin `windows-2025` if not.

### `install.ps1` and auto-update

- **Checksum (PR #60, #69): no change.** `SHA256SUMS` is made after signing, over the signed zip, so
  the check in `upgrade.ps1` works as before.
- **`install.ps1`: no change needed.** Optional and not recommended now: an `Unblock-File` on the
  copied exes would silence the very warning signing is meant to earn.
- **Optional hardening, a later PR:** before `-Apply` swaps the exes, `upgrade.ps1` checks that
  each new exe's signature is `Valid` and from the pinned publisher, and refuses otherwise
  (`AIQ-UPGRADE-SIGNATURE`). This narrows the residual risk in the private-alpha threat model:
  "someone who controls the GitHub repo can publish both a bad zip and a matching checksum". With
  signing behind a required-reviewer environment, a repo takeover alone can no longer produce an
  update AIQ installs. Catch: `-Apply` runs the **installed** `upgrade.ps1`, so the check protects only
  from the release *after* the one that ships it, and it must allow the unsigned-to-signed step once.

## If Imran is not eligible for Artifact Signing

Buy an OV certificate from a CA that offers cloud signing usable from GitHub Actions (for example
DigiCert KeyLocker or SSL.com eSigner; prices vary, not checked). The workflow change is the same shape:
a sign step before the zip, with that CA's action or CLI in place of `azure/artifact-signing-action`.
A USB-token certificate cannot sign on a hosted runner, so it would mean signing on Imran's laptop by
hand. Avoid that.

## Not in scope

Buying, enrolling or creating any account; MSI, MSIX or winget packaging; macOS signing; choosing a
licence (it would make SignPath possible, but that is a separate decision).
