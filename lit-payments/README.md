# lit-payments

Ops-facing billing service. Magic-link auth + (later) admin credit portal
and LITKEY payment gateway. Deployed outside the TEE, on Railway.

See `plans/lit-payments-app.md` (in the repo root, on the planning branch)
for the full design.

This PR ships **foundation + magic-link auth + login UI only**. The credit
portal endpoints and admin UI come in a follow-up PR.

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
STRIPE_SECRET_KEY=rk_test_...            # not used yet; parsed eagerly
ROCKET_SECRET_KEY=$(openssl rand -base64 32)  # required for private cookies in release
ROCKET_PORT=8000
```

### 3. Run

```sh
cd lit-payments
cargo run
```

Visit <http://localhost:8000/login>, enter `chris@litprotocol.com` (or
whatever's seeded in `migrations/20260518000002_seed_operators.sql`),
check your inbox, click the link.

## Deploy to Railway

1. New Railway project → "Empty Project".
2. Add a **PostgreSQL** plugin. Railway exposes `DATABASE_URL` to the
   service automatically.
3. Create a service from this GitHub repo. In service settings:
   - **Root Directory**: leave blank (build context = repo root).
   - **Dockerfile Path**: `lit-payments/Dockerfile`.
   - **Watch Paths**: `lit-payments/**`, `lit-billing-core/**`.
4. Add env vars on the service:
   - `MAGIC_LINK_SIGNING_KEY` — `openssl rand -base64 32`
   - `RESEND_API_KEY` — from Resend dashboard
   - `MAIL_FROM` — `noreply@mail.litprotocol.com` (or your verified sender)
   - `PUBLIC_BASE_URL` — `https://payments.litprotocol.com`
   - `STRIPE_SECRET_KEY` — a Stripe **restricted** key
   - `ROCKET_SECRET_KEY` — `openssl rand -base64 32`
5. Generate a public domain in Railway settings. Point your CNAME at it.
6. Deploy. Migrations run on first boot.

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
