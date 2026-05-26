---
name: lit-triggers-testing
description: "Use when an agent needs to smoke test or operate the deployed lit-triggers service API: authenticate, create webhook/schedule/chain-event triggers, fire test inputs, inspect runs, and clean up."
version: 1.0.0
author: Lit Protocol
license: MIT
metadata:
  hermes:
    tags: [lit-protocol, chipotle, lit-triggers, api-testing, railway]
    related_skills: []
---

# lit-triggers Testing Skill

## Overview

`lit-triggers` is an outside-the-TEE service that runs Lit Actions in response to webhooks, cron schedules, or EVM chain events. It stores only scoped Chipotle usage API keys, encrypted at rest. It does **not** store a Chipotle admin API key, and the backend does not mint usage keys.

Use this file as agent-consumable instructions for testing a deployed `lit-triggers` instance. It is intentionally operational: define the environment variables, log in with a magic-link session cookie, create triggers through the JSON API, fire inputs, inspect run history, and delete test triggers.

## When to Use

Use this when:

- A new agent needs to test the deployed Railway service.
- You need API examples for webhook, schedule, or chain-event trigger creation.
- You need to verify the public `/health`, auth, trigger CRUD, dispatcher, scheduler, or chain listener behavior.
- You want to hand an agent a single file and ask it to test the service without reading the whole codebase.

Do not use this for:

- Minting a scoped usage API key from a Chipotle admin key on the backend. That is intentionally browser-only or done externally.
- Testing `/api/triggers/<id>/test` as a real execution path. It currently returns `501 Not Implemented`.
- Assuming bearer-token auth for the lit-triggers API. API auth is a Rocket private session cookie from magic-link login.

## Required Test Inputs

Set these locally before using the examples:

```bash
export LT_BASE_URL='https://<deployed-lit-triggers-domain>'
export LT_EMAIL='<your-test-email>'
export SCOPED_USAGE_API_KEY='<chipotle-usage-key-scoped-to-execute-in-the-target-group>'
export COOKIE_JAR="$(mktemp)"
```

Optional:

```bash
export CHIPOTLE_ACCOUNT_ADDRESS='<display-only-account-address>'
export TEST_GROUP_ID='<chipotle-group-id-used-when-the-usage-key-was-minted>'
```

Important constraints:

- `SCOPED_USAGE_API_KEY` must be able to execute the action code/CID in the target Chipotle group. If it is invalid or under-scoped, trigger creation can still succeed, but runs will fail when the dispatcher calls Chipotle.
- `action_cid` is derived server-side from `action_code`. CIDs identify action code content, not ownership.
- The service never returns stored usage API keys in API responses.

## Smoke Test the Deployment

```bash
curl -fsS "$LT_BASE_URL/health"
```

Expected response:

```text
ok
```

## Authenticate

Request a magic link:

```bash
curl -fsS -X POST "$LT_BASE_URL/auth/request" \
  -H 'content-type: application/x-www-form-urlencoded' \
  --data-urlencode "email=$LT_EMAIL"
```

Expected JSON:

```json
{"ok":true}
```

Open the magic link from the email in a browser, or paste the full magic-link URL into curl to create a cookie session:

```bash
export MAGIC_LINK='<full https://.../auth/verify?token=... link from email>'
curl -fsSL -c "$COOKIE_JAR" -b "$COOKIE_JAR" "$MAGIC_LINK" >/dev/null
```

Verify the session:

```bash
curl -fsS -b "$COOKIE_JAR" "$LT_BASE_URL/api/me" | jq .
```

If this returns `401`, the cookie was not captured. Re-run the verify URL with `-L -c "$COOKIE_JAR" -b "$COOKIE_JAR"`, or log in in a browser and use the UI instead.

## Common API Shapes

Create trigger:

```http
POST /api/triggers
content-type: application/json
cookie: private session cookie
```

Required JSON fields:

- `name` string
- `kind` one of `webhook`, `schedule`, `chain_event`
- `action_code` string
- `default_params` object, usually `{}`
- `usage_api_key` string, only accepted on create/update and never returned
- `config` object, shape depends on `kind`

Optional JSON fields:

- `chipotle_account_address` string or null, display/debug only
- `max_runs_per_minute` integer
- `max_queued_runs` integer

Read/list/update/delete:

```bash
curl -fsS -b "$COOKIE_JAR" "$LT_BASE_URL/api/triggers" | jq .
curl -fsS -b "$COOKIE_JAR" "$LT_BASE_URL/api/triggers/$TRIGGER_ID" | jq .
curl -fsS -X PATCH -b "$COOKIE_JAR" "$LT_BASE_URL/api/triggers/$TRIGGER_ID" \
  -H 'content-type: application/json' \
  -d '{"enabled":false}' | jq .
curl -fsS -X DELETE -b "$COOKIE_JAR" "$LT_BASE_URL/api/triggers/$TRIGGER_ID" -i
```

List runs:

```bash
curl -fsS -b "$COOKIE_JAR" "$LT_BASE_URL/api/triggers/$TRIGGER_ID/runs?limit=20&offset=0" | jq .
```

Run statuses are typically `queued`, `running`, `success`, `failed`, or `retrying`.

## Test Action Code

Use a harmless Lit Action that echoes `js_params`/`params` back through `Lit.Actions.setResponse`. Adjust if Chipotle's Lit Action runtime changes its globals.

```bash
read -r -d '' TEST_ACTION_CODE <<'EOF'
(async () => {
  const input = typeof params !== 'undefined' ? params : {};
  Lit.Actions.setResponse({
    response: JSON.stringify({ ok: true, input })
  });
})();
EOF
```

If this action fails in Chipotle, use a known-good action from the target Chipotle group and keep the rest of the trigger payloads the same.

## Webhook Trigger Test

Create a webhook trigger:

```bash
WEBHOOK_CREATE_PAYLOAD=$(jq -n \
  --arg name "agent webhook smoke $(date -u +%Y%m%dT%H%M%SZ)" \
  --arg action_code "$TEST_ACTION_CODE" \
  --arg usage_api_key "$SCOPED_USAGE_API_KEY" \
  --arg account "${CHIPOTLE_ACCOUNT_ADDRESS:-}" \
  '{
    name: $name,
    kind: "webhook",
    action_code: $action_code,
    default_params: { agent_smoke: true, trigger_type: "webhook" },
    usage_api_key: $usage_api_key,
    chipotle_account_address: (if $account == "" then null else $account end),
    max_runs_per_minute: 10,
    max_queued_runs: 20,
    config: {}
  }')

WEBHOOK_TRIGGER=$(curl -fsS -b "$COOKIE_JAR" -X POST "$LT_BASE_URL/api/triggers" \
  -H 'content-type: application/json' \
  -d "$WEBHOOK_CREATE_PAYLOAD")

echo "$WEBHOOK_TRIGGER" | jq .
export WEBHOOK_TRIGGER_ID=$(echo "$WEBHOOK_TRIGGER" | jq -r '.id')
```

Fire it:

```bash
curl -fsS -X POST "$LT_BASE_URL/webhook/$WEBHOOK_TRIGGER_ID" \
  -H 'content-type: application/json' \
  -d '{"hello":"from-agent","trigger":"webhook"}' | jq .
```

Expected webhook response:

```json
{"run_id":"<uuid>","status":"queued"}
```

Inspect runs until terminal:

```bash
for i in $(seq 1 20); do
  curl -fsS -b "$COOKIE_JAR" "$LT_BASE_URL/api/triggers/$WEBHOOK_TRIGGER_ID/runs?limit=5" | jq .
  sleep 3
done
```

Expected result if the scoped usage key and action are valid: latest run eventually reaches `success`. If it reaches `failed`, inspect `.runs[0].error` and `.runs[0].response`.

## Schedule Trigger Test

Create a schedule trigger. Use a cron expression no more frequent than once per minute. Sub-30-second schedules are intentionally rejected.

```bash
SCHEDULE_CREATE_PAYLOAD=$(jq -n \
  --arg name "agent schedule smoke $(date -u +%Y%m%dT%H%M%SZ)" \
  --arg action_code "$TEST_ACTION_CODE" \
  --arg usage_api_key "$SCOPED_USAGE_API_KEY" \
  '{
    name: $name,
    kind: "schedule",
    action_code: $action_code,
    default_params: { agent_smoke: true, trigger_type: "schedule" },
    usage_api_key: $usage_api_key,
    max_runs_per_minute: 5,
    max_queued_runs: 10,
    config: { cron: "* * * * *" }
  }')

SCHEDULE_TRIGGER=$(curl -fsS -b "$COOKIE_JAR" -X POST "$LT_BASE_URL/api/triggers" \
  -H 'content-type: application/json' \
  -d "$SCHEDULE_CREATE_PAYLOAD")

echo "$SCHEDULE_TRIGGER" | jq .
export SCHEDULE_TRIGGER_ID=$(echo "$SCHEDULE_TRIGGER" | jq -r '.id')
```

Wait up to two minutes and inspect runs:

```bash
for i in $(seq 1 40); do
  curl -fsS -b "$COOKIE_JAR" "$LT_BASE_URL/api/triggers/$SCHEDULE_TRIGGER_ID/runs?limit=5" | jq .
  sleep 3
done
```

Expected input shape for schedule runs includes:

```json
{
  "source": "schedule",
  "event": {
    "scheduled_at": "<RFC3339 timestamp>",
    "cron": "* * * * *"
  }
}
```

## Chain Event Trigger Test

Prerequisites:

- Railway env var for the target chain RPC is set, e.g. `BASE_RPC_URL` for `base`.
- The target contract emits events often enough to observe.
- The scoped usage key can execute the action.

Supported chain keys and RPC env vars:

- `ethereum` / `ETHEREUM_RPC_URL`
- `base` / `BASE_RPC_URL`
- `arbitrum` / `ARBITRUM_RPC_URL`
- `bsc` / `BSC_RPC_URL`
- `polygon` / `POLYGON_RPC_URL`

Create a Base USDC `Transfer(address,address,uint256)` trigger. Use a recent `start_block` if you know one; otherwise omit it and the service starts from its configured initial lookback window.

```bash
CHAIN_CREATE_PAYLOAD=$(jq -n \
  --arg name "agent chain smoke $(date -u +%Y%m%dT%H%M%SZ)" \
  --arg action_code "$TEST_ACTION_CODE" \
  --arg usage_api_key "$SCOPED_USAGE_API_KEY" \
  '{
    name: $name,
    kind: "chain_event",
    action_code: $action_code,
    default_params: { agent_smoke: true, trigger_type: "chain_event" },
    usage_api_key: $usage_api_key,
    max_runs_per_minute: 20,
    max_queued_runs: 20,
    config: {
      chain: "base",
      contract_address: "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
      event_signature: "Transfer(address,address,uint256)"
    }
  }')

CHAIN_TRIGGER=$(curl -fsS -b "$COOKIE_JAR" -X POST "$LT_BASE_URL/api/triggers" \
  -H 'content-type: application/json' \
  -d "$CHAIN_CREATE_PAYLOAD")

echo "$CHAIN_TRIGGER" | jq .
export CHAIN_TRIGGER_ID=$(echo "$CHAIN_TRIGGER" | jq -r '.id')
```

Poll for runs:

```bash
for i in $(seq 1 60); do
  curl -fsS -b "$COOKIE_JAR" "$LT_BASE_URL/api/triggers/$CHAIN_TRIGGER_ID/runs?limit=5" | jq .
  sleep 5
done
```

Expected input shape for chain-event runs includes:

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

Topic filters can include up to three entries after topic0. Each entry may be a 32-byte topic string, an array of topic strings, or `null` for wildcard. Example filtering `from` address for an indexed ERC-20 `Transfer` topic:

```json
{
  "chain": "base",
  "contract_address": "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
  "event_signature": "Transfer(address,address,uint256)",
  "topic_filters": [
    "0x000000000000000000000000<lowercase-20-byte-address-without-0x>",
    null
  ]
}
```

## Cleanup

Disable or delete test triggers when done:

```bash
for id in "$WEBHOOK_TRIGGER_ID" "$SCHEDULE_TRIGGER_ID" "$CHAIN_TRIGGER_ID"; do
  [ -n "$id" ] && [ "$id" != "null" ] || continue
  curl -fsS -X PATCH -b "$COOKIE_JAR" "$LT_BASE_URL/api/triggers/$id" \
    -H 'content-type: application/json' \
    -d '{"enabled":false}' | jq .
done
```

Or delete them permanently:

```bash
for id in "$WEBHOOK_TRIGGER_ID" "$SCHEDULE_TRIGGER_ID" "$CHAIN_TRIGGER_ID"; do
  [ -n "$id" ] && [ "$id" != "null" ] || continue
  curl -fsS -X DELETE -b "$COOKIE_JAR" "$LT_BASE_URL/api/triggers/$id" -i
done
```

## Troubleshooting

- `401` from `/api/*`: session cookie missing/expired. Request a new magic link and verify it with `curl -L -c "$COOKIE_JAR" -b "$COOKIE_JAR"`.
- `400 {"error":"usage_api_key_required"}`: create payload omitted `usage_api_key` or it was blank.
- `400 {"error":"cron_required"}` or `invalid_cron`: schedule config must include valid `config.cron`; use `* * * * *` for smoke tests.
- `400 {"error":"invalid_chain_event_config"}`: check chain key, address length, event signature, `topic_filters`, and `start_block` type.
- `202` webhook response but no terminal run: dispatcher may still be running, queue may be backed up, or app sleeping/replica settings may be wrong.
- Run reaches `failed` with Chipotle `401`/`403`: scoped usage key is invalid or lacks permission for this action/group.
- No chain-event runs: check Railway RPC env var for that chain, app logs, confirmation depth, initial lookback, and whether the contract emitted matching logs.
- `/api/triggers/<id>/test` returns `501`: expected for now; use webhook firing, schedule waits, or real chain events as test paths.

## Verification Checklist

- [ ] `/health` returns `ok`.
- [ ] Magic-link login creates a valid session; `/api/me` returns the test user.
- [ ] Webhook trigger can be created and `/webhook/<id>` returns `202` with a `run_id`.
- [ ] Webhook run reaches `success` with a valid scoped usage key/action.
- [ ] Schedule trigger creates at least one run within two minutes.
- [ ] Chain-event trigger creates runs when matching logs exist and the chain RPC env var is configured.
- [ ] Test triggers are disabled or deleted after the smoke test.
