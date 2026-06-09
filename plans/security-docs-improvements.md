# Security & Verification docs — gap analysis and improvement plan

Status: IN PROGRESS (2026-06-09)
Reviewer framing: read the docs as a CISO evaluating Lit Chipotle for production use.

## Progress (2026-06-09)

On-chain facts pulled live from Base (`cast`), now used in the docs:
- Safe `0xF688…1098`: **threshold 2, 4 owners** (2-of-4), Safe v1.4.1.
- DstackApp `0x3F91…05FfC`: `owner()` = the Safe (no timelock contract in between).
- DstackApp `allowAnyDevice()` = false (device IDs are whitelisted).
- ⇒ **No timelock**; governance change takes effect on quorum execution.

Done this session:
- ✅ A — `docs/architecture/verification/attestation.mdx` (new, links proofofcloud.org)
- ✅ B — `self-hosting.mdx` "Governing upgrades yourself" + cross-links
- ✅ 1,2 — `docs/architecture/verification/upgrade-governance.mdx` (new; real 2-of-4
  facts, propose→approve→execute, what signers verify, self-host governance)
- ✅ 6 — `SECURITY.md` (new; disclosure policy, scope, contacts)
- ✅ 5 — `architectureDocs/deployment/incident-response.md` (new; runbook w/ TODO/DECIDE)
- ✅ 4 — `architectureDocs/deployment/secrets-management.md` (new; out-of-band injection)
- ✅ reconciled `vm-code-upgrade.md` with real facts + link to published governance page
- ✅ `docs.json` nav updated (attestation first, upgrade-governance after onchain-kms)

Open **TODO/DECIDE** items embedded in the drafts that need org input before they're
authoritative:
- Incident commander / Phala account owner / comms roles (incident-response.md).
- Exact Phala stop/drain command for emergency CVM takedown.
- Backstop above a 2-key Safe compromise (raise threshold? timelock? guardian?).
- Secret IAM/role scopes, rotation cadence, dev/prod account separation.
- Telemetry export scrubbing/allow-list before GCP egress.
- Wiring governance-write alerts into Grafana (item 9).

Still TODO (lower priority / needs facts):
- 3 — reproducible builds: partially covered by upgrade-governance "what signers
  verify" + existing chain-of-trust Sigstore row. Decide whether to guarantee
  bit-for-bit reproducibility and write a dedicated page if so.
- 7 — custom-domain warning: published docs already point users at the real
  attestable host (`api.chipotle.litprotocol.com`); the insecure path is the dev
  redirect (CPL-5). Lower user-facing risk than feared — add a short note + track CPL-5.
- 8 — TCB / OS-image patch governance page.
- 10 — telemetry egress (stub added in secrets-management.md; expand if needed).

---

## TL;DR

The **verifier-facing** docs are strong — a skeptical external user can confirm a
running CVM is genuine TDX hardware executing on-chain-whitelisted code
(`docs/architecture/verification/*`, `architectureDocs/deployment/trust-stack-verification.md`,
`derot-key-issuance.md`). The hard cryptographic-verification half is done well.

The **operator / governance / incident** half is missing, thin, or scattered in
internal docs that contradict the published ones. Almost everything published is
written for *the reader verifying us*; almost nothing is written for *us operating
the thing*. That's the gap this plan closes.

We also lack a plain-English explainer of **what attestation even is** before the
how-to-verify pages, and we don't yet make the **self-hosting = self-governance**
point: a self-hosting operator can govern and approve our releases themselves, on
their own timeline.

---

## What's already good (don't regress these)

- `verification/onchain-kms.mdx` — names the real contracts, the Safe owner
  (`0xF688…1098`), the four on-chain gates, and gives `cast`/`curl` commands.
- `trust-stack-verification.md` — cleanly separates what dstack-verifier checks
  automatically vs. what is on-chain governance.
- `derot-key-issuance.md` / `vm-code-upgrade.md` — the "no whitelist → no keys"
  model and the two-independent-actions upgrade flow are well explained.

---

## New docs to write (the two asks from this session)

### A. "What is attestation and how does it work" (NEW, published)
A conceptual page that sits *before* the how-to-verify pages in the
"Security & Verification" tab. Audience: a security reviewer who has never used a
TEE. Should cover, in plain English:
- What a TEE / Intel TDX is and what "confidential compute" guarantees (and does
  not — e.g. known side-channel caveats).
- What an attestation *quote* is: hardware-signed evidence of exactly what code
  booted (MRTD / RTMR0–2 = OS, RTMR3 = the app's compose hash).
- How a remote party turns "I got a quote" into "I trust this server": Intel root
  cert → quote signature → measurement match → on-chain whitelist check.
- Why this replaces "trust the cloud provider" with "trust math + public chain."
- Link out to **https://proofofcloud.org/** as the canonical movement/landing
  page for verifiable cloud, and to Phala's chain-of-trust docs.
- End by funneling into `quick-verify` → `full-verification`.

Nav: add to `docs/docs.json` "Security & Verification" group, as the **first**
entry (conceptual before procedural), e.g. `architecture/verification/attestation`.

### B. Self-hosting as self-governance (UPDATE `self-hosting.mdx` + upgrade doc)
Make explicit that self-hosting is also a **governance** posture, not just an ops
posture: if you self-host, **you** own the DstackApp contract / Safe, so **you**
approve which Lit releases run and on **your** timeline. The hosted service rolls
upgrades on Lit's schedule; a self-hoster can pin a reviewed compose-hash, audit a
new release, and whitelist it only when they're satisfied. Add a short
"Governing upgrades yourself" subsection and link it from the upgrade-governance
page (item 1 below).

---

## Gaps to fix (prioritized)

### P0 — the trust chain has ungoverned human/secret links

**1. Authoritative "how we govern an upgrade" page (NEW).**
`onchain-kms.mdx` names the real Safe; `vm-code-upgrade.md` only speaks generically
("wallet, multisig, timelock, or DAO"). Reconcile into one page stating, for the
production deployment:
- Safe **threshold (M-of-N)** and signers *by role*.
- **Is there a timelock?** State it explicitly either way (the upgrade doc implies
  one "gives stakeholders time to review" but never says one is configured).
- Signer **key custody** (hardware wallets), onboarding/offboarding, and quorum
  behavior when a signer-holding employee leaves.
- Cross-link to the self-hosting self-governance subsection (item B).

**2. Release-approval policy — what signers verify before signing.**
The Safe is the *mechanism*; the human gate is undocumented. Document that signers
must confirm the compose-hash derives from reviewed, merged source and reproduce
it before signing `addComposeHash`. A blind-signed hash makes the multisig theater.

**3. Reproducible builds — the unstated linchpin.**
The whole model rests on "compose-hash ⇒ the open-source code you can audit."
Nothing documents that the published image is reproducibly buildable from public
source, or how a signer/auditor regenerates the compose-hash from `main` and
matches it on-chain. This is the single most important missing doc — it anchors
both item 2 and all external verification.

**4. Secrets management (NEW).**
`docker-compose.phala.yml` injects Stripe live keys, AWS Route53 keys, a GCP
service-account JSON, and the Base RPC as encrypted Phala env vars — **out of band
and not part of the compose-hash measurement.** Document: who can set/read CVM
secrets, whether they're in the TEE measurement, rotation policy, dev-vs-prod
separation, and blast radius if `PHALA_CLOUD_API_KEY` leaks (it can redeploy /
reconfigure the CVM — arguably a higher-value target than one Safe signer, and it
gets one table row in `deployment.md` today).

### P1 — operational security hygiene

**5. Incident-response / key-compromise / revocation runbook (NEW).**
`vm-code-upgrade.md` covers planned rollback only. Add: signer-key compromise,
leaked Phala API key, a bad compose-hash that got whitelisted, break-glass
emergency revocation of a live CVM (does removing the compose-hash stop a
*running* CVM, or only its next restart?), and Base/KMS-availability failure modes.

**6. `SECURITY.md` / responsible-disclosure policy (NEW).**
No vuln-reporting contact, no coordinated-disclosure process, no bug-bounty
statement. Standard checklist item, fully absent.

**7. Known-insecure custom-domain path — track it.**
`deployment.md:179` admits the `*.litprotocol.com` → `*.phala.network` redirect is
"INSECURE" (CPL-5) — right instinct, wrong containment. It's buried internally with
no risk owner/target date, and there's **no warning in the published docs** that the
litprotocol.com URL isn't the attestable endpoint. Add a user-facing note on the
verification pages and a tracked risk acceptance.

### P2 — completeness / defense-in-depth

**8. TCB / OS-image patch governance.**
The verifier checks "TCB SVN … latest patches," but nothing says who monitors
Intel TDX TCB recoveries and Phala OS-image releases, or how `allowedOsImages`
gets rotated. That's a recurring governance action absent from the upgrade doc.

**9. Defensive monitoring of governance writes.**
`onchain-kms.mdx` teaches *users* to audit Safe history. Nothing says Lit alerts on
its *own* governance-contract writes (unexpected `addComposeHash`, owner change).
Detection on the contract, not just an after-the-fact audit trail.

**10. Telemetry egress.**
The `otel-collector` ships to GCP from inside the CVM — an undocumented data-plane
egress out of the TEE. One line on what's in those logs/metrics and the guarantee
that secrets/plaintext don't leave.

---

## Suggested delivery order

1. P0 items 1–4 (governance page, release-approval policy, reproducible builds,
   secrets management) — these are what an auditor would push on first.
2. New attestation explainer (A) + self-hosting self-governance (B) — high-signal,
   low-risk, publishable quickly.
3. `SECURITY.md` (6) and the custom-domain warning (7) — fast wins.
4. Remaining P1/P2 (incident runbook, TCB governance, monitoring, telemetry).

## Files touched / created

| Action | Path |
|---|---|
| NEW | `docs/architecture/verification/attestation.mdx` (item A) |
| NEW | `docs/architecture/verification/upgrade-governance.mdx` (items 1, 2) |
| NEW | `docs/architecture/verification/reproducible-builds.mdx` (item 3) |
| NEW | `docs/architecture/verification/secrets.mdx` (item 4) — or a deployment-side doc |
| NEW | `SECURITY.md` (repo root, item 6) |
| NEW | `architectureDocs/deployment/incident-response.md` (item 5) |
| UPDATE | `docs/architecture/self-hosting.mdx` (item B) |
| UPDATE | `architectureDocs/deployment/vm-code-upgrade.md` (reconcile with item 1) |
| UPDATE | `docs/architecture/verification/*` (custom-domain warning, item 7) |
| UPDATE | `docs/docs.json` (nav for new published pages) |
