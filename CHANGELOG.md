# Changelog

User-facing changes to the Chipotle API and stack. For the full history, see
`git log` and the [release tags](https://github.com/LIT-Protocol/chipotle/tags).
Release verification (image digests, attestation, governance) is covered in the
[upgrade governance docs](https://developer.litprotocol.com/architecture/verification/upgrade-governance).

## Unreleased

### Security
- `registerWalletDerivation` now enforces a global first-owner binding
  (`pkpId → master account`): a wallet address can only ever be registered by
  the account that first registered it, closing a cross-account PKP hijack via
  publicly visible derivation paths (#575). `getWalletDerivation` fails closed
  on the same binding, neutralizing hijack registrations that already happened
  before the upgrade. Wallets registered before the upgrade have no binding yet
  and remain resolvable (and thus still exploitable) until a one-time on-chain
  backfill (`backfillPkpOwners`) runs — operators should run it promptly after
  the facet upgrade to close that window.

### Changed
- **All API error responses are now JSON** (`{error, message, fix, docs_url}`)
  instead of HTML error pages. See the
  [Errors reference](https://developer.litprotocol.com/management/errors).
- **Invalid API keys now return `401 Unauthorized`** (previously `402` on
  billed endpoints and `400`/`500` on billing endpoints). `402 Payment
  Required` is reserved for real accounts with insufficient credits, and its
  body now states the amount needed, your balance, and how to fund.
- **Billing outages now return `503`** instead of `402`.
- **Failed requests are no longer charged.** The flat $0.01 management charge
  settles only after the operation succeeds.
- `POST /create_wallet` added; `GET /create_wallet` still works but is
  deprecated (metered writes don't belong on GET).

### Added
- Optional starter credits for new accounts (`STARTER_CREDITS_CENTS`, default
  off): lets the quickstart run before adding funds.
- `CONTRIBUTING.md`, expanded `.env.example`, and per-component READMEs.

### Fixed
- `POST /new_account` no longer intermittently fails with a `NoAccountAccess`
  500. The endpoint issues two sequential on-chain writes (`newAccount` then
  `registerWalletDerivation`); on a load-balanced RPC endpoint the second call's
  pre-send simulation could land on a backend that had not yet imported the
  freshly-mined `newAccount` block, so the account looked nonexistent and the
  AccountConfig access check reverted. Account creation now waits for the new
  account to become visible to the read RPC before the second write, and retries
  that write a few times to absorb residual backend lag. This also unblocks the
  k6 new-account correctness gate on staging deploys.
- Write endpoints (`new_account`, `create_wallet`, …) can no longer hang
  indefinitely after an RPC outage. On-chain sends now pin nonces from a
  locally managed per-signer allocator that *reserves* the nonce at read time
  (concurrent borrowers of one signer get distinct nonces instead of
  colliding) and is invalidated on any send failure or receipt timeout
  (alloy's optimistic nonce cache is never rolled back after a dropped
  broadcast, so a poisoned signer could never recover). Every RPC step in the
  send pipeline — simulation, nonce fetch, broadcast, receipt wait — now has a
  hard deadline, including an outer bound on `get_receipt` (alloy's own
  watcher timeout does not cover its receipt-fetch RPC awaits). Signer leases
  carry an id, so a slow borrower whose lease was force-freed can no longer
  free the next borrower's lease; the stale threshold now exceeds the
  worst-case bounded send so cleanup is purely leak recovery; and the pool's
  rebalancer pins nonces from the same allocator, runs off the dispatcher
  task, and no longer aborts remaining wallets after one failure.
  Root-caused from the 2026-09-03 prod incident where two wedged payer
  wallets absorbed nearly all signer leases and `POST /new_account` timed out
  for days; hardened further after an adversarial cross-model review.
- On-chain writes confirm ~2-5s sooner: the RPC receipt poller now ticks every
  2s (matching block time on the configured chains) instead of alloy's 7s
  default for HTTP transports, which also shortens how long each signer lease
  is held.
- Release builds no longer report a spurious `-modified` version suffix
  (`.dockerignore` excluded a tracked file, dirtying `git describe` inside the
  image build).

## Historical releases

| Tag | Date |
|--------|------------|
| v1.1.7 | 2026-06-02 |
| v1.1.6 | 2026-05-31 |
| v1.1.4 | 2026-05-29 |
| v1.1.3 | 2026-05-29 |
| v1.1.2 | 2026-05-11 |
| v1.1.1 | 2026-05-06 |
| v1.1.0 | 2026-05-06 |
| v1.0.x | 2026-03-29 → 2026-04-20 |
| v0.1–v0.2 | 2026-03-13 → 2026-03-17 |

(Tags predating this file aren't annotated here — `git log <tag>` has the detail.)
