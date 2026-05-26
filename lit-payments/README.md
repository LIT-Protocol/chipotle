# lit-payments

Ops-facing billing service. Magic-link auth + admin credit portal + (later)
LITKEY payment gateway. Deployed outside the TEE, on Fly.io.

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
STRIPE_SECRET_KEY=rk_test_...            # Stripe restricted key
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

## Deploy to Fly.io

The `fly.toml` at the repo root configures this service. The build context
is the repo root (so the lit-billing-core sibling crate is reachable from
the Dockerfile). All `fly` commands run from the repo root.

### 1. Create the app

```sh
fly apps create lit-payments
```

(Or `fly launch --no-deploy` to let flyctl walk you through it; reuse the
existing `fly.toml` when prompted.)

### 2. Provision Postgres

Pick one — both expose a `DATABASE_URL`-shaped connection string:

- **Fly Postgres (Managed Postgres)**:
  ```sh
  fly postgres create --name lit-payments-db --region iad
  fly postgres attach lit-payments-db --app lit-payments
  ```
  The attach command sets `DATABASE_URL` on the app automatically.

- **External (Supabase / Neon / etc.)**: set `DATABASE_URL` manually:
  ```sh
  fly secrets set --app lit-payments DATABASE_URL='postgres://...'
  ```

### 3. Set the rest of the secrets

```sh
fly secrets set --app lit-payments \
  MAGIC_LINK_SIGNING_KEY="$(openssl rand -base64 32)" \
  RESEND_API_KEY=re_... \
  MAIL_FROM='noreply@mail.litprotocol.com' \
  PUBLIC_BASE_URL='https://payments.litprotocol.com' \
  STRIPE_SECRET_KEY=rk_... \
  LITKEY_DISCOUNT_BASIS_POINTS=0 \
  ROCKET_SECRET_KEY="$(openssl rand -base64 32)"
```

`ROCKET_SECRET_KEY` is required by Rocket for private (encrypted) cookies
in release builds. The other vars are documented above under
"Local development."

### 4. Custom domain

```sh
fly certs create --app lit-payments payments.litprotocol.com
```

Fly prints the DNS records (CNAME or A/AAAA) to add. Once they propagate,
the cert provisions automatically.

### 5. Deploy

```sh
fly deploy
```

Migrations run on first boot of every release; safe to redeploy any time.
Health checks hit `/health` (configured in `fly.toml`).

### Updating

`fly deploy` again. Fly's blue-green deploy flips traffic when the new
machine passes its health check.

## Mail sender setup (Resend + `mail.litprotocol.com`)

The default sender is `noreply@mail.litprotocol.com`. To avoid touching
the root-domain SPF/DKIM records that Google Workspace uses:

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
