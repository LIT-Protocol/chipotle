# lit-triggers

Reactive Lit Action runner service. Users sign in with magic links, create trigger configs, and store scoped Chipotle usage API keys encrypted at rest. The service supports webhook, scheduled, and EVM chain-event triggers that enqueue runs for the shared dispatcher.

## Environment

Required:

- `DATABASE_URL` — Postgres connection string
- `MAGIC_LINK_SIGNING_KEY` — base64, at least 32 bytes
- `USAGE_KEY_ENCRYPTION_KEY` — base64, at least 32 bytes
- `RESEND_API_KEY` — Resend API key for magic links
- `MAIL_FROM` — sender address
- `PUBLIC_BASE_URL` — public service URL used in magic links
- `ROCKET_SECRET_KEY` — base64, 32 bytes; required by Rocket private cookies in release/Fly deployments

Optional:

- `CHIPOTLE_API_BASE_URL` — defaults to `https://api.chipotle.litprotocol.com`
- `PORT` — mapped to `ROCKET_PORT` on startup for fly.io-style platforms
- `WEBHOOK_MAX_BODY_BYTES` — defaults to `262144`
- `WEBHOOK_IP_MAX_REQUESTS_PER_MINUTE` — defaults to `60`
- `WEBHOOK_USER_MAX_REQUESTS_PER_MINUTE` — defaults to `120`
- `WEBHOOK_TRIGGER_MAX_REQUESTS_PER_MINUTE` — defaults to `60`
- `WEBHOOK_DEFAULT_MAX_QUEUED_RUNS` — defaults to `100`
- `CHAIN_POLL_INTERVAL_SECS` — defaults to `15`
- `CHAIN_CONFIRMATION_DEPTH` — defaults to `12`
- `CHAIN_MAX_BLOCK_RANGE` — defaults to `500`
- `CHAIN_RPC_TIMEOUT_SECS` — defaults to `10`
- `CHAIN_INITIAL_LOOKBACK_BLOCKS` — defaults to `100`
- `ETHEREUM_RPC_URL`, `BASE_RPC_URL`, `ARBITRUM_RPC_URL`, `BSC_RPC_URL`, `POLYGON_RPC_URL` — optional EVM RPC endpoints. The chain listener skips enabled triggers for chains whose RPC URL is not configured.

## Local development

```bash
cd lit-triggers
cargo +1.91 test
cargo +1.91 run
```

Run Postgres locally and set the environment above before starting the server. Migrations run on boot.

## Fly.io deployment

A dedicated Fly config lives at the repo root:

```bash
fly deploy -c fly.lit-triggers.toml
```

Create or attach Postgres first and set secrets before deploying. Example:

```bash
fly apps create lit-triggers
fly postgres create --name lit-triggers-db --region iad
fly postgres attach --app lit-triggers lit-triggers-db

openssl rand -base64 32  # MAGIC_LINK_SIGNING_KEY
openssl rand -base64 32  # USAGE_KEY_ENCRYPTION_KEY
openssl rand -base64 32  # ROCKET_SECRET_KEY

fly secrets set --app lit-triggers \
  MAGIC_LINK_SIGNING_KEY='<base64-32-byte-key>' \
  USAGE_KEY_ENCRYPTION_KEY='<base64-32-byte-key>' \
  ROCKET_SECRET_KEY='<base64-32-byte-key>' \
  RESEND_API_KEY='<resend-api-key>' \
  MAIL_FROM='Lit Triggers <triggers@example.com>' \
  PUBLIC_BASE_URL='https://lit-triggers.fly.dev'
```

Set chain RPC URLs only for chains you want to enable:

```bash
fly secrets set --app lit-triggers \
  BASE_RPC_URL='https://...' \
  ETHEREUM_RPC_URL='https://...'
```

Health check:

```bash
curl https://lit-triggers.fly.dev/health
```

Operational notes:

- Run one machine for v1. The scheduler and chain listener use Postgres advisory locks, but single-machine operation keeps timing and queue behavior easiest to reason about.
- Do not set `auto_stop_machines=true` for this service; scheduled and chain-event triggers rely on a continuously running worker.
- `DATABASE_URL`, signing keys, encryption keys, Resend credentials, and RPC URLs should be Fly secrets, not `[env]` values.
- If you deploy under a different Fly app name, use `fly deploy -c fly.lit-triggers.toml -a <app>` and set `PUBLIC_BASE_URL` to that app's HTTPS URL.

## API foundation

Authenticated routes:

- `GET /api/me`
- `POST /api/triggers`
- `GET /api/triggers`
- `GET /api/triggers/<id>`
- `PATCH /api/triggers/<id>`
- `DELETE /api/triggers/<id>`
- `GET /api/triggers/<id>/runs`
- `POST /api/triggers/<id>/test` — returns `501 Not Implemented` until test execution is wired up

Usage API keys are accepted only on create/update and are never returned by API responses.

## Static admin UI

The dashboard at `/` is a plain HTML/CSS/JS app served from `lit-triggers/static/`. It supports profile display, logout, trigger creation/edit/delete, recent run inspection, and kind-specific config helpers for webhook, schedule, and EVM chain event triggers.

For new triggers, users can either:

1. Paste a Lit/Chipotle admin API key into the browser-only mint flow. The UI calls Chipotle's `add_usage_api_key` endpoint directly with a narrow `execute_in_groups` scope, clears the admin key field after the mint attempt, and sends only the scoped usage key to lit-triggers.
2. Paste a pre-minted scoped usage key manually and skip browser minting.

The UI phrases CIDs as action code identities: a CID identifies the action code content that will be executed with the supplied scoped usage key and group permissions; it does not imply ownership of that CID.
