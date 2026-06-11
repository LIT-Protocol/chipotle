# Cross-Venue Funding-Rate Monitor

**A Lit Action reads Hyperliquid perp funding and Coinbase spot for a list of
coins, annualizes the funding rate, and emails you a plain-text table when it
crosses a threshold — zero credentials anywhere, and the email comes from the
runtime's server-mediated `sendEmail` op, not from code that could spoof it.**

Part of `plans/ccxt-venue-layer-and-email-approval.md` (M4 example 5); doubles
as a certified Flows Wallet template.

## The idea

Perp funding is what longs pay shorts (or vice versa) to keep the perp pinned
to spot. Hyperliquid pays it **hourly**, so a small-looking rate compounds:

```
annualized % = hourly rate × 24 × 365 × 100      0.00005/hr ≈ 43.8% a year
```

Sustained extremes are exactly the thing you want a nudge about — carry you
are paying, or carry you could be earning (short perp + hold spot). Per coin
the action takes one Hyperliquid `fetchFundingRate` (hourly rate + mark) and
one Coinbase `fetchTicker COIN/USD` as the spot/basis reference. If any
coin's |annualized funding| exceeds `thresholdPct`, one alert goes out via:

```javascript
await Lit.Actions.sendEmail({ to, subject, text }); // -> { accepted: true }
```

`sendEmail` is deliberately narrow (plan D6): fixed from-domain, plain text
only, per-account quota. Because an action that can email is an action that
can burn that quota, setup pins the monitor's **exact CID** in its group — the
scoped usage key cannot execute anything else.

Both data legs are public. There is no API key, no PKP, and nothing to seal.

## Files

| Path | Purpose |
| --- | --- |
| `action/funding-rate-monitor.js` | The Lit Action. Fetches funding + spot per coin via the `LitVenues` global, annualizes, computes basis, emails the table beyond the threshold. Returns `{ rows, alerted }`. |
| `scripts/_lit.js` | Concatenates `lit-venues/dist/lit-venues.iife.js` + the action (the exact bytes the pinned CID covers) and runs code against `/lit_action`. |
| `scripts/_env.js` | Minimal `.env` reader / upserter, inlined so the folder is self-contained. |
| `scripts/setup.js` | One-shot: compute the CID, pin it in a fresh group, mint a scoped usage key, smoke-run with an unreachable threshold (no email possible). |
| `scripts/monitor.js` | One monitoring pass; prints the table and whether an alert was sent (`-- --json` for raw output). |

## Walkthrough

### 0. Build lit-venues (prerequisite)

```bash
cd ../../lit-venues && npm install && npm run build && cd -
```

Scripts read `../../lit-venues/dist/lit-venues.iife.js` and fail with build
instructions if it is missing.

### 1. Install + configure

```bash
cp .env.example .env
npm install
```

Set in `.env`:

- `LIT_API_KEY` — your account-level (master) API key from the
  [dashboard](https://dashboard.chipotle.litprotocol.com). The account must be
  funded. The `sendEmail` op and venue layer are rolling out per the plan — if
  your environment doesn't have them yet, point `LIT_API_BASE` at one that
  does (dev/staging) and fund an account there.
- `ALERT_EMAIL` — where alerts go (leave empty for report-only mode).
- Optionally `COINS` (default `BTC,ETH`) and `THRESHOLD_PCT` (default `20`).

### 2. Run setup

```bash
npm run setup
```

Four steps: compute the CID over bundle+action, pin it in a fresh group, mint
a scoped usage key, and smoke-run the monitor with an unreachable threshold
and no recipient — proving the pipeline without any chance of an email. The
first call polls with retries while the new key's grant propagates.

### 3. Run a pass

```bash
npm run monitor
```

```
Funding monitor @ 2026-06-11T17:30:02.000Z  (threshold 20%)

COIN    FUNDING/HR   ANNUALIZED%  HL MARK     CB SPOT     BASIS%
BTC     0.0000125    10.95        104250      104301.5    -0.05
ETH*    0.0000391    34.25        2501.3      2502.1      -0.03

* beyond threshold — alert email sent to you@example.com
```

To see the alert path fire, set `THRESHOLD_PCT=1` and run again — funding is
almost never under 1% annualized in both coins at once.

## Run it on a schedule

This repo's example runs one pass per `npm run monitor`. For a standing
monitor, put the same action on a [lit-triggers](../lit-triggers) `schedule`
trigger (the D7 cron pattern — `uptime-insurance` there shows the shape): the
trigger invokes the pinned action on an interval and nothing else changes.
Budget: 2 fetches per coin per tick, plus at most one email per tick.

## Notes

- **Quota discipline.** Coins are capped at 4 inside the action → ≤ 8 outbound
  fetches per run (runtime cap is 50; this repo budgets ≤10 per example).
- **Float math is fine here** — the action displays rates and never places an
  order. lit-venues keeps all order paths in exact decimal strings.
- **Egress honesty.** Coinbase public data works from US egress (proven in the
  M0 spike). Hyperliquid restricts some egress regions: on a
  `venue_unavailable` geo error, set `HL_SANDBOX=true` (testnet has funding
  too) or route via an egress proxy (plan D4, see `lit-venues/README.md`).
- **Basis is indicative.** Perp mark (USDC-margined) vs Coinbase USD spot,
  sampled seconds apart — good for a monitor, not an arb engine.
- **CID pinning cuts both ways.** Editing the action or rebuilding the bundle
  changes the CID; re-run `npm run setup`.
