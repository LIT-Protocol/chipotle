# lit-triggers

Reactive Lit Action runner service. Users sign in with magic links, create trigger configs, and store scoped Chipotle usage API keys encrypted at rest. Phase 1 provides auth, schema, and CRUD only; workers/webhooks execute in later phases.

## Environment

Required:

- `DATABASE_URL` — Postgres connection string
- `MAGIC_LINK_SIGNING_KEY` — base64, at least 32 bytes
- `USAGE_KEY_ENCRYPTION_KEY` — base64, at least 32 bytes
- `RESEND_API_KEY` — Resend API key for magic links
- `MAIL_FROM` — sender address
- `PUBLIC_BASE_URL` — public service URL used in magic links

Optional:

- `CHIPOTLE_API_BASE_URL` — defaults to `https://api.chipotle.litprotocol.com`
- `PORT` — mapped to `ROCKET_PORT` on startup for fly.io-style platforms

## Local development

```bash
cd lit-triggers
cargo +1.91 test
cargo +1.91 run
```

Run Postgres locally and set the environment above before starting the server. Migrations run on boot.

## API foundation

Authenticated routes:

- `GET /api/me`
- `POST /api/triggers`
- `GET /api/triggers`
- `GET /api/triggers/<id>`
- `PATCH /api/triggers/<id>`
- `DELETE /api/triggers/<id>`
- `GET /api/triggers/<id>/runs`
- `POST /api/triggers/<id>/test` — returns `501 Not Implemented` until worker phases

Usage API keys are accepted only on create/update and are never returned by API responses.

## Static admin UI

The dashboard at `/` is a plain HTML/CSS/JS app served from `lit-triggers/static/`.
It supports profile display, logout, trigger creation/edit/delete, recent run
inspection, and kind-specific config helpers for webhook, schedule, and EVM chain
event triggers.

For new triggers, users can either:

1. Paste a Lit/Chipotle admin API key into the browser-only mint flow. The UI
   calls Chipotle's `add_usage_api_key` endpoint directly with a narrow
   `execute_in_groups` scope, clears the admin key field after the mint attempt,
   and sends only the scoped usage key to lit-triggers.
2. Paste a pre-minted scoped usage key manually and skip browser minting.

The UI phrases CIDs as action code identities: a CID identifies the action code
content that will be executed with the supplied scoped usage key and group
permissions; it does not imply ownership of that CID.
