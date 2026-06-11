# Attested Multi-Venue Portfolio Snapshot

**One Lit Action reads balances across Binance, Coinbase, and Hyperliquid and
merges them into a single snapshot with exact per-asset totals. The venue API
keys are sealed against a vault PKP and decrypted only inside the TEE, only by
this exact code — the machine that runs the snapshot never holds a key.**

Part of `plans/ccxt-venue-layer-and-email-approval.md` (M4 example 1); doubles
as a certified Flows Wallet template.

## The idea

"Read-only" exchange keys still leak: a key that can list balances can profile
a treasury or front-run a rebalance, and parking it on a portfolio server just
moves the problem. Here the keys exist in plaintext in exactly one place — the
TEE, for the duration of one action run. `setup.js` seals everything you
configure into a single `venue-credentials-v1` ciphertext:

```javascript
// setup (once): seal in the TEE, store only ciphertext
ciphertext = await Lit.Actions.Encrypt({ pkpId, message: JSON.stringify(creds) });

// runtime (every snapshot): decrypt in the TEE, fetch, return only balances
creds = JSON.parse(await Lit.Actions.Decrypt({ pkpId, ciphertext }));
```

The permission group pins the **exact CID** of the snapshot code (lit-venues
bundle + action, hashed together) — no wildcard. The scoped usage key can run
that audited code and nothing else, so nothing else can ever decrypt the
credentials. Each venue is optional, one fetch each, and a venue failing
(geo-block, revoked key, downtime) reports a typed `VenueError` in its slot
without killing the rest. Hyperliquid needs no credential at all — perp
account reads take just an address. Totals are summed per asset with
`LitVenues.addDec` (exact decimal strings, no float drift).

## Files

| Path | Purpose |
| --- | --- |
| `action/portfolio-snapshot.js` | The Lit Action. Decrypts the sealed `venue-credentials-v1` blob, fetches balances from each configured venue via the `LitVenues` global, merges totals. |
| `scripts/_lit.js` | Concatenates `lit-venues/dist/lit-venues.iife.js` + the action (the exact bytes the pinned CID covers) and runs code against `/lit_action`. |
| `scripts/_env.js` | Minimal `.env` reader / upserter, inlined so the folder is self-contained. |
| `scripts/setup.js` | One-shot: compute CIDs, create the vault PKP, pin the CIDs in a group, mint a scoped usage key, seal the credentials in-TEE. |
| `scripts/snapshot.js` | Take a snapshot and pretty-print it (`-- --json` for raw output). |

## Walkthrough

### 0. Build lit-venues (prerequisite)

```bash
cd ../../lit-venues && npm install && npm run build && cd -
```

Scripts read `../../lit-venues/dist/lit-venues.iife.js` and fail with these
instructions if it is missing.

### 1. Install + configure

```bash
cp .env.example .env
npm install
```

Set in `.env`:

- `LIT_API_KEY` — your account-level (master) API key from the
  [dashboard](https://dashboard.chipotle.litprotocol.com); the account must be
  funded. The venue layer is rolling out per the plan — if your environment
  lacks it, point `LIT_API_BASE` at one that has it (dev/staging).
- At least one venue: `BINANCE_API_KEY`+`BINANCE_API_SECRET` (read-only;
  `BINANCE_SANDBOX=true` for the spot testnet), `COINBASE_API_KEY`+
  `COINBASE_API_SECRET` (read-only CDP key; PEM on one line with `\n`), or
  `HYPERLIQUID_ACCOUNT_ADDRESS` (no key — reads are public).
- `EGRESS_PROXY_URL` if you use binance.com: it geo-blocks US egress (451),
  and the proxy routes venue calls through `Lit.Actions.proxiedFetch` in-TEE.

### 2. Run setup

```bash
npm run setup
```

Five steps: compute the two CIDs (snapshot code + seal helper), create the
vault PKP, pin both CIDs in a fresh group, mint a scoped usage key, and seal
the credentials by running the helper inside the TEE. The first action call
polls with retries while the new key's group grant propagates.

Afterwards the runtime path needs only the usage key and the ciphertext — you
can delete the plaintext key lines from `.env` (re-sealing needs them again).

### 3. Take a snapshot

```bash
npm run snapshot
```

```
Portfolio snapshot @ 2026-06-11T17:21:08.000Z

VENUE         ASSET   FREE                TOTAL
binance       USDT    1043.20             1243.20
coinbase      USDT    250.00              250.00
hyperliquid   USDC    512.77              512.77

TOTAL         USDT    1493.2
TOTAL         USDC    512.77
```

## Notes

- **Quota discipline.** One outbound fetch per configured venue — at most 3
  per run (`Decrypt` is a TEE op, not a fetch), well inside the runtime's
  50-fetch cap and this repo's ≤10-per-action budget for examples.
- **What sealing does and doesn't buy.** Your machine sees the plaintext keys
  once, at seal time. What sealing removes is the *standing* exposure:
  snapshot runners, cron boxes, CI — anything holding the usage key +
  ciphertext — can produce snapshots but can never extract keys.
- **CID pinning cuts both ways.** Editing the action or rebuilding the bundle
  changes the CID; re-run `npm run setup` (fresh PKP, key, and seal).
- **Venue honesty.** Coinbase Advanced Trade has no sandbox — reads hit live.
  binance.com from US egress needs the proxy (or use the spot testnet).
  Hyperliquid balances are its USDC perp margin account.
- **"Attested" in v0** means: produced inside the TEE by code whose hash is
  pinned. The returned JSON is not yet signed; to make it independently
  verifiable, sign the snapshot with `Lit.Actions.getLitActionPrivateKey()` —
  the CID-bound identity key (see [`solana-signer`](../solana-signer)).
