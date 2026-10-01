# Dashboard password authentication

A TypeScript Cloudflare Worker with D1 storage for **new** dashboard accounts. The
browser stretches passwords with Argon2id (64 MiB, 3 iterations, 4 lanes), derives
separate encryption/authentication keys with HKDF-SHA-256, and encrypts the regular
Lit API key directly with AES-256-GCM. The Worker receives only the derived auth
credential and ciphertext. Normal dashboard operations still call the existing
Lit API and payments services with the decrypted key in `sessionStorage`.

There is **no forgotten-password recovery**, legacy-key import, migration,
account linking, or Lit API backend change. Email proves address ownership; it
cannot unlock existing ciphertext or replace credentials. Password changes require
the current password and atomically replace ciphertext, salt, and verifier.

## Develop and test

Use Node 22 and pnpm 10.11.0. Run package commands from `lit-dashboard-auth/`:

```sh
pnpm install --frozen-lockfile
pnpm build
pnpm check
pnpm test
pnpm migrate:local
pnpm dev
```

Create an ignored `.dev.vars` containing random `AUTH_SECRET` (at least 32
characters) and `RESEND_API_KEY`. Set `MAIL_FROM` to a verified sender in
`wrangler.jsonc` for live local email testing. The dashboard serves at
`http://localhost:8080/dapps/dashboard/`, with the auth Worker on `localhost:8787`.
Use the same hostname, not a mix of `localhost` and `127.0.0.1`. For the complete
real Lit API stack, follow the repository's local runbook.

The Worker and browser tests need no live blockchain or email account:

```sh
# From e2e/ after installing its dependencies:
AGENT_SCREENSHOTS=1 pnpm exec playwright test --config playwright.password.config.ts
```

This separate Playwright configuration starts real local Workers/D1 and serves
the actual dashboard. Email delivery and existing Lit/payment endpoints are
simulated. It does not run or claim to test a live TEE/chain. Node tests use D1 in
Miniflare and the exact browser crypto source. The generated browser worker is
committed at `lit-static/dapps/dashboard/password-crypto-worker.js`; rebuild after
crypto source changes. CI verifies it matches source. `hash-wasm` 4.12.0 is pinned,
self-hosted, and licensed in `browser/HASH-WASM-LICENSE`; no runtime crypto CDN is
used. The Argon2 lanes do not require browser threads or SharedArrayBuffer.

## Provision and deploy

Deployment is separate from merging this code. Do the following for **each** of
staging and production:

1. Create a D1 database named `chipotle-auth-staging` / `chipotle-auth-production`.
   Keep read replication disabled for auth-state freshness. Record its ID.
2. Choose a Worker custom domain under the same site as the dashboard (e.g.
   `auth.chipotle.litprotocol.com`). Confirm Cloudflare zone ownership, the exact
   Pages dashboard origin, and an email sender/domain verified with Resend.
3. Configure the corresponding GitHub environment with variables
   `CLOUDFLARE_ACCOUNT_ID`, `AUTH_DATABASE_ID`, `AUTH_DOMAIN`,
   `AUTH_DASHBOARD_URL` (full URL ending in `/dapps/dashboard/`), and
   `AUTH_MAIL_FROM`. Set secrets `CLOUDFLARE_API_TOKEN`, `AUTH_SECRET`, and
   `RESEND_API_KEY`. The Cloudflare token needs Worker deployment, D1 migration,
   and the custom-domain permissions for that account/zone. Use distinct random
   auth secrets per environment. This secret authenticates synthetic login
   parameters and encrypts transient email-outbox payloads, **not account keys**.
4. Run the **Dashboard password authentication** workflow manually for that
   environment. It runs tests, applies SQLite migrations, deploys the Worker,
   and provisions its secrets. Initial deployment fails closed until secrets are
   set. Subsequent releases must retain backwards-compatible schemas and the
   same `AUTH_SECRET` unless intentionally rotating it.
5. Test email verification/delivery, signup, signup-upload retry, login, settings,
   invalid credentials, and the custom-domain cookie/CORS behavior in staging.
6. Set repository variable `LIT_AUTH_STAGING_URL` / `LIT_AUTH_PROD_URL` to the
   corresponding HTTPS **origin**, then run the existing static deployment.
   The main Pages project (`dashboard.dev.litprotocol.com`) uses staging;
   production uses the explicit production static workflow. An empty variable
   disables the password choice and keeps the existing API-key/wallet UI. Do not
   point arbitrary Pages preview domains at production auth.

   The Worker accepts exactly one dashboard origin (`AUTH_DASHBOARD_URL`), and
   its `SameSite=Lax` cookie is only sent when the auth domain is same-site with
   the dashboard. `*.pages.dev` is a public suffix, so a dashboard served only
   from `pages.dev` can never use this service. The `next` Pages project
   (`lit-static-next.pages.dev`) therefore deploys with password login disabled.

The auth Worker doesn't serve static files or proxy Lit API traffic. It has a
separate deployment so frontend and auth versions can be staged independently.
The checked-in Wrangler config is local only; `scripts/deploy.mjs` generates a
validated ignored deployment config from environment settings. It never embeds
secrets in the static site. `GET /health` is a process health endpoint, not a
promise that email configuration, D1, and the delivery provider are healthy.

## Routes and state

See [openapi.json](openapi.json) for the HTTP surface. All `/auth/v1` requests
require the configured dashboard `Origin` and `X-Chipotle-Auth: 1`. JSON mutations
also require a session CSRF token and matching `id`/`version` after login or email
verification. An opaque Secure/HttpOnly/SameSite=Lax cookie authorizes only this
storage service. Idle expiry is 30 minutes; absolute expiry is 12 hours (one hour
for a signup-verification session). D1 holds hashes of sessions and email tokens.

State transitions:

- `verified`: address verified, no password chosen. A new email link can resume it.
- `reserved`: verifier registered, not yet creating a Lit account. Login with the
  chosen password can resume it.
- `creating`: a conditional `/signup/begin` write claimed the single creation
  attempt. The browser calls the unchanged Lit `new_account` endpoint, retains
  the returned API key in tab storage, encrypts it, then uploads ciphertext.
- `active`: initial ciphertext saved. The account binding cannot be replaced.
  Repeating the same initial upload is idempotent; replacement is rejected.

Email verification is confirmed by POST; scanners performing GET do not consume
links. Links use a URL fragment removed by the UI before further interaction.
Address changes require current-password login and confirmation sent to the new
address. The previous address receives a notification. Password changes also
send a notification. Outbox payloads are encrypted and retried by a five-minute
Cron Trigger, with provider idempotency keys. Expired records are cleaned up.

Signup is **not** an atomic transaction across D1 and the blockchain. A lost
creation response or closed tab before saving its key can orphan an account.
The UI never blindly retries creation. If upload fails after a key was received,
that tab retains the original key and exact encrypted record for retry, and
exposes a manual backup field. Do not clear site storage or create another
account to resolve an ambiguous creation. This is the unchanged core API's
idempotency limitation, not password recovery.

## Security and operations

- A D1 dump permits offline password guessing; Argon2id is the protection against
  that. Auth rate limits protect online attempts, not leaked ciphertext. The
  stored fast verifier hashes an already stretched, domain-separated secret,
  never a plaintext password. There is no stored decryption key.
- A compromised Worker cannot derive the encryption key from that auth secret,
  but can steal auth credentials, corrupt records, or deny service. A malicious
  frontend can steal passwords and plaintext API keys. Existing JavaScript/XSS
  exposure of `sessionStorage` is unchanged. This is not a new TEE custody claim.
- Password/email changes and logout revoke storage sessions. They cannot revoke
  API keys already exported or loaded in other tabs. Backend credential rotation
  remains a separate on-chain operation/design.
- Rate limits use conditional D1 counters, per IP and per email/identity; no KV
  cache is used for authorization. Query/session/token errors fail closed.
- Request bodies, tokens, ciphertexts, passwords and raw upstream errors are not
  logged. Operational failure logs use fixed messages and mail HTTP status only.
  Enable request logging only after verifying URL/header redaction.
- Back up D1 and verify Time Travel retention for the selected plan. Target RPO
  <=15 minutes and RTO <=4 hours only after a measured restore drill. Invalidate
  sessions and email tokens after restoring a backup, and reconcile credential
  versions before reopening login: old ciphertext can require an old password.
  Database restoration cannot reset a forgotten password.
- Keep `AUTH_SECRET` backed up separately for pending mail. Rotation invalidates
  pending outbox payloads and synthetic parameter identity; clear expired/pending
  mail tokens and have users request new verification links. Existing registered
  password ciphertext does not depend on that service secret.
- Roll back Worker code only to a version compatible with the retained schema
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
