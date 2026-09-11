---
name: lit-agent-keychain
description: Use Lit Agent Keychain v2 for owner-approved agent access to encrypted credentials.
version: 2.0.0
---

# Lit Agent Keychain v2

1. Generate an agent identity on the agent device with `keychain init identity.json`.
2. Give only its public key to the owner. The owner signs in with a wallet, passkey,
   or Google account, encrypts a secret locally, and explicitly approves that key.
3. Download the public **Agent config** and use `@lit-protocol/keychain` with the local
   private identity. `get(name)` decrypts a recipient-encrypted result; `stripeBalance`
   invokes the strict Stripe integration without revealing its credential.

Never request an owner's private key or Google token, and never ask the backend to
mint a grant. There are no setup bearer tokens or managed per-tenant PKP vaults.
Agent identity and Lit execution billing are separate. Keep identity files private
and avoid logging credentials returned by `get` or the CLI.

The operator can replay older still-valid owner permissions, including undoing a
revocation. It cannot invent new owner permissions. The frontend/SDK, Lit runtime,
selected sign-in provider, and policy freshness service are trusted as documented
in SECURITY.md. Strict Stripe actions cannot export or arbitrarily migrate secrets;
keep the original credential for reimporting into a future action release.

See sdk/README.md for executable examples and README.md for deployment and recovery.
