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
// Use it without logging it. For "use inside Lit" secrets the value never leaves
// the enclave; run the secret's catalog action instead:
// const balances = await keychain.use("STRIPE_API_KEY");
// const reply = await keychain.use("OPENAI_API_KEY", {
//   model: "gpt-4o-mini",
//   messages: [{ role: "user", content: "Summarize this ticket" }],
// });
keychain.destroy();
```

`keychain.list()` reports each secret's action and input shape; `ACTIONS` exports the
full catalog compiled into the client. `keychain actions` prints it from the CLI and
`keychain use <identity> <config> <name> '<json-input>'` runs one.

CLI reads write the requested result to stdout. Avoid sending credential output to logs:

```sh
keychain get ./agent-identity.json ./API_KEY.keychain.json API_KEY
```

For tools that need the raw credential in their environment, `run` skips stdout
entirely. It decrypts the export-release secrets in the config, places each in the
child's environment under the secret's name, hands the child your terminal, and exits
with the child's status. The value never appears in your shell history, agent
transcript, or logs:

```sh
keychain run ./agent-identity.json ./STRIPE_API_KEY.keychain.json -- stripe balance retrieve
keychain run ./id.json ./db.keychain.json --only DATABASE_URL -- psql
keychain run ./id.json ./cfg.keychain.json --env OPENAI_PROD=OPENAI_API_KEY -- python agent.py
```

`--only A,B` injects a subset; `--env SECRET=ENV_VAR` renames a variable for tools
that expect a specific name (secret names are always `[A-Z][A-Z0-9_]*`, so every
one is already a valid variable name). "Use inside Lit"
secrets have no value to inject and are skipped with a note on stderr; naming one
under `--only` is an error. The child inherits the parent environment. Any process
running as the same user can read another process's environment, so `run` is a
handoff to a tool you trust, not a sandbox.

Tools that read credentials from a path (service-account JSON, kubeconfig, SSH and
TLS keys, `.npmrc`) take `--file SECRET=PATH`. The file is created with mode 0600
before the command starts, is never overwritten if it already exists, and is removed
when the command exits. A `--file` secret stays out of the environment unless `--env`
names it too. Multi-line values such as PEM keys are written byte for byte.

```sh
keychain run ./id.json ./gcp.keychain.json --file GCP_SA=/tmp/sa.json -- \
  env GOOGLE_APPLICATION_CREDENTIALS=/tmp/sa.json gcloud storage ls
keychain run ./id.json ./k8s.keychain.json --file KUBECONFIG_PROD=./kubeconfig -- \
  kubectl --kubeconfig ./kubeconfig get pods
```

If the CLI itself is killed with SIGKILL the file cannot be cleaned up; prefer a
tmpfs path such as `/dev/shm` on Linux for anything long-lived.

Set `CHIPOTLE_USAGE_API_KEY` for a CLI billing-key override, or pass
`{ usageApiKey }` as the SDK constructor's third argument. After the owner replaces
the execution key, update every agent using the old key.

## Endpoint attestation

Before the first request to a known Lit origin, the SDK proves the endpoint is a
genuine Intel TDX machine running the governed Lit Chipotle release. It fails
closed: any unmet check throws an `Attestation:` error and nothing is sent.

1. Parses the TDX v4 quote from `GET /attestation` and verifies its ECDSA-P256
   chain: quote, attestation key, QE report, PCK certificate, Intel PCK CA, and a
   pinned Intel SGX Root CA public key.
2. Replays the dstack event log and requires RTMR0-3 to match the quote.
3. Requires the measured `app-id` to be the pinned DstackApp, the measured
   `compose-hash` to equal SHA-256 of the served `app_compose`, and every
   container image in it to be digest-pinned.
4. Confirms on Base that the compose hash is whitelisted in DstackApp and the OS
   image in DstackKms, both governed by the Lit Safe multisig. The lookup rotates
   through public Base RPC endpoints (`BASE_PUBLIC_RPC_URLS`), since each one
   throttles bursts from a single IP; a definitive "not whitelisted" answer is
   never retried elsewhere. Pin your own endpoint with `KEYCHAIN_BASE_RPC_URL`
   (CLI and MCP) or `{ attestation: { ...CHIPOTLE_ATTESTATION_POLICY, rpcUrl } }`
   (SDK) for guarantees stronger than a public RPC offers.
5. In Node (CLI and MCP), binds the live TLS certificate to the enclave through
   the dstack-ingress evidence quote, so the connection terminates inside the TEE.

```sh
keychain attest                 # prints the full report for the default origin
```

The result is cached per connection for one hour. `new Keychain(key, config,
{ attestation: false })` disables it; `{ attestation: policy }` pins a different
`{ appId, kmsContract, rpcUrl }`. Unknown origins such as local test adapters are
not attested. Set `KEYCHAIN_SKIP_ATTESTATION=1` for the CLI and MCP in development.

Not covered: Intel TCB status and PCK revocation collateral (the dstack-verifier
performs those), and quote freshness, because the public quote carries no caller
nonce; step 5 supplies freshness by binding the certificate you connected with.
Browsers run steps 1-4 only. See `protocol/attestation.ts` and
https://developer.litprotocol.com/architecture/verification/attestation.

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

Tools: `list_secrets` (names, permitted operation and input shape, no values),
`get_secret`, one tool per catalog action (`stripe_balance`, `openai_chat`,
`github_read_file`, `slack_post_message`, `supabase_tables`; each takes `name` and,
where the action declares one, `input`; `list_actions` or `keychain actions` shows
the current catalog), `list_actions`, and `agent_public_key` (for the owner to
approve). The server is
intentionally local rather than hosted: decryption needs the agent's private
identity, and a remote endpoint would hand that key and every plaintext to whoever
runs it, which the Keychain trust boundary forbids. Nothing but JSON-RPC is written
to stdout. Tool results enter the model context like any other tool output, so grant
agents only the secrets they need.

## When a request is refused

The action's only failure answer is a bare `access_denied`, by design: nothing
about the policy or the upstream service leaks through the enclave. Before it
spends an execution, the SDK checks the signed policy it already fetched and names
what it can see; every such message starts with `Access denied:` and ends with what
to do. `explainDenial(policy, agentPublicKey, operation, version, envelopeHash)`
is the exported check.

| Message                                                       | Cause                                                                            |
| ------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| `… the owner disabled this secret`                            | The owner switched the secret off.                                               |
| `… the owner's permission expired at <time>`                  | The policy ran past its expiry (30 days by default, 90 max).                     |
| `… agent <key> is not approved for this secret`               | The public key is not in the policy, or the wrong identity file.                 |
| `… approved for version N … not the current version M`        | The secret was rotated; the owner must re-approve the agent.                     |
| `… Lit ran the <action> action … did not complete`            | Policy permits it; the upstream call failed (usually a rejected credential).     |
| `… Lit refused to release "<name>" although the policy …`     | Policy changed between fetch and execution, or the enclave rejected the request. |
| `Secret "X" was created with the <action> action; call use()` | `get` on a connected service, or `use` on a stored secret.                       |
| `Attestation: no Base RPC endpoint answered …`                | Public Base RPCs throttled or down; retry or set `KEYCHAIN_BASE_RPC_URL`.        |

Any other `Attestation:` message means the endpoint failed a hardware or
governance check. Do not disable attestation to get past it.

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

Set `client.progress = (message) => …` to receive short status messages during
`login()`. The first sign-in for a vault provisions its Chipotle groups and
execution key on-chain, which takes roughly 30 seconds; later sign-ins take a
few seconds.

Owner/browser integrations can use `OwnerClient`, `LitConnection`, and
`authorizationTypedData`; see `web/src/identities.ts` for wallet, passkey and Google
signers. Owner approvals use short-lived proofs; stored ciphertext/policy receipts
outlive a login session. No owner or agent private key is stored by Keychain.

The operator is trusted for availability and the latest policy. It can replay old,
still-valid permissions, but cannot forge owner authorization. Requests can repeat
within their signed validity window; this SDK does not promise one-time execution.
A compromised permitted agent can disclose any credential it receives.

Build from this repository with `npm ci && npm run build` in `lit-agent-keychain/`.
