# @lit-protocol/lit-venues

Native exchange venue connectors for Lit Actions. Binance (+ binance.us), Coinbase Advanced Trade, and Hyperliquid perps behind one small, auditable, ccxt-shaped interface. Part of `plans/ccxt-venue-layer-and-email-approval.md` (D1, D8).

Design constraints, by construction:

- **Action-runtime portable.** Only `fetch` + plain JS — no Node built-ins, no `Buffer`, no WebCrypto assumptions. Request signing (HMAC-SHA256, Ed25519, ES256 JWT, EIP-712/secp256k1) uses bundled `@noble/hashes` + `@noble/curves`.
- **Quota-aware.** Single-symbol market lookups (`fetchMarket`), no full-market loads, small responses — designed for the 50-outbound-fetch / 1MB-response action limits. Pre-fetched rules can be injected via `markets` to spend zero fetches.
- **Exact decimals.** Amounts/prices are decimal strings end to end; `addDec` / `subDec` / `roundDownToIncrement` do scaled-BigInt math. Hyperliquid's signed action hash commits to `wireDecimal`-normalized strings — no float ever touches an order.
- **Egress-ready.** A `proxy` config routes through the in-TEE `Lit.Actions.proxiedFetch` op (plan D4/M2); inert elsewhere.

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

PKP-native venues need no sealed credential at all — the signing key is the action-bound TEE key:

```js
async function main() {
  const hl = LitVenues.createVenue({
    venueId: 'hyperliquid',
    credentials: {
      keyType: 'pkp-eip712',
      privateKey: await Lit.Actions.getLitActionPrivateKey(), // never leaves the TEE
      accountAddress: '0xMASTER…', // agent mode: PKP trades for the user's master account
    },
  });
  return { positions: await hl.fetchPositions() };
}
```

Once published, the ESM build (`dist/lit-venues.mjs`) becomes importable via the integrity-pinned jsDelivr import path instead of inlining.

## Commands

```sh
npm install
npm test         # vitest — signing pinned to Binance docs / RFC 8032 / Hyperliquid SDK vectors
npm run build    # esbuild → dist/lit-venues.iife.js (+ .mjs), prints size + sha384
npm run typecheck
node scripts/verify-live.mjs   # live conformance against REAL exchange APIs (public always; authed when keys present)
```

The M0 spike (`e2e/tests/api/lit-venues-spike.spec.ts`) executes the IIFE bundle inside a real Lit Action and fetches public tickers — including a Binance-testnet probe that doubles as the egress-geography measurement (HTTP 451 ⇒ US egress, route via proxy per plan D4).

## Surface

`createVenue({ venueId, credentials?, sandbox?, proxy?, fetchImpl?, nowMs?, markets?, signFn?, slippageBps?, builder? })` → `VenueClient`:
`fetchTicker`, `fetchMarket`, `fetchBalances`, `createOrder`, `cancelOrder`, `fetchOpenOrders`, `fetchMyTrades`,
plus the optional perp surface (plan D8): `fetchPositions`, `setLeverage`, `fetchFundingRate`.

Errors are `VenueError` with a unified taxonomy: `auth`, `insufficient_funds`, `bad_symbol`, `rate_limited`, `venue_unavailable`, `invalid_request`, `unknown` (plus `httpStatus` / raw `venueCode`).

Venue notes:

- **binance** — `sandbox: true` → `testnet.binance.vision`. HMAC by default; set `keyType: 'ed25519'` for Binance's recommended self-generated keys (PKCS8 PEM, hex, or base64 accepted). binance.com geo-blocks US egress (451) — the error message says so explicitly.
- **binanceus** — separate venue id, no testnet.
- **coinbase** — Advanced Trade with CDP keys (ES256 JWT; Coinbase does not support Ed25519 here). PEMs are accepted with real newlines OR the literal `\n` sequences of the CDP JSON download. No sandbox exists; `sandbox: true` throws rather than pretending. Market BUY orders take `quoteAmount` (quote-asset size) per Advanced Trade semantics.
- **hyperliquid** — PKP-native perps (plan D8): no API key; every trading action is an EIP-712 signature (msgpack action hash → phantom agent, chainId 1337), pinned byte-for-byte to the official SDK's test vectors in `test/hyperliquid-signing.test.ts`. `sandbox: true` → `api.hyperliquid-testnet.xyz` (full lifecycle testable). Reads work with `accountAddress` alone; trading needs `privateKey` (or a custom `signFn`). When the key is a registered agent (API wallet), reads auto-resolve its MASTER via the venue's `userRole` lookup (cached) — agent accounts themselves are always empty. `approveAgent()` lets a master key grant the PKP trade-only powers — agents structurally cannot withdraw, transfer, or even move USDC between spot and perp (deposits land in SPOT; the master must transfer spot→perp before perp margin exists). Balances/positions read the PERP clearinghouse (spot is a plan non-goal). Market orders are aggressive IOC limits (`slippageBps`, default 5%); prices obey the 5-sig-fig and `MAX_DECIMALS − szDecimals` rules (integers always valid); sizes are quantized to `szDecimals`. Optional `builder` attaches a builder-code fee to orders.

Withdrawal endpoints are deliberately absent (plan: policy-gated sweeps go through the email-approval primitive).
