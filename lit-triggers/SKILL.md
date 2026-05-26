---
name: lit-triggers-setup
description: "Use when an agent needs to help a user set up Lit Triggers at triggers.litprotocol.com: authorize the agent from a logged-in browser session, create webhook/schedule/chain-event triggers, inspect runs, and clean up."
version: 1.1.0
author: Lit Protocol
license: MIT
metadata:
  hermes:
    tags: [lit-protocol, chipotle, lit-triggers, agents, triggers]
    related_skills: []
---

# Lit Triggers Setup Skill

## Overview

Lit Triggers runs Lit Actions when something happens: an inbound webhook, a cron tick, or a matching EVM chain event. The service is available at:

```text
https://triggers.litprotocol.com
```

This skill is written for agents. Your job is to help the user set up real triggers, not just smoke-test the service. Start by authorizing yourself through the user's logged-in browser session, then use the API with your local bearer token.

Security model:

- The user's Lit/Chipotle admin API key should stay in the browser or with the user. Do not ask the backend to store it.
- `lit-triggers` stores only scoped Chipotle usage API keys, encrypted at rest.
- A scoped usage key must have permission to execute the target action/group. If you do not have one, ask the user to mint one in the dashboard or provide a pre-minted scoped usage key.
- `action_cid` is computed by `lit-triggers` from `action_code`; CIDs identify code content, not ownership.

## 1. Authorize This Agent

Generate a local random bearer token and save it somewhere only this agent/session can read:

```bash
mkdir -p ~/.lit-triggers
python3 - <<'PY'
import pathlib, secrets
path = pathlib.Path.home() / '.lit-triggers' / 'agent-token'
if not path.exists():
    path.write_text(secrets.token_urlsafe(48))
    path.chmod(0o600)
print(path.read_text().strip())
PY
```

Build an authorization URL. The URL contains only a hash challenge, not the raw bearer token:

```bash
python3 - <<'PY'
import base64, hashlib, pathlib, urllib.parse
raw = (pathlib.Path.home() / '.lit-triggers' / 'agent-token').read_text().strip()
challenge = base64.urlsafe_b64encode(hashlib.sha256(raw.encode()).digest()).rstrip(b'=').decode()
print('https://triggers.litprotocol.com/agent/authorize?' + urllib.parse.urlencode({'challenge': challenge}))
PY
```

Open that URL in the user's browser, or send it to the user and ask them to open it.

Expected browser flow:

1. If the user is not logged in, the site asks for email magic-link login.
2. After login, the site redirects back to the agent authorization page.
3. The user clicks **Authorize agent**.
4. The page says the agent is authorized.

After approval, verify API access:

```bash
python3 - <<'PY'
import pathlib, subprocess
raw = (pathlib.Path.home() / '.lit-triggers' / 'agent-token').read_text().strip()
auth = 'authorization: ' + 'Bearer ' + raw
subprocess.run([
    'curl', '-fsS',
    '-H', auth,
    'https://triggers.litprotocol.com/api/me',
], check=True)
PY
```

If verification returns `401`, ask the user to repeat the authorization flow with the generated URL. Do not generate a new token unless the user wants to replace the old one.

## 2. Decide What Trigger to Create

Ask the user for the trigger goal:

- **Webhook:** external service POSTs JSON or text to a generated URL.
- **Schedule:** cron expression fires automatically.
- **Chain event:** EVM logs matching a chain/contract/event signature fire the action.

You also need:

- Trigger name.
- Action code to run.
- Default params JSON, usually `{}`.
- A scoped Chipotle usage API key that can execute the action/group.
- Optional run limits (`max_runs_per_minute`, `max_queued_runs`).

If the user has only a Lit/Chipotle admin API key, prefer telling them to use the dashboard's browser-only mint flow, then give you the scoped usage key. Do not send an admin API key to the `lit-triggers` backend.

## 3. API Helper Pattern

Use this helper pattern in scripts so the bearer token stays local:

```bash
python3 - <<'PY'
import json, pathlib, subprocess
TOKEN = (pathlib.Path.home() / '.lit-triggers' / 'agent-token').read_text().strip()
AUTH = 'authorization: ' + 'Bearer ' + TOKEN
BASE = 'https://triggers.litprotocol.com'

def curl_json(method, path, body=None):
    cmd = ['curl', '-fsS', '-X', method, '-H', AUTH]
    if body is not None:
        cmd += ['-H', 'content-type: application/json', '-d', json.dumps(body)]
    cmd.append(BASE + path)
    out = subprocess.check_output(cmd, text=True)
    return json.loads(out) if out.strip() else None

print(json.dumps(curl_json('GET', '/api/me'), indent=2))
PY
```

For longer workflows, put the helper in a temporary script and delete it after use if it includes sensitive values.

## 4. Create a Webhook Trigger

Payload shape:

```json
{
  "name": "Example webhook",
  "kind": "webhook",
  "action_code": "...Lit Action JS...",
  "default_params": {},
  "usage_api_key": "scoped Chipotle usage key",
  "max_runs_per_minute": 10,
  "max_queued_runs": 20,
  "config": {}
}
```

Example agent script:

```bash
python3 - <<'PY'
import json, pathlib, subprocess
TOKEN = (pathlib.Path.home() / '.lit-triggers' / 'agent-token').read_text().strip()
AUTH = 'authorization: ' + 'Bearer ' + TOKEN
BASE = 'https://triggers.litprotocol.com'
USAGE_KEY = input('Paste scoped Chipotle usage key: ').strip()
ACTION = r'''
(async () => {
  Lit.Actions.setResponse({ response: JSON.stringify({ ok: true, params }) });
})();
'''.strip()
body = {
    'name': 'Agent-created webhook',
    'kind': 'webhook',
    'action_code': ACTION,
    'default_params': {'created_by': 'agent'},
    'usage_api_key': USAGE_KEY,
    'max_runs_per_minute': 10,
    'max_queued_runs': 20,
    'config': {},
}
out = subprocess.check_output([
    'curl', '-fsS', '-X', 'POST',
    '-H', AUTH,
    '-H', 'content-type: application/json',
    '-d', json.dumps(body),
    BASE + '/api/triggers',
], text=True)
trigger = json.loads(out)
print(json.dumps(trigger, indent=2))
print('Webhook URL:', BASE + '/webhook/' + trigger['id'])
PY
```

The webhook receives JSON bodies as `params.event`, and selected safe headers as `params.headers`. Fire it with:

```bash
curl -fsS -X POST 'https://triggers.litprotocol.com/webhook/<trigger-id>'   -H 'content-type: application/json'   -d '{"hello":"world"}'
```

## 5. Create a Schedule Trigger

Schedule config requires a cron expression:

```json
{ "cron": "* * * * *" }
```

Use 5-field cron or 6-field cron with seconds. Sub-30-second schedules are rejected because the scheduler scans every 30 seconds.

Example body differences from webhook:

```json
{
  "kind": "schedule",
  "config": { "cron": "*/5 * * * *" }
}
```

Schedule runs include input like:

```json
{
  "source": "schedule",
  "event": {
    "scheduled_at": "<RFC3339 timestamp>",
    "cron": "*/5 * * * *"
  }
}
```

## 6. Create a Chain Event Trigger

Supported chains:

- `ethereum`
- `base`
- `arbitrum`
- `bsc`
- `polygon`

The deployment must have the corresponding RPC variable configured. If no runs appear for a chain, ask the operator to verify the Railway variable (`BASE_RPC_URL`, `ETHEREUM_RPC_URL`, etc.).

Required chain-event config:

```json
{
  "chain": "base",
  "contract_address": "0x...",
  "event_signature": "Transfer(address,address,uint256)"
}
```

Optional fields:

- `start_block`: integer or hex string.
- `topic_filters`: up to three entries after topic0. Entries may be a 32-byte topic string, an array of topic strings, or `null` wildcard.

Base USDC `Transfer` example:

```json
{
  "kind": "chain_event",
  "config": {
    "chain": "base",
    "contract_address": "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
    "event_signature": "Transfer(address,address,uint256)"
  }
}
```

Chain-event runs include input like:

```json
{
  "source": "chain_event",
  "event": {
    "chain": "base",
    "chain_id": 8453,
    "contract_address": "0x...",
    "event_signature": "Transfer(address,address,uint256)",
    "log": { "transactionHash": "0x...", "logIndex": "0x..." }
  }
}
```

## 7. Inspect and Manage Triggers

List triggers:

```bash
python3 - <<'PY'
import pathlib, subprocess
TOKEN = (pathlib.Path.home() / '.lit-triggers' / 'agent-token').read_text().strip()
AUTH = 'authorization: ' + 'Bearer ' + TOKEN
subprocess.run(['curl', '-fsS', '-H', AUTH, 'https://triggers.litprotocol.com/api/triggers'], check=True)
PY
```

Get runs for a trigger:

```bash
python3 - <<'PY'
import pathlib, subprocess
trigger_id = input('Trigger id: ').strip()
TOKEN = (pathlib.Path.home() / '.lit-triggers' / 'agent-token').read_text().strip()
AUTH = 'authorization: ' + 'Bearer ' + TOKEN
subprocess.run(['curl', '-fsS', '-H', AUTH, f'https://triggers.litprotocol.com/api/triggers/{trigger_id}/runs?limit=20'], check=True)
PY
```

Disable a trigger:

```bash
python3 - <<'PY'
import json, pathlib, subprocess
trigger_id = input('Trigger id to disable: ').strip()
TOKEN = (pathlib.Path.home() / '.lit-triggers' / 'agent-token').read_text().strip()
AUTH = 'authorization: ' + 'Bearer ' + TOKEN
subprocess.run([
    'curl', '-fsS', '-X', 'PATCH',
    '-H', AUTH,
    '-H', 'content-type: application/json',
    '-d', json.dumps({'enabled': False}),
    f'https://triggers.litprotocol.com/api/triggers/{trigger_id}',
], check=True)
PY
```

Delete a trigger only when the user explicitly asks:

```bash
python3 - <<'PY'
import pathlib, subprocess
trigger_id = input('Trigger id to delete: ').strip()
TOKEN = (pathlib.Path.home() / '.lit-triggers' / 'agent-token').read_text().strip()
AUTH = 'authorization: ' + 'Bearer ' + TOKEN
subprocess.run(['curl', '-fsS', '-X', 'DELETE', '-H', AUTH, f'https://triggers.litprotocol.com/api/triggers/{trigger_id}'], check=True)
PY
```

## API Reference

Authenticated API calls accept either the browser session cookie or:

```http
Authorization: Bearer LOCAL_AGENT_TOKEN
```

Endpoints:

- `GET /api/me`
- `GET /api/triggers`
- `POST /api/triggers`
- `GET /api/triggers/<id>`
- `PATCH /api/triggers/<id>`
- `DELETE /api/triggers/<id>`
- `GET /api/triggers/<id>/runs?limit=20&offset=0`
- `POST /api/triggers/<id>/test` currently returns `501 Not Implemented`; do not rely on it.
- `POST /webhook/<id>` is public for webhook triggers and returns `202` with a run id when queued.

## Troubleshooting

- `401` from `/api/*`: the local agent token has not been authorized, was mistyped, or was revoked. Repeat the authorize URL flow.
- Browser lands on login instead of authorization: expected if the user is logged out. After magic-link login it should return to `/agent/authorize?...`.
- `400` from `/agent/authorize`: generated challenge was invalid. Regenerate the URL with the command in this skill.
- `400 {"error":"usage_api_key_required"}`: trigger creation needs a scoped usage key.
- Run reaches `failed` with Chipotle `401`/`403`: scoped usage key is invalid or not scoped for this action/group.
- No chain-event runs: verify RPC env var, start block/lookback, confirmation depth, and that matching logs exist.

## Verification Checklist

- [ ] User authorized the agent in browser.
- [ ] `GET /api/me` works with `Authorization: Bearer LOCAL_AGENT_TOKEN`.
- [ ] Trigger payload uses a scoped usage API key, not an admin key.
- [ ] Trigger was created and listed by the API.
- [ ] Webhook/schedule/chain-event behavior matches the user's requested trigger type.
- [ ] Run history was checked after setup.
- [ ] Any temporary/test triggers were disabled or deleted when finished.
