# @lit-protocol/keychain

The agent generates and holds an Ed25519 signing key. An owner authorizes its
public key for exact secret versions. The SDK checks signed metadata, submits a
recipient-bound signed request, and decrypts the action-signed HPKE response locally.
Execution goes directly to Chipotle with a scoped, per-vault usage key funded by
Keychain. That billing key does not authorize secret access by itself.

```sh
npm install @lit-protocol/keychain@2.2.0
npx @lit-protocol/keychain@2.2.0 init ./agent-identity.json
```

Give only the **public key** to the owner. On **Secrets → + Add agent**, approve
that key for selected secrets. The agent is ready on its next request: no config
file, refresh, download or restart. Keep the identity file private (mode 0600).
Reuse an existing identity rather than generating a new one when already approved.

`LiveKeychain` uses `https://keychain.litprotocol.com` by default. Set `serviceUrl`
(or CLI/MCP `KEYCHAIN_SERVICE_URL`) for another Keychain service. Its separately
trusted `litApiUrl` defaults to `https://api.chipotle.litprotocol.com`; CLI/MCP
`KEYCHAIN_LIT_API_URL` can explicitly choose another trusted Lit origin. Discovery
cannot replace that trust anchor. Do not copy a Lit URL from an untrusted response.

Every `await list()`, `get()` and `use()` authenticates afresh using a one-use,
60-second, service-bound signed challenge and retrieves current grants. A running
MCP server sees owner additions/removals on its next tool call. Discovery returns
no plaintext or private keys, only approved locators and execution-only billing
bootstrap. Lit still requires the agent signature and owner authorization.

`list()` returns `id` (`vaultId/secretId`), `name`, vault/secret IDs, release,
operation and input shape. Use `id` if names overlap across vaults; ambiguous bare
names fail, never select the first vault. `run` injects every export secret by
default; when two vaults share a name, give each a variable with
`--env VAULT/SECRET=ENV_VAR` or pick one with `--only VAULT/SECRET`.
A discovery request is capped at 1,000 candidate secrets; larger inventories fail
explicitly, never silently truncate. Revocation means the next request; plaintext
already received and already-authorized in-flight operations cannot be recalled.

## Stored secret → get/run

In the owner UI, explicitly choose **stored secret / export**, name it `MY_SECRET`,
enter a disposable test value and approve your agent public key. Save this example
as `read-secret.mjs` beside the private identity file and run `node read-secret.mjs`.

```js
import { readFile } from "node:fs/promises";
import { LiveKeychain } from "@lit-protocol/keychain";
const identity = JSON.parse(await readFile("./agent-identity.json", "utf8"));
const keychain = new LiveKeychain(identity.privateKey);
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
importing it as `STRIPE_API_KEY`, approve the agent. See
[provider setup](https://keychain.litprotocol.com/PROVIDERS.md) for credential scopes,
all five providers and the required Supabase JSON object. Save as `balance.mjs`
and run `node balance.mjs` from the directory containing the private JSON files:

```js
import { readFile } from "node:fs/promises";
import { LiveKeychain } from "@lit-protocol/keychain";
const identity = JSON.parse(await readFile("./agent-identity.json", "utf8"));
const keychain = new LiveKeychain(identity.privateKey);
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

`await keychain.list()` reports each secret's action and input shape; `ACTIONS` exports the
full catalog compiled into the client. `npx @lit-protocol/keychain@2.2.0 actions` prints it from the CLI and
`npx @lit-protocol/keychain@2.2.0 use <identity> <name> '<json-input>'` runs one.

CLI reads write the requested result to stdout. Avoid sending credential output to logs.
`npx @lit-protocol/keychain@2.2.0 --help` prints usage and `--version` prints the
installed version. Identity files with an explicit unsupported version or a public
key inconsistent with their private key are rejected; do not hand-edit key fields.
After `keychain.destroy()`, create a new client before calling `get`, `use` or
`attest` again: destroyed instances fail locally.

```sh
npx @lit-protocol/keychain@2.2.0 get ./agent-identity.json API_KEY
```

For tools that need the raw credential in their environment, `run` skips stdout
entirely. It discovers the currently approved export-release secrets, places each in the
child's environment under the secret's name, hands the child your terminal, and exits
with the child's status. The Keychain CLI itself does not print the value; a child
program can still log or disclose it, including into model context:

```sh
npx @lit-protocol/keychain@2.2.0 run ./agent-identity.json -- stripe balance retrieve
npx @lit-protocol/keychain@2.2.0 run ./id.json --only DATABASE_URL -- psql
npx @lit-protocol/keychain@2.2.0 run ./id.json --env OPENAI_PROD=OPENAI_API_KEY -- python agent.py
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
npx @lit-protocol/keychain@2.2.0 run ./id.json --file GCP_SA=/tmp/sa.json -- \
  env GOOGLE_APPLICATION_CREDENTIALS=/tmp/sa.json gcloud storage ls
npx @lit-protocol/keychain@2.2.0 run ./id.json --file KUBECONFIG_PROD=./kubeconfig -- \
  kubectl --kubeconfig ./kubeconfig get pods
```

If the CLI itself is killed with SIGKILL the file cannot be cleaned up; prefer a
tmpfs path such as `/dev/shm` on Linux for anything long-lived. Unlinking does not
guarantee erasure of bytes on disk; clean up files left after a crash yourself.

Live clients retrieve the current per-vault execution billing key on every operation,
including after rotation. `CHIPOTLE_USAGE_API_KEY` and the legacy `Keychain`
constructor's third-argument override apply only to static configs.

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
npx @lit-protocol/keychain@2.2.0 attest                 # prints the full report for the default origin
```

Independently compare your chosen `litApiUrl` with the endpoint supplied by your trusted
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
with any MCP client in one line; pass only the private identity path for live discovery:

```sh
# Claude Code
claude mcp add lit-keychain -- npx -y @lit-protocol/keychain@2.2.0 mcp /absolute/path/agent-identity.json
# Codex CLI
codex mcp add lit-keychain -- npx -y @lit-protocol/keychain@2.2.0 mcp /absolute/path/agent-identity.json
```

Cursor, Windsurf and similar clients take the same command in their JSON config:

```json
{
  "mcpServers": {
    "lit-keychain": {
      "command": "npx",
      "args": [
        "-y",
        "@lit-protocol/keychain@2.2.0",
        "mcp",
        "/absolute/path/agent-identity.json"
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
Restart after editing the MCP registration itself, not after owner approvals.
Absolute paths remove working-directory ambiguity.

Read-only smoke test: call `agent_public_key`, `list_actions`, then `list_secrets`.
Compare the public key with the owner's approval and inspect operations before
calling a provider tool. These metadata checks do not prove live Lit execution.
Only call `get_secret` if you intend to place plaintext into the model context.

- Missing file / wrong cwd: verify the absolute identity and config paths locally;
  do not upload their contents to support. Confirm the client's user can read them.
- Duplicate names across vaults: use the qualified `id` from `list_secrets`. Never
  rely on selection order. Legacy static configs reject conflicting names.
- Billing-key replacement: live clients rediscover the current key automatically.
  Legacy static clients need fresh configs or overrides and a restart.
- A failed write can have succeeded upstream: Slack posts and Supabase inserts
  are not exactly-once. Inspect provider state before retrying; see provider recipes.

Tools: `list_secrets` (names, permitted operation and input shape, no values),
`get_secret`, one tool per catalog action (`stripe_balance`, `openai_chat`,
`github_read_file`, `slack_post_message`, `supabase_tables`; each takes `name` and,
where the action declares one, `input`; `list_actions` or `npx @lit-protocol/keychain@2.2.0 actions` shows
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
| Agent identity | JSON from `npx @lit-protocol/keychain@2.2.0 init`: `{ v: 2, privateKey: <64 hex>, publicKey: <64 hex> }` |
| Agent config   | JSON `*.keychain.json`: `{ v: 2, litApiUrl, usageApiKey, secrets }`                                      |
| Usage key      | Opaque Chipotle string (currently 44-character base64). Billing only.                                    |
| Secret value   | Whatever `get` returns. Never log it.                                                                    |

`describeCredential(value)` classifies a value; `assertAgentIdentity`,
`assertAgentConfig` and `assertUsageApiKey` throw messages that name the mix-up.
The constructor and CLI apply them, so swapping the identity and config arguments
or pasting a private key as the usage key fails before any request is sent.

The config pins each action manifest/CID and an independently trusted Lit endpoint.
Never replace that endpoint using a URL supplied by the Keychain API. Clients
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

## Optional legacy / advanced static configs

`Keychain(privateKey, config, options)` and explicit CLI config-file arguments
remain supported for existing integrations. Its synchronous `list()` is a snapshot;
new approvals require a new export. Secret reads still check current policy. The
website's **Advanced: legacy static config → Download agent config** exports only
successful approvals; it is not required by live clients. Legacy MCP can merge
multiple configs from one vault, but different vault billing keys must not be merged.
Protect those files as billing credentials. Prefer `LiveKeychain` for all new agents.

Scoped execution keys fund execution, not authorization. A former agent retaining
one can consume the vault's sponsored execution budget until the owner rotates it,
but cannot decrypt/use a secret denied by its current honest policy. The service
never supplies master/account-management keys or keys from an unapproved vault.
