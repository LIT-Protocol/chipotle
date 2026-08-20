# Lit Chat

Private web chat served from a TEE. Implementation of `plans/tee-chat-app.md`
(PR #627): P1 scope plus the section 4.4 admin console.

> Your conversations are private — by architecture, not by policy.
> Your chat history is post-quantum encrypted at rest, and Lit cannot access it.

## What's here

| Part | What | Where |
|---|---|---|
| 1 | Chat web UI (conversations, SSE streaming, model picker, privacy panel) | `static/`, served by `lit-chat` |
| 2 | Inference: OpenRouter SSE proxy inside a dedicated Phala CVM | `src/openrouter.rs`, `docker-compose.phala.yml` |
| 3 | Off-TEE encrypted storage (envelope encryption, AAD-bound AES-256-GCM) | `migrations/`, `src/envelope.rs`, `src/store/` |
| 4 | Admin console: key custody/rotation, spend caps, breaker, audit | `admin-static/`, `src/admin/`, `lit-chat-admin` binary |

One crate, two binaries, one image:

- **`lit-chat`** (port 8000) — consumer app. Runs migrations, bootstraps
  OpenRouter keys from `encrypted_env` into the encrypted `provider_keys`
  store (first boot only).
- **`lit-chat-admin`** (port 8100) — admin console. Magic-link + mandatory
  passkey (WebAuthn) login, TEE-MAC'd roster, hash-chained audit log,
  write-only key custody (masked hints everywhere, no reveal endpoint),
  OpenRouter provisioning-API rotation.

## Security architecture (the short version)

- **User KEK**: `get_key("chat/v1/user/{user_ref}", "chat-kek")` against this
  CVM's own dstack socket, keccak-wrapped. Derived on demand, never stored.
- **Envelope**: random per-conversation DEK wrapped by the KEK; messages,
  titles, and usage metadata are AES-256-GCM with AAD binding
  `(conversation_id, message_id, seq, role)` — swapped/reordered/role-flipped
  rows fail decryption.
- **Sessions are TEE-signed, never DB-rooted** (`chat/v1/session-mac`); the DB
  holds only a revocation list. Magic-link identity travels in an
  enclave-signed token; the `magic_links` row is a replay guard only.
- **Anonymous → account migration** rewraps N DEKs (O(conversations)) inside
  one transaction; the email-derived ref (`HKDF(namespace, lower(email))`)
  IS the lookup — no email, email hash, or UUID mapping in the DB.
- **Admin isolation**: separate binary/port/ingress, separate DB role with no
  grants on chat tables (`db/roles.sql`), separate session MAC key, roster
  rows MAC'd in-enclave, audit rows MAC'd + hash-chained.
- **Metering** (shadow mode in P1): micro-USD accumulator encrypted under the
  user KEK; aggregate day totals (non-attributable) drive the spend breaker
  which degrades to accounts-only before hard-off.
- **Logging is content-free**: `info` level, no bodies/titles/emails/raw refs.

## Local development

One command on macOS with Docker installed:

```bash
./scripts/dev-up.sh
```

First run prompts for an optional OpenRouter API key and an optional admin
email, persists them (plus a generated dev master key) to a gitignored
`.env.local`, starts Postgres in Docker (`lit-chat-postgres`, port 5433,
named volume), builds both binaries without `--features production`, and
serves chat on http://localhost:8000 and the admin console on
http://localhost:8100. Sign-in codes print to the tailed logs (no email
needed). `--reconfigure` re-runs the prompts (keeping the master key — it
anchors every KEK and MAC, so regenerating it orphans local history);
`--down` stops the apps and the Postgres container.

Manual equivalent, if you'd rather drive it yourself:

```bash
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres
export PUBLIC_BASE_URL=http://localhost:8000
export LIT_CHAT_DEV_MASTER_KEY=$(openssl rand -base64 32)
export LIT_CHAT_DEV_ECHO_CODES=true            # sign-in codes go to the log
export OPENROUTER_API_KEY=sk-or-v1-...         # optional: real inference
cargo +1.91 run --bin lit-chat
```

Admin console:

```bash
export LIT_CHAT_ADMIN_RP_ID=localhost
export LIT_CHAT_ADMIN_ORIGIN=http://localhost:8100
export LIT_CHAT_BOOTSTRAP_ADMINS=you@litprotocol.com
cargo +1.91 run --bin lit-chat-admin
```

Tests: `cargo +1.91 test`.

## Deploy (own CVM, own governance)

1. `docker build -t <registry>/lit-chat:<tag> .` (from this directory), push,
   capture the `@sha256:` digest.
2. Substitute `${DOCKER_IMAGE_LIT_CHAT}` (and domain/AWS placeholders) into
   `docker-compose.phala.yml`, then `phala deploy -c ... --kms base` with the
   secrets as `-e KEY=VALUE` encrypted env (see the compose header for the
   full list).
3. After first boot (migrations applied), run `db/roles.sql` as the DB owner
   and switch each service's `DATABASE_URL` to its own role.
4. Verify: `GET /info` (compose_hash), `GET /attestation?nonce=<hex>`
   (nonce-bound TDX quote), and the key-continuity test in section 7.5 of the
   PRD before any real traffic.

Environment reference: see `src/config.rs` (every variable is read there).

## Explicit non-claims

This is a P1+admin implementation. Not included, per the PRD's phasing:
Stripe billing flush (the accumulator runs in shadow mode), Lit Action tool
calls, BYOK, in-enclave models (P3), per-session attestation nonce UX in the
frontend. Deletion is honest-mode: hard delete now, backups age out, no
"crypto-shred" language anywhere in the UI.
