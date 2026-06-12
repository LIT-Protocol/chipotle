# Auto Top-Up — Final Implementation Plan

**Status:** Locked. Ready to implement.
**Scope:** Stripe-paying users only. LITKEY / crypto users are out of scope.
**Architecture:** Sync-credit, single-webhook-trigger. US cards only.

---

## 1. Goal

After any Lit Action or management deduction drops a user's Stripe customer balance below a configured threshold, automatically charge their saved card off-session for a configured top-up amount, subject to a configured monthly cap. UI matches Claude Console and OpenAI Platform's auto-recharge modals. Auto top-up is opt-in.

---

## 2. Product framing

### Scope constraints (decided)
- **All card payment methods, including EU / SCA-required cards.**
- **Card payment method only.** No ACH, bank debits, wallets.
- **Soft cap.** UI says "approximately $X/month." Cap can be exceeded by ~1 top-up amount in rare races; documented trade-off.
- **Bias toward more top-ups, never fewer.** A missed top-up means a Lit Action fails mid-flight. An extra charge means an over-credited account that Sally can refund. The design favors over-charging.

### Charge and credit happen synchronously

When the trigger evaluates and decides to charge, `paymentIntents.create` with `off_session=true, confirm=true` returns the final status synchronously for US cards (and most EU cards exempted via MIT prior-auth). On `succeeded`, the handler immediately calls `balance_transactions` to credit the user's wallet. **No `payment_intent.succeeded` webhook is used.** The only webhook is `customer.updated`, which is the trigger.

### SCA recovery (for cards that require authentication despite the MIT exemption)

When Stripe returns `authentication_required` on an off-session charge, we save the pending PI id, send the user a tokenized email link, and bring them back on-session. The dashboard recovery page calls `stripe.confirmCardPayment(client_secret)`; Stripe.js renders the bank's 3DS challenge inside its iframe; on success, `confirmCardPayment` returns synchronously and we credit through the normal sync path. **No webhook needed for this either.**

### What we are not building
- Hard monthly cap with reservation ledger.
- `payment_intent.succeeded` / `payment_intent.payment_failed` webhooks.
- Currency support beyond USD.
- Refund-aware cap accounting.
- Daily / rolling restrictions on public-tier API keys.
- Multi-card support per customer.

---

## 3. Architecture overview

### Trigger source: `customer.updated` webhook

`lit-api-server` deducts credits via `POST /v1/customers/{id}/balance_transactions` (existing behavior, unchanged). Stripe fires `customer.updated` containing the new and previous balance. `lit-payments` listens, filters for actual balance changes, and runs the trigger handler.

**Empirically validated** (June 2026): 21 balance_transactions → 21 `customer.updated` events, 1:1 with no coalescing, balance correct in every payload.

### Components

| Component | Path | Role |
|---|---|---|
| **Dashboard** | `lit-static/dapps/dashboard/` | Adds: auto-top-up modal, save-card flow, status banners. Talks directly to `lit-payments`. |
| **lit-api-server** (TEE) | `lit-api-server/` | **No trigger code, no auto-top-up logic.** Adds one tiny endpoint: `POST /internal/invalidate_balance_cache`. Auth module `billing_auth.rs` extracted to a shared crate for reuse by `lit-payments`. |
| **lit-payments** (Railway) | `lit-payments/` | All auto-top-up logic. Dashboard endpoints, the `customer.updated` webhook handler (which now does the full charge-and-credit synchronously), the reconciliation cron. |
| **Postgres** (inside `lit-payments`) | | Two new tables: `auto_topup_config`, `auto_topup_credits`. |
| **Shared auth crate** | new, e.g. `lit-billing-auth/` | Extracted from `lit-api-server::core::v1::guards::billing_auth`. Accepts wallet signature (EIP-712) or API key. Used by both services. |
| **Stripe** | external | Customer, PaymentMethod, PaymentIntents, balance transactions, one webhook event (`customer.updated`). |

### Storage map

| Data | Where | Why |
|---|---|---|
| Config (5 fields: `enabled`, `threshold_cents`, `topup_amount_cents`, `monthly_cap_cents`, `payment_method_id`) | Postgres `auto_topup_config` | Fast reads, atomic writes, schema evolution, admin queryability |
| Consent (`consent_version`, `consent_signed_at`) | Postgres `auto_topup_config` (same row) | Off-session merchant-initiated charges require recorded consent |
| Card data | Stripe (PaymentMethod attached to Customer) | Never on our servers; PCI scope stays with Stripe |
| Wallet ↔ Stripe Customer mapping | Stripe customer metadata (`metadata.wallet_address`) | Existing pattern, unchanged |
| Charge history | Stripe PaymentIntents (filtered by `metadata.source=auto_topup`) | Source of truth |
| Monthly spend total | Computed on demand by listing PIs | No counter to race on |
| Failure state | Derived by listing recent PIs and counting failures | No counter to race on |
| Credit dedup (1 row per credited PI) | Postgres `auto_topup_credits` | Idempotency for the synchronous credit step; reconciliation can detect missing rows for timed-out attempts |

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
  pending_action_pi_id     TEXT,         -- set when off-session PI returned authentication_required; cleared on terminal status
  pending_action_at        TIMESTAMPTZ,
  recovery_token           TEXT,         -- one-time token for tokenized email recovery link
  recovery_token_expires_at TIMESTAMPTZ,
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
CREATE INDEX ON auto_topup_credits (stripe_balance_transaction_id) WHERE stripe_balance_transaction_id IS NULL;
CREATE INDEX ON auto_topup_config (recovery_token) WHERE recovery_token IS NOT NULL;
```

The partial index on `stripe_balance_transaction_id IS NULL` accelerates the reconciliation query (find rows where the credit was started but didn't finish).

---

## 5. Endpoints

### Dashboard-facing (on `lit-payments`, behind the shared auth module)

| Method + Path | Purpose | Behavior |
|---|---|---|
| `POST /billing/setup_intent` | Save card | Calls `lit_billing_core::customer::find_by_wallet`. Refuses if no Stripe customer (user must do first manual top-up to bootstrap). Creates Stripe SetupIntent (`usage=off_session, customer=cus_xxx`). Returns `client_secret` + publishable key. The dashboard's `stripe.confirmCardSetup` will trigger 3DS at save time for SCA cards — this creates the MIT prior-auth record that lets most future off-session charges skip 3DS. |
| `GET /billing/auto_topup_config` | Read config | Read row from `auto_topup_config`. Return JSON including `pending_action_pi_id` so dashboard knows whether to show the SCA banner. |
| `PUT /billing/auto_topup_config` | Save config | Validate: verify `payment_method_id` belongs to this customer; enforce `cap >= topup_amount`, positive cents, min top-up $5. UPSERT row. |
| `GET /billing/auto_topup_resume?token=...` | SCA recovery landing | Resolves the one-time recovery token from email to a `customer_id`, returns the `client_secret` of the `pending_action_pi_id`. Dashboard uses this client_secret with `stripe.confirmCardPayment` to complete 3DS. Token expires after 24h. |
| `POST /billing/auto_topup_resume/complete` | Apply credit after SCA succeeded | Called by dashboard after `stripe.confirmCardPayment` returns `succeeded`. Body: `{payment_intent_id}`. Looks up the PI in Stripe to verify status, then runs the sync-credit path: INSERT row, balance_transactions, UPDATE row, clear pending state, invalidate cache. |

### Internal (on `lit-api-server`, auth: `X-Internal-Secret`)

| Method + Path | Purpose |
|---|---|
| `POST /internal/invalidate_balance_cache` | Called by `lit-payments` after a successful sync credit. Body: `{customer_id}`. Calls existing `state.balance_cache.invalidate(...)`. |

### Webhook (on `lit-payments`, auth: Stripe-Signature HMAC)

| Method + Path | Purpose |
|---|---|
| `POST /stripe/webhook` | Single event type: `customer.updated`. Used as the auto-top-up trigger. Filtered by `previous_attributes.balance` presence. |

---

## 6. Trigger + sync-credit flow (`customer.updated` handler)

This is now the only place auto-top-up logic runs. The handler does the full evaluate-charge-credit pipeline synchronously.

### Step-by-step

1. **Verify Stripe-Signature** (raw body, HMAC-SHA256, 5-min tolerance, constant-time compare). Invalid → 401.

2. **Parse event. Filter:**
   - `event.type != "customer.updated"` → ignore, return 200.
   - `previous_attributes.balance` not present → ignore (balance didn't actually change). Return 200.

3. **Quick exit on payload data:**
   - Read `event.data.object.balance` (new balance) and `event.data.object.id` (customer_id).
   - Read config from Postgres `auto_topup_config` by customer_id.
   - If `!enabled` → return 200.
   - If `available_credit (= -new_balance) >= threshold_cents` → return 200 (not below threshold).

4. **Acquire per-customer mutex.**
   `moka::sync::Cache<String, Arc<tokio::sync::Mutex<()>>>` with 5-minute TTL keyed by `customer_id`. Serializes parallel events.

5. **Fresh balance fetch** from Stripe via `lit_billing_core::balance::fetch(stripe_client, &customer_id)`. Don't trust the webhook payload after the mutex wait. If `available_credit >= threshold_cents` → release mutex, return 200.

6. **List PIs this month** for the customer: `GET /v1/payment_intents?customer={cus_xxx}&created[gte]={month_start_utc}&limit=100`. Paginate via `starting_after` until `has_more=false`. Client-side filter on `metadata.source == "auto_topup"`.

7. **Derive failure state.** Walk list from most recent backwards, count consecutive PIs in failed states (`status=requires_payment_method` or with `last_payment_error.code` in `card_declined`, `expired_card`, `insufficient_funds`, `incorrect_cvc`, `processing_error`). If `consecutive_failures >= 3`:
   - `UPDATE auto_topup_config SET enabled=false, disabled_reason='failures', updated_at=now() WHERE customer_id=...`
   - Send "card needs updating" email via Resend.
   - Release mutex, return 200.

8. **Cap check.** Sum amounts of all non-failed PIs this month. If `sum + topup_amount_cents > monthly_cap_cents` → release mutex, return 200.

9. **Create off-session PaymentIntent (synchronous):**
    ```
    POST /v1/payment_intents
      customer: cus_xxx
      payment_method: pm_xxx
      amount: topup_amount_cents
      currency: usd
      off_session: true
      confirm: true
      metadata[source]: auto_topup
      metadata[wallet_address]: 0x...
    ```
    Handle the synchronous response:
    - **`status == "succeeded"`** → proceed to step 10.
    - **Error code `authentication_required`** (SCA needed): extract `pi_id` from `error.payment_intent.id`. Generate a one-time `recovery_token` (random 32 bytes, base64url). `UPDATE auto_topup_config SET pending_action_pi_id=$1, pending_action_at=now(), disabled_reason='requires_action', recovery_token=$2, recovery_token_expires_at=now()+'24h'`. Email the user with the recovery link `https://dashboard/recover_topup?token={recovery_token}`. Release mutex, return 200. The PI remains alive at Stripe waiting for on-session 3DS.
    - **Other error (declined, expired, insufficient_funds, etc.)** → send "card declined" email with reason + dashboard banner. Don't credit. Release mutex, return 200. (Consecutive-failure derivation in step 7 will eventually auto-disable.)
    - **HTTP timeout** → log the attempt. Don't credit. Release mutex, return 200. **The reconciliation cron (§9) will detect the orphaned `succeeded` PI on its next run and credit it.**

10. **Credit synchronously (the new sync-credit path):**
    ```sql
    INSERT INTO auto_topup_credits (payment_intent_id, customer_id, amount_cents)
      VALUES ($1, $2, $3)
      ON CONFLICT (payment_intent_id) DO NOTHING
      RETURNING payment_intent_id;
    ```
    - If no row returned (already credited — shouldn't happen here, but defensive) → release mutex, return 200.
    - If row inserted, proceed.

11. **Write the balance transaction (the credit):**
    ```
    lit_billing_core::balance::write_transaction(
      stripe_client,
      customer_id,
      -pi.amount,
      description: "Auto top-up via {pi.id}",
      idempotency_key: "credit:{pi.id}"
    )
    ```
    Idempotency-Key makes this safely retryable. If this call fails (network/timeout), the row in `auto_topup_credits` already exists with `stripe_balance_transaction_id IS NULL`, and the reconciler will retry.

12. **Update the row:**
    ```sql
    UPDATE auto_topup_credits
      SET stripe_balance_transaction_id = $1
      WHERE payment_intent_id = $2;
    ```

13. **Invalidate `lit-api-server`'s balance cache:**
    Fire-and-forget `POST {LIT_API_SERVER_BASE_URL}/internal/invalidate_balance_cache` with `X-Internal-Secret` and `{customer_id}`. Ignore errors (cache will refresh in ≤10 min regardless).

14. **Release mutex. Return 200.**

### Additional clear-pending step on succeeded credit

If the PI we just credited was `pending_action_pi_id` for this customer (i.e. SCA recovery completed via the recovery page):
```sql
UPDATE auto_topup_config
  SET pending_action_pi_id = NULL,
      pending_action_at = NULL,
      disabled_reason = NULL,
      recovery_token = NULL,
      recovery_token_expires_at = NULL
  WHERE customer_id = $1 AND pending_action_pi_id = $2;
```

---

## 6a. SCA recovery flow (user on-session)

Triggered when the user clicks the recovery link in the "Your card requires authentication" email.

1. Email link: `https://dashboard/recover_topup?token={recovery_token}`.
2. Dashboard recovery page calls `GET /billing/auto_topup_resume?token={recovery_token}` on `lit-payments`.
3. lit-payments verifies the token (matches `recovery_token` in `auto_topup_config`, not expired), retrieves the `pending_action_pi_id`, fetches the PI from Stripe to get its `client_secret`, returns `{ client_secret, payment_intent_id }`. Token is single-use — invalidate after this read.
4. Dashboard JS runs `stripe.confirmCardPayment(client_secret)`. Stripe.js renders the bank's 3DS challenge inside its iframe.
5. User authenticates with the bank.
6. `confirmCardPayment` returns synchronously:
   - `result.paymentIntent.status === 'succeeded'` → call a new lit-payments endpoint (or reuse the trigger flow at step 10) to apply the credit using the sync path. **No webhook needed.**
   - `result.error` → show "authentication failed, try a different card." Email/banner persist; user can retry.

7. On successful credit, the trigger handler's additional clear-pending step (above) clears `pending_action_pi_id` and re-enables auto-top-up.

The 3DS UI is rendered by Stripe + the bank inside Stripe's iframe. We never build a PIN entry form (that would be a PCI violation). Our only frontend code is the `confirmCardPayment` call and the success/failure branches.

---

## 7. Webhook signature verification

Applies to the one webhook endpoint (`POST /stripe/webhook`).

1. **Read raw body** with Rocket `Data` handler (NOT JSON extractor — HMAC must verify exact bytes). Size limit ~1 MB.
2. **Parse `Stripe-Signature` header:** `t={timestamp},v1={hex_signature}`.
3. **Reject if `|now - timestamp| > 300s`** (5-minute tolerance).
4. **Compute `HMAC-SHA256(STRIPE_WEBHOOK_SECRET, "{timestamp}.{raw_body}")`**, constant-time compare against `v1` (`subtle::ConstantTimeEq`). Invalid → 401.
5. **Return 5xx on transient errors** (DB unavailable, Stripe API down) so Stripe retries (up to 3 days).
6. **Return 200 ONLY after** all credit work is committed. Returning early and crashing means lost credit.

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

Auth via `X-Internal-Secret` (constant-time compare).

### Auth extraction (parallel refactor)

Move `lit-api-server/src/core/v1/guards/billing_auth.rs` into a shared crate (`lit-billing-auth/`) so `lit-payments` can use the identical wallet-sig + API-key auth flow. Includes the on-chain `allApiKeyHashesToMaster` resolver and wallet → Stripe customer mapping. `lit-payments` gains a dependency on the new crate and the on-chain RPC config (`ALCHEMY_HTTPS_URL` already present for LITKEY).

---

## 9. Reconciliation cron

**This is now load-bearing**, not optional. The sync-credit design intentionally accepts the timeout failure mode; the reconciler is what recovers from it.

### Schedule

Every 15 minutes (configurable). Runs in `lit-payments` as a `tokio` task spawned at startup.

### Logic

```sql
-- 1. Get all enabled auto-top-up customers
SELECT customer_id FROM auto_topup_config WHERE enabled = true;
```

For each customer:

```
-- 2. Fetch recent auto-top-up PIs from Stripe
GET /v1/payment_intents
  ?customer={cus_xxx}
  &created[gte]={now - 7 days}
  &limit=100
filter to metadata.source == "auto_topup" AND status == "succeeded"
```

```sql
-- 3. For each succeeded PI: check whether it was credited
SELECT payment_intent_id, stripe_balance_transaction_id
  FROM auto_topup_credits
  WHERE payment_intent_id = $1;
```

Three cases:

| Case | Action |
|---|---|
| Row exists, `stripe_balance_transaction_id` populated | Credited correctly — skip |
| Row exists, `stripe_balance_transaction_id` NULL | Credit was started but never finished. Retry step 12 of §6 (with the same `credit:{pi.id}` idempotency key — Stripe dedupes if it actually succeeded). Then UPDATE row. |
| No row at all | The trigger never finished crediting (HTTP timeout on PI create, or process crash). Run the full credit: INSERT row, call balance_transactions, UPDATE row. Alert Sally via log/metric so she can sanity-check. |

### Why 7 days lookback

Catches PIs from up to a week ago in case the reconciler itself was broken or paused. Cheap to scan because we filter by customer and use Stripe's `created[gte]`.

### Why 15 minutes, not 24 hours

Average user-visible recovery time: ~7.5 minutes after a timeout. Faster reduces support tickets. Cheap to run — listing PIs for active customers is bounded.

---

## 10. Dashboard changes

### Save-card flow (creates MIT prior-auth)

1. User clicks "Add a card for auto top-up."
2. Dashboard calls `POST /billing/setup_intent` with existing auth headers (wallet sig or API key).
3. Backend returns `{ client_secret, publishable_key }`.
4. Dashboard initializes Stripe.js, mounts Payment Element in **setup mode**.
5. User enters card. Dashboard calls `stripe.confirmCardSetup(client_secret)`.
   - **For SCA cards (EU/etc.): Stripe.js automatically renders the bank's 3DS challenge** inside its iframe. The user authenticates. This is the **MIT prior-auth event** — most future off-session charges on this card can skip 3DS as a result.
   - For non-SCA cards: completes silently.
6. On success, extract `payment_method` id (`pm_xxx`) from the resulting SetupIntent.

### Save-config flow

1. Modal collects: enable toggle, threshold (USD), top-up amount (USD), monthly cap (USD), card picker, consent checkbox.
2. Dashboard calls `PUT /billing/auto_topup_config`.

### SCA recovery page (`/recover_topup?token=...`)

A dedicated dashboard page reached via the email link.

1. Read `token` from query string.
2. Call `GET /billing/auto_topup_resume?token={token}`. Receive `{ client_secret, payment_intent_id }`.
3. Initialize Stripe.js. Call `stripe.confirmCardPayment(client_secret)`. Bank's 3DS UI renders in Stripe's iframe.
4. User authenticates.
5. If `result.paymentIntent.status === 'succeeded'` → show "topped up!" and redirect to billing page.
6. If `result.error` → show "authentication failed, try a different card" with link to update payment method.

Frontend logic for the entire SCA recovery is roughly this:
```js
const stripe = Stripe(publishable_key);
const { client_secret } = await fetch(`/billing/auto_topup_resume?token=${token}`).then(r => r.json());
const result = await stripe.confirmCardPayment(client_secret);
if (result.error) {
  showError(result.error.message);
} else {
  // status === 'succeeded' — backend credit happens automatically via the trigger sync path
  showSuccess();
}
```

### Status banners

| State | Message |
|---|---|
| Enabled, healthy | "Auto top-up: when your balance drops below $X, we'll charge $Y to card ending in ****1234, up to ~$Z/month." |
| `pending_action_pi_id` set (SCA pending) | "Action required to complete your $X auto top-up. Check your email or [Confirm now]." Clicking "Confirm now" takes user to the recovery page using the active token. |
| Disabled by user | "Auto top-up is off. [Enable]" |
| Auto-disabled after failures | "Auto top-up was paused after 3 failed attempts. Please update your card. [Manage]" |

### Email notifications (Resend)

| Trigger | Subject | Body |
|---|---|---|
| `paymentIntents.create` returns declined / expired / etc. | "Your auto top-up couldn't be charged" | Amount, card last4, decline reason, link to update card |
| `paymentIntents.create` returns `authentication_required` (SCA) | "Action required: verify your auto top-up" | Brief explanation, **tokenized recovery link** to `/recover_topup?token=...` |
| Auto-disable after 3 consecutive failures | "Auto top-up paused — update your card" | Reason, link to update card and re-enable |

---

## 11. Concurrency model — three layers

| Layer | Prevents | Mechanism | Scope |
|---|---|---|---|
| 1. Per-customer Tokio mutex (`moka` TTL cache) | Parallel `customer.updated` events for the same customer doing duplicate evaluations | In-memory in `lit-payments` | Per-process |
| 2. Postgres unique constraint on `auto_topup_credits.payment_intent_id` | Double-credit of the same PI (sync handler + reconciler both trying) | `INSERT … ON CONFLICT DO NOTHING` | Permanent |
| 3. Stripe Idempotency-Key on `balance_transactions` write | Double-credit from retry under transient Stripe / network failure | `Idempotency-Key: credit:{pi.id}` | Stripe-global, 24h |

The mutex is optimization. Layers 2 and 3 are correctness primitives.

---

## 12. Edge cases — handled

| Case | Handling |
|---|---|
| 5+ parallel `customer.updated` events for same customer | Mutex serializes; the early balance check (step 3) short-circuits later events whose balance is already above threshold after the first top-up |
| `paymentIntents.create` HTTP timeout | Reconciler (§9) catches the orphaned succeeded PI and credits within ~15 min |
| `balance_transactions` write fails after PI succeeded | Reconciler catches the `stripe_balance_transaction_id IS NULL` row and retries with the same idempotency key |
| Webhook delivered twice / replayed | Step 3 short-circuit (`enabled` / balance check), and downstream `INSERT ... ON CONFLICT` dedup makes the credit safe |
| Webhook handler crashes mid-execution | Stripe retries with backoff (up to 3 days) since we returned 5xx or timed out |
| Card declined | Synchronous response handling in step 9 sends email + dashboard banner; consecutive failure derivation in step 7 disables after 3 |
| `authentication_required` (SCA) | Save `pending_action_pi_id` + recovery token, send tokenized email link, show dashboard banner. User clicks → recovery page calls `stripe.confirmCardPayment(client_secret)` → 3DS challenge → on success, credit applies via the normal sync path. No webhook needed. |
| User toggles off between webhook fire and handler execution | Step 3 reads `enabled=false` from Postgres and short-circuits |
| Trigger fires when balance is already above threshold | Step 3 / step 5 short-circuit |
| User legitimately needs two top-ups in quick succession | Each `customer.updated` event re-evaluates from scratch; no artificial cooldown blocks them |
| `customer.updated` for non-balance changes (email/address/metadata) | Filter on `previous_attributes.balance` — ignore if not present |
| Stripe customer balance cached stale in `lit-api-server` after credit | Step 13 invalidates the cache |
| `customer.updated` event lost | Self-healing — next deduction fires another `balance_transactions` write → another `customer.updated` |
| Pagination on PI list at scale | Use `starting_after` until `has_more=false` |

---

## 13. Edge cases — accepted trade-offs

| Case | What happens | Recovery |
|---|---|---|
| List-endpoint staleness during cap check | `paymentIntents.list` isn't strongly read-after-write consistent. <1% rare race; can result in 1 extra top-up. | Soft cap. Manual refund via admin portal. |
| Rapid back-to-back triggers right after a successful top-up | If `customer.balance` reflection lags briefly behind the credit (rare, observed sub-second in test mode), a second trigger could fire and charge again. Aligns with "bias toward more top-ups" — extra credit, no lost money. | Manual refund via admin portal if user complains. |
| HTTP timeout on `paymentIntents.create` | Charge happened at Stripe, no credit applied immediately. | Reconciler (§9) credits within ~15 min. |
| Reconciler doesn't run for hours (cron failure) | Some users see delayed credit. | Add a heartbeat / alert on the cron. |
| Webhook delivery fails for the full 3-day Stripe retry window | Top-up never fires for that event. | Self-heals on next deduction → next `customer.updated`. |
| User abandons SCA recovery flow | Pending state lingers until user revisits or recovery token expires (24h). Auto-top-up is effectively paused for them. | User can re-trigger by clicking the email link again before expiry, or get a fresh attempt on the next deduction (which fires a fresh `customer.updated`). |
| Refunds / disputes | Don't restore monthly cap. | Documented. |
| Currency other than USD | Not supported. | Out of scope. |

---

## 14. New environment variables

### `lit-api-server`
- `LIT_INTERNAL_SHARED_SECRET` — high-entropy random; auth for cache-invalidation endpoint.

### `lit-payments`
- `LIT_API_SERVER_BASE_URL` — for the cache-invalidation callback.
- `LIT_INTERNAL_SHARED_SECRET` — same value as on lit-api-server.
- `STRIPE_WEBHOOK_SECRET` — from Stripe Dashboard after registering the `customer.updated` endpoint.
- `RECONCILER_INTERVAL_SECS` (optional, default 900) — reconciler frequency.
- `ALCHEMY_HTTPS_URL` — for the on-chain master-key resolver inherited from extracted auth module.

### Dashboard
- `STRIPE_PUBLISHABLE_KEY` — if not already present.

---

## 15. Service-auth requirements for `X-Internal-Secret`

Used only for the cache-invalidation call from `lit-payments` to `lit-api-server`.

- Generated with at least 256 bits of entropy (`openssl rand -base64 32`).
- Stored in env vars only.
- TLS only.
- Constant-time comparison in the handler (`subtle::ConstantTimeEq`).
- Never logged or echoed in error responses.
- Rotation: deploy both services with both old and new secrets accepted, swap, drop old.

---

## 16. Sequence diagrams

### Setup (one-time, including SCA prior-auth for EU cards)

```
USER → DASHBOARD ─► lit-payments POST /billing/setup_intent ─► Stripe (SetupIntent usage=off_session)
                                                                │
USER → DASHBOARD ◄────────── client_secret ─────────────────────┘
USER → DASHBOARD ─► Stripe.js stripe.confirmCardSetup(client_secret)
                       │
                       ▼ (if EU/SCA card)
                   Bank's 3DS challenge in Stripe.js iframe ──► USER authenticates
                       │
                       ▼
                   PaymentMethod attached to Customer (pm_xxx) + MIT prior-auth record
USER → DASHBOARD ─► lit-payments PUT /billing/auto_topup_config ─► Postgres (UPSERT)
```

### Runtime — every Lit Action deduction

```
CLIENT ─► lit-api-server (run Lit Action, deduct credits via balance_transactions)
                                                          │
                                                          ▼
                                                      Stripe POST /v1/customers/{id}/balance_transactions
                                                          │ customer.balance changes
                                                          ▼
                                                      Stripe fires customer.updated webhook
                                                          │
                                                          ▼
              lit-payments POST /stripe/webhook
                  │
                  ├─► verify Stripe-Signature HMAC
                  ├─► filter: previous_attributes.balance present? YES, continue
                  ├─► quick exit: balance >= threshold? then return 200
                  ├─► acquire mutex[customer]
                  ├─► fresh balance::fetch from Stripe (don't trust payload)
                  ├─► list PIs this month, paginate, filter metadata.source=auto_topup
                  ├─► derive failure state → disable if 3+ consecutive
                  ├─► cap check (sum + amount > cap → skip)
                  │
                  ├─► Stripe POST /v1/payment_intents (off_session, confirm)
                  │   sync response:
                  │     ├─ succeeded → continue
                  │     ├─ authentication_required → save pending_action_pi_id + recovery token,
                  │     │     email tokenized link, return 200 (SCA recovery flow below)
                  │     ├─ declined/error → email + return 200
                  │     └─ HTTP timeout → log + return 200 (reconciler handles)
                  │
                  ├─► Postgres INSERT auto_topup_credits ON CONFLICT DO NOTHING
                  ├─► Stripe POST /v1/customers/{c}/balance_transactions
                  │     Idempotency-Key: credit:{pi.id}
                  ├─► Postgres UPDATE auto_topup_credits SET stripe_balance_transaction_id
                  ├─► lit-api-server POST /internal/invalidate_balance_cache (fire-and-forget)
                  └─► release mutex, return 200
```

### SCA recovery (user-driven, after email)

```
User receives email → clicks recovery link → DASHBOARD /recover_topup?token=...
       │
       ▼
DASHBOARD GET lit-payments /billing/auto_topup_resume?token=...
       │
       ▼ lit-payments: verify token, load pending_action_pi_id, fetch PI from Stripe
       │ Returns { payment_intent_id, client_secret }
       ▼
DASHBOARD JS: stripe.confirmCardPayment(client_secret)
       │
       ▼ (Stripe.js renders bank's 3DS challenge in iframe)
       │
       ▼ USER authenticates
       │
       ▼ confirmCardPayment returns synchronously
       │
       ├─ result.paymentIntent.status === 'succeeded'
       │     │
       │     ▼ DASHBOARD POST lit-payments /billing/auto_topup_resume/complete
       │     │     body: { payment_intent_id }
       │     │
       │     ▼ lit-payments runs sync-credit path (§6 step 10-13):
       │     │     - INSERT auto_topup_credits ON CONFLICT
       │     │     - Stripe balance_transactions (Idempotency-Key: credit:{pi.id})
       │     │     - UPDATE row
       │     │     - clear pending_action_pi_id, recovery_token, disabled_reason
       │     │     - invalidate lit-api-server cache
       │     │
       │     ▼ DASHBOARD shows "topped up!"
       │
       └─ result.error → DASHBOARD shows "authentication failed, try another card"
```

### Reconciliation (every 15 minutes)

```
lit-payments cron tick
       │
       ▼
Postgres SELECT customer_id FROM auto_topup_config WHERE enabled
       │
       ▼ for each customer:
Stripe GET /v1/payment_intents?customer=...&created[gte]=now-7d
       filter to metadata.source=auto_topup AND status=succeeded
       │
       ▼ for each succeeded PI:
Postgres SELECT FROM auto_topup_credits WHERE payment_intent_id = pi.id
       │
       ├─ row + balance_transaction_id populated → skip
       ├─ row but balance_transaction_id NULL → retry credit (idempotent), UPDATE row
       └─ no row → run full credit (INSERT row, balance_transactions, UPDATE row), alert Sally
```

---

## 17. Implementation phases

Phases are strictly sequential at the gate level. Total estimated effort: **~2 weeks of focused work**.

### Dependency chain

```
Phase 1: Foundation
  └─► Phase 2: Auth extraction
        └─► Phase 3: Saved card flow (SetupIntent)
              └─► Phase 4: Config CRUD
                    └─► Phase 5: Webhook handler (trigger + sync charge + sync credit)
                          └─► Phase 6: Reconciliation cron
                                └─► Phase 7: Dashboard UI
                                      └─► Phase 8: Operational hardening
                                            └─► Phase 9: Production rollout
```

### Phase 1 — Foundation (~0.5 day)
- Migration with both tables + CHECK constraints + partial index.
- New env vars on both sides.
- `X-Internal-Secret` Rocket guard in `lit-api-server`.
- `POST /internal/invalidate_balance_cache` endpoint on `lit-api-server`.

**Gate:** migration applies; cache-invalidation endpoint returns 200 with correct secret and 401 without.

### Phase 2 — Auth extraction (~2 days)
- New `lit-billing-auth/` crate.
- Move EIP-712 / ChainSecured verifier, API-key → master-wallet resolver, wallet → Stripe customer mapping, caches.
- Service-agnostic Rocket request guard.
- Update both services to depend on the new crate.

**Gate:** existing `lit-api-server` billing endpoints still authenticate identically; a smoke test on a placeholder lit-payments endpoint accepts both wallet-sig and API-key.

### Phase 3 — Saved card flow with SCA prior-auth (~2 days)
- `POST /billing/setup_intent` on lit-payments behind the new shared auth.
- SetupIntent with `usage='off_session'`. Returns `client_secret` + publishable key.
- Refuses with 400 if no Stripe customer (user must do manual top-up first).
- Dashboard uses `stripe.confirmCardSetup` which triggers 3DS in-browser for SCA cards, creating the MIT prior-auth record.

**Gate:** can save a `4242…` test card end-to-end. Can save an SCA test card (`4000 0027 6000 3184`) — 3DS challenge fires in browser, user authenticates, `pm_xxx` saved with prior-auth record at Stripe.

### Phase 4 — Config CRUD (~1 day)
- `GET /billing/auto_topup_config`
- `PUT /billing/auto_topup_config` — validate `pm_xxx` ownership, enforce `cap >= topup_amount`, min top-up $5, UPSERT.

**Gate:** config can be saved, read back, CHECK constraint rejects `enabled=true` with nulls.

### Phase 5 — Webhook handler + sync credit + SCA detection (~3 days)
**This is the core phase.** Full §6 implementation:
- Raw `Data` handler, HMAC verification.
- Mutex cache.
- Balance fetch, PI list with pagination, failure derivation, cap check.
- `paymentIntents.create` with sync response handling:
  - succeeded → credit
  - **authentication_required → save `pending_action_pi_id` + recovery token, email tokenized link**
  - declined/expired/etc. → email
  - timeout → log (reconciler handles)
- Sync credit: INSERT auto_topup_credits, balance_transactions write with idempotency key, UPDATE row.
- Cache invalidation callback.

**Gate:** real `4242…` card → balance drops below threshold → auto-charge fires → balance credited within seconds. Burst of parallel customer.updated for same customer collapses to one charge. Declined card (`4000 ... 0341`) → email sent, no credit.

### Phase 6 — Reconciliation cron (~1 day)
- Spawn `tokio` task at startup, runs every `RECONCILER_INTERVAL_SECS` (default 900).
- Query enabled customers, list their recent auto-topup PIs, find missing credit rows or NULL balance_transaction_ids, fix them.
- Log/alert on any row created or fixed by the reconciler.

**Gate:** kill `lit-payments` mid-charge between PI create and balance_transactions → restart → reconciler finds the orphan within 15 min and credits the user.

### Phase 7 — Dashboard UI + SCA recovery page (~3 days)
- Auto-top-up modal.
- Save-card flow with `stripe.confirmCardSetup` (handles 3DS prior-auth automatically for EU cards).
- Save-config flow.
- Status banners — including new "Action required" banner when `pending_action_pi_id` is set.
- **New SCA recovery page** at `/recover_topup?token=...`:
  - Calls `GET /billing/auto_topup_resume?token=...` to get `client_secret`.
  - Runs `stripe.confirmCardPayment(client_secret)` to render bank's 3DS challenge.
  - On success → calls `POST /billing/auto_topup_resume/complete` to trigger backend credit.
  - On failure → shows error UI with link to update card.

**Gate:** real user can save a card (including an SCA test card) and enable auto-top-up. Lit Action triggers auto-charge. SCA card test: trigger `authentication_required`, user receives email, clicks link, completes 3DS on recovery page, balance credited.

### Phase 8 — Operational hardening (~2 days)
- Email templates: declined, **SCA action-required (with tokenized link)**, auto-disabled.
- Metrics: trigger count, charge success/failure rate, reconciler activity, mutex contention.
- Service-auth secret rotation procedure documented.
- Admin runbook.

**Gate:** failure-counter path tested (3 declines → auto-disable → re-enable after updating card).

### Phase 9 — Production rollout (~1–2 days monitored)
- Register production Stripe webhook endpoint (only `customer.updated`).
- Deploy with env vars wired.
- Feature-flag the dashboard modal.
- Monitor for 24–48 hours.

### Parallelization notes
- Phase 2 can run in parallel with Phase 1 once the migration is reviewed.
- Phase 7 (dashboard styling) can start in parallel with Phase 5/6 once API shapes are frozen.

---

## 18. Local development & testing

### Prerequisites
- Rust toolchain (per `lit-api-server/rust-toolchain.toml`).
- Docker.
- Stripe CLI (`brew install stripe/stripe-cli/stripe`).
- Stripe test-mode account with restricted key (Customers R/W, PaymentIntents W, SetupIntents W, PaymentMethods R, Customer Balance Transactions W).

### Step 1 — Postgres

```sh
docker run --rm -d --name lit-payments-pg \
  -e POSTGRES_PASSWORD=postgres -p 5432:5432 postgres:16
```

### Step 2 — Env vars

`lit-payments/.env`:
```sh
DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres
MAGIC_LINK_SIGNING_KEY=$(openssl rand -base64 32)
ROCKET_SECRET_KEY=$(openssl rand -base64 32)
RESEND_API_KEY=re_test_or_real
MAIL_FROM=noreply@mail.litprotocol.com
PUBLIC_BASE_URL=http://localhost:8000
ROCKET_PORT=8000
STRIPE_SECRET_KEY=rk_test_...
STRIPE_WEBHOOK_SECRET=whsec_...  # from `stripe listen`
LIT_API_SERVER_BASE_URL=http://localhost:8002
LIT_INTERNAL_SHARED_SECRET=$(openssl rand -base64 32)
RECONCILER_INTERVAL_SECS=60  # faster for local testing
ALCHEMY_HTTPS_URL=https://...
```

`lit-api-server/.env` additions:
```sh
LIT_INTERNAL_SHARED_SECRET=<same value>
```

### Step 3 — Apply migrations
Auto-runs on `cargo run`. Or manually:
```sh
sqlx migrate run --database-url postgres://postgres:postgres@localhost:5432/postgres
```

### Step 4 — Forward webhooks

```sh
stripe login
stripe listen --events customer.updated --forward-to http://localhost:8000/stripe/webhook
```
Copy the printed signing secret into `STRIPE_WEBHOOK_SECRET`, restart lit-payments.

### Step 5 — Start services

```sh
# Terminal A: lit-payments
cd lit-payments && cargo run

# Terminal B: lit-api-server
cd lit-api-server && cargo run

# Terminal C: dashboard
cd lit-static/dapps/dashboard && python3 -m http.server 8001

# Terminal D: stripe listen (keep running)
```

### Stripe test cards

| Card | Behavior |
|---|---|
| `4242 4242 4242 4242` | Success — off-session charges work |
| `4000 0000 0000 0341` | Off-session decline (`card_declined`) |
| `4000 0027 6000 3184` | 3DS required (`requires_action`) — treated as failure |
| `4000 0000 0000 9995` | `insufficient_funds` decline |
| `4000 0000 0000 0069` | `expired_card` decline |
| `4000 0000 0000 0127` | `incorrect_cvc` decline |

### Per-phase acceptance tests

**After Phase 2** — both services accept the same wallet-sig + API-key auth.

**After Phase 3** — saving `4242…` attaches `pm_xxx` to the right customer.

**After Phase 4** — config can be saved/read; CHECK constraint rejects invalid states.

**After Phase 5:**
- [ ] Real card saved → `stripe post /v1/customers/{c}/balance_transactions -d "amount=-X" -d "currency=usd"` to drop balance → `customer.updated` fires → trigger handler creates PI, credits user. Verify row in `auto_topup_credits` with non-null `stripe_balance_transaction_id`.
- [ ] Burst: 10 balance_transactions in parallel → only one PI created (mutex + balance-check short-circuit).
- [ ] `4000 0000 0000 0341` saved → trigger fires → `paymentIntents.create` returns declined → email sent, no credit. Repeat 3x → config gets `disabled_reason='failures'`.
- [ ] `4000 0027 6000 3184` saved → trigger fires → returns `requires_action` → email sent, no credit.
- [ ] Webhook signature tampered → handler returns 401.
- [ ] Immediately after a credit, lit-api-server balance fetch returns the new value (cache invalidated).

**After Phase 6:**
- [ ] Manually create a `succeeded` PI in Stripe with `metadata.source=auto_topup` (no corresponding DB row) → reconciler picks it up within `RECONCILER_INTERVAL_SECS` and credits the customer.
- [ ] Create a row in `auto_topup_credits` with `stripe_balance_transaction_id IS NULL` → reconciler completes the credit on next tick.
- [ ] Kill lit-payments between `paymentIntents.create` and `balance_transactions` (insert a sleep in dev to simulate) → restart → reconciler recovers the orphan within 1 min.

**After Phase 7** — full end-to-end flow with real user clicking through the dashboard.

### End-to-end smoke before Phase 9 rollout

- [ ] Full happy path: save card → enable → run actions → auto-charge → credited.
- [ ] Cap reached: set cap=$5, top-up=$5, drive two top-ups in a row → second skipped.
- [ ] Cache invalidation under burst: 5 deductions immediately after a top-up → all see updated balance.
- [ ] Reconciler heartbeat: tail logs/metrics for 24h, ensure cron runs every interval.

### Quick-reference commands

```sh
# Inspect tables
psql $DATABASE_URL -c 'SELECT * FROM auto_topup_config;'
psql $DATABASE_URL -c 'SELECT * FROM auto_topup_credits ORDER BY credited_at DESC LIMIT 10;'

# Manually drop balance to trigger customer.updated
stripe post /v1/customers/cus_xxx/balance_transactions \
  -d "amount=-100" -d "currency=usd" -d "description=test"

# Check Stripe-side state
stripe customers retrieve cus_xxx
stripe payment_intents list --customer cus_xxx --limit 10
```

---

## 19. Verification

Reviewed across four passes:
1. Initial design (Codex consult).
2. Simplified design (Codex consult, resumed).
3. Empirical Stripe test mode validation of `customer.updated` firing 1:1 per balance_transaction (21 events for 21 transactions, no coalescing under slow or parallel load).
4. Codex consult on whether sync-credit is safe under US-card + no-SCA constraints — confirmed defensible with idempotency keys + reconciliation.

Alternative architectures considered and rejected:
- Stripe Billing Meters + Credit Grants — requires migrating off `customer.balance`, touches admin portal / dashboard / lit-api-server / lit-payments. Out of scope per separate strategic decision.
- Webhook-driven credit (`payment_intent.succeeded`) — adds duplicate-delivery handling complexity for marginal benefit over sync-credit + reconciler under the chosen constraints (US-only, no SCA).

---

## 20. Handoff checklist for the implementing agent

- [ ] Read §1–4 (goal, framing, architecture, schema).
- [ ] Read §5–10 (endpoints, sync charge+credit flow, signature verification, `lit-api-server` changes, reconciler, dashboard).
- [ ] Read §11 (three-layer concurrency).
- [ ] Read §12–13 (edge cases handled vs accepted).
- [ ] Read §14–15 (env vars, service-auth).
- [ ] Read §18 and stand up the local stack before coding.
- [ ] Follow §17 phases in order; honor gates.
- [ ] Use per-phase acceptance tests in §18 as gate criteria.

Key facts to internalize:
- **`customer.updated` is the only webhook.** Filter on `previous_attributes.balance`.
- **Charge AND credit are synchronous.** No `payment_intent.succeeded` webhook.
- **Reconciler is load-bearing**, not optional — it's the only path that recovers from HTTP timeouts and partial-write failures.
- **US cards only, no SCA.** `requires_action` is treated as failure.
- **lit-api-server is unchanged** except for one tiny cache-invalidation endpoint.
- **Auth extracted** so dashboard talks to lit-payments directly with identical auth headers.
- **Soft cap, bias toward more top-ups.** Rare double-charge acceptable; missed top-up is not.
- **Webhook handler returns 2xx only after credit committed** to Postgres. Otherwise lost credit.

This document is self-contained. Raise ambiguities — do not guess.
