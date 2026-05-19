# Lit Payments App — Design Plan

Status: draft for review
Author: Chris (with Claude)
Date: 2026-05-18

## Goal

Build a separate, ops-facing app that handles billing operations the main `lit-api-server` shouldn't carry:

1. **Admin credit portal** — a logged-in moderator (Discord support, etc.) can credit a customer's Stripe balance by email or wallet address.
2. **LITKEY → Stripe credit** — let users pay with LITKEY tokens from the dashboard via a "Pay with tokens" button; auto-credit their Stripe balance. Replaces today's manual flow.
3. **Subscription billing** *(deferred)* — set up monthly auto-charged subscriptions for B2B clients, with credits automatically applied each cycle. Build after #1 and #2 ship.

**Phase 1 scope**: build, deploy, and ship #1 and #2. Subscriptions come back in a later phase.

## Non-goals

- Replacing the existing customer dashboard (`lit-static/dapps/dashboard/`). End users still top up there.
- Self-serve subscriptions for end users. The first subscription is a B2B contract Lit sets up; self-serve can come later as a *user-facing* edge in the dashboard, with the backend re-using this app's logic.
- Running inside the TEE. This is a normal cloud service.

## Architecture overview

```
┌────────────────────────────────────┐
│  TEE: lit-api-server               │
│  - /billing/*  (customer top-up,   │
│    balance read, charges)          │
│  - Stripe secret key #1            │
└────────────────────────────────────┘
                  │
                  │ shares: customer identity =
                  │   metadata.wallet_address
                  │
┌────────────────────────────────────┐
│  Non-TEE: lit-payments (NEW)       │
│  - Admin portal (mod credits)      │
│  - Subscription admin + webhook    │
│  - LITKEY listener                 │
│  - Stripe restricted key #2        │
│  - Postgres                        │
└────────────────────────────────────┘
                  │
                  ▼
         Stripe + chain RPC
```

### Repo layout

**Decision**: same monorepo, new top-level crate `lit-payments/`. Separate deployable, shared codebase.

Reason: the customer identity invariant (`metadata.wallet_address` keys the Stripe customer) MUST stay in sync between the two services. Splitting repos invites drift. Same repo, one source of truth, separate Docker images.

```
lit-node-express/
├── lit-api-server/       ← existing (TEE)
├── lit-payments/         ← NEW (non-TEE)
│   ├── src/
│   │   ├── main.rs
│   │   ├── stripe.rs     ← copy of helpers initially; extract to shared crate if/when drift becomes a real risk
│   │   ├── chain.rs      ← LITKEY listener
│   │   ├── db.rs         ← Postgres
│   │   ├── auth.rs       ← password + magic-link
│   │   └── endpoints/
│   │       ├── mod_portal.rs
│   │       ├── subscriptions.rs
│   │       └── webhooks.rs
│   ├── static/           ← admin UI (vanilla HTML/JS, dashboard style)
│   ├── migrations/
│   └── Cargo.toml
└── lit-static/dapps/dashboard/   ← unchanged
```

Tech stack: **Rust + Rocket + vanilla HTML/JS**. Matches existing repo conventions. One toolchain, one deploy story.

### Deployment

- **Hostname**: `payments.litprotocol.com`.
- **Platform**: Railway (service + managed Postgres in the same project).
- **Outside the TEE.** No attestation pain. Operators can hit Railway logs / shell for diagnostics.
- **Ingress**: Railway's default TLS + custom domain.
- **Secrets**: Railway env vars for: Stripe restricted key, DB URL, magic-link signing key + dashboard↔payments HMAC, Resend API key, Slack webhook URL, Alchemy WSS + HTTPS URLs, gateway contract address, treasury Safe address, LITKEY token address, CoinGecko ID, operator allowlist.

### Auth tiers

Two roles, both authenticated against a Postgres `operators` table:

- **`mod`** — can grant credits up to per-grant cap, subject to daily cap. Cannot create subscriptions, change config, or view all customer data.
- **`admin`** — everything: set up subscriptions, change LITKEY rate, view audit logs, manage operators, resolve unattributed payments.

**Mechanism: magic link via email, from day 1.** No shared passwords. Operator enters email → Resend sends a link with a signed, short-lived (15 min) token → click sets a session cookie (~7 days). Audit log records the operator email for every state-changing action.

Operator allowlist is the `operators` table itself — magic-link requests from emails not in the table are silently dropped (response is the same as a successful request to avoid email enumeration).

### Secrets / Stripe key

- The TEE-side `lit-api-server` keeps its current full Stripe secret key.
- `lit-payments` uses a **separate Stripe restricted key** with scopes:
  - `customers:read` + `customers:write`
  - `customer_balance_transactions:read` + `customer_balance_transactions:write`
  - `subscriptions:*`
  - `prices:read`, `products:read`
  - `webhook_endpoints:read`
- Stripe webhook signing secret as env var.
- DB credentials, mod magic-link signing key, chain RPC URL all via env / cloud secret manager.

## Data model (Postgres)

```
operators(id, email, role, created_at, last_login_at)
sessions(token, operator_id, expires_at)

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

subscriptions(
  id,
  stripe_subscription_id,
  stripe_customer_id,
  wallet_address,
  cents_per_cycle,
  status,                         -- active|past_due|canceled
  created_by_operator_id,
  created_at,
  updated_at
)

invoice_credits(
  stripe_invoice_id PRIMARY KEY,  -- idempotency
  stripe_subscription_id,
  cents,
  stripe_balance_transaction_id,
  credited_at
)

litkey_payments(
  id,
  tx_hash,
  log_index,                      -- (tx_hash, log_index) unique together
  source,                         -- 'gateway' | 'direct'
  from_wallet,
  litkey_amount,
  cents_credited,
  rate_used,                      -- cents per LITKEY at credit time
  stripe_customer_id,
  stripe_balance_transaction_id,
  credited_at,
  UNIQUE(tx_hash, log_index)
)

litkey_unattributed_payments(
  id,
  tx_hash,
  log_index,
  from_wallet,
  litkey_amount,
  detected_at,
  resolved_at,                    -- null until admin acts
  resolved_by_operator_id,
  resolution,                     -- 'credited' | 'refunded' | 'ignored'
  stripe_customer_id,             -- set if credited
  notes,
  UNIQUE(tx_hash, log_index)
)

chain_checkpoint(
  chain_id PRIMARY KEY,
  last_processed_block,
  updated_at
)

litkey_rate_history(
  id,
  cents_per_litkey,
  source,                         -- 'coingecko' | 'manual'
  fetched_at,
  updated_by_operator_id,         -- only set when source='manual'
  notes
)
-- "current rate" = SELECT * FROM litkey_rate_history ORDER BY fetched_at DESC LIMIT 1

audit_log(
  id, operator_id, action, payload_json, created_at
)
```

## Feature 1 — Admin credit portal

**API**:
- `POST /admin/grant_credit` — body: `{ email?, wallet_address?, cents, note }`. Headers: session cookie.
  - Look up Stripe customer by email or wallet.
  - Validate caps: per-grant ≤ `MAX_PER_GRANT_CENTS` (default 5000), daily ≤ `MAX_DAILY_CENTS` (default 100000) for this operator.
  - Write `balance_transaction` with `amount = -cents`, description `"Discord grant: {note} (op:{email}, {ISO date})"`.
  - Persist to `grants` table with idempotency key.
- `GET /admin/grants?limit=50` — recent grants by current operator.
- `GET /admin/customer/lookup?email=…` or `?wallet=…` — preview balance + recent activity before granting.

**Lookup branching** (email):
- 0 results → "no Stripe customer with that email. Ask the user to log into the dashboard and set their email under settings."
- 1 result → proceed.
- 2+ results → return all matches with wallet + balance; mod picks by wallet.

**UI**: one page, three sections — Lookup, Grant form, Recent grants. Vanilla HTML/JS.

**Caps**:
- Per-grant: $20 (configurable).
- Daily per operator: $100 (configurable).
- No global cross-operator cap.

**Audit**: every grant logs operator email + timestamp to both Postgres and the Stripe description field (visible in `stripe_report`).

**Effort**: 2-3 days.

## Feature 2 — LITKEY → Stripe credit

### Locked-in answers

- **Chain**: Base (L2, OP Stack).
- **Token**: LITKEY at `0xf732a566121fa6362e9e0fbdd6d66e5c8c925e49`. Standard Base bridge deployment (`OptimismMintableERC20`), bridged from the original Ethereum mainnet token.
- **No `permit` (EIP-2612)** and no `transferWithAuthorization` (EIP-3009). Payment flow is two transactions: `approve` then `pay`.
- **Approval pattern**: exact-amount only in v1.
- **Confirmation depth**: 5 blocks on Base.
- **Gas**: cents per tx on Base.
- **Treasury**: existing company Safe on Base. Incoming transfers don't require Safe signatures; outgoing fund moves go through the normal Safe multisig flow.
- **Listener**: Alchemy WSS subscription as primary + 60s reconciliation poll as safety net.
- **Rate source**: CoinGecko free public API, polled every 5 min. Last-known rate cached in DB.
- **Minimum payment**: $5 equivalent at the current rate.
- **Audit**: internal only (you).

### User flow

User is logged into the dashboard at `dashboard.litprotocol.com`. They click "Pay with tokens", which opens (in a new tab) `payments.litprotocol.com/payWithLitkey?stripeCustomerId=<id>&amount=<cents>` — or the dashboard passes a signed token so the customer ID can't be spoofed (see "Authorization" below). The page:
1. Connects the user's wallet (Wagmi / wallet picker).
2. Shows: "You'll send `{N} LITKEY` (≈ ${cents/100} credit at current rate)."
3. User clicks Pay → wallet pops up → user signs.
4. Page polls for the credit application; on success, shows confirmation + link back to dashboard.

### Attribution architecture

The core problem: if everyone pays to a single company wallet, you can't tell whose payment is whose. We solve it with **a payment gateway contract**, with a fallback path for direct sends.

**Primary path: PaymentGateway contract**

Deploy a small contract:

```solidity
contract LitkeyPaymentGateway {
    IERC20 public immutable litkey;
    address public immutable treasury;

    event Payment(
        bytes32 indexed customerIdHash, // keccak256(customerId)
        address indexed payer,
        uint256 amount,
        string customerId               // raw, for off-chain lookup
    );

    function pay(uint256 amount, string calldata customerId) external {
        litkey.transferFrom(msg.sender, treasury, amount);
        emit Payment(keccak256(bytes(customerId)), msg.sender, amount, customerId);
    }
}
```

The dashboard's Pay button calls `litkey.approve(gateway, amount)` then `gateway.pay(amount, stripeCustomerId)`. The listener watches the `Payment` event, not raw `Transfer`. Customer ID is baked into every event — zero attribution ambiguity even with concurrent payments. Funds land directly in the treasury (no sweeping).

~~If LITKEY supports EIP-2612 `permit`, expose a `payWithPermit` variant.~~ Not applicable — LITKEY on Base is the bridge token, no `permit`. Two-tx (`approve` + `pay`) is the only path.

**Fallback path: direct-to-treasury sends**

Some users will paste the treasury address and send LITKEY directly (skipping the contract). The listener also watches raw `Transfer` events `to == treasury`:
- If `event.from` matches a Lit customer (`metadata.wallet_address`), credit them automatically. Description: `"LITKEY direct send tx {tx_hash} from {from}"`.
- Otherwise insert into `litkey_unattributed_payments` and notify admin via Slack/email. Admin can manually attribute via the admin portal.

### Authorization (preventing customer-ID spoofing)

The dashboard must not put `stripeCustomerId` in the URL as plain text — anyone could craft a link that credits someone else's account. Two options:

- **Signed URL** *(recommended)*: dashboard signs `{stripeCustomerId, expiresAt}` with an HMAC shared between dashboard and payments service. URL is `…?token=<base64(payload).<hmac>>`. Payments service verifies before showing the form. Short expiry (~15 min) so links can't be cached and reused.
- **Cross-domain session**: payments service trusts the dashboard's session cookie (requires same parent domain + cookie scoping). More moving parts.

Go with signed URL.

### Listener

Two-channel design: WebSocket for low-latency event delivery, periodic poll as the catch-up safety net.

- **Primary: WebSocket subscription** (Alchemy WSS). Subscribe to:
  - `Payment` events from the gateway contract.
  - `Transfer` events with `to == treasury` on the LITKEY token.
- On disconnect, reconnect with exponential backoff (1s → 2s → 4s → … capped at 30s). Keep retrying forever.
- **Safety net: poll every 60s** for any logs in `(last_processed_block, latest_block - 5)` we may have missed during a reconnect window. Same handler, same idempotency check — if WS already saw it, the DB insert is a no-op.
- **Checkpoint**: `chain_checkpoint.last_processed_block` is advanced only by the poll path (WS observations don't move the checkpoint forward, since they may be ahead of the confirmation depth).
- **Confirmation depth**: 5 blocks. Defer crediting until `latest_block - event_block >= 5`.
- **Idempotency**: `(tx_hash, log_index)` unique in `litkey_payments`. Concurrent WS + poll observations of the same log → second insert errors with unique-violation → ignored.

Total: ~150 LoC over a pure-polling listener. Worth it for sub-second event detection.

### Credit flow (gateway events)

1. New `Payment` event observed at confirmation depth.
2. Idempotency check on `(tx_hash, log_index)`.
3. Parse `customerId` from event → look up Stripe customer (validate it exists).
4. `cents = litkey_amount * rate_cents_per_litkey` from `litkey_config` (rate snapshotted at credit time).
5. Write `balance_transaction` with `amount = -cents`, description `"LITKEY payment: tx {tx_hash}"`.
6. Persist row to `litkey_payments`.

### Rate management

**Oracle: CoinGecko free public API.** No API key required. Endpoint:

```
GET https://api.coingecko.com/api/v3/simple/price
  ?ids=<litkey-coingecko-id>
  &vs_currencies=usd
```

- Poll every 5 minutes from a background task.
- Write to a `litkey_rate_history` table on every fetch (id, cents_per_litkey, fetched_at, source). Most recent row is the active rate.
- If a fetch fails or returns nonsense (zero / null / >100x recent value): keep the last-known rate, log + Slack alert.
- If the last-known rate is older than 1 hour: pause crediting and alert admin (don't silently credit at stale rates).
- Admin can override the live rate from the admin UI for emergencies (manual override row in `litkey_rate_history` with `source=manual`). Every override is logged in `audit_log`.

**Quote vs. credit semantics.** The payment page shows a *quote* — the rate at page-load — and refreshes the quote every 30s. When the user starts the `approve` tx, the amount is **frozen at the quote-time rate**. At confirmation time we credit using the rate **in effect at the moment of the on-chain `Payment` event** (i.e., the most recent oracle reading at credit time). In practice these match closely; if the rate moved a lot between quote and confirmation, the user might get slightly more or less credit than the page showed. The page should display this as "estimated credit, finalized at confirmation."

**CoinGecko ID**: `lit-protocol` (from `https://www.coingecko.com/en/coins/lit-protocol`).

### Edge cases

- **Sender pays via gateway but `customerId` doesn't match any Stripe customer** → `litkey_unattributed_payments` + admin alert. Should be rare (dashboard generates the customerId from the user's authenticated session).
- **Reorgs** → 5-confirmation depth on Base prevents.
- **Overpayment** → user pays more LITKEY than expected; we credit at the going rate (no refund flow in v1).
- **Rate drift between approve and pay** → if the page is left open and the rate moves, the displayed credit estimate diverges from what the user actually receives at confirmation time. The contract executes whatever amount was approved — it doesn't know the rate. Mitigation: refresh the rate quote every 30s on the page; freeze the amount once the user starts the `approve` tx; show the final credit amount as "estimated, settled at confirmation".
- **Contract upgrade** → contract is intentionally minimal & immutable; if we need to change behavior, deploy a new contract and update the listener.

### What gets built

- Solidity contract (`LitkeyPaymentGateway`) + deploy script + audit (internal, plus maybe a brief external review since real money flows).
- Chain listener (Rust, polling).
- Signed-URL token issuer in the existing dashboard backend (small change to `lit-api-server`).
- `payments.litprotocol.com/payWithLitkey` page (vanilla HTML/JS + Wagmi).
- Admin views for unattributed payments + rate config.
- Postgres tables: `litkey_payments`, `litkey_unattributed_payments`, `litkey_config`.

**Effort**: 2-3 weeks (contract is fast; the audit, end-to-end test on testnet, and rate UX polish take the time).

## Feature 3 — Subscription billing *(deferred to phase 2)*

Detailed design preserved for later; build after #1 and #2 are in production.

Use **Stripe Subscriptions natively**. Stripe handles dunning, retries, receipts, invoicing. We handle the credit application on `invoice.paid`.

**Setup flow (admin UI)**:
1. Admin enters customer email + monthly amount + optional description.
2. App looks up Stripe customer; creates a `Product` + `Price` if needed (or reuses by amount).
3. App creates a `Subscription` with `collection_method=charge_automatically` and the saved default payment method, OR `collection_method=send_invoice` if the client wants to pay manually each cycle.
4. Persist to `subscriptions` table.

**Recurring credit flow**:
1. Stripe sends `invoice.paid` webhook to `POST /webhooks/stripe`.
2. App verifies Stripe signature.
3. Idempotency: insert into `invoice_credits` (PK = `stripe_invoice_id`); if conflict, ignore.
4. Write `balance_transaction` with `amount = -invoice.amount_paid`, description `"Subscription cycle {invoice.id}"`.
5. Update `subscriptions.status` based on the webhook event.

**Dunning / failed payment**: track via `invoice.payment_failed` and `customer.subscription.updated`. Do NOT claw back already-credited balance — Stripe's collection method handles retries; if the subscription ultimately cancels, future credits stop, past credits stand.

**Decisions to confirm (when we revisit)**:
- Credit on `invoice.paid` ✅ (industry standard; recommended).
- Onboarding payment method: the new client almost certainly wants to pay by ACH (Stripe ACH Direct Debit) or wire. Card surcharges on $X,000/month are real. Likely answer: `collection_method=send_invoice` and they pay each invoice manually via Stripe's payment page. Confirm with the client.
- Receipt emails: enable Stripe's native invoice emails.

**Effort**: 1-2 weeks when we come back to it (mostly business decisions + testing webhooks).

> **Interim workaround for the B2B client**: until we ship this, the Discord-mod admin portal (feature 1) is the manual stand-in — set up the subscription in Stripe natively, and on each `invoice.paid` an operator runs a grant in the portal. Not great long-term but unblocks signing the client now.

## Decisions (locked in)

| # | Question | Decision |
|---|---|---|
| 1 | Hostname | `payments.litprotocol.com` |
| 2 | Cloud provider | Railway (service + managed Postgres) |
| 3 | Shared crate vs copy | Copy Stripe helpers into new crate now; extract `lit-billing-core` later if drift becomes painful |
| 4 | Operator auth | Magic link via email from day 1 (Resend); 15-min token, 7-day session cookie |
| 5 | Alert channel | Slack incoming webhook (simplest — single env var) |
| 6 | Reconciliation report | Deferred |
| 7 | Refund / clawback flow | Deferred — handle manually in Stripe dashboard for now |
| 8 | LITKEY chain | Base |
| 9 | `permit` / 3009 | Not supported on Base bridge token; two-tx flow |
| 10 | Rate source | CoinGecko free public API (`id=lit-protocol`), polled every 5 min; manual override available |
| 11 | Minimum payment | $5 equivalent |
| 12 | Treasury | Existing company Safe on Base |
| 13 | Contract audit | Internal review only (Chris) |
| 14 | Approval pattern | Exact-amount only |
| 15 | RPC provider | Alchemy (WSS primary + HTTPS for poll fallback) |

## Decisions table — extended (build-time config)

| Item | Value |
|---|---|
| CoinGecko ID | `lit-protocol` |
| Slack channel | `#payment-alerts` |
| Magic-link sender | `noreply@mail.litprotocol.com` (subdomain, isolated from Google Workspace mail flow) |
| Initial operators | `chris@litprotocol.com` (admin); `Salamiademola73@gmail.com` (mod) |
| Per-grant cap | $20 |
| Per-operator daily cap | $100 |
| Global cap | none |

## Open questions

All phase 1 questions are closed. Plan is ready to start building.

**Subscriptions (deferred — answer when we revisit)**

1. **Payment method for the new B2B client** — card, ACH, or invoice-and-wire? Drives the Stripe `collection_method` choice.

## Sequencing

**Phase 1 — build and ship now**

1. **Foundation + Feature 1: admin credit portal** (1-1.5 weeks)
   - Crate skeleton, Railway project, managed Postgres provisioned, deploy pipeline.
   - Magic-link auth wired up end-to-end (Resend integration + session cookies + operators table).
   - Slack-webhook alert helper.
   - Mod credit portal: lookup, grant form, recent-grants list, caps, audit log.
   - **Milestone**: Discord mod logs in via magic link and grants credits.

2. **Feature 2: LITKEY → Stripe credit** (2-3 weeks)
   - Write `LitkeyPaymentGateway` contract; internal review (Chris).
   - Deploy to Base Sepolia → end-to-end test → deploy to Base mainnet with the Safe as treasury.
   - CoinGecko rate poller + `litkey_rate_history` schema + admin manual-override UI.
   - Alchemy WSS listener + 60s reconciliation poll + `(tx_hash, log_index)` idempotency.
   - Signed-URL token issuer in `lit-api-server` (small addition to the existing dashboard backend).
   - "Pay with tokens" link from dashboard → `payments.litprotocol.com/payWithLitkey?token=…`.
   - Wagmi-based payment page: connect wallet → quote → approve → pay → poll for credit → confirmation.
   - Admin views: unattributed-payments queue + manual attribution, rate history + override.
   - **Milestone**: users self-serve LITKEY payments end-to-end on Base mainnet; manual crediting eliminated.

**Phase 2 — later**

3. **Feature 3: Subscriptions** (1-2 weeks when we revisit)
   - Stripe webhook endpoint + signature verification.
   - Subscription admin UI.
   - **Interim**: B2B client uses native Stripe Subscriptions + the admin portal as a manual stand-in.
   - **Milestone**: new B2B client's monthly auto-credit is automated end-to-end.

**Phase 3 — polish (ongoing)**

4. Reconciliation report (compare DB state vs. Stripe balance_transactions weekly).
5. Refund / clawback flow.
6. User-facing edges in main dashboard if/when self-serve subscriptions become a product priority.
7. LITKEY rate via live oracle (replace fixed rate).

## Risks

- **Mod auth abuse** — magic-link + daily caps + audit log. Even with a leaked operator account, blast radius is bounded.
- **Customer identity drift** — both services must keep treating `metadata.wallet_address` as the customer key. Extracting `lit-billing-core` later eliminates this risk.
- **Signed-URL token leakage** — short expiry (15 min) bounds the window; even if leaked, an attacker can only direct a payment to *their own* wallet to credit *the original user's* account, which is harmless.
- **PaymentGateway contract bug** — small surface but real money. Mitigations: keep it minimal & immutable, internal review + brief external audit, deploy on testnet first, cap per-tx amount, monitor on-chain.
- **LITKEY rate staleness** — fixed-rate model means if you forget to update during a price move, customers over- or under-credit. Mitigation: weekly admin reminder, alert if rate hasn't been touched in N days, move to oracle later.
- **Unattributed-payments backlog** — if many users send LITKEY directly to treasury, the admin queue grows. Mitigation: make the dashboard "Pay with tokens" flow the obvious path; document that direct sends require admin intervention.
- **Reorg risk** — 5-confirmation depth on Base mitigates. Risk is already near-zero on Base past a handful of blocks.
- **Webhook reliability** *(phase 2)* — if the payments app is down when Stripe fires `invoice.paid`, Stripe retries for up to 3 days. Acceptable. Monitor webhook 5xx rate.
