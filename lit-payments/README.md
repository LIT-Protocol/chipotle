# lit-payments

Ops-facing billing service. Magic-link auth + admin credit portal + LITKEY
payment gateway. Deployed outside the TEE, on Railway.

See `plans/lit-payments-app.md` (in the repo root, on the planning branch)
for the full design.

## Routes

Public:
- `GET /login` — login page (form posts to `/auth/request`).
- `GET /payWithLitkey?wallet=0x…` — end-user Base mainnet LITKEY payment page for an existing Stripe customer wallet.
- `POST /auth/request` — send magic link (rate-limited per email, 60s cooldown).
- `GET /auth/verify?token=…` — validate token, set session cookie, redirect.
- `GET /api/customer/preview?wallet=0x…` — wallet-scoped customer identity preview for the LITKEY payment page. Returns only `found`, `email`, and `wallet_address`; it does not expose Stripe customer ids or balances.
- `GET /api/litkey/quote` — public LITKEY quote for the end-user payment page; includes `crediting_paused` and omits an effective credit rate while paused.
- `GET /api/litkey/payment-config` — public Base mainnet payment config: chain id, LITKEY token address, and payment gateway address. Fails closed with `503` if the on-chain listener is not configured.
- `GET /api/litkey/payment-status?tx_hash=0x…&wallet=0x…` — wallet-scoped listener status for a submitted transaction; used by the payment page to poll for `credited`, `dust`, `paused`, or `no_customer` after submission.

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

LITKEY pricing is intentionally stored as 18-decimal USD fixed point instead of whole cents because LITKEY can trade below one cent. Example: `$0.006/LITKEY` is stored as `6000000000000000` (`0.006 * 1e18`). The listener settlement helper keeps the on-chain `Payment.amount` in native LITKEY wei, multiplies by `usd_wei_per_litkey` with `num-bigint`, applies `LITKEY_DISCOUNT_BASIS_POINTS`, and rounds only once at the final Stripe-cent boundary.

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
PUBLIC_BASE_URL=http://localhost:8000
STRIPE_SECRET_KEY=rk_test_...            # Stripe restricted key; needs Customers: Read plus Billing > Customer Balance Transaction: Write
ROCKET_SECRET_KEY=$(openssl rand -base64 32)  # required for private cookies in release
ROCKET_PORT=8000

# Optional — cap defaults match the plan ($20 per grant, $100/op/day):
# MAX_GRANT_CENTS=2000
# MAX_DAILY_PER_OPERATOR_CENTS=10000

# Optional — incentive discount for paying with LITKEY, in basis points.
# 0 = no discount. 2000 = 20% off vs credit card, so users receive
# 1 / (1 - 0.20) = 1.25x the market-rate Stripe credit per LITKEY.
# LITKEY_DISCOUNT_BASIS_POINTS=0

# Optional — enable the LITKEY on-chain listener. The current 3c slice starts
# the Alchemy WSS fast path plus HTTPS reconciliation fallback.
# ALCHEMY_WSS_URL=wss://base-mainnet.g.alchemy.com/v2/...
# ALCHEMY_HTTPS_URL=https://base-mainnet.g.alchemy.com/v2/...
# LITKEY_GATEWAY_ADDRESS=0xa2d54cd1D1dF1735718A857aC49CaF9ECaB0093b
# LITKEY_CHAIN_ID=8453
# LITKEY_CONFIRMATIONS=5
# LITKEY_RECONCILIATION_INTERVAL_SECS=60
```

## LITKEY listener runtime status

`/payWithLitkey?wallet=0x…` lets a user credit an existing Stripe customer by
paying LITKEY to the deployed gateway on Base mainnet. The page validates that
the URL wallet maps to a customer, prominently displays the email and wallet that
will be credited, refreshes the quote every 30 seconds until approval starts,
freezes the quote and LITKEY amount for exact-amount approval, calls
`pay(amount, wallet)`, and polls the listener status endpoint scoped by
transaction hash plus credited wallet. The public preview endpoint intentionally
reveals whether the supplied wallet has a Stripe customer and the email that will
be credited so payment-link recipients can verify the destination before
spending. The payment config endpoint fails closed when the listener is disabled,
so users cannot be directed to send LITKEY while automatic crediting is offline.
The main dashboard entry point is intentionally deferred until after production
smoke testing; for now operators can test the standalone page directly.

When `ALCHEMY_WSS_URL`, `ALCHEMY_HTTPS_URL`, and `LITKEY_GATEWAY_ADDRESS` are
configured, the app spawns both an Alchemy WSS logs subscription fast path and a
confirmed-block HTTPS reconciliation loop. WSS waits for the `eth_subscribe`
acknowledgement, subscribes to the configured gateway address plus the exact
`Payment(indexed wallet,indexed payer,uint256 amount)` topic, buffers near-head
logs until they reach the configured confirmation depth, evicts pending logs when
Alchemy sends `removed: true` reorg notifications, credits confirmed logs through
the shared handler, and never advances reconciliation checkpoints. Reconciliation
remains the fallback: each
pass reads the checkpoint, fetches Base logs for the safe range
`(last_processed_block, latest - confirmations)`, processes decoded gateway
`Payment` events through the shared crediting handler, and advances the checkpoint
only after the whole range succeeds.

Crediting pauses when the LITKEY rate row is missing or stale, records dust and
no-customer events without a Stripe balance write, and credits known customers
with the deterministic `PaymentLog::idempotency_key()`. WSS and reconciliation
share the same parser, crediting handler, and `(chain_id, tx_hash, log_index)`
idempotency.

### 3. Run

```sh
cd lit-payments
cargo run
```

Visit <http://localhost:8000/login>, enter `chris@litprotocol.com` (or
whatever's seeded in `migrations/20260518000002_seed_operators.sql`),
check your inbox, click the link.

## Deploy to Railway

Railway config for this service lives in `lit-payments/railway.json` so other
Railway services can add their own subfolder configs later. For this service,
keep the Railway service root/build context at the repo root and set the service
config file path to `lit-payments/railway.json`: the Dockerfile needs repo-root
build context because `lit-payments` depends on the sibling `lit-billing-core`
crate. The service must keep at least one replica running: the LITKEY listener
includes WSS and reconciliation background loops, so do not enable
scale-to-zero/sleep mode for production.

### 1. Create the Railway project/service

In the Railway dashboard:

1. New Project → **Deploy from GitHub repo** → `LIT-Protocol/chipotle`.
2. Select the branch you want to deploy from.
3. Configure this Railway service for the monorepo:
   - **Root Directory / build context:** repo root (`/`), not `lit-payments`.
   - **Config file path:** `lit-payments/railway.json`.
   - `lit-payments/railway.json` points at `lit-payments/Dockerfile`.
4. In service settings, confirm the service is not configured to sleep/scale to
   zero.

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

# LITKEY pricing / listener
LITKEY_DISCOUNT_BASIS_POINTS=0
ALCHEMY_WSS_URL=wss://base-mainnet.g.alchemy.com/v2/...
ALCHEMY_HTTPS_URL=https://base-mainnet.g.alchemy.com/v2/...
LITKEY_GATEWAY_ADDRESS=0xa2d54cd1D1dF1735718A857aC49CaF9ECaB0093b
LITKEY_CHAIN_ID=8453
LITKEY_CONFIRMATIONS=5
LITKEY_RECONCILIATION_INTERVAL_SECS=60
```

Optional operator caps if you want non-default values:

```sh
MAX_GRANT_CENTS=2000
MAX_DAILY_PER_OPERATOR_CENTS=10000
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
- `/api/litkey/payment-config` returns Base mainnet config when the listener env
  is present; it returns `503` if listener config is intentionally absent.
- `/payWithLitkey?wallet=0x...` loads the standalone payment page for smoke
  testing.

Railway deploys should run the binary continuously. If a smoke test submits a
Base payment but status never changes, check Railway logs first for listener WSS
or reconciliation errors.

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
