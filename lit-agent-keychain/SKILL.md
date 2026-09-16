---
name: lit-agent-keychain
description: Use Lit Agent Keychain v2 for owner-approved agent access to encrypted credentials.
version: 2.0.0
---

# Lit Agent Keychain v2

1. Generate an agent identity on the agent device with `keychain init identity.json`.
2. Give only its public key to the owner. The owner signs in with a wallet, passkey,
   or Google account, encrypts a secret locally, and explicitly approves that key.
3. Download the **Agent config** (public locators plus a scoped billing key) and use `@lit-protocol/keychain` with the local
   private identity. `get(name)` decrypts a recipient-encrypted result;
   `use(name, input)` runs the secret's catalog action (Stripe balance, OpenAI chat,
   GitHub file read, Slack message, Supabase table query, …) inside Lit without
   revealing its credential.
   `list()` tells you which applies to each secret and the input shape it takes.
4. For a tool that needs the raw value in its environment, prefer
   `keychain run identity.json CONFIG.keychain.json -- <command>` over `get`. It
   injects each export-release secret as an environment variable named after the
   secret and prints nothing, so the value never enters your context or logs.
   `--only A,B` selects secrets; `--env SECRET=ENV_VAR` renames one;
   `--file SECRET=PATH` writes one to a new mode-0600 file that is removed when
   the command exits, for tools that only read credentials from a path.
5. Or expose it to an MCP client in one line. The server runs locally, next to the
   identity file, and offers `list_secrets`, `get_secret`, one tool per catalog
   action (`stripe_balance`, `openai_chat`, `github_read_file`, `slack_post_message`,
   `supabase_tables`; `keychain actions` prints the current list), `list_actions` and
   `agent_public_key`:

   ```sh
   claude mcp add lit-keychain -- npx -y @lit-protocol/keychain mcp ./agent-identity.json ./API_KEY.keychain.json
   ```

Before its first request, the SDK attests the Lit endpoint: it verifies the Intel
TDX quote to a pinned Intel root, replays the event log into the RTMRs, checks the
measured app and compose hash against the on-chain whitelist on Base, and (in Node)
binds the live TLS certificate to the enclave. The on-chain check rotates through
public Base RPC endpoints, which rate limit bursts; set `KEYCHAIN_BASE_RPC_URL` to
pin your own. Never set `attestation: false` or `KEYCHAIN_SKIP_ATTESTATION=1`
outside local development.

## When a request is refused

The enclave answers every failure with the same bare denial so that nothing about
the policy or the upstream service leaks. The client therefore explains what it can
see, and the message tells you who can fix it:

| Message starts with                                           | Meaning                                                                                              | Who fixes it                                              |
| ------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- | --------------------------------------------------------- |
| `Access denied: the owner disabled this secret`               | The owner switched the secret off.                                                                   | Owner: Enable in Keychain.                                |
| `Access denied: the owner's permission expired`               | The signed policy ran past its expiry (30 days by default).                                          | Owner: Renew permissions.                                 |
| `Access denied: agent <key> is not approved`                  | Your public key is not in the policy, or you used the wrong identity file.                           | Owner approves your `publicKey`; check the identity file. |
| `Access denied: agent … approved for version N`               | The secret was rotated since you were approved.                                                      | Owner re-approves the agent (Rotate & approve does this). |
| `Access denied: Lit ran the <action> … did not complete`      | Your permission is fine; the upstream call failed inside the enclave, usually a rejected credential. | Owner checks or rotates the credential.                   |
| `Secret "X" was created with the <action> action; call use()` | You called `get`/`get_secret` on a connected service.                                                | Call `use` or the action's MCP tool instead.              |
| `Attestation: no Base RPC endpoint answered`                  | Public chain RPCs throttled or down.                                                                 | Retry, or set `KEYCHAIN_BASE_RPC_URL`.                    |
| `Attestation: …` (anything else)                              | The endpoint failed a hardware or governance check.                                                  | Stop. Do not disable attestation; report it.              |

## Know what you are holding

There are exactly three artifacts. Two are JSON files and self-describing; only the
secret value itself is a bare string. Never store one in the other's place.

| Artifact       | Shape                                                                       | Sensitivity                                                      |
| -------------- | --------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| Agent identity | JSON `{ "v": 2, "privateKey": <64 hex>, "publicKey": <64 hex> }`            | Private. Only `publicKey` is ever shared.                        |
| Agent config   | JSON `*.keychain.json`: `{ "v": 2, "litApiUrl", "usageApiKey", "secrets" }` | Private. `usageApiKey` is a Chipotle billing key, not authority. |
| Secret value   | Whatever `get` returns                                                      | Do not log, echo, or write to disk.                              |

`usageApiKey` is an opaque string minted by Chipotle (currently base64 of 32 random
bytes, 44 characters ending in `=`). It pays for execution and cannot read a secret
without the agent identity and an owner grant. There are no setup or management
bearer tokens in v2. If you are handed a bare string and are unsure what it is,
`describeCredential(value)` from the SDK classifies it; the SDK and CLI reject an
identity passed as a config, a config passed as an identity, and a private key
passed as a usage key, each with a message naming the mistake.

Never request an owner's private key or Google token, and never ask the backend to
mint a grant. There are no setup bearer tokens or managed per-tenant PKP vaults.
Agent identity and Lit execution billing are separate. Keep identity and config files private
and avoid logging credentials returned by `get` or the CLI. When you only need a
credential for one command, use `keychain run` so it is never printed at all.

The operator can replay older still-valid owner permissions, including undoing a
revocation. It cannot invent new owner permissions. The frontend/SDK, Lit runtime,
selected sign-in provider, and policy freshness service are trusted as documented
in SECURITY.md. "Use inside Lit" actions cannot export or arbitrarily migrate secrets;
keep the original credential for reimporting into a future action release.

See sdk/README.md for executable examples and README.md for deployment and recovery.
