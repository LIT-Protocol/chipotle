# webhook-release-attestation

Verify a GitHub release webhook, then anchor the release on-chain with the
keyless action wallet — a tamper-evident, publicly verifiable record of the
canonical release.

## Why this is interesting

It is the [notary](../webhook-notary) primitive + a destination contract, with
real sender authentication. The action verifies GitHub's `X-Hub-Signature-256`
HMAC over the **raw** request body before anything touches the chain, so only a
genuinely-signed GitHub event can write to the registry — and the write is
signed by a decentralized key, so no single server can forge release records.

> Replace the "release published → registry" logic with "Stripe
> `customer.subscription.created` → `setSubscriber(addr, expiry)`" and you have
> the web2-billing-unlocks-web3-access bridge — same skeleton, same HMAC verify.

## Action & contract

- [`action.js`](./action.js) — verify HMAC → only `release`/`published` → write `(repo, tag, commitish)` to the registry.
- [`ReleaseRegistry.sol`](./ReleaseRegistry.sol) — stores the latest attestation per `(repo, tag)` and emits `Attested`.

## Requires

This example depends on the webhook handler exposing the raw body
(`params.event_raw`) and the `x-hub-signature-256` / `x-github-event` headers —
added in lit-triggers PR #404. Without it, HMAC verification cannot work because
the signed bytes never reach the action.

## Config (`default_params`)

| key | meaning |
|-----|---------|
| `secret` | GitHub webhook secret (use `Lit.Actions.Encrypt`/`Decrypt` in prod) |
| `rpcUrl` | destination chain RPC |
| `registry` | `ReleaseRegistry` address |
| `dryRun` | verify + sign but don't broadcast |

## Create (webhook trigger)

```bash
ACTION=$(cat action.js)
curl -fsS -X POST https://triggers.litprotocol.com/api/triggers \
  -H "authorization: Bearer $LOCAL_AGENT_TOKEN" -H 'content-type: application/json' \
  -d "$(jq -n --arg code "$ACTION" --arg key "$USAGE_API_KEY" --arg secret "$RELEASE_WEBHOOK_SECRET" '{
        name: "release-attestation", kind: "webhook",
        action_code: $code,
        default_params: {
          secret: $secret,
          rpcUrl: "https://sepolia.base.org",
          registry: "0x57b88E15f3e9b2aB62f4114a873a19F6EFEfD375",
          dryRun: true
        },
        usage_api_key: $key, config: {}
      }')"
```

Then point a GitHub repo webhook (Settings → Webhooks) at
`https://triggers.litprotocol.com/webhook/<trigger-id>`, content type
`application/json`, secret = `RELEASE_WEBHOOK_SECRET`, events = *Releases*.

## Setup

1. Create with `dryRun: true`. Publish a release (or replay a delivery); the run
   reports `signer: 0x…` — this action's wallet. Fund it with gas.
2. PATCH `default_params.dryRun` to `false`.

## Verifying an attestation

```bash
cast call 0x57b88E15f3e9b2aB62f4114a873a19F6EFEfD375 \
  'getRelease(string,string)(string,address,uint256)' \
  'owner/repo' 'v1.2.3' --rpc-url https://sepolia.base.org
# → commitish, attester (the action wallet), timestamp
```

## Status

Contract deployed at `0x57b88E15f3e9b2aB62f4114a873a19F6EFEfD375` (Base Sepolia).
End-to-end HMAC verification + on-chain write pending the deploy of PR #404.
