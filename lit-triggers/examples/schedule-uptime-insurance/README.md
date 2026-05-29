# schedule-uptime-insurance

Parametric "productivity insurance": a cron-scheduled action that pays you in
ETH when a service you depend on goes down.

## Why this is interesting

Traditional parametric insurance needs three trusted parties — an **oracle** for
the data, a **keeper** to run the check, and a **multisig** to release funds.
This action collapses all three into one trust-minimized unit: it fetches the
trusted status data, decides, **and** signs the payout — from a pool key (its
own wallet) that no human or server holds. No admin can rug the pool, and no
oracle can be bribed independently of the payout.

The "pool" is simply the action wallet's balance. Fund it to capitalize the
policy; a payout drains it toward the policyholder when the trigger fires.

## Action

[`action.js`](./action.js) — on each cron tick: fetch a Statuspage
`summary.json`, and if `status.indicator` is `major`/`critical`, sign and send a
fixed ETH payout to the policyholder.

## Config (`default_params`)

| key | meaning |
|-----|---------|
| `statusUrl` | Statuspage summary.json (e.g. `https://status.anthropic.com/api/v2/summary.json`) |
| `downIndicators` | indicators that count as down (default `["major","critical"]`) |
| `rpcUrl` | payout chain RPC |
| `policyholder` | address that gets paid |
| `payoutWei` | payout amount in wei |
| `dryRun` | sign but don't broadcast (returns the action wallet address to fund) |
| `test_indicator` | TEST ONLY — override the fetched indicator to exercise the payout |

## Create (schedule trigger)

```bash
ACTION=$(cat action.js)
curl -fsS -X POST https://triggers.litprotocol.com/api/triggers \
  -H "authorization: Bearer $LOCAL_AGENT_TOKEN" -H 'content-type: application/json' \
  -d "$(jq -n --arg code "$ACTION" --arg key "$USAGE_API_KEY" '{
        name: "uptime-insurance", kind: "schedule",
        action_code: $code,
        default_params: {
          statusUrl: "https://status.anthropic.com/api/v2/summary.json",
          rpcUrl: "https://sepolia.base.org",
          policyholder: "0xYourAddress",
          payoutWei: "1000000000000000",
          dryRun: true
        },
        usage_api_key: $key,
        config: { cron: "*/5 * * * *" }
      }')"
```

## Setup

1. Create with `dryRun: true`. The first run reports `pool: 0x…` — that is this
   action's wallet address.
2. Fund that address with the payout amount + gas.
3. PATCH `default_params.dryRun` to `false`.

## Verified end-to-end (Base Sepolia)

With `test_indicator: "critical"` forcing the down branch, a real payout
broadcast:

```json
{ "ok": true, "indicator": "critical", "paid": true,
  "pool": "0xC7BB0742d9649c23A842F26032DD905496263Fdc",
  "to": "0xe12aF7e96A25f580241bd54C392CB16090cA926e",
  "amount_wei": "1000000000000000",
  "txHash": "0xb2b38552a0cad6d39e71166e58d30719212d5845ff8451facc89c7bad4edda5f" }
```

## Production notes

- Debouncing on *N consecutive* failures needs state across runs, which the
  action does not have. Either use a status feed that reports incident duration
  (and pay out when an active incident exceeds a threshold), or keep external
  state. This demo pays on the current indicator.
- Restrict who can fund/claim and add a per-incident payout cap for real use.
