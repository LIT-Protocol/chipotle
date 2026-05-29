# webhook-echo

The canonical starter / smoke test. Returns the parsed body, the source, and
the header keys it received. Use it to confirm a trigger is wired up before
building anything real.

## Action

[`action.js`](./action.js) — returns `{ ok, received_at, source, event, header_keys }`.

## Create

```bash
ACTION=$(cat action.js)
curl -fsS -X POST https://triggers.litprotocol.com/api/triggers \
  -H "authorization: Bearer $LOCAL_AGENT_TOKEN" \
  -H 'content-type: application/json' \
  -d "$(jq -n --arg code "$ACTION" --arg key "$USAGE_API_KEY" '{
        name: "webhook-echo",
        kind: "webhook",
        action_code: $code,
        default_params: {},
        usage_api_key: $key,
        config: {}
      }')"
```

## Fire

```bash
curl -fsS -X POST https://triggers.litprotocol.com/webhook/<trigger-id> \
  -H 'content-type: application/json' \
  -d '{"hello":"world","n":7}'
```

## Sample run output

```json
{
  "ok": true,
  "received_at": "2026-05-29T00:10:41.625Z",
  "source": "webhook",
  "event": { "hello": "world", "n": 7 },
  "header_keys": ["content-type", "user-agent", "x-forwarded-for", "x-forwarded-host", "x-forwarded-proto"]
}
```
