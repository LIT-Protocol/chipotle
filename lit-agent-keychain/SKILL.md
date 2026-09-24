---
name: lit-agent-keychain
description: Use Lit Agent Keychain for owner-approved agent access to encrypted credentials.
version: 2.1.1
---

# Lit Agent Keychain

1. Generate an agent identity on the agent device with `npx @lit-protocol/keychain@2.1.1 init identity.json`.
2. Give only its public key to the owner; reuse an existing identity if one was already
   generated. The owner signs in at https://keychain.litprotocol.com with a wallet,
   passkey, or Google account. On **Agents**, click **+ Add agent**.
   Enter a name and paste the 64-character public key, then select only the secrets
   this agent needs (or use **Select all** for enabled, unexpired secrets; **Clear selection** resets the choice) and click **Approve selected secrets**. Nothing is selected by
   default. An empty vault needs **Add secret** first; disabled/expired secrets need
   separate enabling/renewal. Never request the owner's private key or the agent's identity file.
3. The completion says **Ready to use**. Do not ask the owner for a config download.
   Use `new LiveKeychain(identity.privateKey)` from `@lit-protocol/keychain` or
   `npx @lit-protocol/keychain@2.1.1 list identity.json`. The existing private identity
   stays local. `get(name)` returns an encrypted-to-agent stored secret; `use(name,
input)` runs the connected service inside Lit. `await list()` shows current
   approvals, operation/input shape and a stable `vaultId/secretId` ID.
   All three operations fetch fresh owner approvals. A running MCP client needs no
   restart when the website adds/revokes secrets. Partial approvals remain usable;
   **Retry remaining approvals** resumes the rest. No export is necessary.
   `KEYCHAIN_SERVICE_URL` selects a different service (default keychain.litprotocol.com).
   The trusted Lit origin is configured independently (`KEYCHAIN_LIT_API_URL`, default
   api.chipotle.litprotocol.com), never taken from discovery. Duplicate names across
   vaults are ambiguous: pass the qualified ID instead, never guess which vault.
4. For a tool that needs the raw value in its environment, prefer
   `npx @lit-protocol/keychain@2.1.1 run identity.json -- <command>` over `get`. It
   injects each export-release secret as an environment variable named after the
   secret. The Keychain CLI itself does not print it; a child program can still
   log or disclose it, including into model context. This is not a sandbox.
   `--only A,B` selects secrets; `--env SECRET=ENV_VAR` renames one;
   `--file SECRET=PATH` writes one to a new mode-0600 file that is removed when
   the command exits, for tools that only read credentials from a path.
5. Or expose it to an MCP client in one line. The server runs locally, next to the
   identity file, and offers `list_secrets`, `get_secret`, one tool per catalog
   action (`stripe_balance`, `openai_chat`, `github_read_file`, `slack_post_message`,
   `supabase_tables`; `npx @lit-protocol/keychain@2.1.1 actions` prints the current list), `list_actions` and
   `agent_public_key`:

   ```sh
   claude mcp add lit-keychain -- npx -y @lit-protocol/keychain@2.1.1 mcp /absolute/path/agent-identity.json
   ```

Before execution requests to configured/pinned production origins, the SDK attests the Lit endpoint: it verifies the Intel
TDX quote to a pinned Intel root, replays the event log into the RTMRs, checks the
measured app and compose hash against the on-chain whitelist on Base, and (in Node)
binds the live TLS certificate to the enclave. The on-chain check rotates through
public Base RPC endpoints, which rate limit bursts; set `KEYCHAIN_BASE_RPC_URL` to
pin your own. Never set `attestation: false` or `KEYCHAIN_SKIP_ATTESTATION=1`
outside local development. Unknown origins are not automatically attested; verify
the chosen Lit origin independently and explicitly pin custom deployments (SDK README).

## When a request is refused

Live discovery omits revoked, disabled, expired or stale-version approvals. `Unknown
secret` means no current approval (or a wrong name); call `list()` and ask the owner
to approve/renew the existing public key, not download a file. Static `Keychain`
clients remain supported with **Advanced: legacy static config → Download agent config**.

Discovery is key-possession metadata/billing authentication, not owner authority.
The scoped billing key is reusable for billing only; a retained old key can spend
sponsored execution until rotated but does not bypass Lit policy checks.
Revocation applies on the next request; it cannot retract plaintext or cancel a
previously authorized operation already running. Discovery is capped at 1,000
candidate secrets and fails explicitly if exceeded.

The enclave answers every failure with the same bare denial so that nothing about
the policy or the upstream service leaks. The client therefore explains what it can
see, and the message tells you who can fix it:

| Message starts with                                           | Meaning                                                                                                                            | Who fixes it                                                                   |
| ------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `Access denied: the owner disabled this secret`               | The owner switched the secret off.                                                                                                 | Owner: Enable in Keychain.                                                     |
| `Access denied: the owner's permission expired`               | The signed policy ran past its expiry (30 days by default).                                                                        | Owner: Renew permissions.                                                      |
| `Access denied: agent <key> is not approved`                  | Your public key is not in the policy, or you used the wrong identity file.                                                         | Owner approves your `publicKey`; check the identity file.                      |
| `Access denied: agent … approved for version N`               | The secret was rotated since you were approved.                                                                                    | Owner re-approves the agent (Rotate & approve does this).                      |
| `Access denied: Lit ran the <action> … did not complete`      | Unclassified execution failure after local policy checks: input/credential mismatch, provider error, timeout or size/schema limit. | Owner checks input and provider status privately; do not blindly retry writes. |
| `Secret "X" was created with the <action> action; call use()` | You called `get`/`get_secret` on a connected service.                                                                              | Call `use` or the action's MCP tool instead.                                   |
| `Attestation: no Base RPC endpoint answered`                  | Public chain RPCs throttled or down.                                                                                               | Retry, or set `KEYCHAIN_BASE_RPC_URL`.                                         |
| `Attestation: …` (anything else)                              | A check failed, evidence is unavailable, or the environment cannot verify it.                                                      | Stop. Do not disable attestation; report it.                                   |

## Know what you are holding

Live clients need only an identity file. The optional legacy config is a snapshot
and is not used by live discovery. Never confuse these artifacts with secret values.

| Artifact       | Shape                                                                       | Sensitivity                                                      |
| -------------- | --------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| Agent identity | JSON `{ "v": 2, "privateKey": <64 hex>, "publicKey": <64 hex> }`            | Private. Only `publicKey` is ever shared.                        |
| Agent config   | JSON `*.keychain.json`: `{ "v": 2, "litApiUrl", "usageApiKey", "secrets" }` | Private. `usageApiKey` is a Chipotle billing key, not authority. |
| Secret value   | Whatever `get` returns                                                      | Do not log, echo, or write to disk.                              |

`usageApiKey` is an opaque string minted by Chipotle (currently base64 of 32 random
bytes, 44 characters ending in `=`). It pays for execution and cannot read a secret
without the agent identity and an owner grant. There are no setup or management
bearer tokens. If you are handed a bare string and are unsure what it is,
`describeCredential(value)` from the SDK classifies it; the SDK and CLI reject an
identity passed as a config, a config passed as an identity, and a private key
passed as a usage key, each with a message naming the mistake.

Never request an owner's private key or Google token, and never ask the backend to
mint a grant. There are no setup bearer tokens or managed per-tenant PKP vaults.
Agent identity and Lit execution billing are separate. Keep identity and config files private
and avoid logging credentials returned by `get` or the CLI. When you only need a
credential for one command, use `npx @lit-protocol/keychain@2.1.1 run` to avoid CLI printing; the child
can still disclose it. `get_secret` returns plaintext into model context. With
`--file`, SIGKILL may leave plaintext behind and unlink does not guarantee erasure.

The operator can replay older still-valid owner permissions, including undoing a
revocation. It cannot invent new owner permissions. The frontend/SDK, Lit runtime,
selected sign-in provider, and policy freshness service are trusted as documented
in SECURITY.md. "Use inside Lit" actions cannot export or arbitrarily migrate secrets;
keep the original credential for reimporting into a future action release.

See [sdk/README.md](sdk/README.md) for executable examples, [README.md](README.md)
for deployment and recovery, [PROVIDERS.md](PROVIDERS.md) for provider setup, and
https://developer.litprotocol.com/keychain for the human-facing documentation.
Select stored secret/export before using `get` or `run`; connected services use
`use` or their action MCP tool. Replace example absolute paths with local paths
and start with `agent_public_key`, `list_actions`, then `list_secrets`.
