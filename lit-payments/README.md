# lit-payments

Ops-facing billing service. Magic-link auth + admin credit portal + (later)
LITKEY payment gateway. Deployed outside the TEE, on Fly.io.

See `plans/lit-payments-app.md` (in the repo root, on the planning branch)
for the full design.

## Routes

Public:
- `GET /login` — login page (form posts to `/auth/request`).
- `POST /auth/request` — send magic link (rate-limited per email, 60s cooldown).
- `GET /auth/verify?token=…` — validate token, set session cookie, redirect.

Authenticated (operator session cookie required):
- `GET /` — admin dashboard.
- `GET /api/me` — current operator profile.
- `POST /auth/logout` — delete session.
- `GET /api/customer/lookup?email=…` or `?wallet=…` — preview a Stripe customer + balance.
- `POST /api/grant` — apply a credit (subject to caps + UUID idempotency key).
- `GET /api/grants?limit=N` — recent grants by the calling operator.
- `GET /api/litkey/rate` — current LITKEY market rate plus discount-adjusted credit rate.
- `POST /api/litkey/rate/override` — admin-only manual market-rate override.

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
```

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
