# chain-feed-mirror

Relay a Chainlink price feed to a chain Chainlink does **not** support — with no
trusted relayer holding keys.

## Why this is interesting

Chainlink publishes feeds on major chains but not on every L2 / appchain. This
action watches a Chainlink aggregator's `AnswerUpdated` event on a supported
source chain, reads the new price, and writes it to a `PriceConsumer` contract
on any destination chain reachable by RPC — signed by a key no human holds. The
price originates from a verifiable on-chain Chainlink event, so the relay is
trust-minimized.

> The chain-event **trigger** only supports `ethereum`/`base`/`arbitrum`/`bsc`/`polygon`
> as the source, but the action **body** can write to any EVM chain via `destRpcUrl`.

## Action & contract

- [`action.js`](./action.js) — on each `AnswerUpdated`, mirror `(answer, roundId, updatedAt)` to the destination consumer.
- [`PriceConsumer.sol`](./PriceConsumer.sol) — minimal mirrored feed; `setPrice` accepts only newer rounds.

Source event: `AnswerUpdated(int256 indexed current, uint256 indexed roundId, uint256 updatedAt)`
→ `decoded.arg0 = price`, `arg1 = roundId`, `arg2 = updatedAt`.

## Config (`default_params`)

| key | meaning |
|-----|---------|
| `destRpcUrl` | destination chain RPC (the unsupported chain) |
| `consumer` | `PriceConsumer` address on the destination chain |
| `dryRun` | sign but don't broadcast |

## Create (chain-event trigger)

The `config.contract_address` must be the Chainlink **aggregator** that emits
`AnswerUpdated` (resolve it from a feed proxy with `proxy.aggregator()`).

```bash
ACTION=$(cat action.js)
curl -fsS -X POST https://triggers.litprotocol.com/api/triggers \
  -H "authorization: Bearer $LOCAL_AGENT_TOKEN" -H 'content-type: application/json' \
  -d "$(jq -n --arg code "$ACTION" --arg key "$USAGE_API_KEY" '{
        name: "feed-mirror", kind: "chain_event",
        action_code: $code,
        default_params: {
          destRpcUrl: "https://sepolia.base.org",
          consumer: "0x7767fDF3BEc5Acb36Abe4ac5204F396eB4689Cb7",
          dryRun: true
        },
        usage_api_key: $key,
        config: {
          chain: "base",
          contract_address: "0x<chainlink-aggregator-on-base>",
          event_signature: "AnswerUpdated(int256,uint256,uint256)"
        }
      }')"
```

## Setup

1. Create with `dryRun: true`. The first matching event reports `relayer: 0x…` —
   this action's wallet. Fund it with gas on the destination chain.
2. PATCH `default_params.dryRun` to `false`.

## Verified end-to-end (Base Sepolia destination)

Driven with a representative `AnswerUpdated` payload, the mirror broadcast and
the destination contract reflected it:

```json
{ "ok": true, "source_chain": "base",
  "relayer": "0x96Be64316CD853585c59cF1519aE10380C8E73Ff",
  "answer": "200000000000", "roundId": "18446744073709551", "updatedAt": "1748476800",
  "txHash": "0x9677dac100c127db8da906c1f0b00656343a329821cd44cf9f762b85d7355267" }
```

On-chain read of `PriceConsumer` (`0x7767…9Cb7`) afterward:
`latestAnswer() = 200000000000`, `lastUpdater = 0x96Be…73Ff`.

## Production notes

- Restrict `setPrice` to the known relayer address (the demo is permissionless).
- The relayer wallet needs gas on the destination chain; top it up or meter runs.
