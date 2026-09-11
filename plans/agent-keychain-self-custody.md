# Agent Keychain: self-custodied authorization and action-bound encryption

Status: implemented, 2026-09-11. The prelaunch replacement, browser/SDK, owner-verifying Lit Actions, DB registry, recovery, strict Stripe integration and regression tests are in this PR. See [implementation](../lit-agent-keychain/README.md), [security contract](../lit-agent-keychain/SECURITY.md), and [adversarial review](../lit-agent-keychain/ADVERSARIAL_REVIEW.md). No production deployment or database reset performed.

This replaces the existing operator-signed grant / operator-managed vault design. The user confirms that there are no existing users, compatibility is unnecessary, and the implementation may replace the schema and wipe existing app data. Preserve the previous review as a reference; reassess its findings after the replacement rather than patching around the old architecture.

## Objective and trust boundary

A compromise of the Keychain API, database, grant-signing credentials, or execution billing credentials must not let an attacker forge owner authorization or grant permissions outside valid, previously owner-authorized scope. The operator can replay older signed policies and thereby restore revoked access; that limitation is accepted. Plaintext import happens in the user's browser. Decryption and authorization happen in immutable, approved Lit Action code, with outputs protected for the recipient bound into the agent's signed request.

Remaining trust: the frontend/SDK actually executed by the user, the user's selected wallet, authenticator or Google account and endpoint, the Lit enclave/runtime and key derivation infrastructure, cryptographic implementations, and Keychain for policy freshness/revocation. Availability and traffic/unencrypted metadata privacy are separate guarantees.

For this product, **self-custodied authorization** includes control of an existing wallet, a passkey, or a Google account. Google-only sign-in is a first-class option: it requires neither a passkey nor a separately managed wallet. Google's identity issuance and account recovery are accepted dependencies of that option. Keychain cannot originate owner authorization, but is trusted to serve the latest owner-authorized policy.

The action's private encryption key is derived by Lit inside the trusted execution environment; it is not the user's wallet private key or a key derived from their Google account identifiers. No wallet transaction, funded user account, smart-account deployment, ChainSecured conversion, or paymaster is inherently required for local encryption and owner-authorized access.

Open source enables inspection but does not establish that the served frontend matches audited source. Use reproducible versioned builds, pinned dependencies/action bundles, a locally runnable client, and authenticated release distribution. A compromised authorized agent can disclose plaintext it was permitted to receive.

## Decisions proposed / still open

| Topic | Working proposal | Status |
|---|---|---|
| Wallet connection | React/TypeScript, RainbowKit, wagmi, viem | Recommended; pin mutually compatible versions |
| Passkeys | Native WebAuthn credential as an alternative owner signer | Optional sign-in method; verification must run in the action |
| Google | Google-only sign-in authorizes vault operations directly through Lit Actions | Accepted 2026-09-11; no passkey or wallet required |
| Agent credential | Client-generated signing key and signed scoped delegation | Recommended |
| Encryption boundary | Unique action per secret or deliberately shared security boundary | Recommended; prioritize isolation over minimizing action count |
| Immutable action inputs | Owner verifier, random vault/secret ID, protocol version, release behavior, trusted state sources | Recommended |
| Mutable permissions | Owner-authorized capabilities/policies verified by action | Recommended; support wallet, passkey and Google authorization proofs |
| Revocation freshness | Small DB registry of owner-authorized policies; actions verify proofs and query the selected policy | Accepted 2026-09-11; operator rollback/withholding is trusted, no on-chain registry or relayer |
| In-TEE-only promise | Separate action code with no generic reveal path | Exact recovery/migration/egress semantics need agreement |
| Existing deployment | Replace schema and APIs as needed | Authorized for implementation; no wipe during planning |

RainbowKit is the wallet-connection UI built on wagmi/viem; Scaffold-ETH uses this stack. It is not by itself a passkey identity system. Sources: [RainbowKit](https://rainbowkit.com/docs/introduction), [Scaffold-ETH](https://docs.scaffoldeth.io/).

## Keys and identities

1. **Owner identity:** EVM wallet, WebAuthn credential, or Google account; can approve imports, agent permissions, policy changes, and supported recovery operations. Wallet/passkey users sign directly. Google users present an action-verified ID token bound to the operation or a client-generated session signing key; no persistent user-managed key is required.
2. **Action encryption identity:** generated from the immutable CID by Lit. Only that action's execution gets its private key; another action can request its public key.
3. **Agent signer:** generated on the agent/client device. The database stores only public identity and owner-authorized permissions. The runtime agent never receives owner authority.
4. **Response encryption key:** ephemeral or separate agent encryption key. Its public key is included in the agent-signed request.
5. **Execution/billing credential:** pays for Lit calls. Its holder gains no decryption authority. Sponsorship limits are enforced independently.

Owner verification is a tagged descriptor, not an assumption that every owner is an Ethereum address:

- EOA: recover an EIP-712 signature over the domain-separated operation.
- Passkey: verify the P-256 WebAuthn assertion using the pinned credential public key, expected origin/RP ID, operation-digest challenge, user presence and required user verification. Do not treat a backend session or backend “verified” boolean as sufficient.
- Google: verify a Google ID token against the pinned issuer, OAuth client ID and owner subject inside the action, including its operation/session binding. A Keychain login session is not a substitute for this proof.
- Contract wallets: support ERC-1271 deliberately, with chain/address and trusted state reads; counterfactual wallets require a separate ERC-6492 decision. A wallet appearing in the picker does not establish signature compatibility.

A passkey signs; it need not expose private bytes or support encryption. Do not derive encryption keys from ordinary wallet signatures, JWT text, email, or a Google subject. Recovery may involve backup credentials selected by the owner. Synced passkeys inherit their sync/recovery provider's trust model. RP-ID/origin binding also means arbitrary self-hosted origins cannot automatically use an existing hosted-app passkey.

Use a common `OwnerAuthorization` proof interface throughout imports, policies, capabilities and recovery. Wallet/passkey proofs contain the corresponding signature/assertion. Google proofs contain a scoped ID-token/session proof, or a receipt signed by the pinned authorization action after it verified that proof. Every proof binds the exact authorized object; the API cannot replace it with its own signature.

Sources: [WebAuthn verification](https://www.w3.org/TR/webauthn-3/#sctn-verifying-assertion), [SimpleWebAuthn browser tools](https://simplewebauthn.dev/docs/packages/browser), [EIP-712](https://eips.ethereum.org/EIPS/eip-712), [ERC-1271](https://eips.ethereum.org/EIPS/eip-1271), [ERC-6492](https://eips.ethereum.org/EIPS/eip-6492).

## Why the operator cannot replace the decryptor

The browser builds a reproducible action bundle containing the owner verification anchor, random secret ID, release semantics, and authorization logic. It calculates the CID locally.

Lit derives the action key from this CID. Editing the owner, removing authorization checks, or adding a plaintext-export branch produces another CID and another key. Adding the altered action to an operator-controlled group does not give it the original private key. Anyone may pay to run the original action, but it will still demand valid owner/agent proofs.

This uses existing primitives:
- `lit-api-server/src/actions/client/handle_ops.rs`: private-key requests use the currently executing action's CID.
- `lit-api-server/src/actions/client/op_code_helpers/private_keys.rs`: private/public action-key derivation.
- `lit-api-server/src/dstack/v1/mod.rs`: CID-specific derivation namespace.
- `examples/action-bound-wallet/action/userWallet.js`: existing owner-bound action example.

The [documented action-identity pattern](https://developer.litprotocol.com/lit-actions/patterns#action-identity-signing-%E2%80%94-immutable-proofs) provides the foundation; using it for an encrypted credential envelope is the proposed extension.

Bundle all executable dependencies into the pinned artifact. Fetching mutable JavaScript at runtime would undermine the inference that the CID fixes authorization behavior. The Lit runtime itself remains part of the trusted platform.

## Local import and authenticated encryption

1. Establish the owner verifier and a random secret ID. Compile the action with its release behavior fixed before encryption.
2. Obtain the public encryption key for that exact CID from an independently trusted Lit path. Verify the CID locally; reject a public-key/CID pair supplied only on the authority of Keychain.
3. Generate a random data-encryption key in the browser. AEAD-encrypt the secret locally and wrap that key to the action's public encryption key.
4. Bind protocol version, network/derivation identity, action CID, owner/vault/secret ID, secret version, and release mode in the authenticated envelope.
5. Have the owner authorize the complete encrypted-envelope digest and metadata through their selected identity method. Public-key encryption alone does not authenticate who imported a ciphertext; the authorization proof prevents a malicious database from substituting content or relabeling it.
6. Upload only the encrypted envelope, owner authorization proof, and metadata. Permit a client-side encrypted export/backup of this material.

Cryptographic suite remains a bounded prototype decision. The existing public-action-key API returns secp256k1. Two candidates:
- A well-reviewed secp256k1 ECDH/KDF/AEAD envelope implementation.
- A separately domain-derived X25519 key inside the action with a standardized HPKE suite and an authenticated binding from CID to that public key.

Do not label a custom secp256k1 construction “standard HPKE.” [RFC 9180](https://www.rfc-editor.org/rfc/rfc9180) specifies the hybrid-encryption framework and suites. [noble-curves](https://github.com/paulmillr/noble-curves) provides relevant primitives, but primitive audits do not audit our envelope protocol. Specify byte encodings, key separation, AAD, lengths, and rejection behavior; verify browser/Node/Lit interoperability with independent vectors.

Current `Lit.Actions.Encrypt({pkpId,...})` seals server-supplied plaintext with a managed PKP and is not this local-public-key import flow.

Public-key bootstrap is a release gate. A generic enclave quote alone does not bind a response public key to a CID/session. The existing `/attestation` route requests a quote with no caller challenge/key binding. Decide explicitly between trusting a known authenticated Lit endpoint and adding verification that binds the CID, public key, fresh challenge, and approved runtime to the attested channel.

## Agent enrollment and retrieval

The agent generates a signing key locally, shows its public identity to the owner, and receives an owner-authorized capability. Approval works with a wallet, passkey, or Google-only sign-in. Keychain may store/deliver the capability; it cannot mint one.

Illustrative fields, not final serialization:

```text
Capability:
  protocolDomain, networkId, vaultId, actionCid
  agentPublicKey, allowedSecretIds, allowedOperations
  allowedVersions, policyEpoch, notBefore, expiresAt
  delegationAllowed = false

ReadRequest:
  capabilityHash, actionCid, secretId, version, envelopeHash
  operation, responsePublicKey, nonce, issuedAt, deadline
  agentSignature
```

The action retrieves the policy selected by the DB registry, verifies its owner authorization against the pinned identity/domain, and checks that the capability matches its policy epoch and permission scope. It also verifies all signed validity windows, agent proof of possession, and exact request/envelope binding. It rejects missing or invalid policy responses, but cannot detect the registry returning an older valid policy. It then decrypts inside the TEE and encrypts the result to the signed response public key. The SDK decrypts locally and can still expose a convenient `get(name)` API.

Captured signatures cannot be reused for another secret, operation, action, version, or response key. Replaying the identical request may still repeat execution unless there is trusted nonce-consumption state; a timestamp or signature alone is not single-use protection. Response encryption limits confidentiality impact, not replay costs or side effects.

The SDK can package identity seed plus public locator/configuration as one agent credential experience. Keep the execution credential conceptually separate: operator-minted usage keys must never be the sole proof that authorizes secret access.

### Interpreting “hardcode a hash of the secret”

If “secret” means a high-entropy **agent authentication token**, an immutable hash check can work, provided the token is generated and held by the agent/owner and never revealed to our backend. A bearer token must reach the action through a protected channel; anyone observing it can authenticate. Signatures avoid that transport exposure and give precise request/recipient binding.

Prefer pinning the **owner verifier** and checking owner-authorized agent permissions rather than embedding each agent token hash. Every embedded agent change changes the CID/key and forces re-encryption or rewrapping. A public key is already safe to include; hashing it is optional identification.

Do not publish a hash of the **protected credential value** as authentication. It neither proves an agent's authority nor protects low-entropy values from guessing. Do not embed the final ciphertext into the action that determines its own encryption key: that creates a circular dependency. Bind a random secret ID in code and the final ciphertext digest in the external owner-authorized envelope.

## Mutable policy and revocation

Signatures establish authorization and integrity, not which signed policy is newest. A malicious database can replay a previous signed allowlist. Keeping a revision integer in that same database does not solve rollback.

**Decision, 2026-09-11:** keep the small policy registry in our database and require owner authorization for every policy change. This supersedes the on-chain registry proposal. We explicitly trust the operator for freshness and revocation: the database may withhold an update or serve an older, still-valid authorized state. No registry contract, relayer, gas, ChainSecured account, or paymaster is needed.

Store immutable signed policy records plus a mutable pointer to the selected record for each vault or secret. A record binds the protocol/network domain, vault/action identity, revision/epoch, previous policy hash, permissions or their commitment, disable state, and explicit validity bounds. The owner signs the entire record through `OwnerAuthorization`; an unsigned database field cannot expand its permissions or change its expiry. Revision and previous-hash checks help honest concurrency and auditing, but do not provide cryptographic rollback protection.

“Sign with owner creds” means the wallet/passkey or Google-authorized client session approves the record. Private owner/session credentials never enter our database or backend. For Google, the pinned authorization action can issue a durable receipt for the exact policy after verifying the Google/session proof while valid. The decryptor verifies that receipt and the policy's validity rules; it does not rely on an expired JWT to establish a new authorization. The owner's verification anchor remains pinned in action code, not replaceable through an editable database public-key field. Any credential rotation must trace back to that anchor through owner-authorized rules, with the same accepted rollback limitation.

Revocation flow:

1. The owner approves a new policy removing permissions or disabling the vault/secret, bound to the expected revision and previous policy hash.
2. The API verifies the proof and atomically inserts the record and advances the selected-policy pointer with a compare-and-swap check. Backend verification improves API behavior; the action independently verifies the proof on every use.
3. Before authorizing a read, the Lit Action fetches the selected policy from the pinned registry endpoint. The caller cannot bypass this lookup by supplying an old policy alone. The action verifies the returned policy and requires the capability to match its epoch/commitment; a disabled policy denies access.
4. With an honest backend, subsequent lookups observe the committed revocation. Define caching and in-flight request behavior explicitly; initially avoid policy caching between executions. A malicious backend can return the earlier policy and defeat that revocation.

The accepted rollback scope is broader than one “revoked” flag: the operator can restore any previously owner-authorized, still-valid permission set, including old allowlists, retired versions, disabled agents/vaults, or credential/recovery settings. It cannot invent a new agent identity or exceed the scope actually authorized by the owner. An older policy that explicitly authorized future secret versions may cover those versions too; use exact version/envelope bindings where that breadth is unwanted. Each accessed envelope still needs valid owner authorization.

Replaying policy does not by itself reveal plaintext: a requester still needs a permitted agent private key and must satisfy the action's request/recipient checks. An operator holding or colluding with a formerly authorized agent can regain its old access under a replayed policy. Signed expiry remains enforceable using trusted runtime time; the backend cannot extend it with a new timestamp. Policies/capabilities with no finite expiry can remain replayable indefinitely. Choosing shorter signed lifetimes bounds replay but requires owner-authorized renewal, so specify those lifetimes as a product setting.

A policy registry does **not** enforce a strict per-read quota or one-time operation. Atomic DB counters/nonce consumption can enforce these with an honest backend, and inherit the same operator trust and rollback limitation. Do not claim enforcement against the operator. Likewise, deletion from Postgres alone is not cryptographic deletion, and revocation cannot retract a secret already revealed.

## Google sign-in

**Decision, 2026-09-11:** offer “Continue with Google” as a complete standalone sign-in and vault-authorization method alongside wallet and passkey options. Google account control satisfies the product's custody requirement. Do not require Google users to enroll a passkey, connect a wallet, or retain a separate long-lived signing key. Google's issuance/recovery authority is an accepted property of the selected login method, not an unresolved product blocker.

Google ID-token verification can execute inside a Lit Action. Pin the allowed issuer and OAuth client ID; verify signature/allowed algorithm against Google's rotating keys, audience, expiry, and the exact subject. Use `iss + sub`, not email, as identity. Bind an operation/session public key into the requested nonce and validate it, rather than accepting an ordinary login token for arbitrary secret operations. Never accept a caller-supplied JWKS URL or substitute a Keychain-issued session.

Proposed Google-only flow:

1. The browser generates a temporary session signing key. The Google sign-in nonce binds its public key, protocol domain, scope and requested expiry.
2. The action verifies the Google token and its binding, plus proof that the client holds the session private key. The stable owner identity is the pinned Google issuer/subject with the expected client ID; changing a session key does not change the vault identity or encryption key.
3. Within the authorized scope and lifetime, the session key signs exact import, delegation and policy requests. Keychain never receives the private key and cannot authorize another session by itself. Session proofs must not be renewable or extendable by the backend.
4. For durable envelopes or agent capabilities, the pinned action may issue a code-bound receipt for the exact object authorized while the Google proof was valid. Verify that action identity and enforce the object's explicit expiry/revocation rules later; an expired ID token cannot mint new receipts. Define receipt issuance and verification in the protocol so token expiry does not accidentally invalidate stored secrets or create perpetual management sessions.
5. On a new device or after session expiry, the user signs in with the same Google account and authorizes a fresh session. Recovery follows Google account recovery; no cross-device recovery of a Keychain-generated permanent private key is required.

Binding a fresh token directly to one operation is also valid where a session is unnecessary. Passkeys remain an independently selectable method, not a prerequisite for Google access. No encryption or signing private key is derived from an email, subject identifier, or token string.

Source: [Google ID-token validation](https://developers.google.com/identity/gsi/web/guides/verify-google-id-token), [Google nonce parameter](https://developers.google.com/identity/gsi/web/reference/js-reference).

## In-TEE-only behavior and upgrades

Use separate action templates for recipient-encrypted export and use-without-reveal. The latter contains the permitted operation and constrained egress. It cannot expose a generic decrypt/export/key-wrap method, accept arbitrary URLs/headers to receive the credential, follow redirects to unapproved destinations, or execute caller-supplied code. Request/response shapes and errors/logs need review to prevent indirect exfiltration. A service such as Stripe necessarily receives its API credential over TLS; describe the guarantee as “not revealed to the agent or Keychain,” not literally never leaving the enclave in any form.

Do not pass a raw data key or plaintext to an arbitrary customer action. The action that can derive the key must itself contain the approved use logic, or a future composition protocol must authenticate an equally constrained recipient.

Choose between:
- Strict no-export for this action: no generic owner recovery/export or migration to arbitrary code. Reimport the original credential when moving to a new action, or allow only specifically audited constrained successor paths.
- Owner-controlled export/migration: owner can authorize rewrapping to another action/recipient. This is compatible with denying access to the operator and runtime agents, but not with an unconditional never-export promise.

Replacing action code creates a new encryption identity. Old ciphertext and cached permissions do not disappear. Any migration procedure must respect the release semantics and owner/recovery authority, while retaining the accepted DB rollback limitation. Retiring the old version in the registry does not cryptographically prevent the operator from serving its older valid policy and ciphertext. There is no operator authority to change the immutable decryptor.

## Implementation slices

1. **Cryptographic vertical slice:** fixed EOA owner, one secret, browser encryption to action identity, signed agent capability, recipient-encrypted read. Attack it with alternate owner/code/key/CID/envelope/recipient and replayed requests. No production guarantee until public-key bootstrap is settled.
2. **Google and passkey identity slices:** implement Google-only onboarding, nonce-bound session authorization, action verification of Google proofs, durable authorization receipts, and return access from a fresh device without a passkey or wallet. Test wrong issuer/audience/subject, expired tokens, nonce/session substitution and receipt tampering. Implement the alternative passkey flow with actual browser registration/assertion, direct action verification, origin/challenge/UV negative tests, and second-device/recovery design.
3. **Registry and lifecycle:** implement signed policy records, a transactional selected-policy pointer in Postgres, and action-side lookup/proof verification. Test forged policies, owner substitution, cross-vault/action replay, signed expiry, mismatched capability epochs, concurrent updates, disabled entries and unavailable/invalid responses. Verify revocation with an honest backend and deliberately demonstrate that replaying a valid prior policy can restore its permissions; document this as accepted behavior. Implement credential replacement, recovery and version retirement with the same trust boundary; specify quota/replay semantics separately.
4. **Product replacement:** new React client, Rust storage/session/billing API, owner/agent identities, action manifests, signed policy/envelope tables, and new SDK. Remove grant signer, managed per-tenant vault/service keys, and bearer setup-token authority. Replace obsolete AGENTS.md invariants and public guarantees.
5. **Use-without-reveal:** one narrowly constrained integration first; prove wrong destinations, redirects, arbitrary response reflection and export/migration cannot expose the credential.
6. **Launch validation:** test against the actual Lit runtime, then repeat the earlier product review against the new trust model. Include budgets, data availability, backups/recovery, and CI.

Suggested schema concepts: `accounts`, `owner_credentials`, `vault_manifests`, `secret_envelopes`, `signed_policies`, `policy_registry`, `agent_public_keys`, `capabilities`, `execution_receipts`. The database selects the current policy, while authorization proofs are independently verified against the action's pinned owner anchor. Store only public owner credentials/proofs. Encrypt sensitive labels if metadata confidentiality is required.

## Platform checks required before asserting the guarantee

- Confirm action-key stability across intended network/runtime upgrades and document recovery assumptions.
- Verify the runtime chooses the private-key derivation CID, never a caller-supplied alternate CID.
- Establish trusted public-key/CID bootstrap and exact attestation/channel properties.
- Ensure no private action keys, data keys, plaintext, or bearer proofs reach telemetry. Current `lit-actions/ext/bindings.rs` annotates `op_get_lit_action_private_key` with `#[instrument(skip_all, ret)]`; investigate/remove sensitive return-value tracing as part of validating this design. Logging behavior has not been reproduced here.
- Bundle/pin executable dependencies and validate the same crypto serialization across browser, SDK and runtime.
- Verify registry record integrity independently while explicitly trusting the DB for freshness. Bound responses and validate signed expiry using trusted runtime time. Pin Google JWKS sources and, if contract wallets are supported, their chain/RPC trust anchors.
- Never expose generic root-key export/signing endpoints from a decryptor.
- Test compromise of the complete Keychain database and every operator-held app credential: these cannot fabricate owner proofs, change signed scope/expiry, substitute owner keys, or redirect agent-signed responses. Separately test a formerly authorized agent key plus rolled-back policy to demonstrate the accepted ability to restore old access.

## Final implementation decisions

- One immutable action per secret, with a separate pinned owner authorization action.
- EOA wallets through RainbowKit/wagmi, native P-256 passkeys and standalone Google
  sign-in. Contract-wallet verification is outside this initial release.
- Domain-derived X25519 encryption key, authenticated by the action's secp256k1 root;
  RFC9180 HPKE for wrapping/response encryption. Public-key bootstrap trusts a
  client-configured Lit HTTPS origin and does not claim quote-based attestation.
- Per-secret signed policies default to 30 days and permit at most 90. Owner credential
  membership normally does not expire. Google sessions last 10 minutes. Every read
  looks up current registry state without cross-execution policy caching.
- Separate export and strict Stripe-balance actions. Strict mode has no export/migration
  path; upgrading its action requires original-credential reimport.
- Backups preserve current ciphertext and signed credential/policy settings. Restoration
  is idempotent and cannot overwrite an existing vault's credential settings or policies.
- Local browser, Postgres, SDK and actual Deno runtime validation are automated. Live
  provider configuration and production key continuity remain deployment checks.

Earlier proposal language above records the design discussion; the implementation
README and SECURITY.md specify the final protocol and guarantees.
