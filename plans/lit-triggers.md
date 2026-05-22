# lit-triggers — reactive Lit Action runner

**Status:** draft for discussion
**Owner:** chris@litprotocol.com
**Target deploy:** fly.io (outside the TEE)

## One-line pitch

A service that fires a Lit Action when *something happens*. "Something" is one
of: an inbound webhook, a blockchain event, or a cron tick. The user supplies
a scoped usage API key that can do exactly one thing (call their action), so
the service holds no privileged credentials.

## Why a separate crate?

`lit-api-server` runs *inside* the TEE and must stay minimal/auditable.
Long-lived event listeners (websocket RPC, schedulers, inbound webhooks) are a
poor fit there: they hold connections open, retry, reconnect, persist state.
A separate outside-the-TEE service trades trust for operational sanity — and
the trust boundary is enforced by the scoped usage API key, not by being
inside the enclave.

This crate sits alongside `lit-payments/` in the workspace — same patterns
(Rocket + Postgres + sqlx + tracing). Different deploy target (fly.io vs
Railway) just because fly.io has better support for long-lived workers and
machines-with-state.

## Trust model

The whole design hinges on **scoped usage API keys**. `lit-triggers` is not the
Chipotle account system and does not need to link a user to a Chipotle account
as a first-class identity. It has its own auth model; its job is simply:

> automate your existing Chipotle stuff.

A user:

1. Logs into the `lit-triggers` dashboard.
2. Enters their Lit/Chipotle admin API key **in the browser only**.
3. The frontend calls Chipotle directly to create a usage API key scoped to
   `execute_in_groups: [N]` only — no PKP creation, no group management,
   nothing else.
4. The frontend sends only the scoped usage key (+ trigger config) to the
   `lit-triggers` backend.
5. The admin API key is dropped from JS memory and never leaves the browser.

If `lit-triggers` is compromised, the blast radius is "attacker can call the
already-scoped action with whatever inputs they want." They cannot mint
wallets, change groups, or pivot to other accounts. The service stores scoped
usage keys encrypted at rest (envelope-encrypted with a fly secret) and never
logs them.

The capability boundary is the usage key + group permissions + PKP permissions.
We do not treat a Chipotle account link as part of the v1 security model.

## Trigger types (v1)

### 1. Webhook
- Each trigger gets a unique inbound URL: `POST /webhook/<trigger_id>`.
- No HMAC/signature requirement in v1. Keep webhook setup dead simple.
- Basic abuse controls before enqueueing: IP-based rate limit, user/trigger
  rate limit, max request body size, and max queued runs per trigger.
- Request body (JSON or raw) becomes a value the user's action can read via
  the `params` field on the Lit Action call.
- Webhook handling is async: validate + rate-limit + enqueue a run + return
  `202 Accepted`; a worker calls Chipotle out-of-band.

### 2. Blockchain event
- EVM-only in v1. Configure: chain (chain_id), contract address, event
  signature (e.g. `Transfer(address,address,uint256)`), optional topic filters.
- Backend: websocket subscription where available, with a polling fallback
  using `eth_getLogs` and a watermark stored in Postgres so restarts don't
  miss or duplicate events.
- Each matched log is delivered to the action as decoded params + raw log.

### 3. Scheduled
- Standard cron expression (5-field or 6-field with seconds).
- Tokio-cron-scheduler or `tokio-cron-scheduler`-equivalent. Single-node
  scheduling for v1; if we ever scale beyond one fly machine, leader-elect via
  a Postgres advisory lock.

All three trigger types fan in to the same code path: **build a Lit Action
call request → POST to api.chipotle.litprotocol.com → record the result.**

## Data model (sketch)

```sql
-- The person who owns triggers. Same magic-link auth as lit-payments,
-- or do we authenticate via the Chipotle account API key itself? (open Q below)
CREATE TABLE users (
  id UUID PRIMARY KEY,
  email TEXT UNIQUE NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE triggers (
  id UUID PRIMARY KEY,
  user_id UUID NOT NULL REFERENCES users(id),
  name TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('webhook','chain_event','schedule')),

  -- The action body the user pastes in. We send `code` to Chipotle on every
  -- run; the node computes the CID server-side and treats it identically to
  -- a registered IPFS action. `action_cid` is computed by us on write
  -- (CIDv0, via the ipfs-hasher crate) for display + group-binding UX.
  action_code TEXT NOT NULL,
  action_cid TEXT NOT NULL,
  default_params JSONB NOT NULL DEFAULT '{}'::jsonb,

  -- The scoped usage API key, envelope-encrypted at rest.
  usage_api_key_ciphertext BYTEA NOT NULL,
  usage_api_key_nonce BYTEA NOT NULL,

  -- Optional display/debug metadata only. Do not use this as the auth boundary;
  -- the scoped usage key is the capability. In v1 we don't need to link the
  -- lit-triggers user to a Chipotle account.
  chipotle_account_address TEXT,

  -- Basic anti-abuse knobs. Defaults are set by the service config, but can be
  -- overridden later per user/trigger if needed.
  max_runs_per_minute INTEGER,
  max_queued_runs INTEGER,

  -- Kind-specific config (webhook secret, chain event filter, cron expr).
  config JSONB NOT NULL,

  enabled BOOLEAN NOT NULL DEFAULT true,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Per-run audit log. Trimmed by a background job.
CREATE TABLE trigger_runs (
  id UUID PRIMARY KEY,
  trigger_id UUID NOT NULL REFERENCES triggers(id) ON DELETE CASCADE,
  started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  finished_at TIMESTAMPTZ,
  status TEXT NOT NULL,  -- queued | running | success | failed | retrying
  input JSONB,
  response JSONB,
  error TEXT,
  attempt INTEGER NOT NULL DEFAULT 1
);

-- For chain_event triggers only: where we left off.
CREATE TABLE chain_watermarks (
  trigger_id UUID PRIMARY KEY REFERENCES triggers(id) ON DELETE CASCADE,
  last_block BIGINT NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

## HTTP surface (admin)

```
POST   /api/triggers              create
GET    /api/triggers              list mine
GET    /api/triggers/:id          get one
PATCH  /api/triggers/:id          edit (toggle enabled, rotate key, etc.)
DELETE /api/triggers/:id          delete
GET    /api/triggers/:id/runs     paginated run history
POST   /api/triggers/:id/test     dry-run fire (uses recorded payload or a stub)
```

## HTTP surface (public)

```
POST   /webhook/:trigger_id       webhook receiver
GET    /health                    fly health check
```

## Background workers

Spawn on boot from `main.rs`:

1. **Scheduler** — loads all enabled `kind='schedule'` triggers, registers
   them with the cron scheduler, fires them.
2. **Chain listener** — one task per (chain, RPC endpoint). Multiplexes all
   `kind='chain_event'` triggers for that chain so we don't open N
   subscriptions. Reads watermarks on boot; updates them after each delivered
   batch.
3. **Run dispatcher** — pulls `status='queued'` rows from `trigger_runs`,
   POSTs to Chipotle, writes the result. Exponential backoff on transient
   errors. Per-trigger concurrency limit.
4. **Rate limiter / admission control** — rejects webhook requests before
   enqueueing if an IP, user, trigger, body-size, queue-depth, or concurrency
   limit is exceeded. This is intentionally basic in v1; no billing gate and
   no webhook HMAC/signature requirement yet.
5. **Janitor** — trims `trigger_runs` older than N days; purges expired
   sessions.

## Decisions (locked in 2026-05-20)

### 1. Auth — admin key never touches our backend

Two distinct concerns, kept separate:

- **Logging into the management UI.** Magic-link (port the `auth` module
  from `lit-payments` — same email-based HMAC-signed-token flow). On first
  login we create a `users` row keyed by email. This is `lit-triggers`' own
  auth model and does not need to be linked to a Chipotle account in v1.

- **Provisioning the scoped usage key for a trigger.** The user pastes their
  **Lit/Chipotle admin API key into the browser, not the server.** Frontend JS:
  1. Calls Chipotle's `add_usage_api_key` directly from the browser with
     a narrow scope: `execute_in_groups: [<group_id>]`, everything else false.
  2. Receives the freshly minted `usage_api_key`.
  3. POSTs *only* the usage key (+ the group/action config) to `lit-triggers`.
  4. The admin API key is dropped from JS memory and never persisted anywhere.

  The backend treats the scoped usage key as the capability. It may optionally
  derive Chipotle account metadata for display/debugging, but it does not need
  a hard "link this `lit-triggers` user to this Chipotle account" flow.

  Optional escape hatch: the user can also paste a pre-made usage key
  directly, skipping the auto-mint flow, if they prefer to manage scopes
  themselves.

This is the right answer — it preserves the property that the most powerful
credential (admin key) is only ever present in the browser tab for a few
hundred milliseconds during setup.

### 1b. Abuse controls — basic quotas, not billing or webhook signatures

Anything that runs on Chipotle is monetized Chipotle usage, so the goal is not
to prevent Chipotle calls. The goal is to keep `lit-triggers` itself from being
abused as a free unauthenticated queue/CPU sink.

For v1:

- No payment gate for `lit-triggers` itself.
- No HMAC/signature requirement for webhooks yet.
- Webhook requests are async: accept, enqueue, return `202`, execute later.
- Enforce simple limits before enqueueing:
  - per-IP request rate,
  - per-user request/run rate,
  - per-trigger request/run rate,
  - max request body size,
  - max queued runs per trigger,
  - per-trigger concurrency cap in the dispatcher.

This gives us enough protection for a first public/beta version while keeping
setup friction low. Billing or stronger webhook auth can be added later if real
usage patterns justify it.

### 2. Multi-tenant.

One fly app, many users. Per-user data isolation enforced at the SQL layer
via a `user_id` foreign key on every row, and at the Rocket layer via a
request guard that derives `user_id` from the session cookie.

### 3. Chains in v1.

Ethereum mainnet, Base, Arbitrum, BSC, Polygon — all EVM, one shared
listener implementation parameterized by `chain_id` + RPC URL.

**Tron** is the odd one out. It runs an EVM-compatible execution layer but
its RPC surface is *not* `eth_*`-compatible — it uses TronGrid's HTTP API
(`/walletsolidity/getblock`, `/wallet/getcontractinfo`, log filtering via
`/wallet/gettransactioninfobyblocknum`, etc.). Plan: ship the EVM listener
first (PR 4), then add a separate Tron listener (PR 4b) that conforms to
the same internal `EventSource` trait. Don't try to shoehorn Tron into the
EVM path.

Chain config lives in `config.rs` as a static table:

```rust
pub struct ChainSpec {
    pub key: &'static str,          // "ethereum", "base", "tron", ...
    pub chain_id: u64,
    pub kind: ChainKind,            // Evm | Tron
    pub default_rpc_envvar: &'static str,
    pub default_ws_envvar: Option<&'static str>,
}
```

### 4. Payload → params.

Whole event/payload passed as `params.event`. No DSL. The user's action
already runs JS — it can pluck fields itself.

### 5. Action specification — inline only (CID computed by us).

The Lit node accepts either an inline `code` string or a CID; if you pass
`code` it computes the CID server-side and looks the action up by it. **So
the user doesn't need to publish to IPFS at all** — they paste JS into our
UI, we store the JS, and at call time we send `code` to Chipotle.

We do, however, need to compute the CID *ourselves* for two reasons:

1. To help the user wire up groups (groups grant permission to specific
   CIDs — the user needs the CID we'll be calling with so they can
   `manage_ipfs_ids_in_groups` it in).
2. To stably identify "the same action" across edits (CID changes when the
   code changes — handy for showing "this trigger's action has been edited"
   in the UI).

Computation: pull in the existing `ipfs-hasher` crate that `lit-core` already
uses in `lit-core/lit-core/src/utils/ipfs.rs:266`. CIDv0 (`Qm…`), same as
what the node computes — verified by `ipfs_cid_of_content()` against
`IpfsHasher::default()`. We'll expose a small wrapper:

```rust
pub fn cid_for_action_code(code: &str) -> String { /* IpfsHasher::default().compute(code.as_bytes()) */ }
```

Schema impact: drop the `action_cid` column. Keep `action_code TEXT NOT NULL`
and add a computed-on-write `action_cid TEXT NOT NULL` for display/lookup.

Important terminology: nobody "owns" a CID. A CID identifies action code
content, not a user, account, PKP, or payment relationship. Users own/control
PKPs and use groups, then grant those groups access to CIDs. If two users paste
the same action code, they naturally get the same CID, and that's fine. The UI
should phrase this as "execute CID X using your scoped usage key / group
permissions," not "execute another user's CID."

### 6. Naming — `lit-triggers`.

Confirmed.

## Still to nail down (lower priority)

- **Retry policy.** Default: 3 attempts, exponential backoff (1s → 5s → 30s),
  then mark failed. Webhook responses are 202-and-run-async. Flagging this
  as not-yet-locked-in but I'll proceed with these defaults unless you push
  back.

## Out of scope for v1

- Non-EVM chains (Solana, Bitcoin, Cosmos).
- Action chaining / DAGs (output of one trigger feeds another).
- A built-in template gallery / no-code action builder.
- Multi-region failover. Single fly machine is fine until it isn't.
- A Slack/email destination that *isn't* a Lit Action. Everything goes
  through an action; if you want to email, write an action that emails.

## Rough milestones

1. ✅ **PR 1 — foundation (implemented 2026-05-22).** Crate skeleton, Postgres + sqlx, magic-link auth, encrypted scoped usage-key storage, `users` + `triggers` tables, CRUD API, no workers yet. Tests pass with `cargo +1.91 test`.
2. ✅ **PR 2 — webhook trigger (implemented 2026-05-22).** Public webhook endpoint + run dispatcher +
   basic IP/user/trigger rate limits + async enqueue + call out to Chipotle.
   End-to-end first trigger type working.
3. **PR 3 — scheduled trigger.** Cron scheduler + worker integration.
4. **PR 4 — chain event trigger.** EVM listener with watermarks. Biggest
   chunk by far.
5. **PR 5 — admin UI.** Static HTML/JS dashboard for managing triggers, in
   the same spirit as `lit-payments/static/`.
6. **PR 6 — fly.io deploy.** `fly.toml`, secrets, health checks, docs.
