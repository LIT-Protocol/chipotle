# lit-payments

Ops-facing billing service. Magic-link auth + admin credit portal + LITKEY
payment gateway + **auto top-up**. Deployed outside the TEE, on Railway.

See `plans/lit-payments-app.md` for the original design and
`plans/auto-top-up.md` for the auto top-up feature design.

## Auto top-up — concise architecture

Users opt in to "auto-recharge" via the dashboard. When their Stripe customer
balance drops below a configured threshold, we charge their saved card off-session
for a configured amount, subject to a configured monthly cap.

### Components

| Component | Role | New for auto top-up? |
|---|---|---|
| **Dashboard** (`lit-static/dapps/dashboard`) | Auto-top-up modal, save-card flow, status banners, SCA recovery page | new UI |
| **lit-api-server** (TEE) | Existing Lit Action server. Deducts credits via `balance_transactions` (unchanged). | one new endpoint: `POST /internal/invalidate_balance_cache` |
| **lit-payments** (Railway, this service) | Hosts all auto-top-up logic: dashboard endpoints, the `customer.updated` webhook handler (sync charge + sync credit), the reconciler cron | new endpoints + new tables |
| **Postgres** (inside lit-payments) | Existing DB. Two new tables: `auto_topup_config`, `auto_topup_credits`. | new tables |
| **Stripe** | Customer, PaymentMethod, PaymentIntent, `customer.balance` ledger | new webhook subscription: `customer.updated` |
| **Shared auth crate** (`lit-billing-auth/`) | Wallet-sig + API-key validation extracted from `lit-api-server::billing_auth` | new crate |

### Where data lives

| Data | Location |
|---|---|
| Config (threshold, top-up amount, monthly cap, payment_method_id, consent, SCA pending state) | Postgres `auto_topup_config` (1 row per customer) |
| Card data | Stripe (PaymentMethod attached to Customer); we only store the `pm_xxx` reference |
| User credit ledger | Stripe `customer.balance` (negative = credit) — unchanged from before |
| Wallet ↔ Stripe Customer mapping | Stripe customer metadata (`wallet_address`) — unchanged |
| Charge history | Stripe PaymentIntents (filtered by `metadata.source=auto_topup`) |
| Credit dedup | Postgres `auto_topup_credits` (1 row per credited PaymentIntent) |

### Component interaction

```
USER ─► DASHBOARD ─► lit-payments        (save card, save config, SCA recovery)
                       │
                       ▼
                   Postgres auto_topup_config

CLIENT ─► lit-api-server (deduct credits — existing flow, unchanged)
                       │
                       ▼
                   Stripe balance_transactions write → customer.balance drops
                       │
                       ▼
                   Stripe fires customer.updated webhook
                       │
                       ▼
                   lit-payments POST /stripe/webhook
                       │  verify HMAC, mutex, fresh balance fetch,
                       │  list PIs (failure derivation + cap check)
                       │
                       ▼
                   Stripe paymentIntents.create (off_session, confirm) — synchronous
                       │
              ┌────────┴────────┐
              ▼ succeeded       ▼ authentication_required, declined, timeout
       INSERT auto_topup_credits   set pending state / email + banner / log
              │
              ▼
       Stripe balance_transactions (credit, Idempotency-Key: credit:{pi.id})
              │
              ▼
       UPDATE auto_topup_credits SET stripe_balance_transaction_id
              │
              ▼
       Fire-and-forget POST lit-api-server /internal/invalidate_balance_cache
              │
              ▼
       release mutex, return 200

Every 15 min: reconciler cron
   reads Postgres auto_topup_config + auto_topup_credits
   lists recent succeeded auto-topup PIs from Stripe
   for any PI missing a credit row or with NULL balance_transaction_id → completes the credit idempotently
```

### Left-to-right runtime flow

```
CLIENT  ─►  lit-api-server  ─►  Stripe                ─►  Stripe fires  ─►  lit-payments         ─►  Stripe                  ─►  Postgres                ─►  Stripe                  ─►  Postgres                ─►  lit-api-server
 run        deduct credits      balance_transactions     customer.updated   /stripe/webhook           paymentIntents.create       INSERT auto_topup_credits   balance_transactions         UPDATE row                 POST /internal/
 action     (existing)          write                    webhook            (verify HMAC, mutex,      (off_session, confirm)      ON CONFLICT DO NOTHING      Idempotency-Key:             SET stripe_balance_         invalidate_balance_cache
                                                                            balance fetch, list PIs,                                                          credit:{pi.id}               transaction_id              (fire-and-forget)
                                                                            cap check, failure
                                                                            derivation)
```

### Three layers of dedup defense

| Layer | Prevents | Mechanism |
|---|---|---|
| 1. Per-customer Tokio mutex (`moka` TTL cache) in lit-payments | Wasted parallel processing | In-process |
| 2. Postgres unique constraint on `auto_topup_credits.payment_intent_id` | Double-credit (sync + reconciler races, webhook replays) | `INSERT … ON CONFLICT DO NOTHING` |
| 3. Stripe Idempotency-Key on credit writes | Double-credit under transient retry within Stripe's 24h cache | `credit:{pi.id}` |

### Key invariants

- **Charge AND credit are synchronous** in the same `customer.updated` handler.
- **One webhook only**: `customer.updated`. No `payment_intent.succeeded`, no `payment_intent.payment_failed`.
- **Reconciler cron** (15-min default) is the recovery path for HTTP timeouts and partial writes.
- **SCA recovery** is a separate user-driven flow: email link → on-session `stripe.confirmCardPayment` → 3DS challenge → success → credit via the same sync path.
- **lit-api-server unchanged** except for the cache-invalidation endpoint.
- **Dashboard talks only to lit-payments**, using the same wallet-sig + API-key auth via the extracted shared crate.

## Routes

Public:
- `GET /login` — login page (form posts to `/auth/request`).
- `GET /payWithLitkey?wallet=0x…` — end-user Base mainnet LITKEY payment page for an existing Stripe customer wallet.
- `POST /auth/request` — send magic link (rate-limited per email, 60s cooldown).
- `GET /auth/verify?token=…` — validate token, set session cookie, redirect.
- `GET /api/customer/preview?wallet=0x…` — wallet-scoped customer identity preview for the LITKEY payment page. Returns only `found`, `email`, and `wallet_address`; it does not expose Stripe customer ids or balances.
- `GET /api/litkey/quote` — public LITKEY quote for the end-user payment page; includes `crediting_paused` and omits an effective credit rate while paused.
- `GET /api/litkey/payment-config` — public Base mainnet payment config: chain id, LITKEY token address, and payment gateway address. Fails closed with `503` if chain verification is not configured.
- `POST /api/litkey/payment-claim` — wallet-scoped transaction claim for the browser payment page. Accepts `{ "tx_hash": "0x…", "wallet": "0x…" }`, fetches the transaction receipt, verifies the configured gateway emitted the expected `Payment` event for that wallet, and applies credit idempotently from that receipt.

Authenticated (operator session cookie required):
- `GET /` — admin dashboard.
- `GET /api/me` — current operator profile.
- `POST /auth/logout` — delete session.
- `GET /api/customer/lookup?email=…` or `?wallet=…` — preview a Stripe customer + balance.
- `POST /api/grant` — apply a credit (subject to caps + UUID idempotency key).
- `GET /api/grants?limit=N` — recent grants by the calling operator.
- `GET /api/litkey/rate` — current LITKEY market rate plus discount-adjusted credit rate for operators. Rates are returned as decimal strings in `usd_wei_per_litkey` / `effective_usd_wei_per_litkey`, where `1 USD = 1e18` units.
- `POST /api/litkey/rate/override` — admin-only manual market-rate override. Body: `{ "usd_wei_per_litkey": "6000000000000000" }` for `$0.006/LITKEY`.

## LITKEY rate precision

LITKEY pricing is intentionally stored as 18-decimal USD fixed point instead of whole cents because LITKEY can trade below one cent. Example: `$0.006/LITKEY` is stored as `6000000000000000` (`0.006 * 1e18`). The settlement helper keeps the on-chain `Payment.amount` in native LITKEY wei, multiplies by `usd_wei_per_litkey` with `num-bigint`, applies `LITKEY_DISCOUNT_BASIS_POINTS`, and rounds only once at the final Stripe-cent boundary.

## Local development

You need: Rust toolchain (per `../lit-api-server/rust-toolchain.toml` →
1.91), Postgres, a Resend account, and a free port.

### 1. Start Postgres

```sh
docker run --rm -d --name lit-payments-pg \
  -e POSTGRES_PASSWORD=postgres \
  -p 5432:5432 \
  postgres:16
```

### 2. Set env vars

Copy this into a `.env` and adjust:

```sh
DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres
MAGIC_LINK_SIGNING_KEY=$(openssl rand -base64 32)
RESEND_API_KEY=re_...                    # https://resend.com → API keys
MAIL_FROM=noreply@mail.litprotocol.com   # must be a verified Resend sender
PUBLIC_BASE_URL=http://localhost:8001
STRIPE_SECRET_KEY=rk_test_...            # Stripe restricted key; needs Customers: Read plus Billing > Customer Balance Transaction: Write
ROCKET_SECRET_KEY=$(openssl rand -base64 32)  # required for private cookies in release
# Port 8001 in dev to avoid colliding with lit-api-server on 8000.
# The dashboard expects lit-payments at this port (see auth.js).
ROCKET_PORT=8001
# CORS allowlist (in addition to PUBLIC_BASE_URL). Comma-separated.
# Add the dashboard origin here when the dashboard runs on a different
# port — typical local dev is the same host:port as the api-server
# (http://localhost:8000) since both static apps are served from there.
# CORS_ALLOWED_ORIGINS=http://localhost:8000

# Optional — cap defaults match the plan ($20 per grant, $100/op/day):
# MAX_GRANT_CENTS=2000
# MAX_DAILY_PER_OPERATOR_CENTS=10000

# Optional — incentive discount for paying with LITKEY, in basis points.
# 0 = no discount. 2000 = 20% off vs credit card, so users receive
# 1 / (1 - 0.20) = 1.25x the market-rate Stripe credit per LITKEY.
# LITKEY_DISCOUNT_BASIS_POINTS=0

# Optional — enable LITKEY browser payment verification.
# ALCHEMY_HTTPS_URL=https://base-mainnet.g.alchemy.com/v2/...
# LITKEY_GATEWAY_ADDRESS=0xa2d54cd1D1dF1735718A857aC49CaF9ECaB0093b
# LITKEY_CHAIN_ID=8453

# AccountConfig chain — used by the BillingAuth Rocket guard to verify
# wallet signatures (EIP-712 chain_id pinning) and to resolve API keys to
# their on-chain billing wallet. Same chain lit-api-server reads from
# NodeConfig.toml; lit-payments now runs the lookup in-process via
# OnChainBillingResolver instead of an internal HTTP hop. The RPC URL
# is normally https://yellowstone-rpc.litprotocol.com; the contract
# address must match lit-api-server's NodeConfig.toml.
LIT_ACCOUNTS_RPC_URL=https://yellowstone-rpc.litprotocol.com
LIT_ACCOUNTS_CHAIN_ID=175188
LIT_ACCOUNTS_CONTRACT_ADDRESS=0x...  # same value as lit-api-server NodeConfig.toml

# Existing — still required, but now only used for the post-credit
# cache-invalidation callback to lit-api-server (no longer carries
# wallet-sig or API-key verification).
# LIT_API_SERVER_BASE_URL=http://localhost:8000
# LIT_INTERNAL_SHARED_SECRET=$(openssl rand -base64 32)

# Optional — gas funder (see "Gas funder" section below). Off entirely
# unless GAS_FUNDER_PRIVATE_KEY is set. Leave GAS_FUNDER_ENABLED unset to
# run in OBSERVE mode (alerts only, no on-chain sends).
# GAS_FUNDER_PRIVATE_KEY=0x...           # dedicated low-value hot wallet
# GAS_FUNDER_ALERT_EMAIL=you@litprotocol.com
# GAS_FUNDER_RPC_URL=https://base-mainnet.g.alchemy.com/v2/...   # falls back to ALCHEMY_HTTPS_URL
# GAS_FUNDER_LOW_WATER_WEI=500000000000000      # 0.0005 ETH — top up below this
# GAS_FUNDER_HIGH_WATER_WEI=5000000000000000    # 0.005  ETH — top up to this
# GAS_FUNDER_MAX_TX_WEI=5000000000000000        # 0.005  ETH — per-tx ceiling
# GAS_FUNDER_DAILY_CAP_WEI=50000000000000000    # 0.05   ETH — rolling 24h ceiling
# GAS_FUNDER_HOTWALLET_MIN_WEI=20000000000000000 # 0.02  ETH — "reload me" alert below this
# GAS_FUNDER_ENABLED=true                # flip on once observe mode looks right
# GAS_FUNDER_CHAIN_ID=8453               # default Base mainnet
# GAS_FUNDER_INTERVAL_SECS=900           # default 15m
# GAS_FUNDER_INCLUDE_ADMIN=true          # also monitor/fund the admin payer
```

## LITKEY browser payment claim flow

`/payWithLitkey?wallet=0x…` lets a user credit an existing Stripe customer by
paying LITKEY to the deployed gateway on Base mainnet. The page validates that
the URL wallet maps to a customer, prominently displays the email and wallet that
will be credited, refreshes the quote every 30 seconds until approval starts,
shows the market LITKEY rate, configured discount percentage,
discount-adjusted credit rate, and discounted LITKEY amount, freezes the quote
and LITKEY amount for exact-amount approval, calls
`pay(amount, wallet)`, then posts the resulting transaction hash plus credited
wallet to `/api/litkey/payment-claim`. The claim endpoint fetches that exact
transaction receipt, verifies the configured gateway emitted the expected
`Payment` log for the wallet, and runs the idempotent crediting handler. There is
no browser status poller and no background WSS/reconciliation loop in the running
service; the user's known transaction hash is the source of truth. The public
preview endpoint intentionally
reveals whether the supplied wallet has a Stripe customer and the email that will
be credited so payment-link recipients can verify the destination before
spending. The payment config endpoint fails closed when chain verification is
disabled, so users cannot be directed to send LITKEY while automatic crediting is
offline.
The main dashboard entry point is intentionally deferred until after production
smoke testing; for now operators can test the standalone page directly.

Crediting pauses when the LITKEY rate row is missing or stale, records dust and
no-customer events without a Stripe balance write, and credits known customers
with the deterministic `PaymentLog::idempotency_key()` based on
`(chain_id, tx_hash, log_index)`.

### 3. Run

```sh
cd lit-payments
cargo run
```

Visit <http://localhost:8000/login>, enter `chris@litprotocol.com` (or
whatever's seeded in `migrations/20260518000002_seed_operators.sql`),
check your inbox, click the link.

## Gas funder

Keeps the lit-api-server **API payer pool** topped up so on-chain writes
(`new_account`, etc.) never fail with `insufficient funds for gas`.

**Why it's needed.** lit-api-server signs writes from a pool of payer wallets
whose keys live in the dstack TEE. Pool signer-selection is round-robin and
**not balance-aware**, so a single drained wallet causes intermittent 500s.
The in-TEE admin payer only rebalances on pool *resize*, never continuously,
and nothing alerts on low balances. lit-payments runs out of the TEE hot path
(Railway, single instance, already polling), so it's the natural home for an
automated funder + alerter. Code: `src/gas_funder/`.

**What it does each tick** (`GAS_FUNDER_INTERVAL_SECS`, default 15m):

1. Fetches the **live** payer set from lit-api-server
   (`GET /core/v1/get_api_payers`, plus `get_admin_api_payer` when
   `GAS_FUNDER_INCLUDE_ADMIN`). The pool rotates, so it's re-read every tick.
2. Reads each balance on-chain.
3. Tops up any payer below `LOW_WATER` up to `HIGH_WATER`, clamped by
   `MAX_TX` per send and a rolling-24h `DAILY_CAP` (summed from
   `gas_funding_events`).

**Modes.** Off entirely unless `GAS_FUNDER_PRIVATE_KEY` is set. With it set:

- **OBSERVE** (default — `GAS_FUNDER_ENABLED` unset/false): reads balances and
  emails alerts, but broadcasts **nothing**. Deploy here first to verify the
  resolved addresses, thresholds, and tick logs before it can move funds.
- **ACTIVE** (`GAS_FUNDER_ENABLED=true`): actually sends top-ups.

**Safety rails.** Per-tx ceiling + rolling-24h ceiling; a `pending` row is
written *before* each send (single instance + await-receipt ⇒ sequential
nonces, no double-spend on restart); the hot wallet is checked to cover the
whole round before any send. Routine top-ups are **silent** (info logs only).

**Alerts** (email via Resend, to `GAS_FUNDER_ALERT_EMAIL`, deduped by a
cooldown): hot-wallet-low **"reload me"** (the one wallet a human watches),
send failure, hot wallet can't cover the round, and daily cap reached while
payers are still low. In OBSERVE mode it also emails what it *would* have sent.

**Custody note.** This puts a spendable key on Railway, outside the TEE. Treat
it as a **gas tank**: dedicate a fresh key, keep its balance small, and let the
"reload me" alert bound the loss if it's ever compromised.

See the env vars in [Local development](#2-set-env-vars) and
[Set service variables](#3-set-service-variables).

## Deploy to Railway

Railway config for this service lives in `lit-payments/railway.json` so other
Railway services can add their own subfolder configs later. For this service,
keep the Railway service root/build context at the repo root and set the service
config file path to `lit-payments/railway.json`: the Dockerfile needs repo-root
build context because `lit-payments` depends on the sibling `lit-billing-core`
crate. Keep the service awake in production for fastest user-facing payment
confirmation after a wallet transaction is mined.

### 1. Create the Railway project/service

In the Railway dashboard:

1. New Project → **Deploy from GitHub repo** → `LIT-Protocol/chipotle`.
2. Select the branch you want to deploy from.
3. Configure this Railway service for the monorepo:
   - **Root Directory / build context:** repo root (`/`), not `lit-payments`.
   - **Config file path:** `lit-payments/railway.json`.
   - `lit-payments/railway.json` points at `lit-payments/Dockerfile`.
4. In service settings, keep the service awake for fastest user-facing payment
   confirmation.

Equivalent CLI flow if you prefer:

```sh
railway login
railway link        # choose or create the lit-payments project
railway up          # from repo root; service config path should be lit-payments/railway.json
```

### 2. Add Postgres

Use Railway's Postgres plugin for the cheapest/simple path:

1. Project → **New** → **Database** → **Add PostgreSQL**.
2. In the `lit-payments` service variables, reference the plugin's connection
   string as `DATABASE_URL`.

Railway usually exposes the plugin URL as a variable you can reference from the
service, for example:

```sh
DATABASE_URL=${{Postgres.DATABASE_URL}}
```

External Postgres (Neon/Supabase/etc.) is also fine; set `DATABASE_URL` to that
connection string instead.

### 3. Set service variables

Set these on the Railway `lit-payments` service. `PORT` is provided by Railway;
`main.rs` translates it to Rocket's port at startup.

```sh
ROCKET_ADDRESS=0.0.0.0
RUST_LOG=info
PUBLIC_BASE_URL=https://<your-railway-domain-or-payments-domain>
DATABASE_URL=${{Postgres.DATABASE_URL}}
MAGIC_LINK_SIGNING_KEY=<openssl rand -base64 32>
ROCKET_SECRET_KEY=<openssl rand -base64 32>
RESEND_API_KEY=re_...
MAIL_FROM=noreply@mail.litprotocol.com
STRIPE_SECRET_KEY=rk_live_...   # restricted key: Customers Read + Billing > Customer Balance Transaction Write

# LITKEY pricing / browser payment verification
LITKEY_DISCOUNT_BASIS_POINTS=0
ALCHEMY_HTTPS_URL=https://base-mainnet.g.alchemy.com/v2/...
LITKEY_GATEWAY_ADDRESS=0xa2d54cd1D1dF1735718A857aC49CaF9ECaB0093b
LITKEY_CHAIN_ID=8453
```

Optional operator caps if you want non-default values:

```sh
MAX_GRANT_CENTS=2000
MAX_DAILY_PER_OPERATOR_CENTS=10000
```

Optional gas funder (see [Gas funder](#gas-funder)). Off unless
`GAS_FUNDER_PRIVATE_KEY` is set:

```sh
GAS_FUNDER_PRIVATE_KEY=0x...                    # dedicated low-value hot wallet
GAS_FUNDER_ALERT_EMAIL=you@litprotocol.com
GAS_FUNDER_RPC_URL=https://base-mainnet.g.alchemy.com/v2/...  # falls back to ALCHEMY_HTTPS_URL
GAS_FUNDER_LOW_WATER_WEI=500000000000000        # 0.0005 ETH
GAS_FUNDER_HIGH_WATER_WEI=5000000000000000      # 0.005  ETH
GAS_FUNDER_MAX_TX_WEI=5000000000000000          # 0.005  ETH per tx
GAS_FUNDER_DAILY_CAP_WEI=50000000000000000      # 0.05   ETH per rolling 24h
GAS_FUNDER_HOTWALLET_MIN_WEI=20000000000000000  # 0.02   ETH — "reload me" alert
# Start WITHOUT GAS_FUNDER_ENABLED (observe mode: alerts only). Once the
# tick logs and emails look right, set GAS_FUNDER_ENABLED=true to send.
# GAS_FUNDER_ENABLED=true
```

Generate secrets locally and paste the values into Railway:

```sh
openssl rand -base64 32   # MAGIC_LINK_SIGNING_KEY
openssl rand -base64 32   # ROCKET_SECRET_KEY
```

### 4. Deploy and verify

Deploy from the Railway dashboard or CLI, then verify:

```sh
curl -fsS https://<your-railway-domain-or-payments-domain>/health
curl -fsS 'https://<your-railway-domain-or-payments-domain>/api/litkey/payment-config'
```

Expected:

- `/health` returns `ok`.
- `/api/litkey/payment-config` returns Base mainnet config when chain verification
  env is present; it returns `503` if that config is intentionally absent.
- `/payWithLitkey?wallet=0x...` loads the standalone payment page for smoke
  testing.

If a smoke test submits a Base payment but credit is not applied, keep the tx hash
from the page and check Railway logs for `/api/litkey/payment-claim` errors.

### Mail sender setup

Use `MAIL_FROM=noreply@mail.litprotocol.com`. To avoid touching the root-domain
SPF/DKIM records that Google Workspace uses:

1. In Resend → Domains, add `mail.litprotocol.com`.
2. Add the SPF (`TXT @ "v=spf1 include:_spf.resend.com ~all"`) and DKIM
   records to the **subdomain** zone in your DNS provider.
3. Verify in Resend.
4. Set `MAIL_FROM=noreply@mail.litprotocol.com`.

## Operator allowlist

Operators are seeded by SQL migration
(`migrations/20260518000002_seed_operators.sql`). To add or remove
operators later, add a new migration. There is intentionally no admin
"manage operators" UI in v1 — the operator set is small and rare to
change.

## Auth flow

1. User visits `/login` and submits their email.
2. `POST /auth/request` checks the operators table. If the email is on
   the allowlist, send a magic link (15-min HMAC-signed token); otherwise
   silently no-op. Same response in both cases (no enumeration).
3. User clicks the link → `GET /auth/verify?token=…` validates the token,
   creates a session row, sets a `lit_payments_session` private cookie,
   redirects to `/`.
4. `/` renders the signed-in page; `/api/me` returns the current operator.
5. `POST /auth/logout` deletes the session row and clears the cookie.

The Rocket `Operator` request guard does all the lookup for protected
routes — just add `operator: Operator` to a route handler.
