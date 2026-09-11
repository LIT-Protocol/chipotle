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
that binding. The new public endpoint returns only the already-public secp256k1 identity.

Import creates a random 256-bit DEK and a 96-bit AES-GCM IV. Metadata is authenticated
as AAD; HPKE wraps the DEK with the same metadata in its domain-separated info context.
HPKE uses DHKEM(X25519, HKDF-SHA256), HKDF-SHA256 and AES-256-GCM (RFC9180 IDs 32/1/2).
Owner receipts bind the complete envelope digest. Agent responses use a fresh
recipient X25519 key and HPKE info bound to the signed request hash.

Random keys and byte buffers are cleared where practical; JavaScript strings, garbage
collection and provider/browser internals do not offer guaranteed memory erasure.
No secrets, owner private keys or agent private keys are persisted by the API.
Names, public identities, permissions and traffic metadata are not encrypted.

## Strict Stripe mode

The separate Stripe action cannot export, rewrap, sign arbitrary data, call arbitrary
URLs, accept custom headers/body, follow redirects, or execute caller code. It only
GETs `https://api.stripe.com/v1/balance`, then returns bounded numeric amounts,
three-letter currency codes and a boolean, encrypted to the agent. Raw upstream
responses, errors and headers are never returned or logged. Stripe necessarily
receives its credential over TLS and remains a trusted service for that operation.
Owners must retain the original credential to reimport into a future action release.

## Budgets, audit and recovery

Sponsorship counters are persisted and incremented atomically before execution.
The execution key remains server-side, so clients cannot bypass these app counters
with it. They limit attempts, not exact dollars. Global anonymous capacity is finite;
an attacker may consume that capacity and deny service. No read-count/one-time-use
guarantee exists against an operator who can roll back the database. Identical signed
requests may execute repeatedly until expiry, so new integrations must account for
external side effects; Stripe balance reads have no write side effects.

Management writes and their audit events share a transaction. The activity feed is
not a complete execution ledger: an independent Lit payer can execute an action
without going through this API. Action-signed responses are verifiable evidence of
individual completed results; browser/API cookies alone are not.

Encrypted exports contain current ciphertext versions, their signed policy/manifests,
the root authority descriptor, and signed recovery credential settings. They contain
neither private keys nor historical ciphertext versions. Credential restoration only
initializes a missing vault and cannot overwrite existing credential settings. Restoring does not overwrite a different existing secret. Google-only users
recover by signing in to the same Google account. Alternative recovery credentials
must be approved while an existing owner is available; retain the root vault descriptor
when changing credentials. Lost owner credentials without an approved recovery path
cannot be replaced by an operator-issued reset token.

## Release validation boundaries

Local tests use real WebCrypto, browser WebAuthn, the actual Deno worker and PostgreSQL,
with synthetic Lit keys and Google issuance fixtures. They do not prove live Google
OAuth configuration, production TEE measurements, or future Lit root-key continuity.
Before launch, deploy the public-key/telemetry changes, configure Google and wallet
providers, run a live import/read/rotation/restart smoke test against the intended Lit
network, and retain an independent recovery copy of the original strict-mode credential.
