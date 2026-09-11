# @lit-protocol/keychain v2

The agent generates and holds an Ed25519 signing key. An owner authorizes its
public key for exact secret versions. The SDK checks signed metadata, submits a
recipient-bound signed request, and decrypts the action-signed HPKE response locally.
Execution is sponsored by Keychain; billing credentials never reach the agent.

```sh
npm install @lit-protocol/keychain
npx keychain init ./agent-identity.json
```

Give the **public key** to the owner. In Keychain, approve it on a secret and download
**Agent config**. Keep `agent-identity.json` private; it is created with mode 0600 and
existing files are never overwritten. The config contains public locators, no private key.

```js
import { readFile } from "node:fs/promises";
import { Keychain } from "@lit-protocol/keychain";
const identity = JSON.parse(await readFile("./agent-identity.json", "utf8"));
const config = JSON.parse(
  await readFile("./STRIPE_API_KEY.keychain.json", "utf8"),
);
const keychain = new Keychain(identity.privateKey, config);
const secret = await keychain.get("STRIPE_API_KEY");
// Use it without logging it. For strict Stripe-only secrets:
// const balances = await keychain.stripeBalance('STRIPE_API_KEY');
keychain.destroy();
```

CLI reads write the requested result to stdout. Avoid sending credential output to logs:

```sh
keychain get ./agent-identity.json ./API_KEY.keychain.json API_KEY
```

The config pins each action manifest/CID and an independently trusted Lit endpoint.
Never replace that endpoint using a URL supplied by the Keychain API. V2 clients
must use the action templates from the same immutable release as the vault.

Owner/browser integrations can use `OwnerClient`, `LitConnection`, and
`authorizationTypedData`; see `web/src/identities.ts` for wallet, passkey and Google
signers. Owner approvals use short-lived proofs; stored ciphertext/policy receipts
outlive a login session. No owner or agent private key is stored by Keychain.

The operator is trusted for availability and the latest policy. It can replay old,
still-valid permissions, but cannot forge owner authorization. Requests can repeat
within their signed validity window; this SDK does not promise one-time execution.
A compromised permitted agent can disclose any credential it receives.

Build from this repository with `npm ci && npm run build` in `lit-agent-keychain/`.
