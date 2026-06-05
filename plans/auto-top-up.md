# Auto Top-Up — Implementation Plan

**Status:** Locked. Ready to implement.
**Scope:** Stripe-paying users only. LITKEY / crypto users are out of scope.
**Repository:** `chipotle` (this repo).

---

## 1. Goal

After every Lit Action deduction (and every management deduction), if the user's available Stripe credit falls below a threshold they configured, automatically charge their saved card off-session for a configured top-up amount, subject to a configured monthly cap. UI is modeled on Claude Console and OpenAI Platform's auto-recharge modals.

**Auto top-up is opt-in.** Users configure it via the dashboard after saving a card.

---

## 2. Product framing

### What the user sees
- A new "Auto top-up" section in the dashboard's billing UI.
- A modal with: enable toggle, threshold input, top-up amount input, monthly cap input, saved-card picker.
- Status banner: shows current rule in plain English when enabled, or a "set up a card" CTA when disabled.
- Failure banner: shown when auto-top-up is auto-disabled after 3 consecutive failures, or when the last off-session attempt requires SCA re-authentication.

### Soft cap (explicit trade-off)
The monthly cap is **best-effort**, not a contract. UI copy must say "approximately $X/month" or "up to ~$X/month with rare overage." Overshoot is bounded by one top-up amount at 5-minute idempotency-key bucket boundaries.

### What we are not building
- Hard monthly cap with reservation ledger.
- Currency support beyond USD.
- Refund-aware cap accounting (refunds do NOT restore monthly cap capacity).
- Automated daily/rolling restrictions for public-tier API keys (future feature).
- Reconciler cron — manual recovery via Sally's admin portal for the rare "webhook lost for 3 days" case.

---

## 3. Architecture overview

### Components

| Component | Path / Location | Role |
|---|---|---|
| **Dashboard** | `lit-static/dapps/dashboard/` | Vanilla HTML/JS. Adds config modal, save-card flow, status banners. |
| **lit-api-server** | `lit-api-server/`, runs in TEE | Existing API server. Adds: 4 new dashboard-facing endpoints behind existing `billing_auth.rs`, 1 fire-and-forget trigger call after every deduction, 1 internal cache-invalidation endpoint. |
| **lit-payments** | `lit-payments/`, on Railway | Existing service. Adds: 3 internal-only endpoints, 1 Stripe webhook receiver, 2 Postgres tables. |
| **Postgres** | inside `lit-payments` DB | Existing DB. Adds 2 new tables. |
| **Stripe** | external | Customer, PaymentMethod, PaymentIntent, balance transactions, webhooks. |

### Auth model
- Dashboard sends the same headers it already sends today: either an API key OR a wallet signature (EIP-712 ChainSecured).
- `lit-api-server`'s existing `billing_auth.rs` guard (`src/core/v1/guards/billing_auth.rs`) verifies both and derives the Stripe `customer_id`.
- `lit-payments` is internal-only. Its endpoints accept only:
  - `X-Internal-Secret` from `lit-api-server` (high-entropy shared secret, TLS-only, constant-time compare, never logged).
  - `Stripe-Signature` HMAC on the webhook endpoint.

### Storage map

| Data | Where it lives | Why |
|---|---|---|
| Config (`enabled`, `threshold_cents`, `topup_amount_cents`, `monthly_cap_cents`, `payment_method_id`) | Postgres `auto_topup_config` | Fast reads (~1ms), atomic writes, schema evolution, queryable for support |
| Consent record (`consent_version`, `consent_signed_at`) | Postgres `auto_topup_config` (same row) | Off-session merchant-initiated charges require recorded user consent |
| Card data | Stripe (PaymentMethod attached to Customer) | Never touches our servers; PCI scope stays with Stripe |
| Wallet ↔ Stripe Customer mapping | Stripe customer metadata (`metadata.wallet_address`) | Existing pattern; unchanged |
| Charge history (PaymentIntents) | Stripe (filtered by `metadata.source=auto_topup`) | Source of truth for what was charged |
| Monthly spend total | Computed by listing Stripe PIs | No counter to race on |
| Failure state | Derived by listing last N PIs and counting failures | No counter to race on |
| Credit dedup (1 row per credited PI) | Postgres `auto_topup_credits` | Permanent dedup beyond Stripe's 24h idempotency cache |

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
  disabled_reason          TEXT,         -- NULL, 'manual', 'failures', 'card_invalid'
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

Validation constraints enforce: `enabled=true ⇒ all required fields non-null`, `cap >= topup_amount`, min top-up $5.00 (matches existing one-shot floor), positive cents, USD-only (implicit).

---

## 5. Endpoints

### Dashboard-facing (on `lit-api-server`, behind existing `billing_auth.rs`)

| Method + Path | Purpose | Behavior |
|---|---|---|
| `POST /billing/setup_intent` | Save card | Calls `lit_billing_core::customer::find_or_create_by_wallet`. Refuses if user has no Stripe customer AND has never made a manual top-up (we don't auto-create customers here; user must do their first manual top-up first to bootstrap the Stripe customer). Then creates Stripe SetupIntent via `lit_billing_core::StripeClient::post_with_idempotency("setup_intents", [usage=off_session, customer=cus_xxx])`. Returns `client_secret` to dashboard. |
| `GET /billing/auto_topup_config` | Read config | Forwards to `lit-payments`'s `GET /internal/auto_topup_config?customer_id=...` with `X-Internal-Secret`. Returns config JSON to dashboard. |
| `PUT /billing/auto_topup_config` | Save config | Body: `{enabled, threshold_cents, topup_amount_cents, monthly_cap_cents, payment_method_id, consent_version}`. Server-side validation: verify `payment_method_id` belongs to this customer (`GET /v1/customers/{cus_xxx}/payment_methods` and check membership), enforce `cap >= topup_amount`, etc. Forwards to `lit-payments`'s `PUT /internal/auto_topup_config`. |
| `POST /internal/invalidate_balance_cache` | Drop cached balance | Called by `lit-payments` after every successful auto-credit. Body: `{customer_id}`. Calls `state.balance_cache.invalidate(&customer_id)` (same primitive as existing `confirm_payment_intent` flow at `lit-api-server/src/stripe.rs:612`). Auth: `X-Internal-Secret`. |

### Internal (on `lit-payments`, auth: `X-Internal-Secret`)

| Method + Path | Purpose | Behavior |
|---|---|---|
| `GET /internal/auto_topup_config` | Read config row | Query Postgres by `customer_id`. Return JSON or 404. |
| `PUT /internal/auto_topup_config` | Upsert config row | UPSERT into `auto_topup_config`. Validates against CHECK constraint. Returns updated row. |
| `POST /internal/trigger_topup` | Evaluate + maybe charge | Body: `{customer_id, wallet_address}`. See §6 for the full handler logic. Returns 202 immediately, processes async. |

### Webhook (on `lit-payments`, auth: Stripe-Signature HMAC)

| Method + Path | Purpose |
|---|---|
| `POST /stripe/webhook` | Receive `payment_intent.succeeded` / `payment_intent.payment_failed`. Stripe-Signature verified with `STRIPE_WEBHOOK_SECRET`. See §7 for handler logic. |

---

## 6. `/internal/trigger_topup` handler — detailed flow

Called by `lit-api-server` after every deduction (chunk flush, final flush, management deduction). Fire-and-forget; never blocks the caller.

### Step-by-step

1. **Acquire per-customer mutex.**
   `let lock = customer_mutex_cache.get_or_insert(customer_id).await;`
   `let _guard = lock.lock().await;`
   Mutex cache is `moka::sync::Cache<String, Arc<tokio::sync::Mutex<()>>>` with 5-minute TTL (avoids unbounded growth).
   Serializes concurrent triggers for the same customer; optimization only — correctness rests on the idempotency key.

2. **Read config from Postgres.** Single row by `customer_id`.

3. **If `!enabled` → release, return.**

4. **Fetch current Stripe balance** via `lit_billing_core::balance::fetch(stripe_client, &customer_id)`.
   Stripe customer balance is stored as a negative number (negative = credit owed). Available credit = `-balance`.
   **If available credit ≥ `threshold_cents`** → release, return. (User is not actually below threshold despite the trigger firing.)

5. **List recent auto-top-up PaymentIntents from Stripe.**
   `GET /v1/payment_intents?customer={cus_xxx}&created[gte]={month_start_utc}&limit=100`, paginate via `starting_after` until `has_more=false`.
   Client-side filter on `metadata.source == "auto_topup"` (the list endpoint doesn't filter by metadata server-side).

6. **Derive failure state.**
   Walk the list from most recent backwards, counting consecutive PIs in failed states.
   "Failed" = status in (`requires_payment_method`) OR `last_payment_error.code` in (`card_declined`, `expired_card`, `insufficient_funds`, `incorrect_cvc`, `processing_error`, etc.).
   If `consecutive_failures >= 3`:
   - `UPDATE auto_topup_config SET enabled=false, disabled_reason='failures', updated_at=now() WHERE customer_id=...`
   - Send email + dashboard banner.
   - Release mutex, return.

7. **Recent-PI short-circuit.**
   If any PI from the list is in a non-failed state (`succeeded`, `processing`, `requires_action`) and was `created` in the last 10 minutes → release, return. ("Already topped up recently.")

8. **Cap check.**
   Sum `amount` of all non-failed PIs this month. If `sum + topup_amount_cents > monthly_cap_cents` → release, return. (Cap reached.)

9. **Compute deterministic Stripe Idempotency-Key.**
   `key = format!("auto_topup:{}:{}", customer_id, unix_ts_secs / 300)`
   Same key across all parallel triggers within the same 5-minute window for the same customer → Stripe dedupes server-side.

10. **Create off-session PaymentIntent.**
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
      Idempotency-Key: {key}
    ```
    On `authentication_required` error: set a flag on the config row (e.g. via `disabled_reason='requires_action'`) so the dashboard shows "action required" banner. User must re-authenticate on-session next visit.
    On `card_declined` / `expired_card` / etc.: webhook will fire `payment_intent.payment_failed` and trigger the counter-derivation in step 6 next time. No write here.
    On network/timeout: do nothing. Webhook may still arrive when Stripe finishes processing.

11. **Release mutex.**
    Do NOT credit synchronously. The wallet balance increment happens in the webhook handler.

---

## 7. Webhook handler — detailed flow

Endpoint: `POST /stripe/webhook` on `lit-payments`.

### Step-by-step

1. **Read raw body** with Rocket `Data` handler (NOT the JSON extractor — HMAC must verify exact bytes). Apply a size limit (e.g., 1 MB).

2. **Verify Stripe-Signature header.**
   - Parse `t={timestamp},v1={hex_signature}`.
   - Reject if `|now - timestamp| > 300s` (5-minute tolerance).
   - Compute `HMAC-SHA256(STRIPE_WEBHOOK_SECRET, "{timestamp}.{raw_body}")`.
   - Constant-time compare against `v1`. Invalid → 401.

3. **Parse JSON event.** Extract `event.type` and `event.data.object`.

4. **Optionally re-fetch the PaymentIntent by id** (`GET /v1/payment_intents/{pi.id}`) for defense against Stripe API version drift in the event payload shape.

5. **Branch on `event.type`:**

   **`payment_intent.succeeded`:**
   - If `pi.metadata.source != "auto_topup"` → return 200 (not ours).
   - Atomic dedup insert:
     ```sql
     INSERT INTO auto_topup_credits (payment_intent_id, customer_id, amount_cents)
       VALUES ($1, $2, $3)
       ON CONFLICT (payment_intent_id) DO NOTHING
       RETURNING payment_intent_id;
     ```
   - If no row returned (already credited) → return 200.
   - If row inserted → call `lit_billing_core::balance::write_transaction(stripe_client, customer_id, -pi.amount, description="Auto top-up via {pi.id}", idempotency_key="credit:{pi.id}")`.
   - On success, `UPDATE auto_topup_credits SET stripe_balance_transaction_id=$1 WHERE payment_intent_id=$2`.
   - Call `POST {LIT_API_SERVER_BASE_URL}/internal/invalidate_balance_cache` with `X-Internal-Secret` and `{customer_id}`. (Fire-and-forget; ignore errors — Stripe cache will refresh in ≤10 min regardless.)
   - Return 200.

   **`payment_intent.payment_failed`:**
   - If `pi.metadata.source != "auto_topup"` → return 200.
   - Send email + dashboard banner to user. (Email template: "Your auto top-up of $X failed: {reason}. Please update your card.")
   - Do NOT write a counter. The disable decision is derived live in `/trigger_topup` by listing PIs and counting failures.
   - Return 200.

   **Any other event type:** return 200 without action.

6. **Always return 200** on successful processing (so Stripe doesn't retry). On internal errors (DB down, Stripe down), return 5xx so Stripe retries (up to 3 days).

---

## 8. `lit-api-server` changes

### One change: fire-and-forget trigger after every deduction

Hook the shared `charge()` function in `lit-api-server/src/stripe.rs:337` (covers both `charge_lit_action_time()` and `charge_management()`). After the existing optimistic decrement and fire-and-forget Stripe write, additionally:

```rust
let customer_id = customer_id.clone();
let wallet = wallet_address.clone();
let payments_base = state.config.lit_payments_base_url.clone();
let secret = state.config.lit_internal_shared_secret.clone();
tokio::spawn(async move {
    let _ = reqwest::Client::new()
        .post(format!("{}/internal/trigger_topup", payments_base))
        .header("X-Internal-Secret", secret)
        .json(&json!({ "customer_id": customer_id, "wallet_address": wallet }))
        .send()
        .await;
});
```

- Fire-and-forget. Never blocks the Lit Action / management response.
- Fires after **every** chunk flush during a Lit Action and after final flush, AND after every management deduction. Many triggers per action is fine — the mutex + idempotency-key collapse them. Mid-action top-ups are a **feature** (a long-running Lit Action that drains the balance can top up mid-flight and continue).
- Errors logged but never propagated.

### New internal endpoint

`POST /internal/invalidate_balance_cache` on `lit-api-server`:
- Body: `{customer_id}`.
- Auth: `X-Internal-Secret`.
- Calls the existing `state.balance_cache.invalidate(&customer_id)` primitive (`stripe.rs:612` precedent).
- Returns 200.

---

## 9. Dashboard changes

### Save-card flow

New modal opened from the billing page.

1. User clicks "Add a card for auto top-up."
2. Dashboard calls `POST /billing/setup_intent` with existing auth headers.
3. Backend returns `{ client_secret, publishable_key }`.
4. Dashboard initializes Stripe.js with the publishable key, mounts the Payment Element in **setup mode** (not payment mode).
5. User enters card. Dashboard calls `stripe.confirmSetup({ elements, confirmParams: { return_url: dashboard_url } })`.
6. On return, dashboard reads `setup_intent` query param, calls `stripe.retrieveSetupIntent(setup_intent_client_secret)` to get the `payment_method` id.
7. Stores `pm_xxx` in local state pending submission.

### Save-config flow

1. Modal collects: enable toggle, threshold (USD input), top-up amount (USD input), monthly cap (USD input), card picker (preselected to newly-saved `pm_xxx` or existing default), consent checkbox with explicit text ("I authorize Lit Protocol to charge my saved card up to $X per month when my balance falls below $Y...").
2. On submit, dashboard calls `PUT /billing/auto_topup_config` with `{enabled, threshold_cents, topup_amount_cents, monthly_cap_cents, payment_method_id, consent_version: "v1"}`.
3. Backend validates and persists.

### Status banners

- **Enabled, healthy:** "Auto top-up: when your balance drops below $X, we'll charge $Y to card ending in ****1234, up to ~$Z/month."
- **Enabled, requires_action (SCA pending):** "Action required: confirm your last auto top-up. [Re-authenticate]."
- **Disabled by user:** "Auto top-up is off. [Enable]"
- **Auto-disabled after failures:** "Auto top-up was paused after 3 failed attempts. Please update your card. [Manage]"

---

## 10. Concurrency model — three layers of defense

| Layer | What it prevents | Mechanism | Scope |
|---|---|---|---|
| 1. Per-customer Tokio mutex (`moka` TTL cache) | Wasted Stripe API calls under burst | In-memory in `lit-payments` | Per-process |
| 2. Deterministic Stripe Idempotency-Key on PaymentIntent create | Multiple PaymentIntents from concurrent triggers | Stripe server-side dedup, 24h cache | Global across instances |
| 3. Postgres unique constraint on `auto_topup_credits.payment_intent_id` | Double-credit on webhook replays (Stripe Dashboard resend up to 15 days, CLI resend up to 30 days) | `INSERT … ON CONFLICT DO NOTHING` | Permanent |

The mutex is optimization. Layers 2 and 3 are correctness primitives.

---

## 11. Edge cases — handled

| Case | Handling |
|---|---|
| 5+ parallel triggers from concurrent Lit Actions | Same idempotency-key → Stripe returns the same PI to all callers → 1 charge |
| HTTP timeout on `paymentIntents.create` | Stripe still fires `payment_intent.succeeded` webhook when the charge settles; webhook handler credits the user |
| Webhook delivered twice / replayed | `INSERT … ON CONFLICT DO NOTHING` skips the second one |
| Card declined (insufficient funds, expired, etc.) | Webhook fires `payment_intent.payment_failed`; next `/trigger_topup` derives consecutive-failure count from listing PIs and disables after 3 |
| SCA required (`requires_action`) | Caught on synchronous create response; flag set on config; dashboard shows "action required" banner; user re-auths on-session next visit |
| User toggles auto-top-up off between trigger fire and handler execution | Handler reads `enabled=false` from Postgres and short-circuits |
| Trigger fires when balance is actually above threshold (e.g., user just topped up manually) | Handler's step 4 (balance fetch) short-circuits |
| `lit-payments` is briefly down when trigger fires | Trigger HTTP call fails; next deduction's trigger retries |
| Stripe customer balance cached stale in `lit-api-server` after top-up | Webhook handler calls `POST /internal/invalidate_balance_cache` after successful credit |
| Many triggers per long Lit Action (chunk flushes) | Mutex + idempotency-key collapse to 1 charge per 5-minute window |
| Stripe API version drift in event payload | Webhook handler re-fetches the PI by id before crediting |
| Pagination on PI list at scale | Use `starting_after` until `has_more=false` |

---

## 12. Edge cases — explicit trade-offs (accepted, not handled)

| Case | What happens | Recovery |
|---|---|---|
| 5-minute idempotency-key bucket boundary race | Two triggers in adjacent 300-second buckets could both fire | Cap accounting in step 8 catches it most times; otherwise overshoot bounded by 1 top-up; UI says "approximately $X/month" |
| Webhook delivery fails for full 3-day Stripe retry window | User paid Stripe, never credited on our side | Manual recovery via Sally's existing admin portal |
| `lit-payments` horizontal scaling | Mutex becomes per-instance, not global | Correctness still holds via idempotency-key + DB unique constraint; only optimization weakens |
| Refunds | Don't restore monthly cap capacity (anti-abuse) | None — out of scope |
| Currency other than USD | Not supported | None — out of scope |
| Month boundary | UTC | Documented in UI copy |

---

## 13. New environment variables

### `lit-api-server`
- `LIT_PAYMENTS_BASE_URL` — e.g., `https://payments.litprotocol.com`
- `LIT_INTERNAL_SHARED_SECRET` — high-entropy random string

### `lit-payments`
- `LIT_API_SERVER_BASE_URL` — e.g., `https://api.litprotocol.com` (for cache invalidation callback)
- `LIT_INTERNAL_SHARED_SECRET` — same value as on `lit-api-server`
- `STRIPE_WEBHOOK_SECRET` — from Stripe dashboard after registering the webhook endpoint

### Dashboard
- `STRIPE_PUBLISHABLE_KEY` — if not already present

---

## 14. Service-auth requirements for `X-Internal-Secret`

- Generated with at least 256 bits of entropy (`openssl rand -base64 32`).
- Stored in env vars only, never in code or commits.
- Connections between `lit-api-server` and `lit-payments` are TLS only (Railway gives this for free externally; internal hop should also be TLS).
- Comparison in the handler uses constant-time equality (`subtle::ConstantTimeEq` in Rust).
- Never logged, never echoed in error responses.
- Rotation procedure: deploy both services with both old + new secrets accepted, swap, then drop old. Document.

---

## 15. Sequence diagrams

### Setup (one-time)

```
USER → DASHBOARD ────► lit-api-server ──► Stripe (SetupIntent create)
       open modal      POST /billing/        │
                       setup_intent          │
                                             ▼
USER → DASHBOARD ◄─────────────────────  client_secret
USER → DASHBOARD ─────► Stripe.js (card entered)
                        ────► Stripe (PaymentMethod attached, pm_xxx)
USER → DASHBOARD ─────► lit-api-server ──► Stripe (verify pm_xxx belongs to customer)
                        PUT /billing/        │
                        auto_topup_config    ▼
                                          lit-payments (UPSERT auto_topup_config)
```

### Runtime (every Lit Action chunk + management deduction)

```
CLIENT ──► lit-api-server  (run Lit Action; deduct credits; fire-and-forget trigger)
                  │
                  ▼  tokio::spawn
              lit-payments POST /internal/trigger_topup
                  │
              acquire mutex[customer]
                  │
                  ▼
              Postgres (read auto_topup_config)
                  │
                  ▼
              Stripe (balance::fetch)
                  │
                  ▼  if available >= threshold, release & return
                  │
              Stripe (list PIs this month, paginated)
                  │
                  ▼  derive failure state; cap check; recent-PI short circuit
                  │
              Stripe (POST /payment_intents, off_session, idempotency-key)
                  │
              release mutex
                  │
                  ▼ (response not awaited; charge proceeds asynchronously at Stripe)
CLIENT ◄── lit-api-server  (Lit Action result returned earlier, never waited)
```

### Webhook (async, seconds to days later)

```
Stripe ──► lit-payments POST /stripe/webhook
              │
              ▼  verify HMAC, parse event
              │
       ┌──────┴──────┐
       ▼             ▼
   SUCCEEDED      FAILED
       │             │
       ▼             ▼
   Postgres       email + dashboard banner
   INSERT auto_topup_credits  (no DB writes)
   ON CONFLICT DO NOTHING
       │
   ┌───┴───┐
   ▼       ▼
  row    no row → return 200 (already credited)
   │
   ▼
   Stripe (POST balance_transactions, Idempotency-Key: credit:{pi.id})
   │
   ▼
   Postgres (UPDATE row with stripe_balance_transaction_id)
   │
   ▼
   lit-api-server POST /internal/invalidate_balance_cache  (fire-and-forget)
   │
   ▼
   return 200 to Stripe
```

---

## 16. Build order

1. **Migration.** `lit-payments/migrations/{timestamp}_auto_topup.sql` with both tables + check constraints.
2. **`lit-payments` internal endpoints.** GET/PUT `/internal/auto_topup_config`, POST `/internal/trigger_topup`. Wire up `moka` mutex cache. Use existing `lit_billing_core` helpers.
3. **`lit-payments` Stripe webhook.** Raw `Data` handler, HMAC verification, dedup insert, balance credit, cache-invalidation callback. Register endpoint in Stripe Dashboard, get signing secret.
4. **`lit-api-server` cache-invalidation endpoint.** `POST /internal/invalidate_balance_cache` calling existing `balance_cache.invalidate`.
5. **`lit-api-server` trigger spawn.** Hook into the shared `charge()` function. Fire-and-forget to `lit-payments`. Wire up env vars.
6. **`lit-api-server` dashboard endpoints.** `POST /billing/setup_intent` (calls `lit_billing_core::customer::find_or_create_by_wallet`, then SetupIntent create). `GET /billing/auto_topup_config` (proxy). `PUT /billing/auto_topup_config` (validate `pm_xxx` ownership, proxy).
7. **Dashboard UI.** Modal, save-card flow with `stripe.confirmSetup`, save-config flow, status banners.
8. **End-to-end test in Stripe test mode.** Real card → enable auto-top-up → run Lit Actions → verify auto-charge fires → simulate `payment_failed` → verify auto-disable after 3 → simulate webhook replay → verify no double-credit.
9. **Operational.** Register production Stripe webhook endpoint, set env vars in deployment configs (Railway for `lit-payments`, TEE config for `lit-api-server`), update Sally's admin portal docs.

---

## 17. What is explicitly NOT in this plan

- Hard monthly cap with reservation ledger. The current design is a soft cap with bounded overshoot; UI copy reflects this.
- Reconciler cron for finding orphaned Stripe PIs without a `auto_topup_credits` row. Manual recovery via Sally's admin portal is the fallback for the rare 3-day-webhook-failure case.
- Currency support beyond USD.
- Daily/rolling restrictions on public-tier API keys (mentioned in the original Adarsh notes as a future feature; out of scope here).
- Refund-aware cap accounting.
- Multi-card support per customer (auto-top-up uses one saved card; user can change it by updating config).

---

## 18. Open questions for product

- Should the consent text say "approximately $X/month" or "up to $X/month" or both? Legal preference.
- What email service do we use for failure notifications? Existing infrastructure or new?
- Failure-counter threshold for auto-disable: 3 (current plan). Confirm with product.
- 10-minute recent-PI short-circuit window. Confirm with product.
- 5-minute idempotency-key bucket window. Confirm with product.
- Minimum top-up amount: $5 (matches existing one-shot floor). Confirm.

---

## 19. Verification

This plan has been independently reviewed by Codex (OpenAI's reasoning model) in two passes — once against the original heavier design and once against this simplified version. All load-bearing findings have been incorporated. Remaining items are implementation details flagged in §6, §7, §8, §13, §14.
