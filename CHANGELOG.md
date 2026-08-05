# Changelog

User-facing changes to the Chipotle API and stack. For the full history, see
`git log` and the [release tags](https://github.com/LIT-Protocol/chipotle/tags).
Release verification (image digests, attestation, governance) is covered in the
[upgrade governance docs](https://developer.litprotocol.com/architecture/verification/upgrade-governance).

## Unreleased

Docs for unreleased features are parked in `docs-reserve/` (see its README for
restore steps) so developer.litprotocol.com — which publishes from `main` —
doesn't describe endpoints the released server lacks.

### Added
- `GET /get_supported_languages`: unauthenticated discovery of the languages,
  runtime versions, and execution methods a node supports (CPL-349 phase 1).
  Docs reserved at `docs-reserve/lit-actions/languages.mdx` until release.
- Permanently delete a PKP wallet from the dashboard.
- Enterprise net-30 committed-use billing (invoice rail alongside Stripe
  credits).
- Auto-funding for API payer gas wallets with low-balance alerts
  (operator-facing).
- Local `lit` CLI for testing any-language actions without a node
  (`lit-actions/local-cli`, developer tooling).

### Changed
- The gVisor any-language runner is now **off by default** and behind a feature
  flag (CPL-359). Its binary is compiled only when the `gvisor` cargo feature is
  enabled (the gVisor runner image build opts in), and `POST /lit_binary_action`
  stays mounted but returns `503` ("feature disabled") unless the api-server is
  started with `LIT_GVISOR_ENABLED=true`.
- The gVisor runner's run-time gate now has **three axes**, all fail-closed
  (CPL-361). `LIT_GVISOR_ENABLED` is rendered per-deploy — testing/manual/staging
  deploys default it **on**, the production deploy defaults it **off** — and a
  new on-chain `GVISOR_RUNNER_ENABLED` node-configuration value in the
  AccountConfig contract must *also* be truthy before `POST /lit_binary_action`
  runs, giving operators a network-wide runner kill-switch that needs no redeploy.
- `max_get_keys_count` is now enforced in the key handlers; oversized
  `get_keys` requests are rejected.

### Security
- `registerWalletDerivation` now enforces a global first-owner binding
  (`pkpId → master account`): a wallet address can only ever be registered —
  or re-registered after deletion — by the account that first registered it,
  closing a cross-account PKP hijack via publicly visible derivation paths
  (#575). Requires a one-time on-chain backfill for wallets registered before
  the upgrade (`backfillPkpOwners`).

## v1.1.10 — 2026-06-22

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
- `POST /new_account` now retries (with backoff) the intermittent 500
  (`NoAccountAccess`) seen when the RPC provider serves the follow-up wallet
  registration's dry-run from a node that hasn't executed the just-mined
  `newAccount` block, riding out the typical propagation window.
- Release builds no longer report a spurious `-modified` version suffix
  (`.dockerignore` excluded a tracked file, dirtying `git describe` inside the
  image build).

## Historical releases

| Tag | Date |
|--------|------------|
| v1.1.9 | 2026-06-12 |
| v1.1.8 | 2026-06-12 |
| v1.1.7.1 | 2026-06-12 |
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
