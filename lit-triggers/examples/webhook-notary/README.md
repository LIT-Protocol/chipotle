# webhook-notary

The minimal "sign with my keyless wallet" primitive. Takes any JSON payload,
computes a deterministic keccak256 digest, and signs it with this action's
wallet — a key held by the Lit network, not by any server or human. The result
is a tamper-evident receipt that only this exact action code could produce.

This is the building block the on-chain examples extend: notary + a destination
contract = release attestation / subscription / oracle.

## Action

[`action.js`](./action.js) — returns `{ notarized_at, signer, canonical, digest, signature, payload }`.

## Create

```bash
ACTION=$(cat action.js)
curl -fsS -X POST https://triggers.litprotocol.com/api/triggers \
  -H "authorization: Bearer $LOCAL_AGENT_TOKEN" \
  -H 'content-type: application/json' \
  -d "$(jq -n --arg code "$ACTION" --arg key "$USAGE_API_KEY" '{
        name: "webhook-notary", kind: "webhook", action_code: $code,
        default_params: {}, usage_api_key: $key, config: {}
      }')"
```

## Fire

```bash
curl -fsS -X POST https://triggers.litprotocol.com/webhook/<trigger-id> \
  -H 'content-type: application/json' \
  -d '{"release":"v1.2.3","commit":"abc123"}'
```

## Sample run output (live-tested)

```json
{
  "notarized_at": "2026-05-29T00:15:42.166Z",
  "signer": "0xEB5D81692900E2237f4635bb15743733FdccDBC3",
  "canonical": "{\"commit\":\"abc123\",\"release\":\"v1.2.3\"}",
  "digest": "0x0cd8f53dd521fb47f4b15864152dfbdb127963d373621dac9c8bcf22205cc6e8",
  "signature": "0x75cf2e78...143b45c81c",
  "payload": { "commit": "abc123", "release": "v1.2.3" }
}
```

## Verifying a receipt

```js
// off-chain, anywhere:
const recovered = ethers.utils.verifyMessage(ethers.utils.arrayify(digest), signature);
// recovered === signer  →  payload is authentic and unmodified
```

The digest uses a recursive sorted-key serialization (`stableStringify`) so any
verifier with the same payload reproduces the exact bytes that were signed.
