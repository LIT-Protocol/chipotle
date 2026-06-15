# Venue connectors — agent handoff

A short, self-contained brief for an agent (or human) who wants to **use** the
venue connectors from PR #483 to build actions/flows. Read this first, then
`lit-venues/README.md` for the full surface and per-venue caveats.

## Mental model (read this before anything else)

`lit-venues` is **not a Lit Action and not a runtime op.** It is a plain
TypeScript library that bundles to an IIFE (`dist/lit-venues.iife.js`) which you
**concatenate ahead of your action code**, exposing a `LitVenues` global that
runs as ordinary JS inside the action sandbox. Nothing about the connectors is
privileged.

What *is* privileged — the two TEE-side runtime primitives this PR adds, which
only exist where the venue layer is deployed:

1. **`Lit.Actions.proxiedFetch`** (`op_lit_proxied_fetch`) — in-TEE egress
   through an authenticated proxy. Needed for geo-blocked venues (binance.com
   returns 451 from US egress). Inert/unnecessary for venues reachable directly
   (Coinbase, Hyperliquid, Binance spot testnet).
2. **Email approval** (`Lit.Actions.sendEmail` / `requestEmailApproval` /
   `checkEmailApproval`) — attestation verified **in-TEE, fail-closed**. Used by
   the policy-gated sweep example.

Consequence for handoff: **these primitives are not in prod yet.** Point
`LIT_API_BASE` at an environment that has the venue layer (dev/staging) — see
the PR's "Remaining (environment, not code)" section. A plain prod key will not
have `proxiedFetch` or the approval ops.

## Prerequisites

The library is **not published to npm yet** (`@lit-protocol/lit-venues`, v0 is
inline-bundle only). Build it locally before running any example:

```bash
cd lit-venues
npm install
npm run build      # esbuild → dist/lit-venues.iife.js (+ .mjs), prints size + sha384
npm test           # optional: 58 unit tests, signing pinned to official vectors
```

Every example's `scripts/_lit.js` reads `../../lit-venues/dist/lit-venues.iife.js`
and fails with these instructions if the bundle is missing. The pinned action
CID covers **bundle + action bytes hashed together** — rebuilding the bundle or
editing the action changes the CID, so you must re-run that example's
`npm run setup` (fresh PKP, group, usage key, and re-seal).

## Where to start — pick the example closest to your goal

| Example | What it demonstrates | Start here if you want… |
| --- | --- | --- |
| `venue-portfolio-read` | Read balances across all 3 venues, sealed `venue-credentials-v1`, exact-decimal merge | the canonical read-only template (simplest) |
| `funding-rate-monitor` | Cross-venue funding-rate reads | a read-only monitor / data feed |
| `venue-twap-order` | D7 chained ticks + markets-cache (zero-quota market rules) | multi-tick / stateful order logic |
| `price-trigger-stop` | `lit-triggers`-armed conditional execution | event/price-driven actions |
| `cex-sweep-with-email-approval` | D6 two-phase human approval (in-TEE attestation) | a flow needing human-in-the-loop approval |
| `hl-agent-perp-policy` | **PKP-native** Hyperliquid perps — no API key, key is the action-bound TEE key | PKP-native trading / agent-wallet patterns |

Each example folder is self-contained: `README.md` (full walkthrough),
`.env.example`, `action/` (the Lit Action), and `scripts/` (setup + run).

## Minimal usage shape

```js
// dist/lit-venues.iife.js concatenated above → global `LitVenues`
const venue = LitVenues.createVenue({
  venueId: 'binance',                 // 'binance' | 'binanceus' | 'coinbase' | 'hyperliquid'
  sandbox: true,                       // binance/hyperliquid testnet; coinbase throws (no sandbox)
  credentials: { apiKey, secret, keyType },  // omit for hyperliquid reads (address only)
  proxy: creds.egress?.proxyUrl,       // routes via Lit.Actions.proxiedFetch; inert elsewhere
});
const balances = await venue.fetchBalances();
```

Surface: `fetchTicker`, `fetchMarket`, `fetchBalances`, `createOrder`,
`cancelOrder`, `fetchOpenOrders`, `fetchMyTrades`, plus perps
(`fetchPositions`, `setLeverage`, `fetchFundingRate`). Errors are typed
`VenueError` (`auth` / `insufficient_funds` / `bad_symbol` / `rate_limited` /
`venue_unavailable` / `invalid_request` / `unknown`, with `httpStatus` /
`venueCode`). Amounts/prices are **decimal strings** end to end — use
`LitVenues.addDec` / `subDec` / `roundDownToIncrement`, never floats.

## Per-venue gotchas (full detail in README.md)

- **binance** — binance.com geo-blocks US egress (451); use the proxy or
  `sandbox: true` (spot testnet). `keyType: 'ed25519'` for self-generated keys.
- **binanceus** — separate `venueId`, no testnet.
- **coinbase** — CDP keys, ES256-JWT only (no Ed25519). **No sandbox** —
  `sandbox: true` throws. Market BUY takes `quoteAmount`.
- **hyperliquid** — PKP-native, no API key; every trade is an EIP-712 signature
  pinned to the official SDK vectors. Reads need only `accountAddress`; trading
  needs `privateKey`. Agents structurally cannot withdraw. `fetchBalances`
  merges perp + spot USDC.

## Quota discipline

Actions cap at 50 outbound fetches / 1 MB response; this repo's examples target
≤10 fetches. Prefer single-symbol `fetchMarket` over full-market loads, and
inject pre-fetched rules via `markets` to spend zero fetches (see
`venue-twap-order`).

## Further reading

- `lit-venues/README.md` — full surface + signing/decimal/egress design notes.
- `plans/ccxt-venue-layer-and-email-approval.md` — design of record (D1/D4/D6/D8, M0–M4).
- `docs/runbooks/venue-proxy-rotation.md` — egress proxy ops.
- `k6/correctness/venues-*.spec.ts` — public conformance gates (always-on);
  authenticated lifecycles are env-gated on testnet/CDP keys.
