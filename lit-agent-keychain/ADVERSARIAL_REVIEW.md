# Adversarial review of Keychain v2

Reviewed during implementation, 2026-09-11. This is the implementing agent's
adversarial review, not an independent cryptographic audit or production attestation.

## Attacker model

Assume the attacker controls the Keychain API, its complete database and billing
credential, and can run arbitrary other Lit Actions. Keep the user's actual client,
chosen owner credential/provider, trusted Lit TLS origin and Lit runtime/key derivation
trustworthy. Separately give the attacker a formerly authorized agent key and replay
old signed policy records. The latter explicitly models accepted revocation rollback.

## Findings fixed

| Finding                                                                                                                    | Fix and regression evidence                                                                                                                                                                                                                                                                         |
| -------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Operator grant signing and server-side plaintext import in v1 violated owner-only authorization.                           | Removed managed vault provisioning, grant signer, bearer setup tokens and plaintext routes. Browser encryption and action-verified owner receipts replace them; API integration checks old routes are absent and uploads contain no secret/agent private key.                                       |
| A proxy-supplied encryption key would let the operator substitute its own key.                                             | Client computes action CID and fetches its public identity directly from a configured Lit TLS origin. A signed binding authenticates the HPKE key, manifest and fresh challenge. Substitution and altered-code tests fail.                                                                          |
| Action key/decryption returns could appear in tracing. Unexpected response errors also formatted entire protocol messages. | Removed sensitive return tracing and unexpected-response payload formatting; sponsored requests explicitly enable Lit privacy mode so owner proof parameters are not logged. The actual Deno release fixture scans TRACE output for private key and protected plaintext.                            |
| Rust's upstream IPFS hasher discarded blocks produced by `push`, panicking on an exact 262144-byte boundary.               | Vendored the small compatible fix in every affected Cargo workspace. Independent JS/Rust empty, single/multichunk and exact-boundary CID vectors agree.                                                                                                                                             |
| An unverified recovery-owner list could trick a client into approving attacker-chosen additional owners.                   | SDK validates current credential receipts against the pinned authority before presenting/composing changes. Forged credential policy tests fail.                                                                                                                                                    |
| Expiring owner membership together with a session could lock out all owners.                                               | Membership defaults to explicit indefinite validity, while Google sessions retain separate bounded lifetimes and agent policies carry an owner-chosen expiry (finite or none), so an owner-set lifetime can never lock owners out. Proof/credential expiry is rechecked after asynchronous lookups. |
| Concurrent secret creation/policy writes could bypass caps or lose revocations.                                            | Vault row locks, compare-and-swap revisions and transactional audit. Postgres tests race creates, reject stale policies and prove atomic sponsorship limits.                                                                                                                                        |
| Listing full signed policies could exceed client response limits at supported vault capacity.                              | Return compact list summaries; verify complete receipts when opening a secret. Paginated summaries and backup iteration support Standard (1,000) and bounded custom plans.                                                                                                                          |
| Retrying a partial restore could fail forever or roll a current policy back.                                               | Exact existing envelopes are idempotent no-ops, including older versions after rotation. Conflicts remain errors. Integration tests restore twice at the resource cap and preserve newer ciphertext/policy.                                                                                         |
| Ciphertext backups alone omitted the credentials needed by recovery owners after complete database loss.                   | Backups include the signed credential policy. A verified bootstrap can initialize only a missing vault; it never changes an existing vault. Tests recover with a delegated owner and reject forged/rollback replacements.                                                                           |
| Header-only HTTP deadlines left body reads vulnerable to stalls.                                                           | End-to-end deadlines, redirect rejection and byte limits; real local HTTP timeout/oversize/redirect tests.                                                                                                                                                                                          |
| Transient anonymous challenge/budget state could grow indefinitely.                                                        | Global challenge/execution caps precede per-identity allocation, and periodic cleanup removes expired transient state without resetting active budget windows.                                                                                                                                      |
| Injected wallets were absent from RainbowKit without WalletConnect configuration.                                          | Use RainbowKit's injected connector metadata. Chromium connects an injected EOA and completes EIP-712 authorization.                                                                                                                                                                                |

## Subscription and direct-execution review

- Usage keys inherited parent billing-management access. Added a shared billing-owner
  guard on funding, card and auto-recharge routes in both platform services. A resolved
  child key fails even though the normal billing identity guard accepts it.
- A bulk group update can carry only ten CIDs. Use incremental idempotent grants and
  separate recovery/secret groups; cancellation and renewal change a key's group
  permissions. API tests enroll more than ten actual actions against a capped adapter.
- Remote grant/revocation success followed by a lost response could diverge from the
  DB. Commit enrollment intent before granting; persist pending key revocation before
  removal. Confirm ambiguous removal with the master's paginated key inventory.
- Forged or reordered payment events could enable unpaid access. Verify raw-body HMAC,
  freshness and mode; re-fetch Stripe state under the same vault lock; enforce the
  configured price, quantity, metadata, active status and paid invoice. Checkout is
  idempotent and cannot accept a caller-selected customer or price.
- Concurrent creation could exceed paid capacity. Race the last slot at 999 secrets;
  exactly one succeeds. Rotation at 1,000 preserves the count. Cancellation preserves
  ciphertext backups and owner revocation. Renewal restores scoped execution.

Shared parent-account spending with no per-user execution/dollar cap is explicitly
accepted. Stripe subscriptions enforce the storage limit (Free 5, Standard 1,000), not
secret access authorization; execution is sponsored on every plan. Fair use has no
automatic overage charges. See BILLING.md for
scope-reconciliation delay and required platform deployment order.

## Attacks exercised

- Wrong wallet/Google subject, issuer, audience, algorithm, nonce or session key;
  expired proofs; WebAuthn wrong origin, RP hash, challenge, missing UP/UV and bad signature.
- Forged/missing/unavailable registry policy; wrong vault, secret, operation, version,
  ciphertext hash, policy hash, recipient, signature, deadline, encryption key or action code.
- Empty allowlists, disable/revoke, signed expiry, dishonest rollback, identical request
  replay and response tampering. Rollback with the old authorized key succeeds as specified;
  scope/recipient expansion and extending signed expiry fail.
- Stripe export attempts, redirects, attacker-selected destinations and arbitrary upstream
  string/error reflection. Only the fixed numeric/currency projection is released.
- Cross-site session mutations, duplicate/stale management writes, concurrent resource
  allocation, forged credential recovery, backup retries and SDK private-key-file overwrite.

## Evidence and remaining boundaries

The automated suite covers RFC9180 HPKE vectors, JS/Rust canonical receipts and CIDs,
real PostgreSQL management flows, Chromium wallet/passkey/Google-only flows, and the
actual Lit Deno worker with encrypted response verification. Local runtime keys and
Google token issuance are fixtures; JWT signatures and action verification are real.
Tests do not substitute a backend “verified” flag for cryptographic verification.

Accepted limitations remain: hosted client trust; Google issuance/recovery trust;
database freshness/rollback/availability; public metadata; previously disclosed plaintext;
repeated signed requests; incomplete direct-execution audit; bounded anonymous sponsorship
that can be exhausted; and strict-mode credentials requiring original-value reimport for
new action code. These are stated in the UI and SECURITY.md where relevant.

No unresolved authorization bypass was found within this model. This does not establish
absence of bugs. Live Chipotle compatibility passed with temporary scoped keys: public discovery,
full secret-action encryption-key binding, stable derivation across calls, arbitrary
code denial, and secret-group permission removal/restoration. The test key/groups were
removed. Live owner/agent lifecycle on the deployed v2 registry and real Google OAuth
remain deployment checks; this public-key test does not prove that full lifecycle. Keychain relies on Chipotle for
runtime security and derivation-root continuity across provider restarts/upgrades;
direct infrastructure access is not an application validation prerequisite. The PR
neither deploys services nor resets a production database.
