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
- `ROCKET_SECRET_KEY` — base64, 32 bytes; required by Rocket private cookies in release deployments

Optional:

- `CHIPOTLE_API_BASE_URL` — defaults to `https://api.chipotle.litprotocol.com`
- `PORT` — mapped to `ROCKET_PORT` on startup for Railway/container platforms
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

## Agent-consumable testing docs

`SKILL.md` is a handoff file for agents that need to test a deployed `lit-triggers` instance. It covers magic-link authentication, trigger CRUD, webhook firing, schedule polling, chain-event setup, run inspection, and cleanup.

Give a testing agent:

- `lit-triggers/SKILL.md`
- the deployed `LT_BASE_URL`
- a test email that can receive magic links
- a scoped Chipotle usage API key for the target group/action permissions

## Railway deployment

The service owns its Railway config at `lit-triggers/railway.json`. In Railway, set this service's root directory to `lit-triggers` so multiple Railway services/projects can coexist in the monorepo without sharing one root config.

With the service root set to `lit-triggers`, Railway uses `railway.json` to build `Dockerfile`, run `/app/lit-triggers`, keep one replica awake, and health-check `/health`.

Create a Railway project with:

1. One web service connected to this GitHub repo/branch.
2. One Railway Postgres service in the same project.
3. The web service root directory set to `lit-triggers`.
4. App sleeping disabled. `lit-triggers/railway.json` sets `sleepApplication: false`; keep it disabled in the Railway UI too because scheduled and chain-event triggers rely on a continuously running worker.

Set the web service variables before deploying:

```bash
DATABASE_URL='${{Postgres.DATABASE_URL}}'
MAGIC_LINK_SIGNING_KEY='<base64-32-byte-key>'
USAGE_KEY_ENCRYPTION_KEY='<base64-32-byte-key>'
ROCKET_SECRET_KEY='<base64-32-byte-key>'
RESEND_API_KEY='<resend-api-key>'
MAIL_FROM='Lit Triggers <triggers@example.com>'
PUBLIC_BASE_URL='https://<your-railway-domain>'
RUST_LOG='info,lit_triggers=info'
CHIPOTLE_API_BASE_URL='https://api.chipotle.litprotocol.com'
```

Generate the three base64 keys locally:

```bash
openssl rand -base64 32  # MAGIC_LINK_SIGNING_KEY
openssl rand -base64 32  # USAGE_KEY_ENCRYPTION_KEY
openssl rand -base64 32  # ROCKET_SECRET_KEY
```

Set chain RPC URLs only for chains you want to enable:

```bash
BASE_RPC_URL='https://...'
ETHEREUM_RPC_URL='https://...'
```

Optional webhook/chain tuning variables can also be set on Railway if the defaults are not enough:

```bash
WEBHOOK_MAX_BODY_BYTES='262144'
WEBHOOK_IP_MAX_REQUESTS_PER_MINUTE='60'
WEBHOOK_USER_MAX_REQUESTS_PER_MINUTE='120'
WEBHOOK_TRIGGER_MAX_REQUESTS_PER_MINUTE='60'
WEBHOOK_DEFAULT_MAX_QUEUED_RUNS='100'
CHAIN_POLL_INTERVAL_SECS='15'
CHAIN_CONFIRMATION_DEPTH='12'
CHAIN_MAX_BLOCK_RANGE='500'
CHAIN_RPC_TIMEOUT_SECS='10'
CHAIN_INITIAL_LOOKBACK_BLOCKS='100'
```

Deploy from the Railway UI or CLI after variables are set. Migrations run on boot.

Health check / smoke test:

```bash
curl https://<your-railway-domain>/health
```

Operational notes:

- Run one replica for v1. The scheduler and chain listener use Postgres advisory locks, but single-replica operation keeps timing and queue behavior easiest to reason about.
- Do not enable app sleeping for this service; scheduled and chain-event triggers rely on a continuously running process.
- `DATABASE_URL`, signing keys, encryption keys, Resend credentials, and RPC URLs should be Railway variables, not committed config values.
- If Railway gives you a new public domain, update `PUBLIC_BASE_URL` to that exact HTTPS origin so magic links point back to the deployed app.

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
