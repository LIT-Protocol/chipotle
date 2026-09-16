# Customer-path QA, round 2 — 2026-09-16 (evening)

Second black-box pass over https://keychain.litprotocol.com/ after PR #670 deployed and
`@lit-protocol/keychain` **2.0.3** was published. Same rules as the
[first report](user-qa-2026-09-16.md): published docs and npm only, no source
substitution, dummy secret values, no payment authorized. Repository code was read only
after the findings below were recorded, to implement the fixes in this PR.

## What was exercised against production

- Fresh `npm install @lit-protocol/keychain` (2.0.3, Node 22): `--help`, `--version`,
  `init` (0600, never overwrites), `actions`, `attest` (all six checks including
  `tls-certificate-in-tee`), strict NodeNext TypeScript consumer compiles.
- **Wallet sign-in** (mock EIP-1193/EIP-6963 wallet in headless Chromium): first sign-in
  25 s with progress text; later sign-ins ~2 s. Sign out disconnects the wallet, so the next
  sign-in goes through the connect modal again.
- **Passkey sign-in** (virtual CTAP2 authenticator): create 22 s, existing passkey 2 s.
  Adding a recovery passkey ends the session as advertised; ordinary sign-in afterwards
  returns to the **original vault** with its secret (the #670 regression is fixed).
- Stored secret: create, approve CLI agent, download config; CLI `get`/`run`
  (`--env`, `--file`, exit-status propagation), SDK `get`/`list`/`attest`/`destroy`,
  MCP `initialize`/`tools/list`/`list_secrets`/`get_secret`/`agent_public_key`.
- Disable → CLI "owner disabled" → Enable; Revoke → "not approved" → re-approve;
  Renew (91 rejected, 60 accepted); Rotate & approve → old config reads v2.
- Connected service: Stripe with a syntactically valid fake key; malformed credential
  refused client-side; `use` from CLI → "Lit ran the action but it did not complete"
  message; `get`/`run` on a service secret refused with pointers to `use`.
- Replace execution key: old config → 401; new config works; `CHIPOTLE_USAGE_API_KEY`
  override works.
- Recovery & backups (download backup), Activity, Stripe Checkout opened (not paid),
  mobile viewport 390 px (no horizontal overflow), all hosted docs routes and every
  external link in them.
- Google sign-in was **not** completed (headless; no Google account). Real provider
  credentials were **not** used, so successful `use` remains unverified.

## Defects and gaps found → fixed in this PR

| #   | Finding                                                                                                                                                              | Fix                                                                                                                                            |
| --- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | **Export secret** silently wrote the plaintext value to `NAME.json` in Downloads with no confirmation and no on-screen feedback; easy to mistake for "Agent config". | Confirmation dialog naming the file and warning it is plaintext; success notice afterwards.                                                    |
| 2   | **Rotate & approve reset the permission expiry** to 30 days: a 60-day renewal became 30 days without any hint.                                                       | Rotation keeps a still-valid expiry; only an expired policy restarts at 30 days. Hint copy updated.                                            |
| 3   | Sidebar showed **"1 agents"**.                                                                                                                                       | Pluralised.                                                                                                                                    |
| 4   | "How agents use this secret" printed a bare `keychain run …`, which does not exist after a local install (the README itself says every example must use npx).        | Uses the shared `NPX_KEYCHAIN` pin.                                                                                                            |
| 5   | Name field rejected `my key` with only the browser's generic "Please match the requested format"; the public-key field did the same for non-hex input.               | `title` hints on both inputs spell out the accepted format.                                                                                    |
| 6   | "Connect a service" card listed "Stripe, OpenAI, GitHub, Slack" but not Supabase.                                                                                    | Added.                                                                                                                                         |
| 7   | Action docs read "See provider setupfor exact credential formats" (missing space).                                                                                   | Fixed.                                                                                                                                         |
| 8   | CLI/SDK `Unknown secret` did not say which secret or what the config contains (MCP already did).                                                                     | `Unknown secret "X". This agent config contains: A, B`. Regression test added.                                                                 |
| 9   | After the owner replaced the execution key, agents got only `Request failed (401): API key not recognized — it does not resolve to any account.`                     | Message now explains the key was most likely replaced and how to fix it. Stays an `HttpError` (login retry unaffected). Regression test added. |
| 10  | No favicon (404).                                                                                                                                                    | SVG favicon matching the brand mark.                                                                                                           |
| 11  | `SKILL.md` front matter still said `version: 2.0.0`.                                                                                                                 | 2.0.3.                                                                                                                                         |

## Observations not changed here (product / config decisions)

- **Stripe Checkout shows "Workgraph, Inc"** and is a **live-mode** session (`cs_live_…`).
  Business name is Stripe account configuration, not code.
- WalletConnect project id is still the `unused-injected-only` placeholder, so the connect
  modal offers only browser-injected wallets (no mobile/WalletConnect path).
- Creating one secret asks a wallet user for **three EIP-712 signatures** (manifest,
  envelope, policy); approving an agent adds one more. Protocol-inherent, but worth a
  sentence in the UI before the first prompt.
- **No way to remove a secret**. Free is 5 slots and "secrets are never deleted", so a
  user who tries five test values is stuck until they subscribe. Consider delete/archive
  that frees the slot, or say clearly that disabled secrets still count.
- **Activity** rows show only the event kind and time (`Policy Updated` ×N) — no secret or
  agent name. The audit table stores only `event` and `object_hash`; adding a label needs
  a schema change.
- Sign out disconnects the wallet; returning users click Connect Wallet → Mock/Browser
  Wallet → Sign in with wallet. Acceptable but three clicks.
- Package install is ~25 MB because both `dist/index.js` and `dist/node.js` (3.6 MB each)
  ship with 5.8 MB source maps. Dropping `.map` files from `files` would halve it.
- `/robots.txt` and unknown paths return a JSON `{"error":"not_found"}` 404 rather than
  the SPA shell; fine for an app, but a deep link to `/secrets` cannot exist.
- One `401 POST /core/v1/lit_action` is visible in the browser console during every
  first sign-in while the SDK waits for the fresh execution key. Expected, but a console
  watcher may report it.

## Validation

`npm run build`, `npm test` (89 passed, 3 skipped before the new tests; 15/15 in
`sdk-regressions` after), `tsc --noEmit`, `prettier --check`. No template bytes changed
(`actions/catalog.lock.json` untouched). New SDK messages were also verified from the
local build against the production vault created during this session.
