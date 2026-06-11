# Price-Trigger Stop

**A stop-loss where the trigger can pull the trigger but can never aim the
gun.** A price poller — a [lit-triggers](https://triggers.litprotocol.com)
cron or a local loop — invokes a Lit Action on a heartbeat. The action
re-fetches the price from the venue **inside the TEE**, and only if
`last <= stopPrice` places a market sell via [`lit-venues`](../../lit-venues),
fenced by a floor price, a max amount, and a deterministic idempotency key.

## Trust model

The thing that makes this a *policy* and not a bot script:

- **The poller only invokes.** lit-triggers has no native "price" trigger
  kind, and that's the right shape: the service fires `main(params)` on a
  cron; the price condition is evaluated in the attested action against the
  venue's own API. A compromised poller can waste ticks — it cannot lie about
  the price.
- **The bounds live in the invocation policy, not the trigger.** `stopPrice`,
  `floorPrice`, `maxAmount`, `triggerId` are frozen into the trigger's
  `default_params` at arm time by you. A cron fire carries only
  `{source, scheduled_at, cron}` — it cannot override them. Re-aiming requires
  re-arming, an owner-authenticated operation.
- **Fences are enforced venue-deep.** Below the floor the action *refuses to
  sell* (a stop is protection, not an obligation to dump into a gap); the
  amount is capped and floored onto the venue lot grid in exact decimal-string
  math (`roundDownToIncrement` — floats never touch sizes/prices); on
  hyperliquid the order is `reduceOnly`, so it structurally cannot oversell.
- **Hardening further:** the lit-triggers *operator* could in principle alter
  stored params. For treasury-grade stops, inline the bounds as constants in
  `action/priceStop.js` — then they're part of the CID, like
  [`solana-signer`](../solana-signer)'s `MAX_LAMPORTS`, and changing them
  changes the action's identity.

## Idempotency (the double-fire problem)

A poller can fire twice. Three layers keep that from double-selling:

1. **Derived `clientOrderId`.** Both invocation modes derive it from the same
   `TRIGGER_ID`, so concurrent fires collide venue-side. On hyperliquid the
   cloid must match `/^0x[0-9a-f]{32}$/`, so the id is hex-derived: UUID-ish
   seeds verbatim (first 32 hex chars), others hex-encoded and zero-padded.
   The action also checks `fetchOpenOrders` for the id before placing.
2. **Position fences.** Venues only dedupe client ids among *open* orders, so
   a fill frees the id — which is why the spot path clamps the sell to the
   free base balance (a re-fire finds nothing to sell) and the perp path is
   `reduceOnly` (a re-fire on a flat position is venue-rejected).
3. **`maxAmount`** bounds the worst case regardless.

## Files

| Path | Purpose |
| --- | --- |
| `action/priceStop.js` | The Lit Action. Plain JS, no imports — the lit-venues IIFE bundle is concatenated above it. Fetches the ticker, applies stop/floor/max/idempotency fences, places at most one market sell. |
| `scripts/_lit.js` | Composes `bundle + action`, runs `/lit_action`, unwraps the envelope. Retries only auth-propagation errors — never a possibly-traded attempt. |
| `scripts/_venue.js` | Builds the action's params from `.env` — shared by both invocation modes so they run the identical policy. |
| `scripts/_env.js` | Minimal `.env` reader / upserter. |
| `scripts/setup.js` | One-shot: composed-code CID, group, scoped usage key, in-runtime probe, registration, `TRIGGER_ID` generation. |
| `scripts/arm.js` | Registers the poller as a lit-triggers **schedule** trigger (browser agent-authorize handshake); `--off` disarms. |
| `scripts/poll.js` | The same poller as a local loop — no lit-triggers dependency. `--once` for your own crontab. |

## Walkthrough

Prerequisites, honestly: (1) build the venue library once —
`cd ../../lit-venues && npm install && npm run build`; (2) a Lit account +
**master API key** ([dashboard](https://dashboard.chipotle.litprotocol.com));
(3) **Binance spot testnet** keys from <https://testnet.binance.vision>
(GitHub login; pre-funded play balances — the testnet is reachable from US
egress; binance.com mainnet is geo-blocked (451) and needs the D4 proxy);
(4) something to protect — market-buy a little testnet BTC first, or just run
with `DRY_RUN=true` (the default).

```bash
cp .env.example .env      # set LIT_API_KEY, venue keys, and the stop policy
npm install
npm run setup
```

Then choose your poller — **local** (no lit-triggers dependency):

```bash
npm run poll              # polls every POLL_INTERVAL_SEC until triggered
```

or **armed** (hosted cron):

```bash
npm run arm               # browser opens — click "Authorize agent"
# ...watch runs via the printed curl, or the lit-triggers dashboard
npm run disarm            # after it fires (or to stand down)
```

To see it fire immediately and safely: set `STOP_PRICE` *above* the current
price (instant trigger) with `DRY_RUN=true`, and watch the action return
`wouldPlace` without trading. Then aim it for real: realistic `STOP_PRICE`,
`DRY_RUN=false`.

## Quota discipline

Untriggered tick: **1 fetch** (the ticker) of the 50-fetch action quota — an
every-minute poller is cheap. Triggered path worst case: ticker + open-orders
+ market + balances + create-order = **5** (≤4 on hyperliquid, which skips the
balance clamp but adds an internal mid-price fetch; pass a pre-fetched
`market` in params to shave one more — the markets-cache injection pattern).
This example's self-imposed ceiling is 10.

## Hyperliquid option

`VENUE_ID=hyperliquid` turns the stop into a perp position-protector: the sell
is `reduceOnly`, sized to `szDecimals`, signed EIP-712. With
`VENUE_USE_ACTION_KEY=true` the signer is the action's own CID-bound TEE key —
have the master account `approveAgent` it once; agents structurally cannot
withdraw, so even this armed stop could never move funds off-venue.

## Production notes

- **Credentials:** `arm.js` stores `default_params` (including venue keys)
  with the lit-triggers service — fine for testnet, loudly warned otherwise.
  Production keys belong in sealed `venue-credentials-v1` ciphertext,
  decrypted in-TEE with `Lit.Actions.Decrypt` (see the lit-venues README).
- **A stop is one-shot.** After `sold: true`, disarm. The fences make extra
  fires safe-by-construction (and a re-fire between fill and balance
  settlement can at most sell residue within `maxAmount`), but they'll keep
  appearing as runs.
- **Shorts:** this action protects a long (`last <= stopPrice` → sell).
  Invert the comparison and side for a short stop; mirror the floor (a
  ceiling) when you do.
- **Liveness:** a 1-minute cron can miss a fast wick — inherent to polling.
  Tighten the cron (≥30s floor on lit-triggers) or accept it; the floor price
  decides what happens if price gaps through the stop.
