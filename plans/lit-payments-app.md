# Lit Payments App — Design Plan

Status: implementation in progress — see Sequencing section for what's shipped vs. remaining
Author: Chris (with Claude)
Date: 2026-05-18 (last updated: 2026-05-21)

## Current state (TL;DR for a new session)

- **Shipped to `next`**: `lit-billing-core` extraction (PR #358), `lit-payments` foundation + magic-link auth (PR #359), admin credit portal + Fly.io deploy target (PR #370).
- **Open**: `LitkeyPaymentGateway` Solidity contract + Hardhat project (PR #373).
- **In progress**: rate poller (3b) implemented in this PR worktree; Alchemy WSS listener (3c) and Wagmi payment page + dashboard link (3d) remain. See the Sequencing section near the bottom for what each entails and recommended order.

The rest of this doc is the original design + decisions, kept for context. Implementation details that diverged from the plan are noted inline.

## Goal

Build a separate, ops-facing app that handles billing operations the main `lit-api-server` shouldn't carry:

1. **Admin credit portal** — a logged-in moderator (Discord support, etc.) can credit a customer's Stripe balance by email or wallet address.
2. **LITKEY → Stripe credit** — let users pay with LITKEY tokens from the dashboard via a "Pay with tokens" button; auto-credit their Stripe balance. Replaces today's manual flow.

## Non-goals

- Replacing the existing customer dashboard (`lit-static/dapps/dashboard/`). End users still top up there.
- Running inside the TEE. This is a normal cloud service.
- Subscription billing. Out of scope for this plan.

## Architecture overview

```
┌────────────────────────────────────┐
│  TEE: lit-api-server               │
│  - /billing/*  (customer top-up,   │
│    balance read, charges)          │
│  - depends on lit-billing-core     │
│  - Stripe secret key #1            │
└────────────────────────────────────┘
                  │
                  │ shares: lit-billing-core
                  │ (customer identity + Stripe HTTP)
                  ▼
┌────────────────────────────────────┐
│  lit-billing-core (NEW crate)      │
│  - Stripe HTTP client              │
│  - get_customer_by_wallet          │
│  - balance_transaction helpers     │
│  - search customers by email       │
└────────────────────────────────────┘
                  ▲
                  │
┌────────────────────────────────────┐
│  Non-TEE: lit-payments (NEW)       │
│  - Admin portal (mod credits)      │
│  - LITKEY listener + gateway       │
│  - Stripe restricted key #2        │
│  - Postgres                        │
└────────────────────────────────────┘
                  │
                  ▼
         Stripe + Base RPC
```

### Repo layout

Same monorepo, two new top-level crates. `lit-billing-core` holds the customer-identity invariant (`metadata.wallet_address` keys the Stripe customer) and the raw HTTP plumbing. Both services depend on it — single source of truth.

```
lit-node-express/
├── lit-api-server/         ← existing (TEE); now depends on lit-billing-core
├── lit-billing-core/       ← NEW shared crate
│   └── src/
│       ├── lib.rs
│       ├── client.rs       ← StripeClient (creds + HTTP, no caches)
│       ├── customer.rs     ← get_customer_by_wallet, search_by_email,
│       │                     set_customer_email
│       ├── balance.rs      ← read balance, write balance_transactions
│       └── types.rs        ← ReportCustomer, ReportBalanceTx, etc.
├── lit-payments/           ← NEW (non-TEE)
│   ├── src/
│   │   ├── main.rs
│   │   ├── chain.rs        ← LITKEY listener (WSS + poll)
│   │   ├── db.rs           ← Postgres
│   │   ├── auth.rs         ← magic-link
│   │   └── endpoints/
│   │       ├── portal.rs
│   │       └── pay_with_litkey.rs
│   ├── static/             ← admin UI + payWithLitkey page
│   ├── contracts/          ← LitkeyPaymentGateway.sol
│   ├── migrations/
│   └── Cargo.toml
└── lit-static/dapps/dashboard/   ← unchanged
```

### Extracting lit-billing-core

The existing `lit-api-server/src/stripe.rs` mixes three concerns:
1. Stripe HTTP + customer identity + balance primitives → **move to `lit-billing-core`**
2. Caching layer (`customer_cache`, `balance_cache`, `wallet_cache`, `balance_refresh_in_flight`) → **stays in `lit-api-server`**, wraps the core's `StripeClient`
3. API-server-specific flows (`charge`, `charge_management`, `charge_lit_action_time`, `create_payment_intent`, `confirm_payment_and_credit`, `resolve_wallet_address`) → **stays in `lit-api-server`**

`lit-payments` uses `lit-billing-core` directly with no caching layer (not hot-path). `lit-api-server` keeps its existing `StripeState` but it now composes a `StripeClient` from the core + the existing caches.

Risk: the extraction touches existing production code (`lit-api-server`). Mitigation: pure refactor in a single PR with no behavior change, full test pass before lit-payments starts.

Tech stack: **Rust + Rocket + vanilla HTML/JS**. Matches existing repo conventions.

### Deployment

- **Hostname**: `payments.litprotocol.com`.
- **Platform**: Fly.io (app + Fly Postgres, or external Postgres via Supabase/Neon). `fly.toml` lives at the repo root so the Dockerfile can pull in the sibling `lit-billing-core/` crate via the repo-root build context.
- **Outside the TEE.** No attestation pain; operators can `fly logs` / `fly ssh console` for diagnostics.
- **Ingress**: Fly's default TLS + `fly certs create payments.litprotocol.com`.
- **Secrets** (`fly secrets set …`): Stripe restricted key, DB URL, magic-link signing key, Resend API key, Alchemy WSS + HTTPS URLs, gateway contract address, treasury Safe address, LITKEY token address, CoinGecko ID. Plus `ROCKET_SECRET_KEY` for Rocket's private-cookie encryption.

### Auth tiers

Two roles, both authenticated against the Postgres `operators` table:

- **`mod`** — grant credits up to per-grant/daily caps. Cannot change config or resolve unattributed payments.
- **`admin`** — everything: change LITKEY rate, manage operators, resolve unattributed payments.

**Mechanism: magic link via email, from day 1.** Operator enters email → Resend sends a link with a signed, short-lived (15 min) token → click sets a session cookie (~7 days). Magic-link requests for emails not in the `operators` table are silently dropped (same response as success, to prevent enumeration).

### Stripe key scopes

`lit-payments` uses a **separate Stripe restricted key** with scopes:
- `customers:read` + `customers:write`
- `customer_balance_transactions:read` + `customer_balance_transactions:write`

That's it — no subscription or webhook scopes needed for this plan.

## Data model (Postgres) — minimal

```
operators(
  id, email, role, created_at, last_login_at
)

sessions(
  token, operator_id, expires_at
)

grants(
  id,
  operator_id,
  stripe_customer_id,
  wallet_address,
  email,                          -- snapshot at grant time
  cents,
  note,
  stripe_balance_transaction_id,
  idempotency_key,
  created_at
)
-- audit trail = SELECT * FROM grants

litkey_payments(
  id,
  tx_hash,
  log_index,
  payer_wallet,                   -- msg.sender on the contract call
  credited_wallet,                -- event.wallet
  litkey_amount,
  cents_credited,
  rate_used,                      -- cents per LITKEY at credit time
  stripe_customer_id,
  stripe_balance_transaction_id,
  credited_at,
  UNIQUE(tx_hash, log_index)
)
-- audit trail = SELECT * FROM litkey_payments

litkey_rate(
  id PRIMARY KEY DEFAULT 1 CHECK (id = 1),  -- single-row table
  cents_per_litkey,
  source,                         -- 'coingecko' | 'manual'
  fetched_at,
  updated_by_operator_id          -- null when source='coingecko'
)

chain_checkpoint(
  chain_id PRIMARY KEY,
  last_processed_block,
  updated_at
)
```

Six tables, each justified:
- `operators` + `sessions` — magic-link auth.
- `grants` — portal action log + idempotency.
- `litkey_payments` — credit record + idempotency.
- `litkey_rate` — current rate (single row) + who last touched it.
- `chain_checkpoint` — listener resume point.

No separate `audit_log` table: every state-changing action is recorded on its own resource row with the actor (`operator_id`) and timestamp. Logins are server logs only.

## Feature 1 — Admin credit portal

**API**:
- `POST /admin/grant_credit` — body: `{ email?, wallet_address?, cents, note }`, session cookie.
  - Look up Stripe customer by email or wallet (via `lit-billing-core`).
  - Validate caps.
  - Write `balance_transaction` with `amount = -cents`, description `"Discord grant: {note} (op:{email}, {ISO date})"`.
  - Persist to `grants` with idempotency key.
- `GET /admin/grants?limit=50` — recent grants by the current operator.
- `GET /admin/customer/lookup?email=…` or `?wallet=…` — preview balance before granting.

**Lookup branching by email**:
- 0 results → "no Stripe customer with that email. Ask the user to log into the dashboard and set their email under settings."
- 1 result → proceed.
- 2+ results → return all matches with wallet + balance; mod picks by wallet.

**UI**: one page, three sections — Lookup, Grant form, Recent grants. Vanilla HTML/JS.

**Caps**:
- Per-grant: $20 (env-configurable).
- Daily per operator: $100 (env-configurable).
- No global cross-operator cap.

**Audit**: every grant logs operator email + timestamp to the `grants` table and to the Stripe description field (visible in `stripe_report`).

## Feature 2 — LITKEY → Stripe credit

### Locked-in answers

- **Chain**: Base.
- **Token**: LITKEY at `0xf732a566121fa6362e9e0fbdd6d66e5c8c925e49` (Base bridge deployment, `OptimismMintableERC20`).
- **No `permit` / no EIP-3009.** Two-tx flow: `approve` then `pay`.
- **Approval pattern**: exact-amount only.
- **Confirmation depth**: 5 blocks.
- **Treasury**: existing company Safe on Base.
- **Listener**: Alchemy WSS primary + 60s reconciliation poll.
- **Rate source**: CoinGecko free public API, `id=lit-protocol`, polled every 5 min.
- **Minimum payment**: $5 equivalent.
- **Audit**: internal review (Chris) on the contract.

### User flow

User in the dashboard at `dashboard.litprotocol.com` clicks "Pay with tokens" → opens `payments.litprotocol.com/payWithLitkey?wallet=<address>` in a new tab.

1. Page reads the wallet from the URL → looks up the Stripe customer → **prominently displays the account that will be credited** (email + wallet) for the user to confirm.
2. User connects their paying wallet (Wagmi). May be the same as the credited wallet or a different one (e.g., a separate Metamask holding LITKEY).
3. Page shows "You'll send N LITKEY (~$X credit at current rate)."
4. User clicks Pay → wallet pops up twice (`approve`, then `pay`).
5. Page polls for credit confirmation. On success: confirmation + link back to dashboard.

**Note on URL spoofing.** The wallet in the URL is unauthenticated — anyone can craft a link to credit any account. We don't try to prevent that. The worst case is one of:
- Someone gifts a credit to another account (no harm).
- A phishing attacker tricks a victim into paying with an attacker-chosen `wallet=` param, sending the credit to the attacker. Mitigation: the page shows the account-to-be-credited prominently and asks the user to confirm before the wallet pop-up. A user who reads the screen catches this.

### Attribution architecture

Single company wallet receiving from many users = attribution ambiguity. Solved with a payment gateway contract — direct sends to the Safe are unsupported.

```solidity
contract LitkeyPaymentGateway {
    IERC20 public immutable litkey;
    address public immutable treasury;

    event Payment(
        address indexed wallet,  // wallet to credit (== Lit account wallet)
        address indexed payer,
        uint256 amount
    );

    function pay(uint256 amount, address wallet) external {
        litkey.transferFrom(msg.sender, treasury, amount);
        emit Payment(wallet, msg.sender, amount);
    }
}
```

Dashboard Pay button calls `litkey.approve(gateway, amount)` then `gateway.pay(amount, walletToCredit)`. Listener watches `Payment` — `wallet` (which maps to a Stripe customer via `metadata.wallet_address`) is baked into every event. Zero ambiguity even under concurrent payments. Funds land directly in the Safe.

**Direct sends to the Safe are unsupported.** If a user pastes the Safe address and sends LITKEY directly (skipping the contract), they bypass the listener entirely. Those tokens just sit in the Safe as company income. If a user complains, the mod uses the admin credit portal (Feature 1) to grant credit manually. The portal exists for exactly these one-off cases — we don't need a second machinery for them.

### Listener

WebSocket primary + 60s reconciliation poll:

- **WSS subscription** (Alchemy) to `Payment` events from the gateway contract. Reconnect with exponential backoff (1s → 2s → 4s → … cap 30s).
- **60s poll** for `Payment` logs in `(last_processed_block, latest_block - 5)`. Same handler, same idempotency check.
- **Checkpoint**: `chain_checkpoint.last_processed_block` advanced only by the poll path.
- **Confirmation depth**: defer crediting until `latest_block - event_block >= 5`.
- **Idempotency**: `(tx_hash, log_index)` unique in `litkey_payments`.

### Credit flow

1. `Payment` event observed at confirmation depth.
2. Idempotency check on `(tx_hash, log_index)`.
3. Look up Stripe customer by `metadata.wallet_address == event.wallet`. If none: log a warning and skip — do not credit, do not alert. Anyone calling `pay()` directly with a wallet that isn't a Lit customer is off the supported path; they'll reach out to support if it matters.
4. Read `cents_per_litkey` from `litkey_rate`. Compute `cents = litkey_amount * rate`.
5. Write `balance_transaction` with `amount = -cents`, description `"LITKEY payment: tx {tx_hash}"`.
6. Insert row into `litkey_payments` (rate snapshotted in `rate_used`).

### Rate management

**CoinGecko free public API**, no key required:

```
GET https://api.coingecko.com/api/v3/simple/price?ids=lit-protocol&vs_currencies=usd
```

- Background task polls every 5 min, upserts the single-row `litkey_rate` table.
- If a fetch fails or returns nonsense (zero / null / >100× recent value): keep the last-known rate, log a warning.
- If the row's `fetched_at` is older than 1 hour: pause crediting and log a warning. Stuck payments will surface via user reports; mod can grant manually in the meantime.
- Admin can override the rate from the admin UI (updates the same row with `source='manual'` + operator id). Override is recorded on the row itself — no separate notification.

**Quote vs. credit semantics**: the payment page shows the rate at page-load and refreshes the quote every 30s. Once the user starts the `approve` tx, the LITKEY amount is frozen. At confirmation we credit using the rate **in effect at the on-chain `Payment` event** (the most recent value of `litkey_rate`). Page says "estimated credit, finalized at confirmation."

### Edge cases

- **`event.wallet` doesn't match any Stripe customer** → log + skip. No alert. If the payer cares, they'll contact support and a mod can grant via the credit portal.
- **Direct send to the Safe** → unsupported; tokens just sit in the Safe. User contacts support → mod uses the credit portal.
- **Reorgs** → 5-confirmation depth on Base prevents.
- **Overpayment** → credit at current rate; no refund flow in v1.
- **Rate drift between approve and pay** → contract executes whatever amount was approved; we credit at rate-at-event-time. Page warns "finalized at confirmation."
- **Contract upgrade** → contract is intentionally minimal & immutable; if behavior changes, deploy a new contract and update the listener.

## Decisions (locked in)

| # | Question | Decision |
|---|---|---|
| 1 | Hostname | `payments.litprotocol.com` |
| 2 | Cloud provider | Fly.io (app + Fly Postgres, or external Postgres) |
| 3 | Shared crate | Extract `lit-billing-core` now; both services depend on it |
| 4 | Operator auth | Magic link via Resend, day 1; 15-min token, 7-day session |
| 5 | Alerting | None — rely on server logs (`fly logs`) |
| 6 | Reconciliation report | Deferred |
| 7 | Refund / clawback flow | Deferred — handle manually in Stripe dashboard |
| 8 | LITKEY chain | Base |
| 9 | `permit` / 3009 | Not supported; two-tx flow |
| 10 | Rate source | CoinGecko free public API (`id=lit-protocol`), 5-min poll; manual override |
| 11 | Minimum payment | $5 equivalent |
| 12 | Treasury | Existing company Safe on Base |
| 13 | Contract audit | Internal review only (Chris) |
| 14 | Approval pattern | Exact-amount only |
| 15 | RPC provider | Alchemy (WSS primary + HTTPS poll fallback) |

## Decisions table — extended (build-time config)

| Item | Value |
|---|---|
| CoinGecko ID | `lit-protocol` |
| Magic-link sender | `noreply@mail.litprotocol.com` (subdomain) |
| Initial operators | `chris@litprotocol.com` (admin); `Salamiademola73@gmail.com` (mod) |
| Per-grant cap | $20 |
| Per-operator daily cap | $100 |
| Global cap | none |

## Open questions

All questions are closed. Plan is ready to start building.

## Sequencing

### ✅ 1. Extract `lit-billing-core` — DONE (PR #358, merged to `next`)
   - [x] Moved Stripe HTTP + customer-identity primitives from `lit-api-server/src/stripe.rs` into a new shared `lit-billing-core` crate.
   - [x] `lit-api-server` keeps its caching layer + charge/payment-intent flows, rewires through `lit-billing-core::StripeClient`.
   - [x] Pure refactor: 211 lit-api-server lib tests + 14 stripe_report bin tests + 15 lit-billing-core tests all green.
   - [x] CI wired into `rust-ci.yml` matrix.

### ✅ 2. Foundation + Feature 1: admin credit portal — DONE (PRs #359 + #370, merged to `next`)
   - [x] `lit-payments` crate skeleton (Rocket + sqlx-postgres + Dockerfile). Sibling crate to `lit-billing-core`.
   - [x] Fly.io deploy config (`fly.toml` at the repo root). Originally targeted Railway; switched to Fly.io because Railway's reliability has degraded.
   - [x] Magic-link auth end-to-end (Resend, HMAC-signed tokens, 15-min expiry, 7-day session cookies, operators + sessions Postgres tables, per-email rate limit, spawned email send for timing-leak resistance).
   - [x] Login UI (`/login` + `/`) in vanilla HTML/JS.
   - [x] Admin credit portal: `GET /api/customer/lookup`, `POST /api/grant`, `GET /api/grants`. Caps enforced (default $20/grant, $100/operator/day). End-to-end UUID idempotency (client → server → Stripe Idempotency-Key + DB `UNIQUE(idempotency_key)` + `ON CONFLICT DO NOTHING`).
   - [x] Three-section dashboard at `/`: lookup → match cards with inline grant form → recent grants table.
   - [x] `lit-billing-core` additions for the portal: `customer::find_by_wallet` (non-creating), `customer::search_by_email` + `CustomerSummary`, `balance::write_transaction`.
   - **Milestone hit**: Discord mod can sign in via magic link and grant credits once the Fly app is provisioned.

### 🚧 3. Feature 2: LITKEY → Stripe credit — IN PROGRESS

   **3a.** ✅ `LitkeyPaymentGateway` contract + Hardhat project — DONE (PR #373, open against `next`)
   - [x] Solidity 0.8.28 contract at `lit-payments/contracts/contracts/LitkeyPaymentGateway.sol`. Immutable; `pay(amount, wallet)` pulls LITKEY via `SafeERC20.safeTransferFrom`, forwards to treasury, emits `Payment(indexed wallet, indexed payer, amount)`. Custom-error reverts on zero treasury / wallet / amount.
   - [x] Self-contained Hardhat + TS project at `lit-payments/contracts/` (matches `lit_node_express` conventions). Networks: `hardhat`, `baseSepolia`, `base`.
   - [x] 12 unit tests passing — constructor guards, every revert path, balance + event-arg assertions, multi-payer attribution, cross-wallet payments (payer ≠ credited), indexed-topic filtering.
   - [x] `scripts/deploy.ts` (env-driven, optional Basescan auto-verify) + README.
   - [x] CI workflow `.github/workflows/lit-payments-contracts-ci.yml`.
   - [x] Manual: deployed to Base mainnet at `0xa2d54cd1D1dF1735718A857aC49CaF9ECaB0093b` with the company Safe as treasury. Smoke test: 1 wei of LITKEY sent through the gateway arrived in the company Safe.
   - [x] Deployment invariant documented: `litkey` and `treasury` are immutable constructor args. Changing the token or recipient Safe requires deploying a new `LitkeyPaymentGateway` and updating listener/dashboard config to the new address.

   **3b.** ✅ CoinGecko rate poller + `litkey_rate` schema + admin rate-override UI — IMPLEMENTED IN PR WORKTREE
   - [x] Background task in `lit-payments` polling `https://api.coingecko.com/api/v3/simple/price?ids=lit-protocol&vs_currencies=usd` every 5 min.
   - [x] Migration adding the single-row `litkey_rate` table (`id PRIMARY KEY DEFAULT 1 CHECK (id = 1)`, `cents_per_litkey`, `source`, `fetched_at`, `updated_by_operator_id`).
   - [x] Stale (>1hr) is surfaced on the API/UI and logs a warning so future 3c crediting can pause (no Slack alert — `fly logs` only).
   - [x] Admin UI section on the dashboard to view the current rate + override it (`source='manual'`).
   - [x] **No on-chain dependency** — can ship before 3c.

   **3c.** ⏭️ Alchemy WSS listener + `litkey_payments` + `chain_checkpoint` — NOT STARTED
   - WSS subscription to `Payment` events from the deployed gateway contract; exponential backoff reconnect.
   - 60s reconciliation poll for logs in `(last_processed_block, latest_block - 5)` as a safety net for WS gaps.
   - 5-confirmation depth on Base before crediting.
   - Idempotency on `(tx_hash, log_index)` unique in `litkey_payments`.
   - For each Payment event: read `litkey_rate`, compute `cents = litkey_amount * rate`, look up Stripe customer by `metadata.wallet_address == event.wallet`, write the credit via `lit_billing_core::balance::write_transaction`, persist a `litkey_payments` row.
   - Migrations: `litkey_payments`, `chain_checkpoint`.
   - **Depends on**: 3a contract deployed + address known (`0xa2d54cd1D1dF1735718A857aC49CaF9ECaB0093b` on Base mainnet); 3b rate table existing.

   **3d.** ⏭️ Wagmi `/payWithLitkey` page + dashboard "Pay with tokens" link — NOT STARTED
   - New page at `payments.litprotocol.com/payWithLitkey?wallet=<address>`. Wallet param is plain (no HMAC — see "URL spoofing" risk).
   - Flow: page validates the wallet has a Stripe customer → prominently shows the account-to-be-credited (email + wallet) → user confirms → Wagmi wallet connect → quote (refreshed every 30s, frozen at approve) → approve → pay → poll the listener's grants for credit confirmation.
   - Small change to `lit-static/dapps/dashboard/` to add the "Pay with tokens" button.
   - **Depends on**: 3a contract address (`0xa2d54cd1D1dF1735718A857aC49CaF9ECaB0093b` on Base mainnet); 3c listener handling the resulting `Payment` events.

   **Milestone (when 3a-d ship)**: users self-serve LITKEY payments end-to-end on Base mainnet; the manual "send LITKEY to the wallet and I'll credit you in Stripe" flow is retired.

### Suggested ordering for the remaining PRs

`3b` (rate poller) → `3c` (listener) → `3d` (payment page). 3b is fully independent so it's the cleanest next slice. 3c needs the contract address (deploy on Sepolia first) and the rate table from 3b. 3d depends on both.

## Risks

- **`lit-billing-core` extraction breaks `lit-api-server`** — pure refactor, single PR, gated on full test pass.
- **Mod auth abuse** — magic link + daily caps + per-row audit trail. Even a leaked operator account is capped at $100/day.
- **URL spoofing / phishing the credited wallet** — attacker tricks user into paying with an attacker-chosen `wallet=` param, sending credit to the attacker. Mitigation: page prominently displays the account-to-be-credited (email + wallet) and requires confirmation before the wallet pop-up. We accept residual risk; HMAC-signing the URL is not worth the complexity for the impact.
- **PaymentGateway contract bug** — small surface but real money. Mitigations: keep contract minimal & immutable, internal review, deploy to Base Sepolia first, monitor on-chain.
- **Rate staleness** — `>1hr` since `fetched_at` pauses crediting and logs a warning; admins manually update via the rate-override UI when they notice (via user reports or log monitoring).
- **User sends LITKEY directly to the Safe** — funds sit unattributed in the Safe. Mitigation: mod uses the credit portal to grant manually when a user complains. Documented as unsupported.
- **Reorg risk** — 5-confirmation depth on Base mitigates; risk is already near-zero past a handful of blocks.
