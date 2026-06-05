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
  disabled_reason          TEXT,         -- NULL, 'manual', 'failures', 'card_invalid', 'requires_action'
  pending_action_pi_id     TEXT,         -- set when an off-session PI returns requires_action; cleared on success
  pending_action_at        TIMESTAMPTZ,  -- timestamp of the requires_action event; used for stale-action TTL
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
| `POST /billing/auto_topup_resume_pending` | Resume SCA-pending top-up | Reads the user's `pending_action_pi_id` from config (via `lit-payments` proxy). If set, retrieves the PaymentIntent's `client_secret` from Stripe and returns it to the dashboard so the user can complete the 3DS challenge with `stripe.handleNextAction`. Returns 404 if no pending action. |
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
    On `authentication_required` error (SCA / 3DS required):
       - `UPDATE auto_topup_config SET pending_action_pi_id=$1, pending_action_at=now(), disabled_reason='requires_action' WHERE customer_id=...`
       - Send the **"action required" email** to the user with a deep link back to the dashboard's billing page.
       - The PI itself stays in `requires_action` status at Stripe — it is NOT cancelled. The user re-authenticates on-session via the dashboard resume flow (§9) and the same PI completes.
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
   - **Clear any pending SCA action for this PI**: `UPDATE auto_topup_config SET pending_action_pi_id=NULL, pending_action_at=NULL, disabled_reason=NULL WHERE customer_id=$1 AND pending_action_pi_id=$2`. (Only clears if this PI was the one waiting on SCA — covers the case where the user completed 3DS via the dashboard resume flow.)
   - Call `POST {LIT_API_SERVER_BASE_URL}/internal/invalidate_balance_cache` with `X-Internal-Secret` and `{customer_id}`. (Fire-and-forget; ignore errors — Stripe cache will refresh in ≤10 min regardless.)
   - Return 200.

   **`payment_intent.payment_failed`:**
   - If `pi.metadata.source != "auto_topup"` → return 200.
   - Send email + dashboard banner to user. (Email template: "Your auto top-up of $X failed: {reason}. Please update your card.")
   - If `pending_action_pi_id == pi.id` (this was an SCA-pending PI that the user failed to authenticate or that expired), also clear `pending_action_pi_id`, `pending_action_at`, and reset `disabled_reason` from `requires_action`.
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

### SCA resume flow (when last off-session attempt returned `requires_action`)

This is what the dashboard does when the user clicks the "Re-authenticate" button on the action-required banner (or arrives via the email deep link).

1. On dashboard load, `GET /billing/auto_topup_config` returns `pending_action_pi_id` if non-null. Dashboard renders the action-required banner.
2. User clicks "Confirm now" (banner) or arrives from the email link.
3. Dashboard calls `POST /billing/auto_topup_resume_pending`. Backend reads `pending_action_pi_id` from config, retrieves the PaymentIntent from Stripe, returns `{ payment_intent_id, client_secret }`.
4. Dashboard runs `stripe.handleNextAction({ clientSecret })`. Stripe.js opens the 3DS challenge modal.
5. User completes the 3DS challenge.
6. PI transitions to `succeeded` at Stripe. Stripe fires `payment_intent.succeeded` webhook → existing handler credits the wallet, clears `pending_action_pi_id` + `pending_action_at` + `disabled_reason` (§7).
7. Dashboard polls `GET /billing/auto_topup_config` after `handleNextAction` resolves and reflects the cleared state.

If the user abandons the challenge or fails 3DS, the PI eventually transitions to `requires_payment_method` (= failed) and Stripe fires `payment_intent.payment_failed`. The webhook handler clears the pending state and emails the user a "card needs updating" message. The same trigger handler's failure-derivation logic counts this against the consecutive-failure threshold.

### Status banners

- **Enabled, healthy:** "Auto top-up: when your balance drops below $X, we'll charge $Y to card ending in ****1234, up to ~$Z/month."
- **Enabled, requires_action (SCA pending):** "Action required to complete your $X auto top-up. [Confirm now]" — clicking triggers the resume flow above.
- **Disabled by user:** "Auto top-up is off. [Enable]"
- **Auto-disabled after failures:** "Auto top-up was paused after 3 failed attempts. Please update your card. [Manage]"

### Email notifications

`lit-payments` sends transactional emails via the existing Resend integration. Three templates needed:

| Trigger | Subject | Body content |
|---|---|---|
| Off-session PI returns `authentication_required` | "Action required: confirm your auto top-up" | Amount, card last4, deep link to dashboard's billing page (which auto-renders the resume banner) |
| `payment_intent.payment_failed` webhook | "Your auto top-up couldn't be charged" | Amount, card last4, decline reason (human-friendly), link to dashboard to update card |
| Auto-disable after 3 consecutive failures (set in `/trigger_topup` step 6) | "Auto top-up paused — update your card" | Brief summary of why, link to dashboard to update card and re-enable |

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
| SCA required (`requires_action`) | Caught on synchronous create response; `pending_action_pi_id` set on config; "action required" email sent + dashboard banner shown; user clicks "Confirm now" → dashboard calls `/billing/auto_topup_resume_pending` and runs `stripe.handleNextAction(client_secret)` to complete 3DS; on `payment_intent.succeeded` webhook the pending state is cleared and the wallet is credited (§9 SCA resume flow) |
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

## 16. Implementation phases

Phases are strictly sequential at the gate level — each gates the next. Sub-tasks within a phase can sometimes be parallelized; the phase boundaries cannot. Total estimated effort: **~2 weeks of focused work** for one engineer.

### Dependency chain

```
Phase 1: Foundation
  └─► Phase 2: Saved card flow (SetupIntent)
        └─► Phase 3: Config CRUD
              └─► Phase 4: Trigger + off-session charge
                    └─► Phase 5: Webhook + credit + cache invalidation
                          └─► Phase 6: Dashboard UI
                                └─► Phase 7: Failure handling + operational hardening
                                      └─► Phase 8: Production rollout
```

### Phase 1 — Foundation (~0.5 day)

**Goal:** schema + env vars + service-to-service auth.

Tasks:
- Create migration `lit-payments/migrations/{timestamp}_auto_topup.sql` with `auto_topup_config` and `auto_topup_credits` tables and CHECK constraints (§4).
- Add new env vars on both services: `LIT_PAYMENTS_BASE_URL`, `LIT_API_SERVER_BASE_URL`, `LIT_INTERNAL_SHARED_SECRET`, `STRIPE_WEBHOOK_SECRET`.
- Add an `X-Internal-Secret` Rocket request guard in `lit-payments` (constant-time compare).
- Add the same guard in `lit-api-server` for the new internal cache-invalidation endpoint.
- Add a reusable `reqwest` client helper in `lit-payments` for the future cache-invalidation callback.

**Gate to Phase 2:** migration applies cleanly against a fresh local Postgres; a dummy `/internal/ping` endpoint behind `X-Internal-Secret` returns 200 when given the secret and 401 without it.

### Phase 2 — Saved card flow (SetupIntent) (~2 days)

**Goal:** the user can save a card off-session. This is the single most error-prone Stripe primitive in the project (SCA, 3DS, return URL handling).

Tasks:
- `POST /billing/setup_intent` on `lit-api-server`, behind existing `billing_auth.rs`.
- Inside the handler: call `lit_billing_core::customer::find_by_wallet`. If no Stripe customer exists, return a 400 telling the user to make a manual top-up first. (We do NOT call `find_or_create_by_wallet` here — bootstrap requires a real first payment.)
- Create the Stripe SetupIntent via `lit_billing_core::StripeClient::post_with_idempotency("setup_intents", &[("usage", "off_session"), ("customer", &cus_xxx)], &idempotency_key)`.
- Return `{ client_secret, publishable_key }` to the dashboard.
- No UI yet — exercise the endpoint via curl + a hand-rolled HTML test page or Postman.

**Gate to Phase 3:** can save a Stripe test-mode card (4242…) end-to-end; `pm_xxx` is attached to the right `cus_xxx` (verify via `stripe customers retrieve cus_xxx`).

### Phase 3 — Config CRUD (~1 day)

**Goal:** read/write the per-user config row.

Tasks:
- `GET /internal/auto_topup_config` on `lit-payments` (by `customer_id`).
- `PUT /internal/auto_topup_config` on `lit-payments` (UPSERT; let CHECK constraint reject bad config).
- `GET /billing/auto_topup_config` on `lit-api-server` (forwards to lit-payments).
- `PUT /billing/auto_topup_config` on `lit-api-server`:
  - Verify `payment_method_id` belongs to this `customer_id` via `GET /v1/customers/{cus}/payment_methods` and membership check.
  - Server-side validation: `cap >= topup_amount`, positive cents, min top-up $5.
  - Forward to lit-payments with derived `customer_id`.

**Gate to Phase 4:** can write a config row through the full chain, read it back, see CHECK constraints reject `enabled=true` with null fields.

### Phase 4 — Trigger + off-session charge (~3 days)

**Goal:** the core decision logic. The expensive phase.

Tasks:
- `POST /internal/trigger_topup` on `lit-payments` — full handler per §6:
  - `moka::sync::Cache<String, Arc<tokio::sync::Mutex<()>>>` with 5-min TTL.
  - Read config from Postgres.
  - `lit_billing_core::balance::fetch` and short-circuit if available credit ≥ threshold.
  - List PaymentIntents for customer this month (paginated via `starting_after` until `has_more=false`).
  - Client-side filter on `metadata.source == "auto_topup"`.
  - Derive failure state — walk from most recent backwards, count consecutive failures in `requires_payment_method` or with relevant `last_payment_error.code` values. If ≥3, disable config, return.
  - Recent-PI short circuit (any non-failed PI in last 10 minutes → return).
  - Cap check (sum + topup_amount > cap → return).
  - Compute deterministic Idempotency-Key: `auto_topup:{customer}:{floor(unix_ts/300)}`.
  - `paymentIntents.create` with `customer`, `payment_method`, `amount`, `currency=usd`, `off_session=true`, `confirm=true`, `metadata.source=auto_topup`, `metadata.wallet_address=...`, Idempotency-Key header.
  - Handle synchronous `authentication_required` by setting `pending_action_pi_id`, `pending_action_at`, and `disabled_reason='requires_action'` on the config row, then sending the "action required" email via Resend.
- Hook into `lit-api-server::charge()` (`stripe.rs:337`): after every deduction, `tokio::spawn` a fire-and-forget `POST /internal/trigger_topup` with `customer_id` + `wallet_address`. Never block the calling request.

**Gate to Phase 5:** in Stripe test mode, a real card is charged via off-session PaymentIntent triggered from a Lit Action deduction. Burst of parallel triggers collapses to one charge (verify via Stripe Dashboard).

### Phase 5 — Webhook + credit + cache invalidation (~2 days)

**Goal:** users actually get credited.

Tasks:
- `POST /stripe/webhook` on `lit-payments`:
  - Rocket raw `Data` handler (NOT `Json<>`).
  - HMAC-SHA256 verification with `STRIPE_WEBHOOK_SECRET`, 5-minute timestamp tolerance, constant-time compare (`subtle::ConstantTimeEq`).
  - Optional defensive re-fetch of the PI by id.
  - On `payment_intent.succeeded` (filter `metadata.source=auto_topup`):
    - `INSERT INTO auto_topup_credits ... ON CONFLICT DO NOTHING RETURNING payment_intent_id;`
    - If no row returned → return 200 (already credited).
    - If inserted → `lit_billing_core::balance::write_transaction` with `Idempotency-Key: credit:{pi.id}`.
    - `UPDATE auto_topup_credits SET stripe_balance_transaction_id=$1`.
    - Clear pending SCA state: `UPDATE auto_topup_config SET pending_action_pi_id=NULL, pending_action_at=NULL, disabled_reason=NULL WHERE customer_id=$1 AND pending_action_pi_id=$2`.
    - Fire-and-forget `POST /internal/invalidate_balance_cache` to `lit-api-server`.
  - On `payment_intent.payment_failed` (filter `metadata.source=auto_topup`): send email + dashboard banner. If the failed PI matches `pending_action_pi_id` for this customer, also clear the SCA pending state and reset `disabled_reason` from `requires_action`.
- `POST /internal/invalidate_balance_cache` on `lit-api-server`:
  - Auth: `X-Internal-Secret`.
  - Calls existing `state.balance_cache.invalidate(&customer_id)` (precedent at `lit-api-server/src/stripe.rs:612`).
- Register the webhook endpoint in the Stripe Dashboard (test mode for now), copy the signing secret into `STRIPE_WEBHOOK_SECRET`.

**Gate to Phase 6:** test-mode PaymentIntent → webhook fires → user's Stripe balance is credited → `lit-api-server` cache shows new balance immediately on next read.

### Phase 6 — Dashboard UI (~3 days)

**Goal:** make all the above reachable by a real user.

Tasks:
- Auto-top-up modal in `lit-static/dapps/dashboard`: enabled toggle, threshold input (USD), top-up amount input (USD), monthly cap input (USD), card picker, consent checkbox with explicit consent text.
- Save-card flow:
  - Call `POST /billing/setup_intent`.
  - Initialize Stripe.js with the returned publishable key.
  - Mount Payment Element in **setup mode** (not payment mode).
  - Call `stripe.confirmSetup({elements, confirmParams: {return_url: dashboard_url}})`.
  - On return, read `setup_intent` query param, call `stripe.retrieveSetupIntent(client_secret)`, extract `payment_method`.
- Save-config flow:
  - `PUT /billing/auto_topup_config` with full config + `consent_version` + signed timestamp.
- SCA resume flow:
  - On dashboard load, if `pending_action_pi_id` is non-null in the config, render the "action required" banner.
  - "Confirm now" button calls `POST /billing/auto_topup_resume_pending`, receives `{ payment_intent_id, client_secret }`, calls `stripe.handleNextAction({ clientSecret })`.
  - After `handleNextAction` resolves, re-fetch config and update the banner state.
- Status banners per §9.
- Dashboard reads config from `GET /billing/auto_topup_config` on page load.

**Gate to Phase 7:** a real user can open the dashboard, save a card, enable auto-top-up, run a Lit Action, and see their balance auto-credited within a minute. Using an SCA test card → action-required banner appears → "Confirm now" completes 3DS → balance is credited. No manual API calls required.

### Phase 7 — Failure handling + operational hardening (~2 days)

**Goal:** the feature survives degraded paths.

Tasks:
- Email templates for `payment_failed` (decline reason, "update your card" CTA).
- Email + dashboard banner for SCA `requires_action` (with re-auth CTA).
- Failure derivation returns specific reason codes to dashboard for the disabled banner.
- Service-auth secret rotation procedure documented in the README.
- Logging / metrics in `lit-payments`: trigger count, charge success rate, webhook delivery latency, mutex contention.
- Admin runbook: how Sally recovers a stuck PI from the existing portal (no new admin UI in v1).

**Gate to Phase 8:** declining a test card → user gets email + banner; after 3 consecutive declines → auto-disabled; user re-enables with a new card → works.

### Phase 8 — Production rollout (~1–2 days monitored)

**Goal:** ship.

Tasks:
- Register the production Stripe webhook endpoint; copy live signing secret into `STRIPE_WEBHOOK_SECRET` on Railway.
- Deploy `lit-payments` (Railway) and `lit-api-server` (TEE) with all new env vars wired.
- Feature-flag the dashboard modal for gradual rollout (e.g., enable for internal accounts first).
- Monitor for 24–48 hours: webhook delivery rate, charge approval rate, support ticket volume.
- Rollback plan: feature flag off, no schema rollback required (tables can stay; rows ignored).

### Parallelization notes

- Phase 6 (dashboard styling) can start in parallel with Phase 4 or Phase 5 once the API shapes are frozen at the end of Phase 3.
- Phase 7 (email templates, runbook) can start as soon as Phase 5 lands.
- The Phase 1 migration can be in code review while Phase 2 is being built.

Things that **cannot** be parallelized: Phase 4 before Phase 2 (no card to charge), Phase 5 before Phase 4 (no PaymentIntents to credit), Phase 6 before Phase 5 (UI would show "enabled" without anything actually working).

---

## 17. Local development & testing

Everything in this plan is fully testable locally against Stripe test mode. No staging environment is required to validate the feature end-to-end.

### Prerequisites

- Rust toolchain matching `lit-api-server/rust-toolchain.toml` (currently 1.91).
- Docker (for local Postgres).
- [Stripe CLI](https://docs.stripe.com/stripe-cli) (`brew install stripe/stripe-cli/stripe`).
- A Stripe account with test-mode access. Create a **restricted key** with these permissions: Customers (Read/Write), PaymentIntents (Write), SetupIntents (Write), PaymentMethods (Read), Customer Balance Transactions (Write).
- `sqlx-cli` if you want to run migrations manually: `cargo install sqlx-cli --no-default-features --features postgres`.

### Step 1 — Start local Postgres

```sh
docker run --rm -d --name lit-payments-pg \
  -e POSTGRES_PASSWORD=postgres \
  -p 5432:5432 \
  postgres:16
```

Database URL: `postgres://postgres:postgres@localhost:5432/postgres`.

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
STRIPE_SECRET_KEY=rk_test_...               # restricted test-mode key
STRIPE_WEBHOOK_SECRET=whsec_...             # printed by `stripe listen` (Step 4)

# Auto top-up (new)
LIT_API_SERVER_BASE_URL=http://localhost:8002
LIT_INTERNAL_SHARED_SECRET=$(openssl rand -base64 32)
```

Create `lit-api-server/.env` (additions only — existing TEE-related vars stay as configured):

```sh
LIT_PAYMENTS_BASE_URL=http://localhost:8000
LIT_INTERNAL_SHARED_SECRET=<same value as in lit-payments/.env>
```

### Step 3 — Apply migrations

Migrations auto-run on `cargo run`, so simplest path is to just start `lit-payments`:

```sh
cd lit-payments && cargo run
```

To run migrations manually for inspection / rollback:

```sh
cd lit-payments
sqlx migrate run --database-url postgres://postgres:postgres@localhost:5432/postgres
sqlx migrate info
psql $DATABASE_URL -c '\d auto_topup_config'
psql $DATABASE_URL -c '\d auto_topup_credits'
```

### Step 4 — Forward Stripe webhooks to localhost

In a separate terminal:

```sh
stripe login
stripe listen --forward-to http://localhost:8000/stripe/webhook
```

The CLI prints a webhook signing secret like `whsec_...`. Copy it into `STRIPE_WEBHOOK_SECRET` in `lit-payments/.env` and restart `lit-payments`.

Leave `stripe listen` running in its terminal for the entire session. It forwards every real test-mode webhook event from Stripe to your local service.

### Step 5 — Start all three services

Three terminals:

```sh
# Terminal A: lit-payments
cd lit-payments && cargo run

# Terminal B: lit-api-server
cd lit-api-server && cargo run

# Terminal C: dashboard
cd lit-static/dapps/dashboard && python3 -m http.server 8001
# (or whatever the existing dashboard local serve command is)
```

`stripe listen` is the fourth terminal, kept running.

### Stripe test cards

Use these card numbers against the dashboard's save-card flow to exercise different paths. Any future expiry (e.g., `12/34`) and any CVC (e.g., `123`).

| Card number | Behavior |
|---|---|
| `4242 4242 4242 4242` | Success — saves cleanly, charges succeed off-session |
| `4000 0000 0000 0341` | Off-session charge declines (`card_declined`) — exercises the failure path |
| `4000 0027 6000 3184` | Requires 3DS authentication — exercises `requires_action` / SCA |
| `4000 0000 0000 9995` | `insufficient_funds` decline |
| `4000 0000 0000 0069` | `expired_card` decline |
| `4000 0000 0000 0127` | `incorrect_cvc` decline |

Full list: https://docs.stripe.com/testing

### Test scenarios to verify before merging each phase

#### After Phase 2 — saved card

- [ ] `POST /billing/setup_intent` returns a `client_secret` and `publishable_key` for an existing Stripe customer.
- [ ] Same endpoint returns 400 with a clear "do a manual top-up first" message for a wallet with no Stripe customer.
- [ ] Using `4242…` in the dashboard's setup flow attaches a `pm_xxx` to the right `cus_xxx`. Verify via `stripe customers retrieve cus_xxx` and check `invoice_settings.default_payment_method` or `payment_methods` list.

#### After Phase 3 — config CRUD

- [ ] `PUT /billing/auto_topup_config` with full valid body returns 200, row appears in `auto_topup_config`.
- [ ] `PUT` with `enabled=true` and null `threshold_cents` is rejected by the CHECK constraint with a clear error.
- [ ] `PUT` with a `payment_method_id` not attached to this customer returns 400.
- [ ] `GET /billing/auto_topup_config` returns what was written.

#### After Phase 4 — trigger and off-session charge

- [ ] Setting up a small Lit Action that costs a few cents, manually pushing balance below threshold, and running the action → `paymentIntents.create` is called against Stripe test mode (visible in Stripe Dashboard Events feed).
- [ ] Burst test: 10 parallel `curl POST /internal/trigger_topup` calls (with `X-Internal-Secret`) for the same customer → only one PaymentIntent appears in Stripe (idempotency-key dedupes the rest).
- [ ] Test with `4000 0000 0000 0341` saved card → PI is created with status `requires_payment_method` (declined) → config row gets `disabled_reason` set after enough failures.
- [ ] Test with `4000 0027 6000 3184` SCA card → PI returns `requires_action` → handler sets `pending_action_pi_id` + `pending_action_at` on the config row, sets `disabled_reason='requires_action'`, sends the "action required" email via Resend test mode.

#### After Phase 5 — webhook and credit

- [ ] Use `stripe trigger payment_intent.succeeded` to fire a synthetic webhook → row appears in `auto_topup_credits` → balance transaction is created in Stripe → balance cache invalidation call is logged in `lit-api-server`.
- [ ] Use `stripe events resend evt_xxx` to replay the same `payment_intent.succeeded` event → handler returns 200 immediately without a second credit (verify row count unchanged in `auto_topup_credits` and no new balance transaction in Stripe).
- [ ] Use `stripe trigger payment_intent.payment_failed` → email is dispatched (Resend test mode), dashboard banner appears.
- [ ] Tamper with `Stripe-Signature` header → handler returns 401.
- [ ] After a successful credit, immediately request balance via `lit-api-server` → returns updated value (cache was invalidated).

#### After Phase 6 — dashboard UI

- [ ] User opens dashboard, sees "no card on file" state.
- [ ] User saves a `4242…` card → modal updates to show card on file.
- [ ] User configures threshold/amount/cap, toggles enabled, saves → config persists, status banner shows the rule in plain English.
- [ ] User runs a Lit Action that drops balance below threshold → within ~10 seconds, Stripe balance is credited and dashboard reflects new balance.
- [ ] Use 3DS card, run a Lit Action → dashboard shows "action required" banner → user clicks "Confirm now" → 3DS modal opens → user completes challenge → PI transitions to `succeeded` → balance is credited → banner disappears on next config fetch → `pending_action_pi_id` is cleared in Postgres.
- [ ] Use 3DS card, run a Lit Action → "action required" email arrives in Resend test inbox with deep link → clicking the link lands on dashboard with banner showing → resume flow proceeds as above.
- [ ] SCA failure path: 3DS card, abandon the challenge (close the modal) → PI eventually transitions to `requires_payment_method` → `payment_failed` webhook fires → pending state cleared → user receives "card needs updating" email.

#### After Phase 7 — failure handling

- [ ] Three consecutive declines → auto-disabled banner appears; email sent; toggle is off; user can re-enable after updating card.
- [ ] Operator runbook reviewed and reproducible: take a stuck PI, manually credit via existing admin portal.

#### End-to-end smoke before Phase 8 rollout

- [ ] Full happy path: save card → enable → run action → auto-charge → credited → run another action → balance now sufficient.
- [ ] Webhook replay safety: use Stripe Dashboard "resend event" on a real test-mode event 24+ hours later (or simulate by using a test event from yesterday's CLI).
- [ ] Cap reached: set cap = $5, top-up = $5, drive two top-ups in a row → second is correctly skipped.
- [ ] Cache invalidation under burst: 5 parallel deductions immediately after a top-up → all see the updated balance (none rejected for insufficient credit despite cache TTL).

### Staging / preview environment

No dedicated staging environment is currently documented for `lit-payments` or `lit-api-server`. Two options if local testing isn't enough:

1. **Railway preview environments** — Railway supports per-branch preview deploys. If not already enabled for the `lit-payments` project, ask whoever owns the Railway project (Brendan / Chris) to enable PR previews. Cost is minimal.
2. **Deploy `lit-payments` to a personal Railway service** — clone the project, point at a personal Postgres + the same Stripe test-mode keys, and exercise the webhook flow against a real publicly-reachable URL (vs `stripe listen` which is local-only).

For the TEE-deployed `lit-api-server`, staging is outside the scope of this repo; coordinate with the deployment owner before any non-local testing on the API server side.

### Quick-reference commands

```sh
# Reset local Postgres (drops everything)
docker stop lit-payments-pg && docker rm lit-payments-pg
docker run --rm -d --name lit-payments-pg -e POSTGRES_PASSWORD=postgres -p 5432:5432 postgres:16

# Inspect tables
psql $DATABASE_URL -c 'SELECT * FROM auto_topup_config;'
psql $DATABASE_URL -c 'SELECT * FROM auto_topup_credits ORDER BY credited_at DESC LIMIT 10;'

# Manually trigger a top-up evaluation
curl -X POST http://localhost:8000/internal/trigger_topup \
  -H "X-Internal-Secret: $LIT_INTERNAL_SHARED_SECRET" \
  -H "Content-Type: application/json" \
  -d '{"customer_id":"cus_xxx","wallet_address":"0x..."}'

# Trigger synthetic webhook events
stripe trigger payment_intent.succeeded
stripe trigger payment_intent.payment_failed

# Replay a specific event (idempotency test)
stripe events resend evt_xxx

# Tail Stripe CLI forwards
stripe listen --forward-to http://localhost:8000/stripe/webhook --print-json

# Check a customer in Stripe test mode
stripe customers retrieve cus_xxx
stripe payment_intents list --customer cus_xxx --limit 10
```

---

## 18. What is explicitly NOT in this plan

- Hard monthly cap with reservation ledger. The current design is a soft cap with bounded overshoot; UI copy reflects this.
- Reconciler cron for finding orphaned Stripe PIs without a `auto_topup_credits` row. Manual recovery via Sally's admin portal is the fallback for the rare 3-day-webhook-failure case.
- Currency support beyond USD.
- Daily/rolling restrictions on public-tier API keys (mentioned in the original Adarsh notes as a future feature; out of scope here).
- Refund-aware cap accounting.
- Multi-card support per customer (auto-top-up uses one saved card; user can change it by updating config).

---

## 19. Open questions for product

- Should the consent text say "approximately $X/month" or "up to $X/month" or both? Legal preference.
- What email service do we use for failure notifications? Existing infrastructure or new?
- Failure-counter threshold for auto-disable: 3 (current plan). Confirm with product.
- 10-minute recent-PI short-circuit window. Confirm with product.
- 5-minute idempotency-key bucket window. Confirm with product.
- Minimum top-up amount: $5 (matches existing one-shot floor). Confirm.

---

## 20. Verification

This plan has been independently reviewed by Codex (OpenAI's reasoning model) in three passes — once against the original heavier design, once against the simplified version, and once as a fresh-session sanity check on the locked design plus the "Stripe-native auto-top-up" alternative. All load-bearing findings have been incorporated. Remaining items are implementation details flagged in §6, §7, §8, §13, §14.

The "use Stripe Billing Meters + Credit Grants for native auto-top-up" alternative was explored and rejected: Stripe's own [Billing Credits implementation guide](https://docs.stripe.com/billing/subscriptions/usage-based/billing-credits/implementation-guide) explicitly states that merchants must create the funding invoice themselves, listen for `invoice.paid`, and call the Credit Grants API to actually grant credits. The same three steps (detect, trigger, credit) would apply on a more complex billing platform. Migration off `customer.balance` would also touch admin portal, dashboard, lit-api-server, lit-payments, and existing customer data — out of scope for this feature.

---

## 21. Handoff checklist for the implementing agent

If you're picking up this doc cold, do these in order:

- [ ] Read §1–4 (goal, framing, architecture, schema).
- [ ] Read §5–9 (endpoints, handler flows, dashboard changes).
- [ ] Read §10 (concurrency model) — internalize the three layers of defense.
- [ ] Read §11–12 (edge cases — handled vs accepted trade-offs).
- [ ] Read §13–14 (env vars + service-auth requirements).
- [ ] Read §17 (local development & testing) and stand up the local stack before writing any code.
- [ ] Follow §16 phases in order. Each phase has a gate; do not skip ahead.
- [ ] Use the test scenarios at the end of §17 as your acceptance criteria for each phase.
- [ ] Ask product before starting on §19 open questions (consent text, failure threshold, bucket windows, min top-up).

This document is intended to be self-contained. If something is ambiguous, raise it — do not guess.
