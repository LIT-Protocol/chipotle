# @lit-protocol/keychain v2

The agent generates and holds an Ed25519 signing key. An owner authorizes its
public key for exact secret versions. The SDK checks signed metadata, submits a
recipient-bound signed request, and decrypts the action-signed HPKE response locally.
Execution goes directly to Chipotle with a scoped, per-vault usage key funded by
Keychain. That billing key does not authorize secret access by itself.

```sh
npm install @lit-protocol/keychain@2.0.6
npx @lit-protocol/keychain@2.0.6 init ./agent-identity.json
```

Give the **public key** to the owner. In Keychain, approve it on a secret and download
**Agent config**. Keep `agent-identity.json` private; it is created with mode 0600 and
existing files are never overwritten. The config contains public locators and a
**scoped billing key**. Keep both files private (mode 0600); do not commit them.
The config contains no owner or agent signing private key. An agent approved for
several secrets needs one file, not one per secret: the owner clicks **Config · all
secrets** next to the agent under **Authorized agents**. `get`, `use` and `run` take a
single config; `mcp` merges several from the same vault.

## Stored secret → get/run

In the owner UI, explicitly choose **stored secret / export**, name it `MY_SECRET`,
enter a disposable test value, approve your agent public key and download
`MY_SECRET.keychain.json`. Save this example as `read-secret.mjs`, beside the two
JSON files, and run `node read-secret.mjs` from that directory after installing.

```js
import { readFile } from "node:fs/promises";
import { Keychain } from "@lit-protocol/keychain";
const identity = JSON.parse(await readFile("./agent-identity.json", "utf8"));
const config = JSON.parse(await readFile("./MY_SECRET.keychain.json", "utf8"));
const keychain = new Keychain(identity.privateKey, config);
try {
  const secret = await keychain.get("MY_SECRET");
  // Consume secret in your trusted application here; never log it.
  if (typeof secret !== "string") throw new Error("Expected a stored secret");
} finally {
  keychain.destroy();
}
```

`get` returns plaintext to this process; `get_secret` returns it into model context.
For a trusted child tool prefer `run` below. The export **action** is not an Agent
config download (locators plus billing key) or encrypted backup export (ciphertext).

## Connected service → use/action MCP tool

Obtain a Stripe test credential, choose **Stripe balance / Use inside Lit** when
importing it as `STRIPE_API_KEY`, approve the agent, and download its config. See
[provider setup](https://keychain.litprotocol.com/PROVIDERS.md) for credential scopes,
all five providers and the required Supabase JSON object. Save as `balance.mjs`
and run `node balance.mjs` from the directory containing the private JSON files:

```js
import { readFile } from "node:fs/promises";
import { Keychain } from "@lit-protocol/keychain";
const identity = JSON.parse(await readFile("./agent-identity.json", "utf8"));
const config = JSON.parse(
  await readFile("./STRIPE_API_KEY.keychain.json", "utf8"),
);
const keychain = new Keychain(identity.privateKey, config);
try {
  const balances = await keychain.use("STRIPE_API_KEY");
  console.log(balances); // Bounded balance result, not the credential.
} finally {
  keychain.destroy();
}
```

This mode does not return the credential to the agent. The upstream provider
receives it over TLS. `get`/`run` are not service operations. MCP uses
`stripe_balance` with `{"name":"STRIPE_API_KEY"}`. These are setup recipes,
not evidence that your provider account or a live service call has succeeded.

`keychain.list()` reports each secret's action and input shape; `ACTIONS` exports the
full catalog compiled into the client. `npx @lit-protocol/keychain@2.0.6 actions` prints it from the CLI and
`npx @lit-protocol/keychain@2.0.6 use <identity> <config> <name> '<json-input>'` runs one.

CLI reads write the requested result to stdout. Avoid sending credential output to logs.
`npx @lit-protocol/keychain@2.0.6 --help` prints usage and `--version` prints the
installed version. Identity files with an explicit unsupported version or a public
key inconsistent with their private key are rejected; do not hand-edit key fields.
After `keychain.destroy()`, create a new client before calling `get`, `use` or
`attest` again: destroyed instances fail locally.

```sh
npx @lit-protocol/keychain@2.0.6 get ./agent-identity.json ./API_KEY.keychain.json API_KEY
```

For tools that need the raw credential in their environment, `run` skips stdout
entirely. It decrypts the export-release secrets in the config, places each in the
child's environment under the secret's name, hands the child your terminal, and exits
with the child's status. The Keychain CLI itself does not print the value; a child
program can still log or disclose it, including into model context:

```sh
npx @lit-protocol/keychain@2.0.6 run ./agent-identity.json ./STRIPE_API_KEY.keychain.json -- stripe balance retrieve
npx @lit-protocol/keychain@2.0.6 run ./id.json ./db.keychain.json --only DATABASE_URL -- psql
npx @lit-protocol/keychain@2.0.6 run ./id.json ./cfg.keychain.json --env OPENAI_PROD=OPENAI_API_KEY -- python agent.py
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
npx @lit-protocol/keychain@2.0.6 run ./id.json ./gcp.keychain.json --file GCP_SA=/tmp/sa.json -- \
  env GOOGLE_APPLICATION_CREDENTIALS=/tmp/sa.json gcloud storage ls
npx @lit-protocol/keychain@2.0.6 run ./id.json ./k8s.keychain.json --file KUBECONFIG_PROD=./kubeconfig -- \
  kubectl --kubeconfig ./kubeconfig get pods
```

If the CLI itself is killed with SIGKILL the file cannot be cleaned up; prefer a
tmpfs path such as `/dev/shm` on Linux for anything long-lived. Unlinking does not
guarantee erasure of bytes on disk; clean up files left after a crash yourself.

Set `CHIPOTLE_USAGE_API_KEY` for a CLI billing-key override, or pass
`{ usageApiKey }` as the SDK constructor's third argument. After the owner replaces
the execution key, update every agent using the old key.

## Endpoint attestation

Before execution requests to configured/pinned production Lit origins, the SDK proves the endpoint is a
genuine Intel TDX machine running the governed Lit Chipotle release. It fails
closed: any unmet check throws an `Attestation:` error and no execution request
is sent (verification itself fetches attestation evidence and governance state).

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
5. In Node (programmatic SDK, CLI and MCP), binds the live TLS certificate to the
   enclave through the dstack-ingress evidence quote. Import the SDK by package
   name (`@lit-protocol/keychain`) so Node selects the TLS-aware entry point;
   `dist/index.js` is the browser build and cannot observe the live TLS certificate.
   Automatic programmatic Node binding is new in 2.0.3; 2.0.2 only supplied this
   automatically in the CLI/MCP paths.

```sh
npx @lit-protocol/keychain@2.0.6 attest                 # prints the full report for the default origin
```

Independently compare `config.litApiUrl` with the endpoint supplied by your trusted
deployment operator (hosted default: `https://api.chipotle.litprotocol.com`). Do not
accept a substitute endpoint merely because a config or API response names it.
For a custom deployment, obtain and review its governance policy independently,
then pass `{ attestation: { appId, kmsContract, rpcUrl } }` to the SDK. The CLI/MCP
RPC override changes the RPC only; it does not establish a custom app policy.
Unknown-origin skipping is not equivalent to successful attestation.

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
claude mcp add lit-keychain -- npx -y @lit-protocol/keychain@2.0.6 mcp /absolute/path/agent-identity.json /absolute/path/API_KEY.keychain.json
# Codex CLI
codex mcp add lit-keychain -- npx -y @lit-protocol/keychain@2.0.6 mcp /absolute/path/agent-identity.json /absolute/path/API_KEY.keychain.json
```

Cursor, Windsurf and similar clients take the same command in their JSON config:

```json
{
  "mcpServers": {
    "lit-keychain": {
      "command": "npx",
      "args": [
        "-y",
        "@lit-protocol/keychain@2.0.6",
        "mcp",
        "/absolute/path/agent-identity.json",
        "/absolute/path/API_KEY.keychain.json",
        "/absolute/path/OTHER_SECRET.keychain.json"
      ]
    }
  }
}
```

Replace `/absolute/path` with paths on the machine running the MCP client. They
must exist and be readable by its user; client working directories vary. Node 22+
and npm/npx must be on that client's PATH. The examples pin 2.0.4 for reproducible
installation, not as a claim every production integration passed; review release
notes and retest before upgrading. A local npm install does not make a bare
`keychain` command globally available; all CLI examples use npx explicitly.

For JSON clients, use Cursor's project `.cursor/mcp.json` or user
`~/.cursor/mcp.json`, or Windsurf's `~/.codeium/windsurf/mcp_config.json` (merge the
`mcpServers` entry; do not replace other servers). The Claude Code/Codex commands
above use each client's config manager; inspect its registered server after adding.
Restart the client after edits. Absolute paths remove working-directory ambiguity.

Read-only smoke test: call `agent_public_key`, `list_actions`, then `list_secrets`.
Compare the public key with the owner's approval and inspect operations before
calling a provider tool. These metadata checks do not prove live Lit execution.
Only call `get_secret` if you intend to place plaintext into the model context.

- Missing file / wrong cwd: verify the absolute identity and config paths locally;
  do not upload their contents to support. Confirm the client's user can read them.
- Duplicate secret names across configs: do not rely on selection order. Run
  separate MCP registrations, each with an unambiguous config/name set.
- Billing-key replacement: refresh every config or override and restart all MCP
  processes. This is separate from secret rotation and agent signing-key rotation.
- A failed write can have succeeded upstream: Slack posts and Supabase inserts
  are not exactly-once. Inspect provider state before retrying; see provider recipes.

Tools: `list_secrets` (names, permitted operation and input shape, no values),
`get_secret`, one tool per catalog action (`stripe_balance`, `openai_chat`,
`github_read_file`, `slack_post_message`, `supabase_tables`; each takes `name` and,
where the action declares one, `input`; `list_actions` or `npx @lit-protocol/keychain@2.0.6 actions` shows
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

| Message                                                       | Cause                                                                                                                               |
| ------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `… the owner disabled this secret`                            | The owner switched the secret off.                                                                                                  |
| `… the owner's permission expired at <time>`                  | The policy ran past its expiry (30 days by default; the owner sets the lifetime, or none).                                          |
| `… agent <key> is not approved for this secret`               | The public key is not in the policy, or the wrong identity file.                                                                    |
| `… approved for version N … not the current version M`        | The version changed without updating this grant; reapprove, or use Rotate & approve (which moves existing agents automatically).    |
| `… Lit ran the <action> action … did not complete`            | Unclassified execution failure after local policy checks: input/credential mismatch, provider error, timeout or size/schema limits. |
| `… Lit refused to release "<name>" although the policy …`     | Policy changed between fetch and execution, or the enclave rejected the request.                                                    |
| `Secret "X" was created with the <action> action; call use()` | `get` on a connected service; use its action instead. `use` on export secrets is a different mismatch: use `get`/`run`.             |
| `Attestation: no Base RPC endpoint answered …`                | Public Base RPCs throttled or down; retry or set `KEYCHAIN_BASE_RPC_URL`.                                                           |

Other `Attestation:` messages can indicate a failed check, unavailable evidence,
or an unsupported verification environment. Do not disable attestation to get past it.

## Telling credentials apart

| Artifact       | Shape                                                                                                    |
| -------------- | -------------------------------------------------------------------------------------------------------- |
| Agent identity | JSON from `npx @lit-protocol/keychain@2.0.6 init`: `{ v: 2, privateKey: <64 hex>, publicKey: <64 hex> }` |
| Agent config   | JSON `*.keychain.json`: `{ v: 2, litApiUrl, usageApiKey, secrets }`                                      |
| Usage key      | Opaque Chipotle string (currently 44-character base64). Billing only.                                    |
| Secret value   | Whatever `get` returns. Never log it.                                                                    |

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
