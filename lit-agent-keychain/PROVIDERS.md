# Connected-service setup (catalog v1)

These recipes target the catalog pinned in this release to
`b892f5854ab8b81551d29da1ac058f7a1649e018`. They describe supported inputs, not
successful live provider tests. Start with a test project/account and non-sensitive
data. Provider credentials, account entitlements, resource membership and provider
billing must be configured separately. Never paste real credentials into a chat,
CLI argument, issue or source file.

## Common owner and agent steps

1. Obtain a dedicated provider credential through that provider's console. Limit
   its resources/permissions to the operation below where the provider supports it.
   Check the provider's current permission documentation; Keychain's action allowlist
   is not a substitute for provider-side credential scoping.
2. In Keychain, add a **connected service / Use inside Lit** secret, choose the
   named catalog action, and paste the credential into the owner's secret field.
   Except Supabase, the value is a bare token, not JSON and not `Bearer ...`.
   Keep a separate secure original: this mode cannot export even to the owner.
3. Generate an agent identity with `npx @lit-protocol/keychain@2.0.7 init ./agent-identity.json`.
   Approve only its public key for this secret and a short expiry. Download Agent
   config. Config is not a backup and contains a billing credential; protect it.
4. Use the SDK `use(name, input)` or the CLI below. MCP uses the action id as tool
   name with `{ "name": "SECRET_NAME", "input": ... }`; Stripe needs only `name`.
   Use `list_actions` and `list_secrets` first. Results, including provider text,
   enter model context in MCP; treat returned text as untrusted data.
5. To revoke, disable/remove the agent's approval in Keychain and revoke the key at
   the provider if compromise is suspected. Rotate/reapprove and distribute fresh
   configs as described in the [owner guide](README.md#rotations-and-revocation).

All actions restrict HTTPS destinations, input/output shapes and request budgets.
Inspect `npx @lit-protocol/keychain@2.0.7 actions` for the exact catalog schema and
limits your installed client knows. All results are bounded by 16 KiB. A denial
is not proof of an invalid provider key: check input, provider account status,
resource access, timeouts and size limits privately. Do not bypass attestation.

## Stripe — `stripe_balance`

Create a test-mode secret or restricted key in Stripe's developer dashboard. For a
restricted key, allow the balance-read operation only; confirm Stripe's current
permission controls for `GET /v1/balance`. Use the bare `sk_test_…` / `rk_test_…`
value, name it `STRIPE_API_KEY`, and select **Stripe balance**, not export. Live
keys act on the live account, so begin in test mode.

```sh
npx @lit-protocol/keychain@2.0.7 use ./agent-identity.json ./STRIPE_API_KEY.keychain.json STRIPE_API_KEY
```

SDK: `await keychain.use("STRIPE_API_KEY")`. MCP: `stripe_balance` with
`{"name":"STRIPE_API_KEY"}`. Returns `available`, `pending`, and `livemode`, not
the credential. Check the key's mode/account and balance-read permission on denial.

## OpenAI — `openai_chat`

Create a dedicated project API key in the OpenAI platform console; permit chat
completions and only the intended project/models where available. Ensure that
project has access to the chosen model and a funded budget. Store the bare `sk-…`
key as `OPENAI_API_KEY` with **OpenAI chat** selected.

```sh
npx @lit-protocol/keychain@2.0.7 use ./agent-identity.json ./OPENAI_API_KEY.keychain.json OPENAI_API_KEY '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"Say hello"}],"maxTokens":16}'
```

SDK: pass that JSON object as the second argument to `use("OPENAI_API_KEY", input)`.
MCP: `openai_chat` with that object under `input`. Returns `content`, `model`,
`finishReason`, and bounded `usage`. The action calls `/v1/chat/completions`, not
the Responses API. Check model availability, project billing and rate limits.
Prompts go to OpenAI and can incur provider charges, including on retries.

## GitHub — `github_read_file`

Create a fine-grained personal access token for only the repository you need, with
repository Contents read permission (and any organization approval required by
GitHub). Store the bare `github_pat_…` token as `GITHUB_TOKEN` with **GitHub file
read** selected. The action also accepts classic `ghp_…` tokens, but prefer narrower
resource access. Replace the sample repository/path with one that token may read.

```sh
npx @lit-protocol/keychain@2.0.7 use ./agent-identity.json ./GITHUB_TOKEN.keychain.json GITHUB_TOKEN '{"owner":"LIT-Protocol","repo":"agent-keychain-library","path":"README.md","ref":"main"}'
```

SDK: `use("GITHUB_TOKEN", input)`; MCP: `github_read_file`. Returns `path`, `sha`,
`size`, decoded `content`, and `truncated`. Check repository access/organization
approval, ref/path spelling and file size. Returned private file content is exposed
to the agent. The action does not pin a repository; the provider token must do that.

## Slack — `slack_post_message` (write)

Create/install a Slack app to a test workspace with a bot token and `chat:write`.
Invite the bot to the intended test channel; do not add broader scopes merely to
work around channel membership. Store the bare `xoxb-…` bot token as `SLACK_BOT_TOKEN`
with **Slack post message** selected. Replace the channel id before running:

```sh
npx @lit-protocol/keychain@2.0.7 use ./agent-identity.json ./SLACK_BOT_TOKEN.keychain.json SLACK_BOT_TOKEN '{"channel":"C0123ABC","text":"Keychain test message"}'
```

SDK: `use("SLACK_BOT_TOKEN", input)`; MCP: `slack_post_message`. Optional `threadTs`
replies to a thread. Returns `channel` and `ts`. Check app installation, token scope,
channel membership and Slack rate limits. This action is not restricted to a single
channel by Keychain; provider-side membership controls its reach.

**Do not blindly retry writes.** A timeout or denial after an upstream write does
not prove that no message was sent. Inspect channel history before retrying. The
input has no idempotency-key field; retries can post duplicates.

## Supabase — `supabase_tables`

In a dedicated Supabase test project, obtain the project ref and a server-side
secret key from project API settings. The current action accepts `sb_secret_…` or
legacy service_role JWT (`eyJ…`) keys, not anon/publishable keys, custom domains or
a database connection URL. **Secret/service_role keys bypass RLS. The encrypted
credential's table/column allowlist is the action's access policy, not an RLS user
session.** A filter allowlist permits agents to choose filters; it does not require
a tenant filter or enforce per-row isolation. Use a separate project or appropriately
restricted data for sensitive multi-tenant workloads.

Create a test `public.orders` table with `id` and `status` columns and rows you are
willing to expose. Store the entire JSON object below as `SUPABASE_TABLE_ACCESS`,
selecting **Query Supabase tables inside Lit**. Replace `ref` and `key` privately.
The values below are deliberately non-secret test placeholders; they cannot
authenticate to Supabase.

<!-- supabase-credential -->

```json
{
  "ref": "abcdefghijklmnopqrst",
  "key": "sb_secret_DOCUMENTATION_ONLY_NOT_A_REAL_KEY",
  "schema": "public",
  "tables": {
    "orders": {
      "select": ["id", "status"],
      "filter": ["id", "status"],
      "insert": [],
      "maxRows": 5
    }
  }
}
```

Credential contract (validated by the pinned action's actual parser in docs tests):

- Only `ref`, `key`, `schema`, `tables` are accepted at the top level. `ref`, `key`,
  and `tables` are required. Ref is exactly 20 lowercase letters; schema defaults
  to `public`. Identifiers match `[a-z_][a-z0-9_]{0,62}`.
- `tables` contains 1–64 named rules. Each rule accepts only `select`, `filter`,
  `insert`, `maxRows`. `select` is required and nonempty; each column list has at
  most 64 unique identifiers. `filter` defaults to `select`; `insert` defaults to
  empty (no writes). `maxRows` is an integer 1–1000, default 100.
- `sb_secret_` suffix is 10–200 letters, digits, underscores or hyphens. Legacy
  JWT shape is checked, not its provider authorization. The manifest also limits
  the JSON credential's size. Shape acceptance is not proof a key is valid.

Read input:

<!-- supabase-select -->

```json
{
  "table": "orders",
  "operation": "select",
  "columns": ["id", "status"],
  "filters": [{ "column": "status", "op": "eq", "value": "open" }],
  "limit": 5
}
```

```sh
npx @lit-protocol/keychain@2.0.7 use ./agent-identity.json ./SUPABASE_TABLE_ACCESS.keychain.json SUPABASE_TABLE_ACCESS '{"table":"orders","operation":"select","columns":["id","status"],"filters":[{"column":"status","op":"eq","value":"open"}],"limit":5}'
```

SDK: `use("SUPABASE_TABLE_ACCESS", input)`; MCP: `supabase_tables`. Returns
`{"rows":["{\"id\":1,\"status\":\"open\"}"],"count":1,"truncated":false}`
for a matching sample row (illustrative, not a live result). Each row is a **JSON
string**, not an object. `count` is rows returned by the database before output
truncation, not a total table count. Check `truncated` before treating results as
complete. Reads cap `limit` to the owner's `maxRows`.

To allow inserts, explicitly rotate the credential object with `insert:["status"]`
and reapprove the agent. With that policy, the following input writes a row:

```json
{
  "table": "orders",
  "operation": "insert",
  "rows": [{ "values": [{ "column": "status", "value": "\"open\"" }] }]
}
```

Each cell `value` is a JSON-encoded scalar string, number, boolean or null, not
nested JSON. Inserts cannot include read-only `columns`, `filters`, `order` or
`limit`. The input permits up to 100 rows, but keep requests within `maxRows` too:
the action checks the returned row count **after** the upstream write; denial can
mean rows were already inserted. **Do not blindly retry writes.** There is no
upsert or idempotency-key input. Use a provider-side unique constraint and an
explicit application deduplication design, and inspect the table after uncertain
completion. Changing an allowlist is a secret rotation, requiring reapproval.

On denial, privately check ref/key pairing, table/schema exposure in the Data API,
column spelling, input and credential rules, database constraints, timeouts and
response size. Do not broaden allowlists or disable RLS as a debugging shortcut.

## Provider references

- [Stripe keys](https://docs.stripe.com/keys)
- [GitHub repository content and token permissions](https://docs.github.com/en/rest/repos/contents#get-repository-content)
- [Slack chat.postMessage](https://docs.slack.dev/reference/methods/chat.postMessage/)
- [Supabase API keys](https://supabase.com/docs/guides/api/api-keys)

## Evidence and limits

Docs tests execute the Supabase fixture through the bundled action using synthetic
Lit keys and mocked provider HTTP. They also validate the input with the pinned
catalog schema. This is not live Supabase authentication, production attestation,
or proof any provider account is ready. Physical passkeys, Google sign-in and paid
billing require separate live acceptance checks.
