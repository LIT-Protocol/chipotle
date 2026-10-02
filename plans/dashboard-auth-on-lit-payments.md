# Dashboard password auth: move the storage service from Cloudflare Workers to lit-payments

Status: implemented in this PR. Deployment to Railway staging is automatic on merge; the operator steps are listed at the end.
Date: 2026-10-01.
Supersedes the Cloudflare Workers/D1 deployment described in [email-password-login.md](email-password-login.md) and the previous `lit-dashboard-auth/README.md`.

## Why

The Worker design required `litprotocol.com` to be a Cloudflare zone so Wrangler could attach a custom domain (`auth.<dashboard host>`). The zone is not on Cloudflare: its nameservers are Namecheap's, with some subdomains delegated to Route 53. Moving the apex zone just to host one Worker is a disproportionate change.

`lit-payments` already provides everything the Worker needed and is deployed on Railway with staging and production environments:

| Need | lit-payments already has |
| --- | --- |
| HTTPS origin reachable from the dashboard | `https://staging.payments.litprotocol.com` (staging) and `https://payments.litprotocol.com` (production) |
| Database | Postgres via `sqlx`, runtime migrations in `lit-payments/migrations/` |
| Transactional email | `mail::Mailer` (Resend), `RESEND_API_KEY` and `MAIL_FROM` already configured in both Railway environments |
| CORS allowlist with credentials | `CORS_ALLOWED_ORIGINS` exact-match list, `allow_credentials: true` |
| Background jobs | tokio loops (reconciler, enterprise billing, gas funder) |
| Deploy pipeline | `deploy-lit-payments.yml`: push to `main` deploys staging, `v*` tag deploys production |

The browser side of the feature (Argon2id in a Web Worker, AES-GCM envelopes, `password-client.js`, `password-login.js`) is unchanged. The server stores ciphertext and never sees the password or the API key.

## Decisions

1. **Same HTTP contract.** The Rust implementation serves exactly the `/auth/v1/*` surface in `lit-dashboard-auth/openapi.json` with the same request/response shapes, status codes, error strings, cookie name (`__Host-chipotle_auth`), `X-Chipotle-Auth: 1` requirement, CSRF token and `id`/`version` binding. The dashboard's `password-client.js` needs no change; only the injected base URL changes.
2. **Module, not a new service.** Code lives in `lit-payments/src/dashboard_auth/`. Routes mount alongside the existing `/auth/request` magic-link routes (no path overlap: all new routes are under `/auth/v1/`).
3. **Feature is off until configured.** `DASHBOARD_AUTH_SECRET` unset means every `/auth/v1/*` request returns `503 {"error":"Account service is not configured."}`. This keeps the Railway deploy green before the variables are set and matches the Worker's fail-closed behaviour.
4. **Several dashboard origins per deployment.** `DASHBOARD_AUTH_URLS` is a comma-separated list of full dashboard URLs ending in `/dapps/dashboard/`. Requests must carry an `Origin` header whose value is one of those origins; the verification email link points back at the dashboard URL matching the requesting origin. The origins are appended to the CORS allowlist automatically. Staging can therefore serve both `next.dashboard.chipotle.litprotocol.com` and `dashboard.dev.litprotocol.com`.
5. **Cookie `SameSite` is configurable.** The Worker relied on the auth host being same-site with the dashboard. A Railway-generated `*.up.railway.app` hostname would be cross-site with every dashboard, so `DASHBOARD_AUTH_COOKIE_SAMESITE=none` exists for that case (emits `SameSite=None; Secure; Partitioned`). Both staging (`staging.payments.litprotocol.com` ↔ `next.dashboard.chipotle.litprotocol.com`) and production (`payments.litprotocol.com` ↔ `dashboard.chipotle.litprotocol.com`) are same-site under `litprotocol.com` and keep the default `lax`. CSRF protection never depended on `SameSite`: the exact `Origin` check, the custom header (forces a CORS preflight) and the per-session CSRF token remain.
6. **Postgres schema mirrors D1.** Five tables prefixed `dashboard_auth_` (`users`, `sessions`, `tokens`, `outbox`, `limits`) with the same columns, constraints and conditional-update semantics (`UPDATE ... WHERE state = ... AND version = ... AND verifier = ...`). Wallet addresses stay non-unique (the D1 `0002` migration is folded into the initial Postgres schema). Timestamps stay epoch seconds (`BIGINT`) so the conditional SQL ports one-to-one.
7. **Rate limits stay in the database.** Same counters (`INSERT ... ON CONFLICT DO UPDATE ... RETURNING count`) keyed by an HMAC of the window and label. Client IP comes from the last value of `X-Forwarded-For` (Railway's edge proxy appends exactly one hop), configurable via `DASHBOARD_AUTH_CLIENT_IP_HEADER`, falling back to the socket peer.
8. **Outbox + encryption unchanged.** Pending mail is encrypted at rest with AES-256-GCM under a key derived from `DASHBOARD_AUTH_SECRET`; a tokio job retries every five minutes with Resend idempotency keys and purges expired tokens, sessions, outbox rows and limit counters. Enqueueing also kicks an immediate delivery attempt.
9. **No backwards compatibility.** Nothing is deployed to production and there are no users. The Worker, its D1 migrations, Wrangler config, deploy script and the `dashboard-auth.yml` deploy job are deleted. The two D1 databases created in Cloudflare should be deleted by hand.
10. **`lit-dashboard-auth/` stays as the browser crypto + test harness folder.** It keeps `browser/crypto-worker.js`, the esbuild script that produces the committed `password-crypto-worker.js` bundle, `openapi.json`, `scripts/configure-static.py`, and the test harness. The harness now boots the real `lit-payments` binary against local Postgres with a fake Resend endpoint instead of Miniflare.

## Server design (`lit-payments/src/dashboard_auth/`)

| File | Contents |
| --- | --- |
| `mod.rs` | `DashboardAuthConfig` (parsed from env), constants (`COOKIE`, TTLs), `Failure` responder producing `{"error": …}` JSON with the Worker's status codes, module wiring. |
| `crypto.rs` | HMAC-SHA256 `mac()`, SHA-256 hex, random hex, constant-time compare, outbox AES-GCM encrypt/decrypt, synthetic parameter derivation for unknown emails. |
| `protocol.rs` | Port of `password-protocol.js`: `KDF`, `FORMAT`, `validate_parameters`, `validate_envelope` (hex length checks, account regex, AES-256-GCM, 12-byte nonce, 60-byte ciphertext). |
| `db.rs` | Typed row structs and every SQL statement (users, sessions, tokens, outbox, limits). Multi-statement D1 batches become explicit Postgres transactions. |
| `routes.rs` | Request gate (config present, exact `Origin`, `X-Chipotle-Auth`, JSON-only, 8 KiB body cap, per-IP limit), the 13 endpoints, a JSON 404 catch-all for `/auth/v1/<anything>`, and the `Cache-Control: no-store` / `nosniff` response headers for `/auth/v1/*`. |
| `outbox.rs` | `queue_verification`, `queue_notification`, `deliver()` (Resend with `Idempotency-Key`), `spawn()` periodic job with cleanup. |

Configuration (all read once at boot, in `config.rs`):

| Variable | Required | Meaning |
| --- | --- | --- |
| `DASHBOARD_AUTH_SECRET` | enables feature | ≥32 characters. HMACs rate-limit keys and synthetic parameters, derives the outbox encryption key. Not an account key. Distinct per environment. |
| `DASHBOARD_AUTH_URLS` | when enabled | Comma-separated dashboard URLs, each `https://host/dapps/dashboard/` (`http://localhost:…` allowed for tests). |
| `DASHBOARD_AUTH_ENVIRONMENT` | when enabled | `^[a-z0-9-]{1,40}$`, e.g. `staging`/`production`. Part of the envelope AAD; changing it orphans existing ciphertext. |
| `DASHBOARD_AUTH_COOKIE_SAMESITE` | optional | `lax` (default) or `none`. |
| `DASHBOARD_AUTH_CLIENT_IP_HEADER` | optional | Default `X-Forwarded-For`; last comma-separated value is the client. |
| `DASHBOARD_AUTH_OUTBOX_INTERVAL_SECS` | optional | Default 300. |
| `RESEND_API_BASE_URL` | optional | Default `https://api.resend.com`; the test harness points it at a local capture server. |
| `RESEND_API_KEY`, `MAIL_FROM` | existing | Reused. |

## Frontend and deploy wiring

- `configure-static.py` is unchanged: it injects an HTTPS origin into `password-client.js`.
- `deploy-static.yml`: the `next` job now injects `AUTH_URL: ${{ vars.LIT_PAYMENTS_STAGING_URL }}` so password login is enabled on `next.dashboard.chipotle.litprotocol.com` as soon as staging lit-payments is configured. The `main` job keeps `vars.LIT_AUTH_STAGING_URL` (empty = disabled) so `dashboard.dev.litprotocol.com` is opted in deliberately later by setting that variable to the same staging payments URL.
- `deploy-prod-3-static.yml` keeps `vars.LIT_AUTH_PROD_URL`; set it to `https://payments.litprotocol.com` when production lit-payments is configured.
- `dashboard-auth.yml`: test-only workflow (browser password suite against the Rust server + Postgres service container). The deploy job is removed; lit-payments deploys through `deploy-lit-payments.yml`.
- `dashboard-account-access.yml`: adds a Postgres service container and a `cargo build` of lit-payments; `start-auth-stack.sh` launches the Rust server through the harness.

## Tests

- Rust unit tests for protocol validation, envelope/parameter checks, config parsing and crypto helpers (`cargo test` without a database).
- `lit-dashboard-auth/test/auth.test.mjs` is ported from Miniflare to HTTP against the running Rust server, keeping its scenarios: replay, creation claim, upload idempotency, CSRF, login, concurrent password change atomicity, origin rejection, synthetic parameters, rate limits, cross-identity mutation, email change rollback, expired tokens/sessions, wallet squatting. Database fixtures use `pg` instead of D1.
- Browser suites (`e2e/password`, `e2e/tests/auth`) are unchanged apart from the harness.

## Operator steps after merge

1. Merge to `main`. `deploy-lit-payments.yml` deploys staging automatically; the new migration runs at boot.
2. In Railway, project **Lit Payments**, environment **staging**, service **lit-payments**, add:
   - `DASHBOARD_AUTH_SECRET` = `openssl rand -hex 32`
   - `DASHBOARD_AUTH_URLS` = `https://next.dashboard.chipotle.litprotocol.com/dapps/dashboard/` (append `,https://dashboard.dev.litprotocol.com/dapps/dashboard/` when enabling dev)
   - `DASHBOARD_AUTH_ENVIRONMENT` = `staging`
   - `DASHBOARD_AUTH_COOKIE_SAMESITE` = `lax` (staging mirrors production: both hosts are under `litprotocol.com`)
   Railway redeploys on variable changes.
3. Deploy the `next` static site (`deploy-static.yml`, target `deploy-next`) so the dashboard picks up the auth URL.
4. Test on `https://next.dashboard.chipotle.litprotocol.com/dapps/dashboard/#create-account` in any browser; staging and production are both same-site under `litprotocol.com`.
5. Delete the Cloudflare D1 databases `chipotle-auth-staging` and `chipotle-auth-production`.
6. For production later: set the same four variables on the production environment (`DASHBOARD_AUTH_URLS=https://dashboard.chipotle.litprotocol.com/dapps/dashboard/`, `DASHBOARD_AUTH_ENVIRONMENT=production`, `DASHBOARD_AUTH_COOKIE_SAMESITE=lax`), push a `v*` tag, then set `LIT_AUTH_PROD_URL=https://payments.litprotocol.com` and run the production static deploy.

## Operational notes carried over

- Back up Postgres (Railway backups) rather than D1 Time Travel. Invalidate sessions and tokens after a restore.
- Keep `DASHBOARD_AUTH_SECRET` backed up; rotation invalidates pending outbox mail and synthetic parameters, not stored ciphertext.
- A D1-style dump of `dashboard_auth_users` still only yields Argon2id-protected material; the verifier is a hash of an already stretched secret.
- lit-payments is a single Railway instance. Login shares its availability with billing; the `/health` check and the stale-rate warning are the existing signals.
