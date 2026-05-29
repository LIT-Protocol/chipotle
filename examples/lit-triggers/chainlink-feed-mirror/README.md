# Chainlink Feed Mirror

**Relay a Chainlink price feed to a chain Chainlink doesn't serve — with no
trusted relayer holding keys.** A [lit-triggers](https://triggers.litprotocol.com)
chain-event trigger fires on a Chainlink aggregator's `AnswerUpdated` event on a
supported source chain; the Lit Action reads the new price and writes it to a
`PriceConsumer` contract on any destination chain, signed by a wallet derived
from the action's IPFS CID (the consumer pins it as `updater`).

## Why this is interesting

Chainlink publishes feeds on major chains but not on every L2 / appchain.
Bridging a feed normally means trusting a relayer (a key, a federation) to copy
the value faithfully. Here the value originates from a **verifiable on-chain
Chainlink event**, and the relay is signed by a **keyless wallet tied to this
exact action code** — so the trust assumption is "this content-addressed action
copies the event it was triggered on," not "this relayer operator is honest."

> The chain-event **trigger** only supports `ethereum`/`base`/`arbitrum`/`bsc`/`polygon`
> as the source, but the action **body** can write to any EVM chain via `destRpcUrl`.

```
   Chainlink aggregator        lit-triggers (chain_event)     Lit network          PriceConsumer (dest chain)
   (Base mainnet)              ──────────────────────────     ───────────          ──────────────────────────
   AnswerUpdated  ──────────►  run: main(params)
   (price, roundId, ts)           │ decoded.arg0/1/2
                                  │ check dest RPC chainId == destChainId
                                  │ sign + send setPrice ────────────────────────► setPrice(answer,roundId,ts)
                                  │   from relayer wallet                            require(msg.sender==updater) ✓
                                  ▼                                                  require(newer round) ✓
                              run history
```

## Hardening

- **Source re-verification (the important one).** The action does **not** trust
  the price the trigger hands it. A chain-event trigger supplies a decoded log,
  but anyone holding the usage key could execute the action with a fabricated
  `decoded` payload. So the action takes only the `transaction_hash` and
  `log_index`, re-fetches that receipt from a **hostname-pinned, https-only**
  source RPC (`SOURCE.rpcHost`), and verifies the log was emitted by the expected
  **`SOURCE.aggregator`** with the `AnswerUpdated` topic and enough confirmations.
  It decodes the price from that verified log. The aggregator/chain/host are baked
  constants — editing them changes the action CID + signer, so a modified action
  can't write to the existing `PriceConsumer`. A fabricated price is simply
  ignored (and a wrong-emitter tx is rejected).
- **Destination pin.** `destChainId` is required; the action calls `eth_chainId`
  on `destRpcUrl` and refuses to write unless it matches — a swapped RPC can't
  redirect the relayed price to another chain.
- **Updater pin.** `PriceConsumer.setPrice` reverts unless `msg.sender` is the
  relayer (the action's derived wallet).
- **Stale-round reject.** `setPrice` accepts only strictly-newer `roundId`s
  (gated by an explicit `initialized` flag so a `roundId == 0` write can't bypass
  it), matching Chainlink semantics.

## Files

| Path | Purpose |
| --- | --- |
| `action/feedMirror.js` | The Lit Action: read the decoded `AnswerUpdated`, verify the dest chain, write `setPrice`. |
| `contracts/PriceConsumer.sol` | Mirrored feed; only the pinned relayer can write, only newer rounds. |
| `scripts/setup.js` | One-shot: action CID → group → scoped key → derive + fund relayer → deploy consumer → resolve Chainlink aggregator → authorize → create the chain-event trigger. |
| `scripts/deploy.js` | Hardhat deploy of `PriceConsumer`, pinning the relayer as `updater`. |
| `scripts/mirror.js` | End-to-end client: `--simulate` (deterministic) or watch the real trigger. |
| `scripts/_env.js` | Tiny shared `.env` reader / upserter. |

## Walkthrough

```bash
cp .env.example .env       # set LIT_API_KEY (master) + DEPLOYER_PRIVATE_KEY
npm install
npm run setup              # opens a browser — click "Authorize agent"

# Deterministic check of the relay logic (no waiting for Chainlink):
npm run mirror -- --simulate

# Or watch the real trigger fire on the next on-chain AnswerUpdated:
npm run mirror
```

`setup` deploys `PriceConsumer`, resolves the Chainlink ETH/USD aggregator on
Base mainnet (from its proxy), and creates a chain-event trigger watching its
`AnswerUpdated`. The real trigger fires whenever Chainlink next updates (a price
deviation or its heartbeat — can take minutes), so `--simulate` feeds the same
action a synthetic `AnswerUpdated` through a throwaway webhook to show the relay
immediately, then reads the consumer back:

```
PriceConsumer 0x… — current roundId 0
Creating temporary webhook trigger with the same action...
  queued: {"run_id":"…","status":"queued"}
  run status: success
  action result: {"ok":true,"source_chain":"base","relayer":"0x…","answer":"200000000000","roundId":"1","updatedAt":"…","txHash":"0x…"}
  (temporary trigger deleted)
PriceConsumer now — roundId 1, latestAnswer 200000000000
✓ Chainlink price relayed on-chain by the keyless relayer wallet.
```

## Targeting different feeds / chains

- **Source feed:** set `FEED_SOURCE_PROXY` to any Chainlink price-feed proxy on a
  supported source chain (and `FEED_SOURCE_CHAIN` / `FEED_SOURCE_RPC` to match).
  Setup resolves the underlying aggregator that emits `AnswerUpdated`.
- **Destination:** point `BASE_SEPOLIA_RPC_URL` / `DEST_CHAIN_ID` at any EVM
  chain and deploy `PriceConsumer` there. The action writes wherever `destRpcUrl`
  points — as long as its chainId matches `destChainId`.

## Production considerations

- **Relayer gas.** The relayer wallet pays gas on the destination chain for
  every update; keep it funded, and consider that a busy feed emits often.
- **Decimals & feed identity.** This stores the raw `int256` answer. A real
  consumer should also record the feed's `decimals()` and which feed/pair it is,
  and expose a Chainlink-compatible `latestRoundData()` if downstream contracts
  expect it.
- **Updater rotation.** `updater` is immutable; rotating the action means
  redeploying or adding a governance-gated setter.
- **Liveness.** The mirror is only as fresh as the source feed's updates plus
  the trigger's confirmation depth. Downstream contracts should treat
  `updatedAt` as the freshness bound and reject stale prices.
