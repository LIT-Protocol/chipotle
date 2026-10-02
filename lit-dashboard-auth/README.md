# Dashboard password authentication

Email/password login for **new** dashboard accounts. The browser stretches
passwords with Argon2id (64 MiB, 3 iterations, 4 lanes), derives separate
encryption/authentication keys with HKDF-SHA-256, and encrypts the regular Lit
API key directly with AES-256-GCM. The storage service receives only the derived
auth credential and ciphertext. Normal dashboard operations still call the
existing Lit API and payments services with the decrypted key in `sessionStorage`.

The storage service is the `/auth/v1/*` surface of **lit-payments** (Rust,
Postgres, Railway): `lit-payments/src/dashboard_auth/`. This folder holds the
browser crypto source, the generated bundle's build script, the HTTP contract
([openapi.json](openapi.json)), the static-site configuration script, and the
test harness. Design and operator steps:
[plans/dashboard-auth-on-lit-payments.md](../plans/dashboard-auth-on-lit-payments.md).

There is **no forgotten-password recovery**, legacy-key import, migration,
account linking, or Lit API backend change. Email proves address ownership; it
cannot unlock existing ciphertext or replace credentials. Password changes require
the current password and atomically replace ciphertext, salt, and verifier.

## Develop and test

Use Node 22 and pnpm 10.11.0, a local Postgres (Postgres.app, Homebrew or
`docker run -e POSTGRES_PASSWORD=postgres -p 5432:5432 postgres:16`), and a
debug build of lit-payments:

```sh
cargo build --manifest-path lit-payments/Cargo.toml --bin lit-payments
cd lit-dashboard-auth
pnpm install --frozen-lockfile
pnpm build          # regenerates the committed browser crypto bundle + dist/crypto.js
pnpm test           # API tests over HTTP against the real service and a fresh database
pnpm serve          # dashboard on http://localhost:8080/dapps/dashboard/, auth on :8787
```

`test/server.mjs` creates a throwaway database on `TEST_DATABASE_URL` (default
`postgres://localhost:5432/postgres`; CI uses `postgres://postgres:postgres@…`),
starts `lit-payments/target/debug/lit-payments` (override with `LIT_PAYMENTS_BIN`)
with dashboard auth configured and `RESEND_API_BASE_URL` pointed at a local
capture server, and drops the database on exit. Unrelated lit-payments features
boot with dummy credentials and log their failures; nothing external is reached.

The browser tests need no live blockchain or email account:

```sh
# From e2e/ after installing its dependencies:
AGENT_SCREENSHOTS=1 pnpm exec playwright test --config playwright.password.config.ts
```

That Playwright configuration starts the real service and serves the actual
dashboard; email delivery and existing Lit/payment endpoints are simulated. It
does not run or claim to test a live TEE/chain. The generated browser worker is
committed at `lit-static/dapps/dashboard/password-crypto-worker.js`; rebuild after
crypto source changes. CI verifies it matches source. `hash-wasm` 4.12.0 is pinned,
self-hosted, and licensed in `browser/HASH-WASM-LICENSE`; no runtime crypto CDN is
used. The Argon2 lanes do not require browser threads or SharedArrayBuffer.

## Provision and deploy

lit-payments deploys through `.github/workflows/deploy-lit-payments.yml`: a push
to `main` deploys the Railway **staging** environment, a `v*` tag deploys
**production**. The auth tables are created by the service's own migrations at
boot. The feature stays off (every `/auth/v1` request answers 503) until the
variables below are set on the Railway `lit-payments` service:

| Variable | Value |
| --- | --- |
| `DASHBOARD_AUTH_SECRET` | `openssl rand -hex 32`; distinct per environment. Authenticates synthetic login parameters, rate-limit keys and transient outbox payloads, **not account keys**. |
| `DASHBOARD_AUTH_URLS` | Comma-separated dashboard URLs ending in `/dapps/dashboard/`. Only these exact origins are accepted; verification links point back at the one the request came from. |
| `DASHBOARD_AUTH_ENVIRONMENT` | `staging` or `production`. Part of every envelope's AAD; never change it once accounts exist. |
| `DASHBOARD_AUTH_COOKIE_SAMESITE` | `none` when the dashboard and lit-payments are not same-site (staging: `lit-static-next.pages.dev` ↔ `lit-payments-staging.up.railway.app`); `lax` (default) when they are (production: `dashboard.chipotle.litprotocol.com` ↔ `payments.litprotocol.com`). |

`RESEND_API_KEY` and `MAIL_FROM` are shared with the rest of lit-payments.
Optional: `DASHBOARD_AUTH_CLIENT_IP_HEADER` (default `X-Forwarded-For`, last
value wins) and `DASHBOARD_AUTH_OUTBOX_INTERVAL_SECS` (default 300).

Then enable the dashboard side by injecting the service origin at static deploy
time (`scripts/configure-static.py`, an empty value keeps the API-key/wallet UI):

- `next` (`lit-static-next.pages.dev`): always uses `LIT_PAYMENTS_STAGING_URL`.
- `main` (`dashboard.dev.litprotocol.com`): repository variable `LIT_AUTH_STAGING_URL`.
- production: repository variable `LIT_AUTH_PROD_URL`, used by the production static workflow.

Each dashboard you enable must also be listed in that environment's
`DASHBOARD_AUTH_URLS`. Do not point arbitrary Pages preview domains at
production auth. With `SameSite=None` the cookie is also `Partitioned`;
Safari still blocks cross-site cookies, so the `pages.dev` staging dashboard is
testable in Chrome and Firefox only. Production is same-site and works everywhere.
`GET /health` is lit-payments' process health endpoint, not a promise that email
configuration, Postgres, and the delivery provider are healthy.

## Routes and state

See [openapi.json](openapi.json) for the HTTP surface. All `/auth/v1` requests
require a configured dashboard `Origin` and `X-Chipotle-Auth: 1`. JSON mutations
also require a session CSRF token and matching `id`/`version` after login or email
verification. An opaque Secure/HttpOnly `__Host-` cookie authorizes only this
storage service. Idle expiry is 30 minutes; absolute expiry is 12 hours (one hour
for a signup-verification session). Postgres holds hashes of sessions and email
tokens (`dashboard_auth_*` tables).

State transitions:

- `verified`: address verified, no password chosen. A new email link can resume it.
- `reserved`: verifier registered, not yet creating a Lit account. Login with the
  chosen password can resume it.
- `creating`: a conditional `/signup/begin` write claimed the single creation
  attempt. The browser calls the unchanged Lit `new_account` endpoint, retains
  the returned API key in tab storage, encrypts it, then uploads ciphertext.
  If that call returns a known pre-creation rejection (400, 401, 402, 403, 404, 405, 413, 415, 422, or 429), the
  same tab records the rejection and the next attempt releases the claim via
  `/signup/begin` with `retry: true` and the abandoned operation id, which
  rotates the operation id. 408/499, other unknown statuses, 5xx, and network failures stay unresolved.
- `active`: initial ciphertext saved. The account binding cannot be replaced.
  Repeating the same initial upload is idempotent; replacement is rejected.

Email verification is confirmed by POST; scanners performing GET do not consume
links. Links use a URL fragment removed by the UI before further interaction.
Address changes require current-password login and confirmation sent to the new
address. The previous address receives a notification. Password changes also
send a notification. Outbox payloads are encrypted and retried by a five-minute
background job, with provider idempotency keys. Expired records are cleaned up.

Signup is **not** an atomic transaction across Postgres and the blockchain. A lost
creation response or closed tab before saving its key can orphan an account.
The UI never retries an ambiguous creation; only a received known pre-creation rejection in
the same tab unlocks a retry. If upload fails after a key was received,
that tab retains the original key and exact encrypted record for retry, and
exposes a manual backup field. Do not clear site storage or create another
account to resolve an ambiguous creation. This is the unchanged core API's
idempotency limitation, not password recovery.

## Security and operations

- Wallet addresses in encrypted records are unverified client metadata, not a
  global identity or ownership proof. Duplicate addresses across users are allowed
  so an attacker cannot reserve a victim's public wallet and block their signup.
  Authentication and record access use the user's ID, verified email, password
  credential, and session. The address remains authenticated by AES-GCM within
  each user's record and cannot change during a password update.
- A database dump permits offline password guessing; Argon2id is the protection
  against that. Auth rate limits protect online attempts, not leaked ciphertext.
  The stored fast verifier hashes an already stretched, domain-separated secret,
  never a plaintext password. There is no stored decryption key.
- A compromised service cannot derive the encryption key from the auth secret,
  but can steal auth credentials, corrupt records, or deny service. A malicious
  frontend can steal passwords and plaintext API keys. Existing JavaScript/XSS
  exposure of `sessionStorage` is unchanged. This is not a new TEE custody claim.
  lit-payments already holds Stripe and gas-funder credentials; login now shares
  its availability with billing.
- Password/email changes and logout revoke storage sessions. They cannot revoke
  API keys already exported or loaded in other tabs. Backend credential rotation
  remains a separate on-chain operation/design.
- Rate limits use conditional Postgres counters, per client IP and per
  email/identity; no in-memory cache is used for authorization. Query/session/token
  errors fail closed. The client IP is the last `X-Forwarded-For` value (Railway's
  edge appends one hop); behind a different proxy set `DASHBOARD_AUTH_CLIENT_IP_HEADER`.
- CSRF protection does not depend on cookie `SameSite`: the exact `Origin`
  check, the custom header (forcing a CORS preflight that `rocket_cors` only
  answers for allowlisted origins) and the per-session CSRF token apply always.
- Request bodies, tokens, ciphertexts, passwords and raw upstream errors are not
  logged. Operational failure logs use fixed messages, a SQLSTATE code, and the
  mail provider HTTP status only. Enable request logging only after verifying
  URL/header redaction.
- Back up Postgres (Railway backups). Invalidate sessions and email tokens after
  restoring a backup, and reconcile credential versions before reopening login:
  old ciphertext can require an old password. Database restoration cannot reset
  a forgotten password.
- Keep `DASHBOARD_AUTH_SECRET` backed up separately for pending mail. Rotation
  invalidates pending outbox payloads and synthetic parameter identity; clear
  expired/pending mail tokens and have users request new verification links.
  Existing registered password ciphertext does not depend on that service secret.
- Roll back service code only to a version compatible with the retained schema
  and record format. Once real users have password accounts, hiding the login
  choice is not an acceptable rollback. Keep their login path available.

The initial release keeps the dashboard framework-free. React modernization is a
separate follow-up, not a prerequisite for this feature.

## Password-manager compatibility

Signup, login, verification, and settings use native POST forms, stable field
names/labels, and `username`, `current-password`, and `new-password` autocomplete
hints. The verified signup email stays in the password form; password changes
include the account username. Generation hints request at least 15 characters.
The browser reads field values on submission, including autofill without input
events, and never disables paste. Successful signup, login, and password changes
navigate on the same dashboard origin after storage acknowledges success, leaving
submitted fields intact until unload so managers can detect completion. Failed
submissions stay on the form and clear passwords. Passwords never enter URLs or
application storage, and native form POSTs are intercepted by JavaScript.

CI checks those semantics, silent DOM autofill, Enter/button submission, and
success navigation. It does **not** automate proprietary save/generate prompts.
Before rollout, manually exercise generation/save, logout/autofill/login, and
update-password prompts on the final HTTPS dashboard domain with Chrome Password
Manager, Safari/Apple Passwords, 1Password, and Bitwarden. Use a disposable account
per manager, keep the generated password available, and verify the saved username
and origin. Browser settings and extensions control whether prompts appear.

References: [Chromium password forms](https://www.chromium.org/developers/design-documents/create-amazing-password-forms/)
and [1Password compatible forms](https://www.1password.dev/web/compatible-website-design).
