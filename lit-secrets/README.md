# lit-secrets

Programmable credential access on top of Chipotle — a Turnkey-Secrets-style
"password manager for machines". Control plane only: it stores **ciphertexts**
sealed to per-tenant vault PKPs inside the Chipotle TEE, evaluates policy, and
signs short-lived grants. Plaintext only ever flows Chipotle → agent.

See `plans/programmable-credential-access.md` at the repo root for the design.

## How it works

```
                 ┌─────────────── lit-secrets (this service, Railway) ───────────────┐
  user ──login──▶│ secrets CRUD · agents (usage keys) · policy · grants · audit      │
                 │ Postgres: ciphertext, policy, hashes   (no plaintext, ever)       │
                 └────────────┬────────────────────────────────────┬─────────────────┘
                              │ provision tenant / mint keys       │ POST /api/grants
                              │ (master key, management API)      │ → signed grant + ciphertext
                              ▼                                    ▼
                 ┌──────────── Chipotle (TEE) ───────────┐     agent ── POST /lit_action ──▶ Chipotle
                 │ vault PKP + group per tenant           │            (reader action, agent's usage key)
                 │ encrypt action · reader action · CIDs  │     ◀── plaintext ─── Decrypt in TEE
                 └────────────────────────────────────────┘
```

- **Tenant** = one user → vault PKP + Chipotle group on the operator's account,
  plus a service usage key (runs the encrypt action).
- **Secret** = name → versioned ciphertext (AES-GCM under a TEE-derived key
  bound to the vault PKP). Release tier `plaintext` or `in_tee_only`.
- **Agent** = scoped Chipotle usage key (`execute_in_groups: [tenant group]`).
  Same key authenticates to this API. Revoke ⇒ removed on Chipotle.
- **Grant** = EIP-191-signed JSON `{v, tenant, name, version, pkpId,
  ciphertextHash, release, agent, iat, exp}`. The pinned reader action embeds
  the signer address, verifies the grant, checks it matches the ciphertext and
  vault, decrypts, returns the value to the caller.
- **Policy** (per secret): `allowed_agents`, `max_reads_per_day`, `not_after`.
  Evaluated before a grant is signed; every decision hits `access_log`.

## Environment

| Var | Purpose |
|---|---|
| `DATABASE_URL` | Postgres |
| `MAGIC_LINK_SIGNING_KEY` | base64, ≥32 bytes (`openssl rand -base64 32`) |
| `USAGE_KEY_ENCRYPTION_KEY` | base64, ≥32 bytes — AES key for stored Chipotle usage keys |
| `RESEND_API_KEY`, `MAIL_FROM` | magic-link email |
| `PUBLIC_BASE_URL` | e.g. `https://secrets.litprotocol.com` |
| `CHIPOTLE_API_BASE_URL` | default `https://api.chipotle.litprotocol.com` |
| `CHIPOTLE_MASTER_API_KEY` | master key of the operator's Chipotle account (must be funded) |
| `GRANT_SIGNING_KEY` | hex secp256k1 private key (`openssl rand -hex 32`) |
| `GRANT_TTL_SECS` | default 120 |
| `MAX_SECRET_BYTES` | default 16384 |

**Rotating `GRANT_SIGNING_KEY` changes the reader CID.** Existing tenants keep
the old CID in their group; `/api/tenant` reports `reader_cid_stale` and grants
return `503 reader_not_attached` until the new reader is attached to each group
(a re-attach job is a TODO — see plan).

## Run locally

```bash
createdb lit_secrets
export DATABASE_URL=postgres://localhost/lit_secrets
export MAGIC_LINK_SIGNING_KEY=$(openssl rand -base64 32)
export USAGE_KEY_ENCRYPTION_KEY=$(openssl rand -base64 32)
export GRANT_SIGNING_KEY=$(openssl rand -hex 32)
export RESEND_API_KEY=... MAIL_FROM=... PUBLIC_BASE_URL=http://localhost:8000
export CHIPOTLE_MASTER_API_KEY=...
cargo run
```

Tests: `cargo test`. Lint: `cargo clippy --all-targets -- -D warnings && cargo fmt --check`.

## API

Session / agent-access-token routes (`Authorization: Bearer <agent access token>` or cookie):

| Method | Path | |
|---|---|---|
| `GET` | `/api/tenant` · `POST /api/tenant/provision` | vault status / force provisioning |
| `POST/GET` | `/api/secrets` | create (seals value) / list |
| `GET/PUT/PATCH/DELETE` | `/api/secrets/<name>` | detail+versions / rotate / policy+release+disabled / delete |
| `POST/GET` | `/api/agents` · `DELETE /api/agents/<id>` | mint (key shown once) / list / revoke |
| `POST/GET` | `/api/actions` · `DELETE /api/actions/<id>` | attach/list/detach customer CIDs (in-TEE-only tier) |
| `GET` | `/api/audit?limit=` | access log |

Agent routes (`Authorization: Bearer <usage api key>`):

| Method | Path | |
|---|---|---|
| `POST` | `/api/grants` `{name, version?}` | policy → signed grant + ciphertext + reader code + ready `js_params` |
| `GET` | `/api/reference/<name>?version=` | ciphertext + `pkp_id` for in-TEE use |

Client: `sdk/lit-secrets.js` (served at `/sdk/lit-secrets.js`). Agent playbook: `SKILL.md`.

## Deployment (Railway)

Project **Lit Secrets** (`5da0f592-403d-4acd-bf1e-7194139cd33c`), service `lit-secrets`,
Postgres plugin, environment `production`. Source: `LIT-Protocol/chipotle`, root
directory `lit-secrets` (set via the API — the CLI has no flag for it), Dockerfile
build, healthcheck `/health`, sleeping disabled. Current URL:
`https://lit-secrets-production.up.railway.app` (custom domain
`secrets.litprotocol.com` TODO: `railway domain secrets.litprotocol.com` + CNAME).

Variables set: everything in the table above plus `ROCKET_SECRET_KEY`, `ROCKET_ADDRESS`,
`RUST_LOG`; `DATABASE_URL` is the `${{Postgres.DATABASE_URL}}` reference. The
`GRANT_SIGNING_KEY` is the production reader identity — never regenerate it casually.

After the PR merges, switch the branch: `railway service source connect --repo
LIT-Protocol/chipotle --branch main --service lit-secrets`.

## Verified against prod Chipotle (2026-08-27)

Full flow tested with a real account: provision (create_wallet → add_group →
add_action ×2 → add_action_to_group ×2 → service key) ≈ 25s; seal ≈ 0.3s;
grant + redeem ≈ 0.3–0.4s end to end; rotate → new version readable; policy
denials (`rate_limited`, `release_not_plaintext`) logged; `in_tee_only` secret
decrypted by a customer action attached via `/api/actions`; forged, tampered,
and expired grants rejected inside the TEE by the reader.

Known Chipotle behaviors to be aware of:
- **Revocation lag**: `remove_usage_api_key` invalidates the authz cache only on
  the replica that served it; other replicas may accept the key for up to 300s
  (chipotle issue filed). Treat revoke as "≤5 min", not instant.
- Action throws (e.g. `grant expired`) come back as HTTP **500** with a JSON
  string body, not 4xx. The SDK extracts the message.
- Occasional transient transport errors on the first Chipotle call after boot;
  management calls are safe to retry.

## Trust model

- Postgres compromise leaks ciphertexts (useless without the TEE + group
  permission) and *encrypted* Chipotle usage keys (need `USAGE_KEY_ENCRYPTION_KEY`).
- `GRANT_SIGNING_KEY` compromise lets an attacker forge grants — but only a
  holder of a still-valid tenant usage key can redeem one, and only for that
  tenant's vault. Rotate the key (new reader CID) to invalidate.
- The operator's `CHIPOTLE_MASTER_API_KEY` is the root of trust for group
  membership; treat like a cloud root credential.
- `in_tee_only` secrets are never released by this service under any policy;
  the only decryptors are CIDs the user attached themselves.
