# Email/password login and dashboard modernization

Status: authentication implemented; the storage service moved from Cloudflare Workers/D1 to lit-payments (see [dashboard-auth-on-lit-payments.md](dashboard-auth-on-lit-payments.md)), which replaces the Cloudflare provisioning described below. React modernization remains a separate follow-up. See [implementation and rollout instructions](../lit-dashboard-auth/README.md).
Date: 2026-09-30.

## Outcome and recommended decisions

Make email/password the default dashboard signup and login experience for **new accounts only**. Existing API-key users keep using API-key login, with no migration, import, or password-enrollment path. Keep wallet login unchanged. New password users should never have to copy a master API key to finish onboarding. They can create usage keys for integrations after signing in.

Agreed scope: **no forgotten-password recovery in v1**. Tell users to save their password in a password manager. Email verification establishes an identity; it cannot reset the password or decrypt the account key. Do not implement recovery envelopes, recovery keys, recovery Lit Actions, reset tokens, or support/admin reset endpoints.

Agreed architecture (updated after discussion):

- Keep static frontend hosting on Cloudflare Pages.
- Encrypt and decrypt the API key **in the browser**, using Argon2id and authenticated encryption. Put the decrypted key into the existing `sessionStorage` flow and use the current SDK, billing, and API-key authentication paths.
- Keep `lit-api-server` unchanged for this feature: no password endpoints, dashboard proxy, cookie authentication, or session decryption inside the TEE.
- Add a separate TypeScript auth/storage service on **Cloudflare Workers**, backed by **D1** (managed SQLite). It owns verified email identities, authorization to read/write envelopes, and email verification. It is not on the path of ordinary dashboard API calls.
- Use `sessionStorage`, matching current API-key login. Persistent plaintext `localStorage`/“remember me” is not part of this change.
- Ship authentication in the current modular JavaScript dashboard first. Migrate to React + TypeScript + Vite separately, preserving hosting and URLs.

After login, the browser security model is the same as current API-key login: dashboard JavaScript can read the plaintext master key. The new attack surfaces are encrypted remote backups and password guessing. This plan does not introduce HttpOnly custody of the master key or claim that changing a password can revoke a key already loaded into a browser.

## What exists today

These findings come from this checkout; they do not constitute an inspection of the live Cloudflare, Railway, DNS, or database accounts.

| Area | Current implementation and evidence |
| --- | --- |
| Signup | [`auth.js`](../lit-static/dapps/dashboard/auth.js) collects account name, description, and email, calls `newAccount`, saves the returned key in `sessionStorage`, and shows the key banner. |
| Backend issuance | [`account_management.rs`](../lit-api-server/src/core/account_management.rs), `new_account`, obtains a dstack-derived wallet secret, base64-encodes that secret as the API key, creates the on-chain account, and registers the wallet derivation. **The master key contains wallet private-key material.** |
| Email | [`NewAccountRequest`](../lit-api-server/src/core/v1/models/request.rs) accepts optional email. Account creation forwards it to Stripe best-effort; it is not an authenticated identity or verified account-recovery channel. |
| Existing access | API keys are held in `sessionStorage`. Wallet/ChainSecured access uses a connected signer and on-chain ownership; it is a distinct account mode, not simply another password-account credential. |
| Frontend | [`index.html`](../lit-static/dapps/dashboard/index.html) is 773 lines, but behavior is already split across `auth.js`, `billing.js`, `wallets.js`, `actions.js`, etc. There is no bundler/build step. |
| Local hosting | [`local_test.sh`](../local_test.sh) serves `lit-static/` with `static-web-server` on port 8080. This is not the deployment workflow used for Cloudflare production. |
| Production frontend | [`deploy-prod-3-static.yml`](../.github/workflows/deploy-prod-3-static.yml) checks out an explicit tag/SHA, injects the production API URL, payments URL, and commit identifier, then runs `pages deploy lit-static --project-name=lit-static-chipotle --branch=main`. The workflow exposes `workflow_dispatch`; confirm orchestration in the live release setup rather than relying on its header comment. |
| Development frontend | [`deploy-static.yml`](../.github/workflows/deploy-static.yml) deploys `main` to `lit-static-dev` and `next` to `lit-static-next`. **Both inject the staging API URL** `https://test.chipotle.litprotocol.com`. A main-branch push is not a production frontend release. |
| Public address | The root README identifies `https://dashboard.chipotle.litprotocol.com/dapps/dashboard/`. Confirm its custom-domain binding in Cloudflare before any cutover. |
| API hosting | The Rust API runs in Phala/dstack, separately from the static frontend; see [`docker-compose.phala.yml`](../docker-compose.phala.yml). |
| Other services | `lit-payments`, `lit-triggers`, and `lit-agent-keychain` have Railway configuration and Postgres usage. Their files establish deployment conventions, not proof of current live capacity or backup settings. |

## UX and identity semantics

The sign-in screen offers **Email and password**, **API key**, and **Wallet**, with email/password selected by default. Hide the managed/ChainSecured terminology behind short explanatory help. A wallet sign-in must still clearly communicate that the connected wallet authorizes transactions.

Signup collects email and password; move optional account description into settings and consider deriving a default account name. Allow password-manager autofill, paste, show/hide password, keyboard submission, and accessible field errors. Use `email`, `new-password`, and `current-password` autocomplete appropriately. Proposed policy: at least 15 characters without MFA, support at least 64 characters, no composition rules, no silent truncation or password trimming, and reject commonly breached passwords without sending plaintext to a third party.

Verify the email before creating/funding an on-chain account. The verification link opens a restricted signup session; then the user sets the password and completes creation. This avoids retaining plaintext passwords while waiting for email. Provide resend and expiration UI. Show this copy next to the password field, before the user completes signup:

> Save this password in your password manager. There’s no “Forgot password” option, and we can’t reset it for you.

Repeat a brief save-password reminder on successful signup, then open the dashboard without the existing mandatory API-key storage banner. Use a real password-manager-compatible form; do not add a manual API-key backup requirement or a blocking “I saved it” ceremony. Include the same policy in sign-in help and account settings. Do not display a “Forgot password” link or promise email/support recovery.

If someone forgets their password, a separately saved **master API key** still works through existing API-key login, and an already-unlocked tab can still access the account. Neither email ownership nor a saved usage key substitutes for the master key. V1 does not offer password replacement through those paths: changing an existing password requires the current password. Repeated signup must not overwrite an existing password identity as a reset bypass; there is no existing-account enrollment flow. If the user has lost the password and has no usable master key or unlocked session, this product offers no way to restore access. Do not present wallet login as a fallback for a managed account without a separately designed linking flow.

Use a separate `auth_method` (`password`, `api_key`, `wallet`) from the existing account `mode` (`api`, `sovereign`). Password login is the access method for newly created password-enabled managed accounts; it must not accidentally switch a wallet account into managed custody.

For v1, one verified email identity maps to one **newly created** managed account. Email/password signup always creates a new account; it does not attach credentials to an existing API-key or wallet account. Do not offer “Add password,” “Import API key,” account linking, migration prompts, or enrollment endpoints. Existing API-key users keep their current login and settings experience. Password/email settings appear only for accounts created through the new flow.

Never look up or attach existing accounts by Stripe email. Reusing an email that appears in a legacy Stripe record does not migrate that legacy account, its funds, keys, permissions, or billing identity. A person with an existing account can choose to create a separate new password account, but this is fresh signup, not migration. Duplicate verified identities in the new auth database must be handled without overwriting or merging accounts.

Wallet login continues to work independently. Adding password signup must never transfer a ChainSecured account or replace wallet transaction authorization. The three choices are supported access methods, not interchangeable credentials for every account.

## Encryption and trust model

### Password envelope (browser)

Use a maintained, pinned Argon2id WASM implementation in a Web Worker, and native Web Crypto for AES-GCM, HKDF, and randomness. Review the crypto dependency as part of implementation; the browser crypto addition is part of this feature, independent of React. Do not implement primitives by hand. Web Crypto defines these operations in its [specification](https://www.w3.org/TR/WebCryptoAPI/).

Proposed version-1 format:

1. Generate a random 128-bit salt using `crypto.getRandomValues`. Derive a 256-bit root from the password using **Argon2id v19**, initially **64 MiB memory, 3 iterations, 4 lanes**. This is the memory-constrained profile in [RFC 9106](https://www.rfc-editor.org/rfc/rfc9106.html). Benchmark the WASM implementation on supported desktop and mobile browsers, including browsers without cross-origin-isolated threading. Keep the UI responsive with a worker and target about one second on representative devices; finalize parameters from measurements, not silent runtime downgrades.
2. Use HKDF-SHA-256 to derive separate 256-bit values from that root: an **API-key encryption key** and an **authentication secret**. Use distinct versioned `info` labels, environment and immutable identity binding, and a specified HKDF salt. Freeze the exact encoding in interoperability fixtures. The encryption key and Argon2 root stay in the browser; only the authentication secret is sent to the auth service.
3. Encrypt the exact existing API-key bytes **directly** under the derived encryption key using AES-256-GCM, a fresh random 96-bit nonce, and a full 128-bit authentication tag. There is no separate random data key or key-wrapping layer.
4. Bind the ciphertext to canonical, length-delimited authenticated associated data: environment, envelope version, immutable identity UUID, immutable account identifier, credential version, and purpose (`api-key`). Do not bind mutable email text.
5. Store one versioned encrypted record containing the Argon2 salt/parameters, HKDF format/version, cipher identifier, nonce, ciphertext, tag, and the binding metadata needed to reconstruct associated data. The Worker stores the authentication verifier alongside that record; no encryption key is persisted.

```text
password + random salt → Argon2id → HKDF-SHA-256
                                     ├─ encryption key → AES-256-GCM(API key)
                                     └─ authentication secret → Worker verifier
```

The browser validates algorithm versions and KDF limits **before** allocating memory or running WASM. Use fresh salts/nonces when changing passwords. Upgrades to KDF parameters require browser rederivation and an atomic update of both the envelope and the authentication verifier.

### Authorizing access to stored envelopes

Local decryption verifies the password to the browser, but does not authorize database writes. The separate service must authenticate reads and changes; an email address or a client assertion that “decryption worked” is not proof.

Proposed simple protocol: send the separately derived authentication secret over TLS at registration/login. The Worker stores only a versioned, domain-separated SHA-256 digest of that 256-bit derived secret, bound to the identity, never its plaintext, the root, or the encryption key. Compare fixed-length digests using a timing-safe primitive supported by the runtime. This secret is password-equivalent **for the auth service**, but is not the decryption key. The expensive password stretching remains Argon2id in the browser: a password guess against the database must still reproduce that derivation. This fast verifier applies only to the derived credential, never directly to a human password; it avoids adding a second memory-heavy Argon2 invocation in Workers. Apply server-side abuse limits and review the construction alongside the browser derivation. The browser-derived secret is never accepted by `lit-api-server`. Review this composition before implementation; it is conventional TLS credential authentication, not a PAKE or a zero-knowledge claim.

A pre-login parameter endpoint returns the identity binding and Argon2 parameters needed for derivation. Return stable, server-keyed synthetic parameters for unknown emails, with the same response shape and profile, so absence is not trivially exposed. Login responses are generic; perform the same digest-verification path against a dummy record for unknown identities, with abuse limits before database operations. Authenticated login returns the encrypted API-key record plus a short-lived service session. A stolen auth secret allows envelope access and account-setting actions subject to reauthentication; treat it as a secret in logs and transport.

Both encrypted records and verifiers allow offline password guessing after a database dump. Argon2 slows this down; rate limiting protects online attempts only. Never store the root or encryption key as a verifier. Discard password/root/encryption key/authentication secret after use; keep only the API key needed by the existing dashboard flow. Clear worker buffers where practical, without claiming guaranteed zeroization in JavaScript.

### Password changes; no forgotten-password reset

Password change requires the **current password**, recent auth-service reauthentication, and successful local decryption of the stored API key. Generate a fresh Argon2 salt, derive the new encryption key and authentication secret from the new password, and re-encrypt the same API-key bytes with a fresh AES-GCM nonce and incremented credential-version binding. Submit the new ciphertext record and authentication secret together. Use an atomic version-checked update for the verifier, ciphertext, salt/parameters, and credential version, then revoke auth-service sessions and require normal login. A failed or losing update leaves the old credentials and ciphertext intact; concurrent changes cannot overwrite a successful update. The API key and wallet identity stay unchanged. KDF upgrades use the same decrypt-and-re-encrypt procedure.

Email changes require current-password reauthentication, verification of the new address, notification to the old address, and invalidation of outstanding email-change proofs. Email-verification links only authorize signup/address verification; they never grant access to an existing encrypted vault, replace its verifier, or reset its password. Use random, expiring, single-use verification tokens stored as hashes; consume on POST, not link-preview GET. Rate-limit sending/redemption and exclude token-bearing URLs from logs and referrers.

There is no recovery ciphertext or app-held key that unlocks the stored account envelope. Re-encrypting a single small API key on password changes is inexpensive, so no additional key-wrapping layer is needed. This describes the new auth service's capabilities, not a new guarantee about the existing Lit backend's wallet-key custody. Browser code and the existing API still handle plaintext master keys as they do today.

**Revoking auth-service sessions does not revoke API keys already decrypted into browsers or exported elsewhere.** Those clients can continue using the existing API until the underlying credential is invalidated through the account's supported mechanisms. The current key is wallet secret material; replacing it is not a database update. Define a separate on-chain ownership/key-compromise response before promising remote logout or “secure my account” revocation. Ordinary password changes preserve integrations.

## Browser state and backend integration

Signup and login end by calling the existing `setApiKey` path with the locally decrypted master key. Continue using `sessionStorage`, the existing SDK, and direct API/payments calls, including usage-key overrides. No `/dashboard/v1` gateway or changes to `lit-api-server` authentication are needed. Wallet login remains independent.

A reload in the same tab can continue using its stored API key. Closing the tab normally ends that storage; do not promise an absolute expiry or remote revocation for bearer API keys. The password, root, encryption key, and auth secret must not be persisted alongside the API key. On logout, clear the existing API-key/override/client state and revoke the auth-service session. A failed remote logout must not prevent local clearing. Account switching must clear the prior key before adopting another account.

Plaintext `localStorage` would extend persistence beyond current behavior and is deferred. Malicious JavaScript on the dashboard origin can read `sessionStorage`, just as with today's API-key login. A malicious frontend release can also steal entered passwords. Browser encryption does not remove trust in frontend delivery or the existing Lit API.

The auth service may use a short-lived opaque cookie, `__Host-chipotle_auth`, with `Secure`, `HttpOnly`, `Path=/`, no `Domain`, and `SameSite=Lax`. This cookie authorizes **only encrypted-record and identity management**, not core API calls or automatic vault unlock. Store its random 256-bit token only as a hash server-side. Proposed limits: 30-minute idle timeout and 12-hour absolute lifetime, with reauthentication within five minutes for credential/email changes. A cookie alone cannot decrypt the vault during normal login; a new tab without an API key must ask for the password again.

Use a dedicated auth subdomain under the same site as the dashboard. Apply `credentials: 'include'`, exact environment-specific CORS allowlists, Origin checks, and session-bound CSRF tokens only to the new auth-service requests. Protect signup/login against session swapping as well as settings mutations. Auth responses use `Cache-Control: no-store`. Leave existing API CORS/cookie behavior unchanged. Use staging domains or a development proxy that avoids third-party cookies; previews cannot gain production credential access. Separate production/staging databases, service secrets, cookies, link origins, and allowlists.

## Cloudflare D1 storage and records

Use **Cloudflare Workers + D1**, with one dedicated D1 database per environment and a direct Worker binding. [D1](https://developers.cloudflare.com/d1/) provides managed SQLite semantics without running a database server. No Railway/Neon database, Postgres connection pool, SQLx integration, or core API database connection is needed for this feature. Restrict the database binding and deployment permissions to the auth service and its operators; browsers only access authenticated Worker endpoints.

The main data is one record per user, with the encrypted envelope stored as a versioned JSON document or blob. Keep identity, verifier, credential version, and envelope together so a password change can be a conditional single-row update. Tokens and sessions need a few small supporting tables; this does not require a general account-management database or an ORM.

**Do not use Workers KV as the source of truth.** Its [eventual consistency and lack of atomic transactions](https://developers.cloudflare.com/kv/concepts/how-kv-works/) are a poor fit for single-use email-verification tokens and concurrent password changes. D1 gives us conditional SQL updates, unique constraints, and transactional batches. KV and Durable Objects are not required for the initial storage design.

Implementation rules:

- Use parameterized statements, unique email/account indexes, and explicit version checks. An email-verification token must be consumed with a conditional write, not a Worker-side read followed by an unconditional write.
- For related writes use [D1 transactional batches](https://developers.cloudflare.com/d1/worker-api/d1-database/). A conditional update affecting zero rows does **not** itself fail the batch: gate dependent writes on the successful claim/version transition, or make a constraint/trigger enforce the invariant. Test losing races, not just SQL-error rollback. Do not assume an interactive Postgres transaction can span Worker awaits.
- Keep auth decisions on the primary; do not enable read replication initially. If introduced later, explicitly route fresh credential/revocation checks to primary and review [D1 Sessions semantics](https://developers.cloudflare.com/d1/best-practices/read-replication/). A sequentially consistent replica session alone does not guarantee it sees another client's latest revocation. Do not cache auth records/tokens in KV or the CDN.
- Use SQLite migrations committed beside the Worker, separate local/staging/production bindings, and a tested deployment order with backwards-compatible schema changes. Index email, account identity, token hashes and expiry; periodically delete expired records.
- Validate the selected plan's [Time Travel retention and restore workflow](https://developers.cloudflare.com/d1/reference/time-travel/) and encrypted database export procedures. Proposed launch objectives remain RPO at most 15 minutes and RTO at most 4 hours, established by a restore drill. A restore may revive old tokens or password versions: invalidate auth sessions/verification tokens after restoration and document credential rollback reconciliation before reopening login. A database backup cannot decrypt envelopes without their corresponding passwords; restoring an older ciphertext record may require the old password. Keep encrypted off-platform exports if required by the failure model.
- Budget for both [Worker execution](https://developers.cloudflare.com/workers/platform/pricing/) and [D1 reads/writes/storage](https://developers.cloudflare.com/d1/platform/pricing/); do not assume that free-tier limits constitute a production capacity plan. Monitor query errors/latency and Worker CPU/memory limits.

Proposed minimal tables:

| Table | Essential fields/invariants |
| --- | --- |
| `auth_users` | UUID; original/canonical email; verified timestamp; status; credential version; auth-secret digest/format; browser KDF parameters; immutable chain/contract/account binding; one versioned API-key ciphertext record with salt, KDF parameters, nonce, authentication tag, and binding metadata; provisioning operation ID/state; timestamps. Unique verified canonical email and account binding. Trim email and normalize domains with an explicit case-insensitive login policy; do not strip provider-specific dots/plus suffixes. Pending signup has no account envelope yet. |
| `auth_sessions` | Token hash; user FK; created/last-used/idle/absolute expiry; credential version; revoked timestamp. No encryption key or plaintext master API key. |
| `auth_tokens` | Token hash; purpose; user binding; expiry; used timestamp; credential version. Includes signup and email-change verification proofs; enforce single-use transitions. |
| `auth_outbox` | Pending email type/recipient reference; delivery state and retry times; idempotency identifier. If retry needs a token-bearing link, encrypt that payload under a separate Worker-held key, exclude it from logs, and delete after delivery/expiry. |

Use redacted operational audit events with defined retention; a separate SQL audit table is optional. Never include passwords, raw API keys, auth secrets, or session/verification tokens. Provisioning state in `auth_users` supports upload retries, not exactly-once legacy on-chain creation.

A read-only database dump is the primary envelope-encryption threat model. A database writer can also tamper with identities, sessions, and encrypted envelopes; encryption alone does not protect authorization metadata. Include metadata-integrity and database-write compromise in the security review, including unauthorized envelope replacement and denial of access, rather than claiming protection from every database compromise.

## Auth-service API contract and signup reliability

All new routes below belong to the separate auth service, not `lit-api-server`. Final schemas belong in that service's OpenAPI definition. No recovery/reset routes are exposed; verification-only sessions cannot replace credentials on existing accounts.

| Route | Purpose |
| --- | --- |
| `POST /auth/v1/signup/start`, `/signup/verify` | Send/consume verification proof and establish a restricted registration session. |
| `POST /auth/v1/signup/credentials`, `/signup/begin` | Register derived auth secret/verifier for the verified identity, then atomically claim the one browser-side creation attempt. No plaintext password. |
| `POST /auth/v1/login/parameters` | Real or synthetic identity binding and KDF parameters. |
| `POST /auth/v1/login` | Verify derived auth secret; issue auth-service session and return the encrypted API-key record. |
| `GET /auth/v1/envelope`, `PUT /auth/v1/envelope` | Authenticated envelope retrieval/initial upload with account binding, idempotency and version checks; subsequent changes require reauthentication. |
| `GET /auth/v1/session` | Service-session status and CSRF bootstrap; never automatic plaintext vault unlock. |
| `POST /auth/v1/logout`, `/logout-all` | Revoke auth-service sessions only. |
| `POST /auth/v1/password/change` | Current-password reauthentication and atomic browser-generated credential/ciphertext replacement. No forgot/reset endpoints. |
| `POST /auth/v1/email/start`, `/email/complete` | Reauthenticated, verified email change. |

After email verification and credential registration, the browser calls the existing `newAccount` endpoint normally. On receiving the API key, immediately retain it through the existing `sessionStorage` path, encrypt the API key in the browser worker, and upload the resulting record to the auth service. Only mark password signup complete after durable upload acknowledgement. During an upload outage, show a resumable “Finishing account setup” state and an explicit manual key-backup fallback; do not show misleading success or silently create a replacement account.

The existing `/new_account` has no cross-service transaction with this database. An upload failure can be retried using the same key and operation ID; use a unique account binding and compare-and-swap version checks to prevent overwrites. Serialize signup completion per identity, disable double submission, and reconcile operation status before retrying. An opaque account-creation timeout or tab loss before receiving/storing the key can still orphan an account, as in the legacy flow. **Do not claim crash-safe or exactly-once chain provisioning with an unchanged API.** Do not automatically retry uncertain creation and risk duplicate accounts/credits; surface an explicit unresolved-creation state without promising key retrieval through support. Improving this boundary would be a separately scoped core API idempotency change.

Initial ciphertext upload is part of a pending new-account signup operation, not a general API-key import endpoint. Scope it to that identity's provisioning state, make initial completion immutable/idempotent, and reject account-binding replacement after completion. Password changes update only the existing record's ciphertext/credentials, never which account it represents. Do not add an API-key-to-password conversion or master-key enrollment proof flow.

Mail delivery uses an outbox/retry pattern with a configured transactional sender (existing services use Resend, but confirm credentials/domain ownership). Auth-service/DB outages block fresh password login and settings but do not block already-unlocked dashboard tabs, API-key login, or wallet login.

## Frontend hosting and React

Auth-service hosting and adopting React are independent choices. Cloudflare Pages can serve a Vite build's `dist` output; see [Cloudflare build configuration](https://developers.cloudflare.com/pages/configuration/build-configuration/). React does not require a long-running Node server or SSR for this dashboard.

Keep the frontend on Cloudflare Pages and deploy the separate auth Worker under its own custom subdomain, backed by D1. Moving static hosting is unnecessary for browser encryption or React. There is no dashboard API proxy; ordinary traffic continues directly to the existing core/payments services. The Cloudflare Worker handles identity and encrypted-record authorization; API-key decryption stays in the browser.

Suggested modernization sequence:

1. Separate auth-service requests and the browser crypto worker from the existing Core SDK. Keep login method, account mode, and DOM rendering distinct.
2. Implement browser encryption and login/settings UX in current modules; feed decrypted keys into existing helpers and verify every dashboard operation.
3. Add a separate frontend package (e.g. `dashboard/`) using React, TypeScript, Vite, and pnpm, after accepting the framework migration. `lit-static/AGENTS.md` currently requires framework-free assets unless approved; this plan does not add dependencies. Keep Rust builds separate.
4. Port login, shell/navigation, and account settings first, then feature screens. Preserve wallet signer lifecycle, transaction previews, BigInt boundaries, billing, and existing Core SDK behavior through adapters.
5. Assemble deployment output with the existing `/dapps/dashboard/` path and shared assets. Preserve monitor/verify apps, ABI/SDK paths, existing billing recovery pages, and deep links; do not overwrite all `lit-static` content with a dashboard-only `dist` directory.
6. Update all three frontend deployment paths to build deterministic assets, set the correct API/payments origins, and expose the built commit. Use hashed assets and a revalidated HTML entrypoint. Test rollback and ensure old assets remain available to already-open tabs.

Frontend configuration is public. No database access credentials, service secrets, or email-provider secrets belong in Vite environment variables shipped to the browser. Audit third-party scripts and establish a compatible CSP. The auth cookie does not protect the plaintext API key in `sessionStorage` from XSS.

## Delivery phases and acceptance

1. **Infrastructure/security design:** confirm the actual Cloudflare bindings and environment mapping; inventory all key-consuming frontend calls; choose D1 environment bindings/location requirements and an operational owner; threat-model signup, session, password changes, and exported-key compromise; confirm that email verification cannot bypass the current-password requirement. Benchmark Argon2 WASM on supported desktop/mobile browsers; review domain-separated auth credentials and browser encryption interoperability.
2. **Auth Worker and browser crypto:** TypeScript Worker package, Wrangler configuration, D1 SQLite migrations/bindings, derived-secret verifiers, tokens/mail outbox, rate limits, encrypted-record authorization, browser crypto worker, and resumable uploads. Use pnpm for the new package; keep it independent of Rust crates. Add local Worker/D1 development, isolated staging/production secrets and databases, migration checks, and a dedicated Worker deploy workflow. Deploy the compatible Worker/schema before enabling its frontend UI. Feature-flag the new login method; leave core API contracts and authentication unchanged.
3. **UX rollout:** new-account email signup/login, password-manager save reminders, current-password changes and settings for password-created accounts, all three login choices, and a clear distinction between locked browser state and expired auth-service sessions. Preserve existing API-key users’ flow without migration prompts or conversion settings. Enable internally, then staging, then gradually in production. Do not send unsolicited migration emails as part of implementation.
4. **Operational readiness:** encrypted database restore drill, alerting on auth failure rates/Worker resource limits/mail delivery/D1 errors, service-secret rotation rehearsal, and a documented database-restore/incident runbook. Support documentation must state that forgotten passwords cannot be reset. Rollback must retain schema/key compatibility and access for password-created users; hiding the login UI is not an adequate rollback.
5. **React migration:** separate changes and release gates after authentication is stable.

Required verification for implementation:

- Crypto: deterministic fixtures, wrong password, tampered nonce/tag/associated data, swapped accounts/environments/purposes, old envelope versions, browser KDF bounds, WASM/mobile compatibility, HKDF domain separation, password-change decrypt/re-encrypt round trips, and database-restore compatibility.
- Auth: D1 conditional-write and transactional-batch races, primary-read revocation checks, expiry, verification-token replay and concurrent password changes, atomic verifier/ciphertext/parameter updates, credential-version invalidation, auth-service logout/all sessions, CSRF, strict CORS, account-enumeration behavior, and rate limits across replicas. Verify that logs have no secrets and browser storage contains only the deliberately retained API/usage keys, never passwords or derivation material. Confirm that service-session revocation does not falsely claim to revoke unlocked API keys.
- Identity: no legacy-account import, enrollment, or conversion endpoints/UI; Stripe email cannot claim an account; duplicate signup races fail safely; completed account bindings cannot be replaced; wallet accounts retain wallet-only write authority. Verify email-only sessions, support/admin paths, and repeated signup cannot reset existing passwords or replace envelopes. Creating a new account with an email present in legacy Stripe data must not attach or transfer that legacy account.
- Reliability: creation-response loss and orphan-account handling, upload retries, tab closure, concurrent signup, DB/mail/RPC failures, browser worker failure, Worker CPU/memory limits, Worker redeployment, D1 outage/restore. Already-unlocked API calls must not depend on the auth service.
- End to end: signup, verification, login, reload, current-password change, no-recovery messaging, unchanged legacy API-key login, no migration prompts, all three access modes, all dashboard operations including billing/usage overrides, legacy clients, mobile/password-manager/accessibility behavior, and staging-versus-production isolation.
- Follow the relevant folder instructions when implementing: per-crate Cargo checks for Rust and pnpm/Playwright in `e2e`; no repository-root Cargo workspace. The implementation has a dedicated Worker/D1 and browser-crypto test suite, plus isolated Playwright coverage against the real Worker with simulated Lit API/email services. Live chain and production rollout checks remain deployment steps.

Remaining implementation decisions: D1 location requirements, plan/budget, and restore operator; TypeScript Worker package location and deploy workflow; reviewed browser Argon2 library and auth/record formats; transactional email sender and scheduled outbox retries; staging custom domains; and a separate on-chain response for a compromised exported master key. Cloudflare Workers + D1 hosting, new-account-only password signup with no legacy migration, and deferring forgotten-password recovery are already agreed. A future recovery design is a separate scope decision; no recovery infrastructure is included in v1.

## Implementation notes

- `lit-dashboard-auth/` implements the TypeScript Worker, D1 migration, encrypted verification-email outbox, auth-service sessions, conditional signup state machine, and current-password/email changes. There are no recovery or enrollment routes.
- `password-protocol.js`, `password-client.js`, `password-login.js`, and the generated self-hosted crypto worker integrate the new flow into the existing dashboard. `hash-wasm` 4.12.0 supplies Argon2id; no React migration or core API change is included.
- The actual record schema and HTTP surface are documented in the package README and OpenAPI file. The fixed v1 KDF profile is encoded by the format version; clients reject different costs before allocating memory. Password screening includes a small local common-pattern denylist, not a comprehensive breached-password corpus.
- `.github/workflows/dashboard-auth.yml` tests the service and browser flow and provides an explicit environment-selected deploy job. Existing static release workflows inject the optional auth-service origin. Password UI stays disabled on remote deployments until that origin is configured.
- D1/Worker provisioning, Resend sender credentials, real-environment restore drills, and production signup verification are not performed by this PR. Browser tests exercise the actual crypto and storage code with simulated email/Lit endpoints; they are not live TEE tests.
