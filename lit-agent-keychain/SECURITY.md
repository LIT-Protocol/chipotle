# Keychain v2 security contract

## What the cryptography enforces

Assuming the actual frontend/SDK, chosen identity provider, trusted Lit TLS endpoint,
Lit runtime and derivation root remain trustworthy, compromising the Keychain API,
database and billing key cannot fabricate owner authorization or extend its signed
scope. Ciphertext, metadata, version and release mode are bound together. An agent
must prove possession of its own signing key; each request binds the exact envelope,
operation, policy hash, deadline and response encryption key. Results also carry an
action signature, preventing a proxy from fabricating an encrypted result.

The hosted frontend is part of the trust boundary. Open source does not prove that
a server served an audited build. A compromise that also changes delivered JavaScript
can steal new plaintext/owner approvals. Run a verified client release for a stronger
separation from the hosting operator. Passkeys remain tied to their registered RP/origin.

## Attested Lit endpoint

The SDK, CLI and MCP server attest `VITE_LIT_API_URL`/`litApiUrl` before the first
Chipotle request when a policy is pinned for that origin (`ATTESTED_ORIGINS`). The
check verifies the TDX v4 quote's ECDSA chain to the pinned Intel SGX Root CA,
replays the dstack event log into RTMR0-3, requires the measured app-id, compose
hash (SHA-256 of the served `app_compose`, all images digest-pinned) and OS image,
and confirms both hashes are whitelisted in the DstackApp and DstackKms contracts on
Base. Node callers also bind the observed TLS certificate to the enclave through the
dstack-ingress evidence quote. Any failure blocks the request.

Limits: Intel TCB status, QE identity and PCK revocation collateral are not
checked; the quote has no caller nonce, so freshness comes from the TLS binding,
which browsers cannot perform; the Base RPC endpoint is trusted for the governance
lookup (a lying RPC can only cause false rejections or accept a hash the Safe never
whitelisted). The policy constants are compiled into the client, so the same
"verified client release" caveat above applies.

## Agent-side plaintext handling

`get`, the `get_secret` MCP tool and `keychain run` all deliver plaintext to the
agent host; from there the client is trusted. `run` avoids stdout and passes the
value through the child process's environment. The Keychain CLI itself does not
print it; the child program can still log or disclose it, including into agent
transcripts. It is also within reach of other processes running as
the same user (`ps eww`, `/proc/<pid>/environ`). `--file` writes plaintext to a
mode-0600 file that is created before the child starts, never overwrites an existing
path, and is unlinked when the child exits; a SIGKILL of the CLI leaves it behind and
the bytes may survive on disk after unlink. Neither mode is a sandbox. The CLI zeroes
its copy of the agent key and drops its references to the fetched values once the
child has started; JavaScript strings cannot be scrubbed, so the plaintext may linger
in the CLI process heap briefly before it exits.

## Accepted operator trust

The database holds signed policy records and chooses the current record. The action
queries that selection on every authorization, without a cross-execution policy cache.
The operator can withhold updates, serve an older valid record, restore a disabled
agent/vault or retired version, or restore a previously approved owner credential.
Revision/hash chains give honest concurrency checks, not rollback protection.

Rollback does not create a new agent identity or extend signed expiry. A requester
still needs an allowed private key. An operator colluding with a formerly allowed
agent can restore that agent's prior access while the old permission remains valid.
Owner credential membership may be indefinite; access policies are always finite
and capped at 90 days. Credential receipts never make Google session keys permanent.
The root owner is the initial state when no credential policy exists; withholding a
later credential policy can restore that initial owner, as part of the accepted model.

The operator can deny service or delete/withhold ciphertext and metadata. Database
backups and exported ciphertext help availability; they do not make deletion
cryptographically irreversible. Revocation cannot retract plaintext already received.
With an honest backend, a committed update applies to subsequent policy lookups;
in-flight actions that already authorized may finish. The action rechecks signed
request/policy expiry immediately before decryption/use. An external operation can
complete after its authorization window if already started.

## Identity and session proofs

- Wallet: EOA EIP-712 authorization, fixed domain/version and vault salt, exact object
  digest/operation, nonce and short validity. Wallet picker availability is not proof
  of ERC-1271/6492 support; those wallet types are deliberately outside v2.
- Passkey: P-256 WebAuthn signature over authenticator data and client-data hash;
  exact origin, RP-ID hash, operation challenge, user presence and user verification.
  Cross-origin assertions are rejected. Registration uses discoverable credentials;
  fresh-device sign-in discovers a public descriptor and then obtains a new bound
  assertion. Synced/recovery providers are part of the chosen custody model.
- Google: fixed Google JWKS HTTPS URL, RS256, issuer, exact audience/subject, expiry,
  issued time, optional authorized-party check and nonce binding. A 10-minute client
  session's Ed25519 public key, random nonce, scope, network, registry and lifetime
  are hashed into the Google nonce. Each object approval proves session-key possession.
  Google account issuance/recovery is accepted custody. Email is not the identity.

The immutable authorization action signs a domain-separated receipt for an exact
object while the owner proof is valid. An old ID token cannot mint new receipts after
expiry. Durable ciphertext/permission receipts survive token expiration; they are not
reusable management sessions. Keychain sessions only authorize storage/UI operations
and never replace a receipt. They are HttpOnly, SameSite=Strict, Secure on HTTPS and
expire after 12 hours. Credential changes invalidate existing metadata sessions.

## Encryption protocol

Canonical JSON sorts ASCII property names and permits only safe integer numbers.
Arrays retain order; strings are exact Unicode text, without normalization. Unknown
schema fields, invalid hex/base64url, unsafe numbers and oversized values are rejected.
The protocol is versioned; changes must retain deployed action artifacts.

The action's secp256k1 key comes from Lit's current execution CID. Its X25519 private
key is HKDF-SHA256(root, salt=`lit-keychain/v2`, info=`secret-envelope-x25519`, 32).
The root only signs fixed receipt, key-binding and encrypted-response structures.
The browser verifies an action-signed binding of its challenge, manifest hash and
X25519 public key against the root public key fetched directly from the trusted Lit
TLS origin. Neither the backend nor a generic unattached enclave quote establishes
that binding. The fixed public-discovery Lit Action returns only the already-public secp256k1
identity through the existing Chipotle execution API; it has no private-key operation.

Import creates a random 256-bit DEK and a 96-bit AES-GCM IV. Metadata is authenticated
as AAD; HPKE wraps the DEK with the same metadata in its domain-separated info context.
HPKE uses DHKEM(X25519, HKDF-SHA256), HKDF-SHA256 and AES-256-GCM (RFC9180 IDs 32/1/2).
Owner receipts bind the complete envelope digest. Agent responses use a fresh
recipient X25519 key and HPKE info bound to the signed request hash.

Random keys and byte buffers are cleared where practical; JavaScript strings, garbage
collection and provider/browser internals do not offer guaranteed memory erasure.
No secrets, owner private keys or agent private keys are persisted by the API.
Names, public identities, permissions and traffic metadata are not encrypted.

## Authority releases

The immutable authorization action is released in versions, and every released
version is kept forever in `actions/archive/` (content-addressed, append-only; the
build refuses to run if an archived file is missing or altered). A vault's identity is
the hash of its owner document, not of any code, so a vault keeps working across
releases:

- The API accepts a sign-in receipt from any released authority version bound to the
  vault's owner document. The version that verifies is recorded for the vault
  (`kc_vault_authorities`) and granted to its execution group the first time it is
  used. The vault's current authority only ever moves to a newer release.
- Each secret pins the authority release that approved it. Policy updates, rotations
  and restores for that secret are verified, by the API and by the secret action
  itself, under exactly that release. Clients run the pinned release's bytes,
  fetched by hash from `/api/templates/<sha256>` and checked against the hash list
  compiled into the client, so the registry cannot introduce a release the client was
  not built to trust.
- New secrets pin the newest release the client knows. Sign-in tries the newest
  release first and falls back to the vault's recorded release if the newest denies
  the owner.

Credentials (the vault's owner set) are receipted by whichever release approved
them, and a release can only verify its own receipts. A release that cannot verify
the current credentials receipt treats the vault as having no credentials policy:
the root owner from the vault document is accepted and nothing else is. This grants
no capability beyond the documented operator trust, since the operator can already
serve a null credentials state for any vault, and it never accepts an unverifiable
owner set. Consequence: a vault whose root owner was replaced under release A keeps
operating under A (its recorded release) until the root owner, or an owner approved
under a newer release, re-approves the owner set there.

## "Use inside Lit" actions

Every non-export action comes from the reviewed public catalog
[agent-keychain-library](https://github.com/LIT-Protocol/agent-keychain-library),
pinned to an exact commit and integrity hash in `package.json`/`package-lock.json`. An
action cannot export, rewrap, sign arbitrary data, follow redirects, or execute caller
code. Its manifest declares, and the shared harness enforces inside the enclave: a
credential pattern the plaintext must match before the action runs; an exact HTTPS
hostname allowlist and request budget for the only HTTP client the action can reach;
a shape for the agent's signed input, validated before any key derivation; a shape and
16 KiB cap for the result; and timeouts and response-size limits. The build rejects
action code that imports anything but the contributor library or references `fetch`,
`eval`, dynamic `import`, timers, `crypto` or the Lit runtime. Raw upstream responses,
errors and headers are never returned or logged; agents see only `access_denied`.

For example, the Stripe action only GETs `https://api.stripe.com/v1/balance` and
returns bounded numeric amounts, three-letter currency codes and a boolean. Upstream
providers necessarily receive the credential over TLS and remain trusted for their
operation. Actions whose purpose is to return upstream text (an OpenAI reply, a GitHub
file) can carry whatever that provider returns; review, not the shape, is what keeps a
projection from echoing the credential. Owners must retain the original credential to
reimport into a future action release. Catalog ids are permanent and their template
bytes are pinned in `actions/catalog.lock.json`; deprecated actions stay restorable.

## Budgets, audit and recovery

The master management key and wildcard bootstrap execution key stay server-side.
Per-vault execution-only usage keys are intentionally returned to authenticated owners
and included in agent configs. They are encrypted in the DB with AES-256-GCM, a separate
operator-held key, random nonces and vault-bound AAD. The operator can recover those
billing keys, but they do not grant owner/agent authority or decrypt secrets by themselves.
Protect exported configs as billing credentials. Rotation persists pending revocation
before calling Chipotle and does not report success until removal is confirmed.

Each usage key can execute a fixed recovery/discovery group and, while paid, the
vault's secret group. Enrollment requires a valid owner-signed manifest and matching
immutable source/CID. Incremental group grants avoid Chipotle's ten-CID bulk-update cap.
Cancellation changes key permissions rather than deleting action grants or ciphertext.
Billing-owner guards deny child usage keys access to parent funding/card management.

**No hard per-user execution or dollar cap exists.** Users share the operator's parent
balance. This explicitly accepted limit allows abuse of allowed actions to exhaust
that balance; account funding and provider auto-recharge settings bound exposure.
Free (5 secrets) and the $10/month subscription both include fair use execution with
no automatic overage charges.
Atomic app counters only limit server-sponsored login attempts. Anonymous capacity
can be exhausted independently. The Stripe subscription is a storage/sponsorship
entitlement, not a cryptographic access policy: an independently funded payer may
still run the signed actions after subscription expiry.

With an honest service, storage mutations stop at paid expiry while the vault exceeds
the Free limit. Sponsored execution of enrolled secret actions continues on Free;
lapsing a subscription is not a revocation mechanism and never was a spending
guarantee. The worker runs every minute and refreshes Stripe state every five minutes;
webhooks and explicit refresh also update the DB. Login, owner revocation and encrypted
backups remain available. Data is never automatically deleted on cancellation.

No read-count/one-time-use guarantee exists against an operator who can roll back the
database. Identical signed requests may execute repeatedly until expiry; integrations
must account for external side effects. Stripe balance reads have no write side effects.

Management writes and their audit events share a transaction. The activity feed is
not a complete execution ledger: an independent Lit payer can execute an action
without going through this API. Action-signed responses are verifiable evidence of
individual completed results; browser/API cookies alone are not.

Encrypted exports contain current ciphertext versions, their signed policy/manifests,
the root authority descriptor, and signed recovery credential settings. They contain
neither private keys nor historical ciphertext versions. Credential restoration only
initializes a missing vault and cannot overwrite existing credential settings. Restoring does not overwrite a different existing secret. Google-only users
recover by signing in to the same Google account. Alternative recovery credentials
must be approved while an existing owner is available; download a fresh encrypted
export when changing credentials, since it carries the root authority descriptor. Lost owner credentials without an approved recovery path
cannot be replaced by an operator-issued reset token.

## Release validation boundaries

Local tests use real WebCrypto, browser WebAuthn, the actual Deno worker and PostgreSQL,
with synthetic Lit keys and Google issuance fixtures. They do not prove live Google
OAuth configuration, production TEE measurements, or future Lit root-key continuity.
Before launch, deploy the telemetry and billing-owner guards, configure Stripe, Google and wallet
providers, and run live import/read/rotation/revocation/recovery and Stripe balance
tests through the intended Chipotle API. Keychain uses Chipotle for all TEE execution
and action public-key discovery; application validation requires no direct TEE
infrastructure access. Runtime security and derivation-root continuity across provider
restarts/upgrades remain Chipotle trust assumptions, with infrastructure qualification
owned by the platform. Retain an independent recovery copy of the original strict-mode
credential.
