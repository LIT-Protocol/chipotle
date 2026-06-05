# Auto Top-Up — Implementation Plan

**Status:** Locked. Ready to implement.
**Scope:** Stripe-paying users only. LITKEY / crypto users are out of scope.
**Architecture:** Webhook-driven, lit-payments-owned. lit-api-server is unchanged except for one tiny cache-invalidation endpoint.

---

## 1. Goal

After any Lit Action or management deduction reduces a user's Stripe customer balance below a configured threshold, automatically charge their saved card off-session for a configured top-up amount, subject to a configured monthly cap. UI matches Claude Console and OpenAI Platform's auto-recharge modals.

Auto top-up is opt-in. Users configure it via the dashboard after saving a card.

---

## 2. Product framing

### Soft cap, explicit trade-off

Monthly cap is **best-effort**, not a hard contract. UI copy must say "approximately $X/month." The only way the cap can be exceeded is a rare race against Stripe's PaymentIntent list endpoint, which isn't strongly read-after-write consistent. Probability <1% per opportunity; overshoot bounded by one top-up amount when it fires.

### Bias toward more top-ups, never fewer

A missed top-up means Lit Action execution can fail mid-flight when balance hits zero. That's user-visible and disastrous. An extra top-up means the user has unexpected credits in their account — recoverable via refund through Sally's admin portal. The design intentionally favors over-charging over under-charging in any ambiguous case.

### What we are not building

- Hard monthly cap with reservation ledger.
- Currency support beyond USD.
- Refund-aware cap accounting (refunds do NOT restore monthly cap capacity).
- Automated daily/rolling restrictions for public-tier API keys (future feature).
- Reconciler cron — Stripe's 3-day webhook retry + manual recovery via admin portal cover the rare delivery-failure case.

---

## 3. Architecture overview

### Trigger source: Stripe `customer.updated` webhook

When `lit-api-server` deducts credits via `POST /v1/customers/{id}/balance_transactions`, Stripe fires a `customer.updated` event containing the new balance and the previous balance. `lit-payments` listens to this event, filters for balance changes, and decides whether to top up.

**Empirically validated** in Stripe test mode (June 2026): 21 balance_transactions → 21 `customer.updated` events, 1:1, no coalescing under either slow (0.5s gap) or rapid parallel (10 in 2.8s) load. Each event contains the new `balance` and `previous_attributes.balance` for delta detection.

### Components

| Component | Path / Location | Role |
|---|---|---|
| **Dashboard** | `lit-static/dapps/dashboard/` | Vanilla HTML/JS. Adds config modal, save-card flow, status banners. Talks directly to `lit-payments`. |
| **lit-api-server** | `lit-api-server/`, runs in TEE | Existing API server. **No trigger code, no auto-top-up logic.** Only adds: 1 internal `invalidate_balance_cache` endpoint. The existing wallet/API-key auth module (`billing_auth.rs`) is extracted to a shared crate so `lit-payments` can use it. |
| **lit-payments** | `lit-payments/`, on Railway | Existing service. Adds: 3 dashboard-facing endpoints (now with wallet/API-key auth), 2 Stripe webhook handlers, 2 new Postgres tables. Becomes the home of all auto-top-up logic. |
| **Postgres** | inside `lit-payments` DB | Existing DB. Adds 2 new tables: `auto_topup_config`, `auto_topup_credits`. |
| **Stripe** | external | Customer, PaymentMethod, PaymentIntents, balance transactions, webhooks (`customer.updated`, `payment_intent.succeeded`, `payment_intent.payment_failed`). |
| **Shared auth crate** | new, e.g. `lit-billing-auth/` | Extracted from `lit-api-server::core::v1::guards::billing_auth`. Accepts wallet signature (EIP-712) and API key. Used by both lit-api-server (existing endpoints) and lit-payments (new dashboard endpoints). |

### Why the TEE no longer holds auto-top-up logic

Architectural principle: keep the TEE (lit-api-server) narrowly focused on Lit Action execution and key-usage operations. Anything that can live outside the TEE should. Auto-top-up logic doesn't require a TEE, so it lives in `lit-payments`. The empirical test confirms Stripe's `customer.updated` webhook reliably delivers per-balance-change events, so we don't need an internal trigger from lit-api-server.

### Storage map

| Data | Location | Reason |
|---|---|---|
| Config (5 fields: `enabled`, `threshold_cents`, `topup_amount_cents`, `monthly_cap_cents`, `payment_method_id`) | Postgres `auto_topup_config` | Fast reads, atomic writes, schema evolution, admin queryability |
| Consent record (`consent_version`, `consent_signed_at`) | Postgres `auto_topup_config` (same row) | Off-session merchant-initiated charges require recorded user consent (PSD2 / SCA + Stripe policy) |
| SCA-pending state (`pending_action_pi_id`, `pending_action_at`) | Postgres `auto_topup_config` (same row) | Drives dashboard banner + resume flow |
| Card data | Stripe (PaymentMethod attached to Customer) | Never on our servers; PCI scope stays with Stripe |
| Wallet ↔ Stripe Customer mapping | Stripe customer metadata (`metadata.wallet_address`) | Existing pattern, unchanged |
| Charge history (PaymentIntents) | Stripe (filter by `metadata.source=auto_topup`) | Source of truth |
| Monthly spend total | Computed on demand by listing PIs | No counter to race on |
| Failure state | Derived by listing recent PIs and counting failures | No counter to race on |
| Credit dedup (1 row per credited PI) | Postgres `auto_topup_credits` | Permanent dedup beyond Stripe's 24h idempotency cache, beyond 30-day webhook resend window |

---

## 4. Postgres schema

Migration: `lit-payments/migrations/{timestamp}_auto_topup.sql`

```sql
CREATE TABLE auto_topup_config (
  customer_id              TEXT  PRIMARY KEY,
  wallet_address           TEXT  NOT NULL UNIQUE,
  enabled                  BOOLEAN NOT NULL DEFAULT false,
  threshold_cents          BIGINT,
  topup_amount_cents       BIGINT,
  monthly_cap_cents        BIGINT,
  payment_method_id        TEXT,
  consent_version          TEXT,
  consent_signed_at        TIMESTAMPTZ,
  disabled_reason          TEXT,         -- NULL, 'manual', 'failures', 'card_invalid', 'requires_action'
  pending_action_pi_id     TEXT,         -- set when off-session PI returns requires_action; cleared on terminal status
  pending_action_at        TIMESTAMPTZ,
  updated_at               TIMESTAMPTZ NOT NULL DEFAULT now(),

  CONSTRAINT enabled_requires_config CHECK (
    enabled = false OR (
      threshold_cents IS NOT NULL AND threshold_cents > 0 AND
      topup_amount_cents IS NOT NULL AND topup_amount_cents >= 500 AND
      monthly_cap_cents IS NOT NULL AND monthly_cap_cents >= topup_amount_cents AND
      payment_method_id IS NOT NULL AND
      consent_version IS NOT NULL AND consent_signed_at IS NOT NULL
    )
  )
);

CREATE INDEX ON auto_topup_config (wallet_address);

CREATE TABLE auto_topup_credits (
  payment_intent_id              TEXT  PRIMARY KEY,
  customer_id                    TEXT  NOT NULL,
  amount_cents                   BIGINT NOT NULL,
  stripe_balance_transaction_id  TEXT,
  credited_at                    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX ON auto_topup_credits (customer_id, credited_at);
```

CHECK constraint enforces: `enabled=true ⇒ all required fields non-null`, `cap >= topup_amount`, min top-up $5.00 (matches existing one-shot floor), positive cents, USD-only (implicit).

---

## 5. Endpoints

### Dashboard-facing (on `lit-payments`, behind the shared auth module)

The dashboard talks directly to `lit-payments` for all auto-top-up endpoints. Auth uses the extracted shared module which accepts wallet signature (EIP-712 ChainSecured) or API key — identical to what the dashboard sends to `lit-api-server` today for existing billing endpoints.

| Method + Path | Purpose | Behavior |
|---|---|---|
| `POST /billing/setup_intent` | Save card | Call `lit_billing_core::customer::find_by_wallet`. Refuse if user has no Stripe customer (require first manual top-up to bootstrap). Create Stripe SetupIntent (`usage=off_session, customer=cus_xxx`). Return `client_secret` + publishable key. |
| `GET /billing/auto_topup_config` | Read config | Read row from `auto_topup_config` by customer_id. Return JSON (including `pending_action_pi_id` so dashboard knows whether to show the SCA banner). |
| `PUT /billing/auto_topup_config` | Save config | Validate: verify `payment_method_id` belongs to this customer via `GET /v1/customers/{cus}/payment_methods` membership check; enforce `cap >= topup_amount`, positive cents, min top-up $5. UPSERT row. |
| `POST /billing/auto_topup_resume_pending` | Resume SCA-pending top-up | Read `pending_action_pi_id` from config. If set, retrieve PI's `client_secret` from Stripe and return it to the dashboard for `stripe.handleNextAction`. Return 404 if no pending action. |

### Internal (on `lit-api-server`, auth: `X-Internal-Secret`)

| Method + Path | Purpose | Behavior |
|---|---|---|
| `POST /internal/invalidate_balance_cache` | Drop cached balance | Called by `lit-payments` after every successful auto-credit. Body: `{customer_id}`. Calls existing `state.balance_cache.invalidate(&customer_id)` (precedent at `lit-api-server/src/stripe.rs:612`). |

### Webhooks (on `lit-payments`, auth: Stripe-Signature HMAC)

| Method + Path | Purpose |
|---|---|
| `POST /stripe/webhook` | Single endpoint receiving all three event types: `customer.updated` (trigger), `payment_intent.succeeded` (credit), `payment_intent.payment_failed` (failure handling). Routed by `event.type`. |

---

## 6. `customer.updated` webhook — trigger flow

This is where auto-top-up decisions happen. Fires every time `customer.balance` changes (verified empirically).

### Step-by-step

1. **Verify Stripe-Signature** (see §7 for HMAC details).

2. **Parse event. Filter:**
   - `event.type != "customer.updated"` → ignore, return 200.
   - `event.data.previous_attributes.balance` not present → ignore (balance didn't actually change; could be email/address/metadata update). Return 200.

3. **Quick-exit on payload data:**
   - Read `event.data.object.balance` (new balance).
   - Read user's threshold from Postgres `auto_topup_config` by `customer_id`.
   - If `!enabled`: return 200.
   - If `available_credit (= -new_balance) >= threshold_cents`: return 200. (Not below threshold; nothing to do.)
   - If `pending_action_pi_id` is set: return 200 (SCA flow in progress; don't fire another charge).

4. **Acquire per-customer mutex.**
   `moka::sync::Cache<String, Arc<tokio::sync::Mutex<()>>>` with 5-minute TTL keyed by `customer_id`. Serializes parallel `customer.updated` events for the same customer within this process.

5. **Re-fetch current balance from Stripe** via `lit_billing_core::balance::fetch(stripe_client, &customer_id)`. Don't trust the webhook payload — it could be stale by the time we acquired the mutex. If `available_credit >= threshold_cents`: release mutex, return 200.

6. **List PIs for this customer this month** via `GET /v1/payment_intents?customer={cus_xxx}&created[gte]={month_start_utc}&limit=100`. Paginate via `starting_after` until `has_more=false`. Client-side filter on `metadata.source == "auto_topup"`.

7. **Derive failure state:** walk the list from most recent backwards, count consecutive failed PIs (`status=requires_payment_method` or with `last_payment_error.code` in `card_declined`, `expired_card`, `insufficient_funds`, `incorrect_cvc`, `processing_error`, etc.). If `consecutive_failures >= 3`:
   - `UPDATE auto_topup_config SET enabled=false, disabled_reason='failures', updated_at=now() WHERE customer_id=...`
   - Send the "card needs updating" email via Resend.
   - Release mutex, return 200.

8. **Recent-PI short-circuit:** if any non-failed PI exists in the last 10 minutes → release mutex, return 200 (already topped up recently).

9. **Cap check:** sum amounts of all non-failed PIs this month. If `sum + topup_amount_cents > monthly_cap_cents`: release mutex, return 200 (cap reached).

10. **Create off-session PaymentIntent:**
    ```
    POST /v1/payment_intents
      customer: cus_xxx
      payment_method: pm_xxx (from config)
      amount: topup_amount_cents
      currency: usd
      off_session: true
      confirm: true
      metadata[source]: auto_topup
      metadata[wallet_address]: 0x...
    ```
    No deterministic Idempotency-Key bucket. The mutex serializes within-process; the rare list-staleness race is accepted per the soft-cap product trade-off.

    **Handle the synchronous response:**
    - `status=succeeded`: do nothing here. The `payment_intent.succeeded` webhook will handle crediting.
    - `status=requires_action` (SCA): UPDATE `auto_topup_config SET pending_action_pi_id=$1, pending_action_at=now(), disabled_reason='requires_action'`. Send the "action required" email with deep link to dashboard. (The PI stays alive at Stripe waiting for the user to complete 3DS.)
    - `status=processing` (rare for cards): do nothing here. Webhook will deliver the final outcome.
    - Error response (`card_declined`, `expired_card`, etc.): do nothing here. The `payment_intent.payment_failed` webhook will fire and run the counter-derivation in step 7 next time.
    - HTTP timeout: do nothing. Stripe will fire the `payment_intent.succeeded` webhook when the charge settles, if it was created. Self-healing.

11. **Release mutex. Return 200.** Don't credit synchronously — credit happens in the `payment_intent.succeeded` webhook (§7).

---

## 7. `payment_intent.succeeded` and `payment_intent.payment_failed` webhook — credit / failure flow

This is the SAME endpoint as §6 (`POST /stripe/webhook`); the handler routes by `event.type`. The charge step in §6 is async (SCA, processing, timeouts), so the credit happens here regardless of how the charge completed.

### Webhook signature verification (applies to all events)

1. **Read raw body** with Rocket `Data` handler (NOT the JSON extractor — HMAC must verify exact bytes). Size limit ~1 MB.
2. **Verify `Stripe-Signature` header:**
   - Parse `t={timestamp},v1={hex_signature}`.
   - Reject if `|now - timestamp| > 300s` (5-minute tolerance).
   - Compute `HMAC-SHA256(STRIPE_WEBHOOK_SECRET, "{timestamp}.{raw_body}")`.
   - Constant-time compare against `v1` (`subtle::ConstantTimeEq`). Invalid → 401.
3. **Parse JSON event** and route by `event.type`.

### `payment_intent.succeeded`

1. Filter: `pi.metadata.source != "auto_topup"` → return 200 (not ours; could be a manual top-up).

2. (Optional defensive) GET `/v1/payment_intents/{pi.id}` to use the latest object shape (defends against Stripe API version drift in webhook payload).

3. **Atomic dedup insert:**
   ```sql
   INSERT INTO auto_topup_credits (payment_intent_id, customer_id, amount_cents)
     VALUES ($1, $2, $3)
     ON CONFLICT (payment_intent_id) DO NOTHING
     RETURNING payment_intent_id;
   ```
   - If no row returned (already credited from a prior delivery of this event) → return 200.
   - If row inserted → continue.

4. **Credit the user's wallet balance** via `lit_billing_core::balance::write_transaction(stripe_client, customer_id, -pi.amount, description="Auto top-up via {pi.id}", idempotency_key="credit:{pi.id}")`.

5. **Update row** with the returned `stripe_balance_transaction_id`.

6. **Clear SCA pending state** if applicable:
   ```sql
   UPDATE auto_topup_config
     SET pending_action_pi_id=NULL, pending_action_at=NULL, disabled_reason=NULL
     WHERE customer_id=$1 AND pending_action_pi_id=$2;
   ```

7. **Invalidate lit-api-server's balance cache:**
   `POST {LIT_API_SERVER_BASE_URL}/internal/invalidate_balance_cache` with `X-Internal-Secret` and `{customer_id}`. Fire-and-forget; ignore errors (cache will refresh in ≤10 min regardless).

8. **Return 200.**

### `payment_intent.payment_failed`

1. Filter: `pi.metadata.source != "auto_topup"` → return 200.

2. Send "card declined" email + dashboard banner. Email includes: amount, card last4, decline reason (human-friendly), CTA to update card.

3. If `pending_action_pi_id == pi.id` (this was an SCA-pending PI that the user abandoned or that expired):
   ```sql
   UPDATE auto_topup_config
     SET pending_action_pi_id=NULL, pending_action_at=NULL, disabled_reason=NULL
     WHERE customer_id=$1 AND pending_action_pi_id=$2;
   ```

4. **No counter writes.** The auto-disable decision happens in the `customer.updated` handler at step 7 (§6) by listing recent PIs and counting consecutive failures.

5. **Return 200.**

### What 5xx returns do

If our handler errors (DB unavailable, Stripe API down), return 5xx. Stripe retries the webhook with exponential backoff for **up to 3 days**. Eventual delivery is the safety net for transient infrastructure failures.

**Important:** return 2xx ONLY after the credit/state work is committed. If you ack before committing and then crash, Stripe will not retry and credit can be lost.

---

## 8. `lit-api-server` changes

### The only change: cache-invalidation endpoint

```rust
#[post("/internal/invalidate_balance_cache", data = "<body>")]
async fn invalidate_balance_cache(
    body: Json<InvalidateRequest>,
    state: &State<StripeState>,
    _guard: InternalSecretGuard,
) -> Status {
    state.balance_cache.invalidate(&body.customer_id).await;
    Status::Ok
}
```

Auth: `X-Internal-Secret` header (constant-time compare). Called by `lit-payments` after every successful auto-credit so the next Lit Action sees the new balance immediately rather than waiting for the 10-minute cache TTL.

### Auth extraction (separate refactor, can be done in parallel)

The existing `lit-api-server/src/core/v1/guards/billing_auth.rs` (wallet signature + API key validation) is extracted into a shared crate (e.g., `lit-billing-auth`) so `lit-payments` can use it. Extraction work includes:

- Move the EIP-712 / ChainSecured signature verifier.
- Move the API-key → master-wallet on-chain resolver (uses the `allApiKeyHashesToMaster` contract).
- Move the wallet → Stripe customer mapping helper.
- Move the local caches for these resolutions.
- Refactor the Rocket request guard adapter to not depend on `lit-api-server`-specific state.

`lit-payments` gains a dependency on this crate plus the on-chain RPC client config (`ALCHEMY_HTTPS_URL` or equivalent already used for LITKEY).

---

## 9. Dashboard changes

### Save-card flow

1. User clicks "Add a card for auto top-up."
2. Dashboard calls `POST /billing/setup_intent` on `lit-payments` with existing auth headers (wallet sig or API key — the shared auth module handles either).
3. Backend returns `{ client_secret, publishable_key }`.
4. Dashboard initializes Stripe.js with the publishable key, mounts the Payment Element in **setup mode** (not payment mode).
5. User enters card. Dashboard calls `stripe.confirmSetup({ elements, confirmParams: { return_url: dashboard_url } })`.
6. On return, dashboard reads `setup_intent` query param, calls `stripe.retrieveSetupIntent(client_secret)`, extracts `payment_method`.
7. Stores `pm_xxx` in local state pending submission with the rest of the config.

### Save-config flow

1. Modal collects: enable toggle, threshold (USD), top-up amount (USD), monthly cap (USD), card picker (preselected to the newly-saved `pm_xxx` or existing default), consent checkbox with explicit text ("I authorize Lit Protocol to charge my saved card up to approximately $X per month when my balance falls below $Y...").
2. On submit, dashboard calls `PUT /billing/auto_topup_config` on `lit-payments` with `{enabled, threshold_cents, topup_amount_cents, monthly_cap_cents, payment_method_id, consent_version: "v1"}`.
3. Backend validates and persists.

### SCA resume flow

When last off-session charge returned `requires_action`:

1. On dashboard load, `GET /billing/auto_topup_config` returns `pending_action_pi_id` non-null. Dashboard renders the action-required banner.
2. User clicks "Confirm now" (or arrives via the email deep link).
3. Dashboard calls `POST /billing/auto_topup_resume_pending`. Backend returns `{ payment_intent_id, client_secret }`.
4. Dashboard runs `stripe.handleNextAction({ clientSecret })`. Stripe.js opens the 3DS challenge modal.
5. User completes the challenge.
6. PI transitions to `succeeded` at Stripe. The `payment_intent.succeeded` webhook (§7) credits the wallet and clears the pending state.
7. Dashboard re-fetches config and removes the banner.

If the user abandons the challenge: PI eventually transitions to `requires_payment_method`. The `payment_intent.payment_failed` webhook clears the pending state and emails the user.

### Status banners

- **Enabled, healthy:** "Auto top-up: when your balance drops below $X, we'll charge $Y to card ending in ****1234, up to ~$Z/month."
- **Enabled, requires_action (SCA pending):** "Action required to complete your $X auto top-up. [Confirm now]" — clicking triggers the resume flow.
- **Disabled by user:** "Auto top-up is off. [Enable]"
- **Auto-disabled after failures:** "Auto top-up was paused after 3 failed attempts. Please update your card. [Manage]"

### Email notifications

`lit-payments` sends transactional emails via the existing Resend integration:

| Trigger | Subject | Body |
|---|---|---|
| Off-session PI returns `authentication_required` | "Action required: confirm your auto top-up" | Amount, card last4, deep link to dashboard billing page |
| `payment_intent.payment_failed` webhook | "Your auto top-up couldn't be charged" | Amount, card last4, decline reason, link to update card |
| Auto-disable after 3 consecutive failures | "Auto top-up paused — update your card" | Reason summary, link to update card and re-enable |

---

## 10. Concurrency model — three layers of defense

| Layer | What it prevents | Mechanism | Scope |
|---|---|---|---|
| 1. Per-customer Tokio mutex in `lit-payments` | Wasted Stripe API calls under burst of parallel `customer.updated` events | `moka::sync::Cache<String, Arc<Mutex<()>>>` TTL'd at 5 minutes | Per-process |
| 2. Postgres unique constraint on `auto_topup_credits.payment_intent_id` | Double-credit on webhook replays (Stripe Dashboard resend up to 15 days, CLI resend up to 30 days, well beyond Stripe's 24h idempotency-key cache) | `INSERT … ON CONFLICT DO NOTHING` | Permanent |
| 3. Stripe Idempotency-Key on balance-transactions credit write | Double-credit from concurrent credit attempts (e.g., webhook retries within Stripe's 24h dedup cache) | `Idempotency-Key: credit:{pi.id}` | Stripe-global, 24h |

The mutex is optimization. Layers 2 and 3 are correctness primitives.

---

## 11. Edge cases — handled

| Case | Handling |
|---|---|
| 5+ parallel `customer.updated` events for same customer | Mutex serializes within process; the early balance check (step 3 in §6) short-circuits second/third/etc. events whose balance is already above threshold after the first top-up |
| HTTP timeout on `paymentIntents.create` | Stripe still fires `payment_intent.succeeded` webhook when the PI settles; credit handler runs. Self-healing. |
| Webhook delivered twice / replayed (within 24h or up to 30 days via dashboard/CLI resend) | `INSERT auto_topup_credits ON CONFLICT DO NOTHING` skips the second one |
| Webhook handler crashes mid-execution | Stripe retries (we returned 5xx or timed out) with exponential backoff for up to 3 days |
| Card declined | `payment_intent.payment_failed` webhook fires → email user → next `customer.updated` event derives consecutive failures from listing PIs and disables after 3 |
| SCA required (`requires_action`) | §6 step 10 sets `pending_action_pi_id`; "action required" email sent + dashboard banner; user clicks "Confirm now" → §9 resume flow → 3DS completes → `payment_intent.succeeded` webhook credits the wallet and clears pending state |
| User toggles auto-top-up off between webhook fire and handler execution | Handler reads `enabled=false` from Postgres and short-circuits |
| Trigger fires when balance is actually above threshold (e.g., user just topped up manually) | §6 step 5 (balance fetch from Stripe) short-circuits |
| `customer.updated` for non-balance changes (email/address/metadata) | Filter on `previous_attributes.balance` presence — ignore if not present |
| Stripe customer balance cached stale in `lit-api-server` after auto-top-up | `payment_intent.succeeded` handler calls `POST /internal/invalidate_balance_cache` after successful credit |
| `customer.updated` event lost in transit | Self-healing — next deduction fires another `balance_transactions` write → another `customer.updated` → re-evaluation |
| Stripe API version drift in event payload | Webhook handler optionally re-fetches the PI by id before crediting |
| Pagination on PI list at scale | Use `starting_after` until `has_more=false` |

---

## 12. Edge cases — explicit trade-offs (accepted, not handled)

| Case | What happens | Recovery |
|---|---|---|
| **List-endpoint staleness during cap check** | `paymentIntents.list` doesn't guarantee read-after-write consistency. If trigger A creates a PI and trigger B's list call runs within ms while the list is briefly stale, B can undercount and pass the cap check. Result: cap exceeded by up to 1 top-up. <1% per opportunity. | Soft cap; UI says "approximately $X/month." Manual refund via admin portal if user complains. |
| **Webhook delivery delay** | Stripe delivers `customer.updated` 1–5s after deduction (sometimes longer under load). User running rapid Lit Actions near threshold could exhaust threshold buffer before top-up fires. | Set threshold high enough to absorb typical webhook latency × user's burn rate. |
| **Webhook delivery failure beyond 3-day retry window** | User paid (if charge happened) but not credited on our side, OR top-up never fired. Extremely rare. | Manual recovery via Sally's admin portal. |
| **Refunds / disputes** | Do NOT restore monthly cap capacity. Soft cap remains soft. | Documented. |
| **Currency other than USD** | Not supported. | Out of scope. |
| **Month boundary** | UTC. | Documented in UI copy. |

---

## 13. New environment variables

### `lit-api-server`
- `LIT_INTERNAL_SHARED_SECRET` — high-entropy random string for the cache-invalidation endpoint.

### `lit-payments`
- `LIT_API_SERVER_BASE_URL` — e.g., `https://api.litprotocol.com`. For the cache-invalidation callback.
- `LIT_INTERNAL_SHARED_SECRET` — same value as on lit-api-server.
- `STRIPE_WEBHOOK_SECRET` — from Stripe Dashboard after registering the webhook endpoint.
- `ALCHEMY_HTTPS_URL` (or equivalent) — for the on-chain master-key resolver inherited from the extracted auth module (if not already present for LITKEY).

### Dashboard
- `STRIPE_PUBLISHABLE_KEY` — if not already present.

---

## 14. Service-auth requirements for `X-Internal-Secret`

Used only for the cache-invalidation callback from `lit-payments` to `lit-api-server`. Webhooks use Stripe-Signature HMAC; dashboard endpoints use the shared wallet/API-key auth module.

- Generated with at least 256 bits of entropy (`openssl rand -base64 32`).
- Stored in env vars only, never in code or commits.
- TLS only (Railway and production deployments are HTTPS).
- Constant-time comparison in the handler (`subtle::ConstantTimeEq`).
- Never logged, never echoed in error responses.
- Rotation: deploy both services with both old and new secrets accepted, swap, drop old.

---

## 15. Sequence diagrams

### Setup (one-time)

```
USER → DASHBOARD ─► lit-payments /billing/setup_intent ─► Stripe (create SetupIntent)
                                                          │
USER → DASHBOARD ◄─────────────── client_secret ──────────┘
USER → DASHBOARD ─► Stripe.js (card entered) ─► Stripe (PaymentMethod attached, pm_xxx)
USER → DASHBOARD ─► lit-payments /billing/auto_topup_config ─► Stripe (verify pm_xxx ownership)
                                                                │
                                                                ▼
                                                            Postgres (UPSERT auto_topup_config)
```

### Runtime — every Lit Action deduction

```
CLIENT ──► lit-api-server (run Lit Action, deduct credits via balance_transactions)
                                                          │
                                                          ▼
                                                      Stripe (POST /v1/customers/{id}/balance_transactions)
                                                          │ customer.balance updates
                                                          │
                                                          ▼ Stripe fires customer.updated webhook
                                                          │
              ┌───────────────────────────────────────────┘
              │
              ▼
        lit-payments POST /stripe/webhook (event.type = customer.updated)
              │
              ├─► verify HMAC
              ├─► filter: previous_attributes.balance present? YES
              ├─► quick exit: new balance >= threshold? then return 200
              ├─► quick exit: pending_action_pi_id set? then return 200
              ├─► acquire mutex[customer]
              ├─► fresh balance::fetch from Stripe (don't trust payload)
              ├─► list PIs this month from Stripe (paginated, filter metadata.source=auto_topup)
              ├─► derive failure state → disable if 3+ consecutive
              ├─► recent-PI short-circuit (< 10 min)
              ├─► cap check (sum + amount > cap → skip)
              ├─► POST /v1/payment_intents (off_session=true, confirm=true)
              ├─► handle response (succeeded → wait for webhook; requires_action → set pending + email; declined → wait for failure webhook)
              └─► release mutex, return 200

CLIENT ◄── lit-api-server (Lit Action result returned earlier, never waited)
```

### Webhook — credit (seconds to days later)

```
Stripe ──► lit-payments POST /stripe/webhook (event.type = payment_intent.succeeded)
              │
              ├─► verify HMAC
              ├─► filter: metadata.source = auto_topup? YES
              ├─► INSERT auto_topup_credits ON CONFLICT DO NOTHING
              │     │
              │     ▼
              │     no row returned (already credited) → return 200
              │     row returned (new credit) → continue
              │
              ├─► Stripe POST /v1/customers/{c}/balance_transactions, Idempotency-Key: credit:{pi.id}
              ├─► UPDATE auto_topup_credits SET stripe_balance_transaction_id=...
              ├─► UPDATE auto_topup_config (clear pending_action_pi_id if matching)
              ├─► POST /internal/invalidate_balance_cache to lit-api-server (fire-and-forget)
              └─► return 200
```

### Webhook — failure

```
Stripe ──► lit-payments POST /stripe/webhook (event.type = payment_intent.payment_failed)
              │
              ├─► verify HMAC
              ├─► filter: metadata.source = auto_topup? YES
              ├─► send email + dashboard banner
              ├─► UPDATE auto_topup_config (clear pending if matching this PI)
              └─► return 200
                  (no counter writes — disable derived from listing PIs in next customer.updated)
```

---

## 16. Implementation phases

Phases are strictly sequential at the gate level — each gates the next. Sub-tasks within a phase can be parallelized. Total estimated effort: **~2 weeks of focused work**.

### Dependency chain

```
Phase 1: Foundation
  └─► Phase 2: Auth extraction
        └─► Phase 3: Saved card flow (SetupIntent)
              └─► Phase 4: Config CRUD
                    └─► Phase 5: customer.updated trigger handler
                          └─► Phase 6: payment_intent webhooks (credit + failure)
                                └─► Phase 7: Dashboard UI
                                      └─► Phase 8: Operational hardening
                                            └─► Phase 9: Production rollout
```

### Phase 1 — Foundation (~0.5 day)

**Tasks:**
- Migration `lit-payments/migrations/{timestamp}_auto_topup.sql` with both tables + CHECK constraints.
- Add new env vars: `LIT_API_SERVER_BASE_URL`, `LIT_INTERNAL_SHARED_SECRET` (both sides), `STRIPE_WEBHOOK_SECRET`.
- Add `X-Internal-Secret` Rocket request guard in `lit-api-server` for the new cache-invalidation endpoint.
- Build `POST /internal/invalidate_balance_cache` on `lit-api-server` (calls existing `balance_cache.invalidate`).

**Gate to Phase 2:** migration applies cleanly; a smoke test call to `invalidate_balance_cache` returns 200 with the right secret and 401 without.

### Phase 2 — Auth extraction (~2 days)

**Tasks:**
- Create a new crate (e.g., `lit-billing-auth/`).
- Move from `lit-api-server/src/core/v1/guards/billing_auth.rs`: the EIP-712 / ChainSecured verifier, the API-key → master-wallet on-chain resolver, the wallet → Stripe customer mapping, the caches.
- Refactor the Rocket request guard to be service-agnostic.
- Update `lit-api-server` to depend on the new crate (existing endpoints unchanged externally).
- Update `lit-payments` to depend on the new crate.
- Add on-chain RPC client config to `lit-payments` (`ALCHEMY_HTTPS_URL`, etc.) if not already present.

**Gate to Phase 3:** both services compile and run; existing `lit-api-server` billing endpoints still authenticate identically; a smoke test on a placeholder endpoint in `lit-payments` accepts both wallet-sig and API-key auth.

### Phase 3 — Saved card flow (~2 days)

**Tasks:**
- `POST /billing/setup_intent` on `lit-payments`, behind the new shared auth.
- Calls `lit_billing_core::customer::find_by_wallet`. Returns 400 if no Stripe customer (user must do first manual top-up first).
- Creates Stripe SetupIntent via `lit_billing_core::StripeClient::post_with_idempotency`.

**Gate to Phase 4:** can save a Stripe test-mode card (4242…) end-to-end with both wallet-auth and API-key-auth flows; `pm_xxx` is attached to the right customer.

### Phase 4 — Config CRUD (~1 day)

**Tasks:**
- `GET /billing/auto_topup_config` on `lit-payments` (read row by `customer_id`).
- `PUT /billing/auto_topup_config` on `lit-payments`: verify `pm_xxx` ownership via Stripe, validate `cap >= topup_amount`, positive cents, min top-up $5, UPSERT row.

**Gate to Phase 5:** config can be saved, read back, CHECK constraints reject `enabled=true` with null fields.

### Phase 5 — `customer.updated` trigger handler (~3 days)

**Goal:** the core decision logic. The expensive phase.

**Tasks:**
- `POST /stripe/webhook` on `lit-payments` — raw `Data` handler, HMAC verification, route by `event.type`.
- For `customer.updated`: full handler per §6:
  - Filter on `previous_attributes.balance` presence.
  - `moka` mutex cache per customer.
  - Fresh balance fetch via `lit_billing_core::balance::fetch`.
  - List PIs with pagination, client-side filter.
  - Failure-derivation logic.
  - Cap check.
  - Off-session PaymentIntent create with handling for succeeded / requires_action / processing / errors / timeouts.

**Gate to Phase 6:** in Stripe test mode, a real card is charged via off-session PI triggered from a `balance_transactions` write that drops balance below threshold. SCA card sets `pending_action_pi_id` and sends email. Burst of parallel `customer.updated` events for the same customer collapses to one charge via mutex + early balance check.

### Phase 6 — `payment_intent.succeeded` and `payment_intent.payment_failed` handlers (~2 days)

**Tasks:**
- Branch on `event.type` in the same `/stripe/webhook` endpoint.
- `payment_intent.succeeded` handler per §7: INSERT-ON-CONFLICT dedup → balance credit with idempotent key → UPDATE row → clear SCA pending → invalidate lit-api-server cache.
- `payment_intent.payment_failed` handler per §7: email + banner + clear SCA pending if matching.

**Gate to Phase 7:** test-mode PaymentIntent → webhook delivers → user credited → `lit-api-server` cache invalidated. Webhook resend on the same event → no double credit. Decline test card → email sent.

### Phase 7 — Dashboard UI (~3 days)

**Tasks:**
- Auto-top-up modal: toggle, threshold, top-up amount, monthly cap, card picker, consent.
- Save-card flow with `stripe.confirmSetup` + return URL handling.
- Save-config flow via `PUT /billing/auto_topup_config`.
- SCA resume flow: banner → "Confirm now" button → `POST /billing/auto_topup_resume_pending` → `stripe.handleNextAction` → 3DS modal.
- Status banners per §9.

**Gate to Phase 8:** real user can save a card, enable auto-top-up, run a Lit Action, see balance auto-credited within ~10 seconds. SCA test card flow completes end-to-end.

### Phase 8 — Operational hardening (~2 days)

**Tasks:**
- Email templates (three: action-required, payment-failed, auto-disabled).
- Logging / metrics: trigger count per customer, charge success/failure rate, webhook delivery latency, mutex contention.
- Service-auth secret rotation procedure documented.
- Admin runbook: recovering a stuck PI via existing portal.

**Gate to Phase 9:** failure-counter path tested end-to-end (3 declines → auto-disabled → user can re-enable after updating card).

### Phase 9 — Production rollout (~1–2 days monitored)

**Tasks:**
- Register production Stripe webhook endpoint; copy live signing secret.
- Deploy with all env vars wired.
- Feature-flag the dashboard modal for gradual rollout (internal accounts first).
- Monitor for 24–48 hours: webhook delivery success, charge approval rate, ticket volume.
- Rollback plan: feature flag off, schema can stay (rows ignored).

### Parallelization notes

- Phase 2 (auth extraction) can happen in parallel with Phase 1 once the migration is reviewed.
- Phase 7 (dashboard styling) can start in parallel with Phase 5/6 once API shapes are frozen.
- Phase 8 (email templates, runbook) can start as soon as Phase 6 lands.

Cannot parallelize: Phase 5 before Phase 4 (need config to read), Phase 6 before Phase 5 (need triggers to create PIs to credit).

---

## 17. Local development & testing

Fully testable locally against Stripe test mode. No staging required.

### Prerequisites

- Rust toolchain (per `lit-api-server/rust-toolchain.toml`, currently 1.91).
- Docker (for local Postgres).
- [Stripe CLI](https://docs.stripe.com/stripe-cli) (`brew install stripe/stripe-cli/stripe`).
- A Stripe test-mode account with a restricted key permissioned for: Customers (R/W), PaymentIntents (W), SetupIntents (W), PaymentMethods (R), Customer Balance Transactions (W).
- `sqlx-cli` for manual migration ops: `cargo install sqlx-cli --no-default-features --features postgres`.

### Step 1 — Start local Postgres

```sh
docker run --rm -d --name lit-payments-pg \
  -e POSTGRES_PASSWORD=postgres \
  -p 5432:5432 \
  postgres:16
```

### Step 2 — Configure env vars

Create `lit-payments/.env`:

```sh
DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres
MAGIC_LINK_SIGNING_KEY=$(openssl rand -base64 32)
ROCKET_SECRET_KEY=$(openssl rand -base64 32)
RESEND_API_KEY=re_test_or_real
MAIL_FROM=noreply@mail.litprotocol.com
PUBLIC_BASE_URL=http://localhost:8000
ROCKET_PORT=8000

# Stripe
STRIPE_SECRET_KEY=rk_test_...
STRIPE_WEBHOOK_SECRET=whsec_...   # printed by `stripe listen` in Step 4

# Auto top-up
LIT_API_SERVER_BASE_URL=http://localhost:8002
LIT_INTERNAL_SHARED_SECRET=$(openssl rand -base64 32)

# Auth module (on-chain resolver for API-key auth)
ALCHEMY_HTTPS_URL=https://...
```

Create `lit-api-server/.env` additions:

```sh
LIT_INTERNAL_SHARED_SECRET=<same value as in lit-payments/.env>
```

### Step 3 — Apply migrations

```sh
cd lit-payments && cargo run
# sqlx migrations auto-run on startup
```

Or manually:

```sh
sqlx migrate run --database-url postgres://postgres:postgres@localhost:5432/postgres
sqlx migrate info
psql $DATABASE_URL -c '\d auto_topup_config'
psql $DATABASE_URL -c '\d auto_topup_credits'
```

### Step 4 — Forward Stripe webhooks to localhost

```sh
stripe login
stripe listen \
  --events customer.updated,payment_intent.succeeded,payment_intent.payment_failed \
  --forward-to http://localhost:8000/stripe/webhook
```

CLI prints a webhook signing secret. Copy it to `STRIPE_WEBHOOK_SECRET` and restart lit-payments.

### Step 5 — Start all services

```sh
# Terminal A: lit-payments
cd lit-payments && cargo run

# Terminal B: lit-api-server
cd lit-api-server && cargo run

# Terminal C: dashboard
cd lit-static/dapps/dashboard && python3 -m http.server 8001

# Terminal D: stripe listen (kept running)
```

### Stripe test cards

| Card | Behavior |
|---|---|
| `4242 4242 4242 4242` | Success — off-session charges work |
| `4000 0000 0000 0341` | Off-session decline (`card_declined`) |
| `4000 0027 6000 3184` | 3DS required (`requires_action`) — exercises SCA flow |
| `4000 0000 0000 9995` | `insufficient_funds` decline |
| `4000 0000 0000 0069` | `expired_card` decline |
| `4000 0000 0000 0127` | `incorrect_cvc` decline |

Full list: https://docs.stripe.com/testing

### Acceptance tests per phase

#### After Phase 2 — auth extraction
- [ ] Existing lit-api-server billing endpoints still authenticate with both wallet-sig and API-key.
- [ ] A placeholder lit-payments endpoint accepts the same auth headers and rejects invalid ones with 401.

#### After Phase 3 — saved card
- [ ] `POST /billing/setup_intent` returns `client_secret` for an existing Stripe customer.
- [ ] Returns 400 with clear "first do a manual top-up" message for a wallet with no Stripe customer.
- [ ] Saving `4242…` via dashboard attaches `pm_xxx` to the right `cus_xxx` (verify: `stripe customers retrieve cus_xxx`).

#### After Phase 4 — config CRUD
- [ ] `PUT /billing/auto_topup_config` accepts valid config, returns 200, row appears in Postgres.
- [ ] `PUT` with `enabled=true` and null `threshold_cents` is rejected by CHECK constraint.
- [ ] `PUT` with `pm_xxx` not attached to this customer returns 400.
- [ ] `GET /billing/auto_topup_config` returns what was written.

#### After Phase 5 — customer.updated trigger
- [ ] Manually create a `balance_transaction` via `stripe post /v1/customers/{c}/balance_transactions -d "amount=-X" -d "currency=usd"` so balance drops below threshold → `customer.updated` webhook fires → trigger handler creates a PaymentIntent visible in Stripe Dashboard.
- [ ] Burst test: fire 10 balance_transactions in parallel via Stripe CLI → only one PaymentIntent results (mutex + balance-check short-circuit).
- [ ] Use `4000 0000 0000 0341` saved card → PI is created with `status=requires_payment_method` (declined) → config row's `disabled_reason` set after 3 such failures via the derived-from-list logic.
- [ ] Use `4000 0027 6000 3184` SCA card → `pending_action_pi_id` set, "action required" email sent.

#### After Phase 6 — credit + failure webhooks
- [ ] `stripe trigger payment_intent.succeeded` (or wait for a real PI) → row appears in `auto_topup_credits` → balance transaction created in Stripe → cache invalidation logged in `lit-api-server`.
- [ ] `stripe events resend evt_xxx` to replay the same event → handler returns 200 immediately without a second credit (verify `auto_topup_credits` row count unchanged, no new balance transaction in Stripe).
- [ ] `stripe trigger payment_intent.payment_failed` → email dispatched, banner appears.
- [ ] Tamper with `Stripe-Signature` header → returns 401.
- [ ] After a credit, immediate balance fetch via lit-api-server → returns updated value (cache invalidated).

#### After Phase 7 — dashboard UI
- [ ] User opens dashboard, sees "no card on file" state.
- [ ] Saves a `4242…` card → modal updates to show card on file.
- [ ] Configures threshold/amount/cap, toggles enabled, saves → status banner shows the rule.
- [ ] Runs a Lit Action that drops balance below threshold → within ~10 seconds, balance is credited and dashboard reflects new balance.
- [ ] Uses 3DS card → action-required banner appears → "Confirm now" → 3DS modal → completes → balance credited → banner clears.
- [ ] 3DS abandoned → email arrives, banner eventually clears after failure webhook.

#### End-to-end smoke before Phase 9 rollout
- [ ] Full happy path: save card → enable → run actions → auto-charge → credited.
- [ ] Webhook replay safety: resend a `payment_intent.succeeded` event 24+ hours later → no double credit.
- [ ] Cap reached: set cap=$5, top-up=$5, drive two top-ups in a row → second correctly skipped.
- [ ] Cache invalidation under burst: 5 parallel deductions immediately after a top-up → all see updated balance (none rejected for insufficient credit).

### Staging / preview

No dedicated staging today. Two options if local isn't enough:
1. Enable Railway preview environments for the lit-payments project (per-branch deploys).
2. Deploy lit-payments to a personal Railway service for webhook testing against a publicly-reachable URL (vs. `stripe listen` which is local-only).

For lit-api-server (TEE), staging is outside this repo's scope.

### Quick-reference commands

```sh
# Reset local Postgres
docker stop lit-payments-pg && docker rm lit-payments-pg
docker run --rm -d --name lit-payments-pg -e POSTGRES_PASSWORD=postgres -p 5432:5432 postgres:16

# Inspect tables
psql $DATABASE_URL -c 'SELECT * FROM auto_topup_config;'
psql $DATABASE_URL -c 'SELECT * FROM auto_topup_credits ORDER BY credited_at DESC LIMIT 10;'

# Fire a balance_transaction (triggers customer.updated)
stripe post /v1/customers/cus_xxx/balance_transactions \
  -d "amount=-100" -d "currency=usd" -d "description=manual test"

# Trigger synthetic webhooks
stripe trigger payment_intent.succeeded
stripe trigger payment_intent.payment_failed

# Replay specific event (idempotency test)
stripe events resend evt_xxx

# Tail Stripe CLI in JSON
stripe listen --events customer.updated,payment_intent.succeeded,payment_intent.payment_failed \
  --forward-to http://localhost:8000/stripe/webhook --format json
```

---

## 18. What is explicitly NOT in this plan

- Hard monthly cap with reservation ledger.
- Reconciler cron for orphaned Stripe PIs without a credit row. Manual recovery via admin portal handles the rare 3-day-webhook-failure case.
- Currency support beyond USD.
- Daily / rolling restrictions for public-tier API keys (future feature).
- Refund-aware cap accounting.
- Multi-card support per customer.

---

## 19. Open questions for product

- Consent text wording: "approximately $X/month" or "up to $X/month" or both?
- Email service confirmation (we plan to use existing Resend integration).
- Failure-counter threshold for auto-disable: 3 (current plan).
- 10-minute recent-PI short-circuit window.
- Minimum top-up amount: $5 (matches existing one-shot floor).

---

## 20. Verification

This plan reflects four review passes:

1. **Initial design review** (Codex consult, fresh session) — caught the load-bearing list-endpoint staleness issue and confirmed Postgres dedup table is necessary.
2. **Simplified design review** (Codex consult, resumed session) — identified service-auth gaps, `pending_action` state machine, and SCA recovery flow.
3. **Fresh-session sanity check on the locked design** — verified no missing pieces, flagged the `customer.updated` webhook as undocumented behavior worth empirically testing.
4. **Empirical Stripe test (June 2026)** — fired 21 `balance_transactions` against a test customer (slow loop + parallel burst); received exactly 21 `customer.updated` events, no coalescing, balance correct in every payload, `previous_attributes.balance` present for delta detection. This validated the webhook-driven trigger.

The alternative architecture (Stripe Billing Meters + Credit Grants for native auto-top-up) was explored and rejected: Stripe's own [Billing Credits implementation guide](https://docs.stripe.com/billing/subscriptions/usage-based/billing-credits/implementation-guide) explicitly states merchants must create the funding invoice themselves, listen for `invoice.paid`, and call the Credit Grants API. Same three steps, more complex billing platform. Migration off `customer.balance` would touch admin portal, dashboard, lit-api-server, lit-payments, and existing customer data — out of scope.

---

## 21. Handoff checklist for the implementing agent

If you're picking up this doc cold, do these in order:

- [ ] Read §1–4 (goal, framing, architecture, schema).
- [ ] Read §5–9 (endpoints, two webhook handler flows, lit-api-server change, dashboard flows).
- [ ] Read §10 (three-layer concurrency model).
- [ ] Read §11–12 (edge cases handled vs accepted trade-offs).
- [ ] Read §13–14 (env vars + service-auth).
- [ ] Read §17 (local development & testing) and stand up the local stack before writing any code.
- [ ] Follow §16 phases in order. Each phase has a gate; do not skip ahead.
- [ ] Use the per-phase acceptance tests at the end of §17 as your gate criteria.
- [ ] Ask product before starting on §19 open questions.

Key architectural facts you must internalize before coding:

- **`customer.updated` is the trigger** (empirically validated 1:1 firing in §20). Filter on `previous_attributes.balance`.
- **`payment_intent.succeeded` is the credit path** — never credit synchronously in the trigger handler.
- **lit-api-server is unchanged** except for one tiny cache-invalidation endpoint.
- **Dashboard talks to lit-payments directly** for auto-top-up endpoints (auth via the extracted shared module).
- **Soft cap, bias toward more top-ups never fewer** — the design accepts rare double-charges; it does not accept missing top-ups.
- **Webhook handlers must return 2xx only after all work is committed** — otherwise Stripe won't retry and credit is lost.

This document is intended to be self-contained. If something is ambiguous, raise it — do not guess.
