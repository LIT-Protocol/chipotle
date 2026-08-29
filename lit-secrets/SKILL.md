---
name: lit-secrets
description: "Use when an agent needs policy-gated access to secrets (API keys, tokens, credentials) stored in Lit Secrets at secrets.litprotocol.com: help the user store secrets and mint an agent key, then read secrets at runtime with a single credential."
version: 0.1.0
author: Lit Protocol
license: MIT
metadata:
  hermes:
    tags: [lit-protocol, chipotle, secrets, credentials, agents]
---

# Lit Secrets

Lit Secrets is a password manager for machines. Secrets are sealed inside the
Chipotle TEE; agents get **policy-gated** access with one credential. Two
release tiers per secret:

- `plaintext` (default) — an authorized agent can read the value. The value is
  decrypted inside the TEE by a pinned, auditable reader action and returned
  straight to the agent. The Lit Secrets control plane never sees it.
- `in_tee_only` — only Lit Actions the user has explicitly permitted can decrypt
  the value, and only inside the TEE. Nobody can read it out.

Base URL: `https://secrets.litprotocol.com`

## Two kinds of credential — don't mix them up

| Credential | Who holds it | What it can do |
|---|---|---|
| **Agent access token** (bearer) | A setup agent acting *as the user* | Everything the dashboard can: create/rotate secrets, mint agent keys, edit policies, read audit log |
| **Agent usage API key** | A runtime agent | `POST /api/grants`, `GET /api/reference/<name>`, and running the reader action on Chipotle. Nothing else. |

Runtime agents should only ever hold a usage API key.

## 1. Setup agent: authorize as the user

Same flow as lit-triggers. Generate a local bearer token, hash it, open the
authorize URL in the user's browser:

```bash
python3 - <<'PY'
import base64, hashlib, pathlib, secrets, urllib.parse
p = pathlib.Path.home() / '.lit-secrets' / 'agent-token'
p.parent.mkdir(exist_ok=True)
if not p.exists():
    p.write_text(secrets.token_urlsafe(48)); p.chmod(0o600)
raw = p.read_text().strip()
challenge = base64.urlsafe_b64encode(hashlib.sha256(raw.encode()).digest()).rstrip(b'=').decode()
print('https://secrets.litprotocol.com/agent/authorize?' + urllib.parse.urlencode({'challenge': challenge}))
PY
```

Then verify: `curl -H "Authorization: Bearer $TOKEN" https://secrets.litprotocol.com/api/me` → 200.

## 2. Store secrets (setup agent)

```bash
curl -X POST https://secrets.litprotocol.com/api/secrets \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"name":"OPENAI_API_KEY","value":"sk-...","kind":"api_key","environment":"production"}'
```

The first call provisions the user's vault (a PKP + group on Chipotle; takes a
few seconds). Fields: `name` `[A-Za-z0-9_.-]`, `value` ≤16 KB, optional `kind`,
`environment`, `release` (`plaintext` | `in_tee_only`), `policy`:

```json
{ "allowed_agents": ["<agent uuid>"], "max_reads_per_day": 100, "not_after": "2026-12-31T00:00:00Z" }
```

Other calls: `GET /api/secrets`, `GET /api/secrets/<name>` (versions),
`PUT /api/secrets/<name> {"value": ...}` (rotate → new version),
`PATCH /api/secrets/<name>` (policy/release/disabled), `DELETE /api/secrets/<name>`.

## 3. Mint a runtime agent key (setup agent)

```bash
curl -X POST https://secrets.litprotocol.com/api/agents \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"name":"payments-bot"}'
# -> { "id": "...", "usage_api_key": "…shown once…", "chipotle_api_base_url": "..." }
```

Give `usage_api_key` to the runtime agent (env var). Revoke with
`DELETE /api/agents/<id>` — this removes the key on Chipotle too, so it stops
working immediately everywhere.

## 4. Runtime agent: read a secret

Easiest — the SDK (no dependencies):

```js
import { LitSecrets } from 'https://secrets.litprotocol.com/sdk/lit-secrets.js';
const secrets = new LitSecrets({ usageApiKey: process.env.LIT_SECRETS_KEY });
const key = await secrets.get('OPENAI_API_KEY');
```

Manually, it's two requests:

```bash
# (1) grant — policy is evaluated here; 403 with {"error": "<reason>"} if denied
G=$(curl -s -X POST https://secrets.litprotocol.com/api/grants \
  -H "Authorization: Bearer $LIT_SECRETS_KEY" -H 'Content-Type: application/json' \
  -d '{"name":"OPENAI_API_KEY"}')
# (2) run the reader action on Chipotle with the same key; plaintext comes back to you
curl -s -X POST "$(echo "$G" | jq -r .chipotle_api_base_url)/core/v1/lit_action" \
  -H "Authorization: Bearer $LIT_SECRETS_KEY" -H 'Content-Type: application/json' \
  -d "$(echo "$G" | jq '{code: .action.code, js_params: .js_params}')" | jq -r '.response.value // .response'
```

Denial codes: `secret_disabled`, `release_not_plaintext`, `agent_not_allowed`,
`policy_expired`, `rate_limited`. Grants expire after ~2 minutes; request a
fresh one per read, don't cache them.

## 5. In-TEE-only secrets

For `release: "in_tee_only"`, the user attaches their own Lit Action CID via
`POST /api/actions {"cid": "...", "name": "..."}` (setup agent). The runtime
agent fetches `GET /api/reference/<name>` → `{ ciphertext, pkp_id }` and passes
them as `js_params` to that action, which calls
`Lit.Actions.Decrypt({ pkpId, ciphertext })` and uses the value in-TEE. Any
action in the tenant's group can decrypt any of the tenant's ciphertexts, so
only attach code you've audited.

## Notes

- Never log or echo `usage_api_key` or secret values.
- Chipotle executions are billed to the Lit Secrets operator account; expect
  ~0.5–2s per read.
- `GET /api/audit` (setup agent) lists every grant/reference decision.
