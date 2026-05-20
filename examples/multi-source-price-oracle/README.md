# Multi-Source Price Oracle

**A spot-price oracle that fetches from three independent exchanges, takes
the median, validates the spread, and signs the result for any EVM chain.**

This is the practical "I need an oracle and I don't want to trust one
provider" example. The action hits Coinbase, Kraken, and Bitstamp in
parallel for the asset you ask for, takes the median of whichever sources
responded, rejects the result if the spread between min and max is wider
than the configured threshold (default 1%), and signs the surviving price
into a `PriceOracle` registry contract.

Use this when you want a Chainlink-shaped feed without Chainlink — your
contracts read `oracle.latest("ETH")` and get a signed spot price that no
single venue can manipulate.

## Why median, not strict equality

The [`compliance-transfer-gate`](../compliance-transfer-gate) example uses
strict byte-equality across three sources because the underlying data
(an `isSanctioned(addr) → bool`) doesn't drift between observations.
Spot prices drift between exchanges every second, so the same approach
would never accept a reading. Median instead:

- **Tolerates one outlier.** A single exchange returning a stale, frozen,
  or manipulated price doesn't move the median.
- **Survives one outage.** If Kraken is down, Coinbase + Bitstamp still
  give you a two-source median.
- **Composes with a spread check.** Median alone won't catch the case
  where every exchange is far apart; the action computes
  `(max - min) / median` in basis points and refuses to sign if the
  spread exceeds `maxSpreadBps` (default 100bps = 1%).

## How it works

```
  caller                Lit Action                  Coinbase  Kraken  Bitstamp
    │                       │                          │        │        │
    │ asset = "ETH"         │                          │        │        │
    ├──────────────────────►│                          │        │        │
    │                       │   GET /spot              │        │        │
    │                       │   GET /Ticker            │        │        │
    │                       │   GET /ticker            │        │        │
    │                       ├─────────────────────────►│───────►│───────►│
    │                       │◄─────────────────────────┤◄───────┤◄───────┤
    │                       │ price_a    price_b    price_c              │
    │                       │ sort, take median                          │
    │                       │ check spread ≤ maxSpreadBps                │
    │ sig, price            │ sign with                                  │
    │◄──────────────────────┤ getLitActionPrivateKey()                   │
    │                       │                                            │
    │ PriceOracle.submit(asset, price, decimals, observedAt, sig)       │
    ├───────────────────────────────────────────────────────────────────►│
    │                       │             ecrecover(sig)                  │
    │                       │               == signer ✓                   │
```

The signature uses `Lit.Actions.getLitActionPrivateKey()` — derived from
the action's IPFS CID. The deployed `PriceOracle` pins the address of that
key as `signer`. Editing the action source — say, to change the three
sources or the spread threshold — produces a new CID and therefore a new
signer address; old registries stop trusting the new action.

## Supported assets

The action supports a small hardcoded set (`ETH`, `BTC`, `SOL`) because
each exchange uses different symbol conventions (Kraken legacy markets
prefix `X`/`Z`, BTC is `XBT`, etc.) and a real app maintains its own
mapping. Add more in `SYMBOLS` at the top of `action/priceOracle.js`.

## All three sources are keyless

Coinbase, Kraken, and Bitstamp expose public spot-price endpoints with
no API key requirement and no signup. The example has **no PKP, no
encryption, no API keys** — copy `.env.example`, fill in two values, run
`npm run setup`, done.

(If you want to swap in keyed providers — Polygon.io, Twelve Data,
CryptoCompare — encrypt the keys to a PKP per the pattern in the
`prediction-market-oracle` example.)

## Files

| Path | Purpose |
| --- | --- |
| `action/priceOracle.js` | The Lit Action: fetch in parallel, median, spread check, sign. |
| `contracts/PriceOracle.sol` | On-chain registry storing the signed readings keyed by asset symbol. |
| `scripts/setup.js` | One-shot setup: computes the action CID, derives the action's wallet address, creates and wires the group, deploys the registry. Idempotent. |
| `scripts/deploy.js` | Hardhat deploy; pins the action's derived address as `signer`. |
| `scripts/submit.js` | End-to-end runner: ask the action for a fresh price, submit it on-chain. |
| `scripts/test-medianizer.js` | Zero-dep harness — same medianizer logic, no Lit envelope, no signing. Useful for sanity-checking source connectivity. |
| `scripts/_env.js` | Tiny shared `.env` reader / upserter. |
| `.env.example` | All the env vars you'll fill in. |

## Walkthrough

### 1. Smoke-test the sources

Before involving Lit or any chain, verify your machine can talk to all
three exchanges:

```bash
npm install   # only needed for the scripts that touch hardhat / ethers
npm run test-medianizer
```

Expected output:

```
Fetching ETH/USD from 3 sources...

  coinbase   $2104.65
  kraken     $2104.60
  bitstamp   $2104.62

Median:  $2104.62
Spread:  3 bps (max-min as % of median)
On-chain integer (8 decimals): 210462000000

PASSED: action would sign this price.
```

If a source is unreachable (e.g. Binance from a US-restricted region —
not used here, but the same kind of issue could affect any source from
some hosting environments), this script tells you immediately.

### 2. Fill in your inputs

```bash
cp .env.example .env
```

Edit `.env` and set:
- `LIT_API_KEY` — your **account-level (master) API key** from the
  [Chipotle dashboard](https://dashboard.chipotle.litprotocol.com), *not* a
  scoped usage key. Setup calls management endpoints that revert
  `NotMasterAccount` on scoped keys.
- `DEPLOYER_PRIVATE_KEY` — an EOA with gas on Base Sepolia (or your target chain).
- `SUBMITTER_PRIVATE_KEY` — for `submit.js`; can be the same EOA as the deployer for testing.

### 3. Run setup

```bash
npm run setup
```

Walks through seven steps, printing each as it goes:

1. Compute the action's IPFS CID.
2. Create a permission group with a wildcard action allowlist
   (`cid_hashes_permitted: ["0"]`).
3. Create a scoped usage API key (`execute_in_groups: [groupId]`),
   saved as `LIT_USAGE_API_KEY` in `.env`. The deriver in step 4 and
   `submit.js` afterward both use this for `/lit_action`.
4. Derive the action's wallet address from its CID.
5. Register the action with your account (metadata).
6. Add the specific action CID to the group (audit trail).
7. Deploy `PriceOracle` with the action's wallet address as the signer.

Re-running `npm run setup` does a fresh setup top-to-bottom: every step
creates new on-chain state and overwrites the corresponding key in
`.env`. The previously-minted group / usage key / contract become
orphaned.

### 4. Submit a price reading

```bash
npm run submit -- --asset ETH
```

Expected output:

```
ETH/USD = $2104.62 (median of 3, spread 3 bps)
Sources:
  coinbase   $2104.655
  kraken     $2104.6
  bitstamp   $2104.62
On-chain price: 210462000000 (decimals=8)
tx: 0x...
mined in block 12345678
```

If one source is down, the action still signs with the median of the
remaining two; the response lists the failure under `failedSources`. If
two are down, it returns `{ authorized: false, reason: "only 1/3
sources succeeded" }` and the on-chain step is never reached.

## Reading the oracle from another contract

```solidity
interface IPriceOracle {
    function latest(string calldata asset)
        external
        view
        returns (uint256 price, uint8 decimals, uint64 observedAt, uint64 submittedAt);
}

contract MyConsumer {
    IPriceOracle constant ORACLE = IPriceOracle(0x...);

    function ethUsd() external view returns (uint256) {
        (uint256 price, uint8 decimals, uint64 observedAt, ) = ORACLE.latest("ETH");
        require(block.timestamp - observedAt < 1 hours, "stale price");
        require(decimals == 8, "unexpected precision");
        return price; // raw integer; divide by 10^decimals for the dollar amount
    }
}
```

Consumers enforce their own staleness window — `submit()` records both
`observedAt` (when the Lit Action read the sources) and `submittedAt`
(when the registry recorded it), so contracts can use whichever fits
their threat model.

## Tuning

These knobs are **hardcoded constants at the top of
[`action/priceOracle.js`](./action/priceOracle.js)**, not `js_params`.
Caller-supplied safety thresholds would be theatre — anyone holding the
usage key could ask the action to sign with `MIN_SOURCES = 1` and a
huge spread limit, defeating the median-of-three story this example
sells. Hardcoding them puts the trust anchor in the action's IPFS CID:
edit a constant, the CID changes, the signer address changes, and
existing PriceOracle deployments stop accepting signatures from the
modified action — i.e. you redeploy.

- **`MAX_SPREAD_BPS`** (default 100, i.e. 1%). Raise for volatile
  assets where legitimate cross-exchange spreads are wider; lower for
  high-stakes consumers that want to halt on any unusual market state.
- **`MIN_SOURCES`** (default 2). Set to 3 to require all three sources
  — strictest, but a single provider outage halts the oracle.
- **`DECIMALS`** (default 8). Chainlink-compatible. Bump to 18 if your
  consumers want full ETH-precision fixed-point.
- **Additional sources.** Add entries to `SOURCES` further down in
  `priceOracle.js`. More sources mean median is more robust but
  latency rises to the slowest fetch.

## Production considerations

- **Trust model.** This is "trust the action's code"; if any of the three
  exchanges' HTTPS terminations are compromised the action could be fed a
  bad price for one source — but the median still rejects one bad source.
  Adding a fourth source (median of 4) tolerates one bad and one missing
  simultaneously.
- **Replay protection.** The contract enforces `observedAt` strictly
  increasing per asset. Signatures with old `observedAt` are rejected.
- **Manipulation cost.** To move the median, an attacker needs to
  influence two of three sources within the same window — for major
  exchanges this is enormously expensive. The spread check is the second
  line of defense.
- **Gas cost.** Submitting a reading is one `SSTORE` plus a 21k-ish event;
  fits comfortably under the L2 economics of Base / Optimism / Arbitrum.
- **Upgrade path.** Because the signer address is derived from the
  action's CID, any change to the action source invalidates the deployed
  registry's trust. Plan for either a redeploy on policy changes or a
  rotate-signer setter behind a multisig.
