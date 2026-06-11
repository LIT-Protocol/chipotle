# @lit-protocol/lit-venues

Native exchange venue connectors for Lit Actions. Binance (+ binance.us) and Coinbase Advanced Trade behind one small, auditable, ccxt-shaped interface. Part of `plans/ccxt-venue-layer-and-email-approval.md` (D1).

Design constraints, by construction:

- **Action-runtime portable.** Only `fetch` + plain JS — no Node built-ins, no `Buffer`, no WebCrypto assumptions. Request signing (HMAC-SHA256, Ed25519, ES256 JWT) uses bundled `@noble/hashes` + `@noble/curves`.
- **Quota-aware.** Single-symbol market lookups (`fetchMarket`), no full-market loads, small responses — designed for the 50-outbound-fetch / 1MB-response action limits.
- **Exact decimals.** Amounts/prices are decimal strings end to end; `addDec` / `roundDownToIncrement` do scaled-BigInt math.
- **Egress-ready.** A `proxy` config is forwarded to the runtime fetch as `litProxy` (plan D4/M2); inert elsewhere.

## Usage inside a Lit Action (inline bundle, v0)

```js
// dist/lit-venues.iife.js pasted/concatenated above this line → global `LitVenues`
async function main({ sealedCreds }) {
  const creds = JSON.parse(await Lit.Actions.Decrypt(sealedCreds)); // venue-credentials-v1
  const binance = LitVenues.createVenue({
    venueId: 'binance',
    sandbox: true, // spot testnet
    credentials: { apiKey: creds.apiKey, secret: creds.secret, keyType: creds.keyType },
    proxy: creds.egress?.proxyUrl,
  });
  const balances = await binance.fetchBalances();
  return { balances };
}
```

Once published, the ESM build (`dist/lit-venues.mjs`) becomes importable via the integrity-pinned jsDelivr import path instead of inlining.

## Commands

```sh
npm install
npm test         # vitest — signing verified against Binance docs / RFC 8032 vectors
npm run build    # esbuild → dist/lit-venues.iife.js (+ .mjs), prints size + sha384
npm run typecheck
```

The M0 spike (`e2e/tests/api/lit-venues-spike.spec.ts`) executes the IIFE bundle inside a real Lit Action and fetches public tickers — including a Binance-testnet probe that doubles as the egress-geography measurement (HTTP 451 ⇒ US egress, route via proxy per plan D4).

## Surface

`createVenue({ venueId, credentials?, sandbox?, proxy?, fetchImpl?, nowMs? })` → `VenueClient`:
`fetchTicker`, `fetchMarket`, `fetchBalances`, `createOrder`, `cancelOrder`, `fetchOpenOrders`, `fetchMyTrades`.

Errors are `VenueError` with a unified taxonomy: `auth`, `insufficient_funds`, `bad_symbol`, `rate_limited`, `venue_unavailable`, `invalid_request`, `unknown` (plus `httpStatus` / raw `venueCode`).

Venue notes:

- **binance** — `sandbox: true` → `testnet.binance.vision`. HMAC by default; set `keyType: 'ed25519'` for Binance's recommended self-generated keys (PKCS8 PEM, hex, or base64 accepted). binance.com geo-blocks US egress (451) — the error message says so explicitly.
- **binanceus** — separate venue id, no testnet.
- **coinbase** — Advanced Trade with CDP keys (ES256 JWT; Coinbase does not support Ed25519 here). No sandbox exists; `sandbox: true` throws rather than pretending. Market BUY orders take `quoteAmount` (quote-asset size) per Advanced Trade semantics.

Withdrawal endpoints are deliberately absent (plan: policy-gated sweeps go through the email-approval primitive).
