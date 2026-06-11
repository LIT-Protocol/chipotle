# Venue TWAP Order

**TWAP execution the D7 way: a long-running strategy is NOT one long action —
it's a chain of small ones.** Each tick runs a tiny Lit Action that places at
most ONE policy-fenced child order (via [`lit-venues`](../../lit-venues)) and
returns the strategy state; the caller persists it and passes it back next tick.

## The D7 pattern (chained triggers, not long actions)

Lit Actions are request-scoped (15-minute cap) and fetch-quota'd — a TWAP that
"runs for an hour" is the wrong shape. Instead:

```
  cron / npm run tick          Lit network (one attested run per tick)        venue
  ───────────────────          ────────────────────────────────────────      ──────
  state.json ──► main({state, …})
                   │ fetchMarket  (tick 1 only — then injected from state)
                   │ fetchTicker  → drift fence vs tick-1 reference price
                   │ size slice exactly (BigInt decimal-string math)
                   │ notional fence → createOrder (ONE child order) ───────►  fill
                   ▼
  state.json ◄── { state: {filledSlices, remaining, referencePrice, market, done} }
```

Every tick is an independently attested execution with its own receipt, a
crashed tick loses only that tick, and each run stays far inside the fetch
quota (**≤4 fetches per tick** — see "Quota discipline").

## Files

| Path | Purpose |
| --- | --- |
| `action/twapOrder.js` | The Lit Action: one TWAP tick. Plain JS, no imports — the `lit-venues` IIFE bundle is concatenated above it (global `LitVenues`). Enforces the drift band, per-slice notional cap, venue min-size rules, and an expiry fence, all in exact decimal-string math. |
| `scripts/_lit.js` | Composes `bundle + action`, runs it via `/lit_action`, unwraps the envelope. Retries are gated to auth-propagation errors only — a trading tick is never blind-retried. |
| `scripts/_env.js` | Minimal `.env` reader / upserter, inlined so the folder is self-contained. |
| `scripts/setup.js` | One-shot: composed-code CID, permission group, scoped usage key, in-runtime probe of the bundle, action registration. |
| `scripts/tick.js` | One tick: load `state.json` → run the action → write `state.json` back. This file is the chain link. |

## Walkthrough

Prerequisites, honestly: (1) build the venue library once —
`cd ../../lit-venues && npm install && npm run build` — the scripts inline its
~140KB IIFE bundle; (2) a Lit account + **master API key**
([dashboard](https://dashboard.chipotle.litprotocol.com)); (3) **Binance spot
testnet** keys from <https://testnet.binance.vision> (GitHub login; accounts
come pre-funded with play balances). The testnet is reachable from the
network's US egress; binance.com **mainnet** is geo-blocked (451) and needs
the D4 egress proxy (`VENUE_PROXY_URL`).

```bash
cp .env.example .env      # set LIT_API_KEY, VENUE_API_KEY, VENUE_SECRET
npm install
npm run setup
npm run tick              # slice 1/4 — also records the reference price
npm run tick              # slice 2/4 …
```

Repeat until it prints `TWAP complete`. `state.json` holds the state and the
per-slice order log; delete it to start a fresh TWAP. `DRY_RUN=true` walks
every fence without placing orders.

### Going autonomous

`tick.js` is deliberately cron-shaped. Locally:

```
* * * * * cd /path/to/venue-twap-order && npm run tick >> twap.log 2>&1
```

For a hosted cron, [lit-triggers](https://triggers.litprotocol.com) schedule
triggers (see [`../lit-triggers`](../lit-triggers)) run this same composed
action — **but note**: a schedule trigger passes static `default_params`, and
this example's state rides in params. Three honest ways to close that loop:

1. **Caller-owned state (what this example ships):** your cron runs `tick.js`,
   which owns `state.json`. Simplest, fully working.
2. **lit-triggers + a small updater** that `PATCH`es the trigger's
   `default_params.state` with each run's response (the API supports it).
3. **Sealed in-TEE state:** the action seals state with
   `Lit.Actions.Encrypt({ pkpId, message })`, you store the ciphertext
   anywhere, the next tick passes it back in and `Lit.Actions.Decrypt`s it.
   Mind rollback — an old blob replays old state, so keep the `ticks` counter
   and per-slice `clientOrderId`s.

## Policy fences (all evaluated in-TEE, in exact decimal math)

- **Price band:** the first tick records `state.referencePrice`; any tick whose
  last price is outside `±MAX_DRIFT_BPS` of it **skips the slice** (no order).
- **Per-slice notional cap:** `amount × last > MAX_SLICE_NOTIONAL` skips the
  slice rather than resizing it — a fence, not an optimizer.
- **Venue rules:** sizes are floored onto the lot grid with
  `LitVenues.roundDownToIncrement`; `minAmount`/`minNotional` violations abort
  with a clear reason. The last slice mops up the exact remainder.
- **Expiry:** the strategy marks itself `done` after `MAX_TICKS` (default
  `SLICES*5`) so an out-of-band price can't make a cron run forever.
- **No floats:** sizes/prices are decimal strings end to end — lit-venues'
  `addDec`/`subDec`/`roundDownToIncrement`/`applyBps` plus scaled-BigInt
  multiply/divide helpers in the action. `Number` appears only for counts.

## Quota discipline

Per-tick budget (50 allowed; this example's self-imposed ceiling is 10):
tick 1 = `fetchMarket` + `fetchTicker` + `createOrder` = **3**; tick 2+ = **2**
(+1 internal mid-price fetch for hyperliquid market orders). The tick-2+
saving is the **markets-cache injection** pattern: tick 1 stores the `Market`
(tick/lot rules) in state, later ticks pass it back via `VenueConfig.markets`,
and `fetchMarket` answers without HTTP.

## Hyperliquid option

`VENUE_ID=hyperliquid` (perps; testnet via `VENUE_SANDBOX=true`): no API key
exists — orders are EIP-712 signatures. Provide `VENUE_PRIVATE_KEY`, or set
`VENUE_USE_ACTION_KEY=true` to trade with the action's own CID-bound TEE key
after the master account `approveAgent`s its address — then the trading key
never exists outside the TEE and structurally cannot withdraw.

## Production notes

- **Re-fire honesty:** per-slice `clientOrderId`s (from `TWAP_ID`) dedupe
  concurrent double-fires, but venues only enforce uniqueness among OPEN
  orders — a crash between `createOrder` and the state write can double-place
  a filled slice on re-run. Hence `tick.js` never blind-retries,
  `MAX_SLICE_NOTIONAL × SLICES` bounds exposure, and a failed tick should be
  reconciled against `fetchMyTrades` before re-running.
- **Partial fills:** IOC limit slices decrement state by the venue-reported
  fill, so unfilled remainder rolls into later slices.
- **Coinbase:** market BUYs are quote-sized on Advanced Trade — use
  `ORDER_TYPE=limit` there (the action refuses otherwise); no sandbox exists.
- **Credentials:** this demo passes testnet keys via `js_params`. Real keys
  belong in sealed `venue-credentials-v1` ciphertext, decrypted in-TEE with
  `Lit.Actions.Decrypt` — see the lit-venues README.
