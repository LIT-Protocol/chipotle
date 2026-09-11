# @lit-protocol/keychain v2

The agent generates and holds an Ed25519 signing key. An owner authorizes its
public key for exact secret versions. The SDK checks signed metadata, submits a
recipient-bound signed request, and decrypts the action-signed HPKE response locally.
Execution goes directly to Chipotle with a scoped, per-vault usage key funded by
Keychain. That billing key does not authorize secret access by itself.

```sh
npm install @lit-protocol/keychain
npx keychain init ./agent-identity.json
```

Give the **public key** to the owner. In Keychain, approve it on a secret and download
**Agent config**. Keep `agent-identity.json` private; it is created with mode 0600 and
existing files are never overwritten. The config contains public locators and a
**scoped billing key**. Keep both files private (mode 0600); do not commit them.
The config contains no owner or agent signing private key.

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

Set `CHIPOTLE_USAGE_API_KEY` for a CLI billing-key override, or pass
`{ usageApiKey }` as the SDK constructor's third argument. After the owner replaces
the execution key, update every agent using the old key.

## MCP server

The package ships a local Model Context Protocol server over stdio. Register it
with any MCP client in one line; pass one or more agent configs after the identity:

```sh
# Claude Code
claude mcp add lit-keychain -- npx -y @lit-protocol/keychain mcp ./agent-identity.json ./API_KEY.keychain.json
# Codex CLI
codex mcp add lit-keychain -- npx -y @lit-protocol/keychain mcp ./agent-identity.json ./API_KEY.keychain.json
```

Cursor, Windsurf and similar clients take the same command in their JSON config:

```json
{
  "mcpServers": {
    "lit-keychain": {
      "command": "npx",
      "args": [
        "-y",
        "@lit-protocol/keychain",
        "mcp",
        "./agent-identity.json",
        "./API_KEY.keychain.json"
      ]
    }
  }
}
```

Tools: `list_secrets` (names and permitted operation, no values), `get_secret`,
`stripe_balance`, and `agent_public_key` (for the owner to approve). The server is
intentionally local rather than hosted: decryption needs the agent's private
identity, and a remote endpoint would hand that key and every plaintext to whoever
runs it, which the Keychain trust boundary forbids. Nothing but JSON-RPC is written
to stdout. Tool results enter the model context like any other tool output, so grant
agents only the secrets they need.

## Telling credentials apart

| Artifact       | Shape                                                                            |
| -------------- | -------------------------------------------------------------------------------- |
| Agent identity | JSON from `keychain init`: `{ v: 2, privateKey: <64 hex>, publicKey: <64 hex> }` |
| Agent config   | JSON `*.keychain.json`: `{ v: 2, litApiUrl, usageApiKey, secrets }`              |
| Usage key      | Opaque Chipotle string (currently 44-character base64). Billing only.            |
| Secret value   | Whatever `get` returns. Never log it.                                            |

`describeCredential(value)` classifies a value; `assertAgentIdentity`,
`assertAgentConfig` and `assertUsageApiKey` throw messages that name the mix-up.
The constructor and CLI apply them, so swapping the identity and config arguments
or pasting a private key as the usage key fails before any request is sent.

The config pins each action manifest/CID and an independently trusted Lit endpoint.
Never replace that endpoint using a URL supplied by the Keychain API. V2 clients
must use the action templates from the same immutable release as the vault.

Owner management requests allow two minutes for initial Chipotle group/key
provisioning and billing operations; direct action requests retain their separate
execution timeout. `OwnerClient` accepts an optional fourth `managementTimeoutMs`
constructor argument.

Owner/browser integrations can use `OwnerClient`, `LitConnection`, and
`authorizationTypedData`; see `web/src/identities.ts` for wallet, passkey and Google
signers. Owner approvals use short-lived proofs; stored ciphertext/policy receipts
outlive a login session. No owner or agent private key is stored by Keychain.

The operator is trusted for availability and the latest policy. It can replay old,
still-valid permissions, but cannot forge owner authorization. Requests can repeat
within their signed validity window; this SDK does not promise one-time execution.
A compromised permitted agent can disclose any credential it receives.

Build from this repository with `npm ci && npm run build` in `lit-agent-keychain/`.
