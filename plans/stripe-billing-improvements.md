# Stripe Billing — Shrink the TEE Surface

Status: design proposal
Author: Chris (with Claude)
Date: 2026-06-15

## TL;DR

`lit-api-server` (which runs inside a TEE) currently owns the entire Stripe lifecycle: customer creation, email registration, PaymentIntent creation, payment confirmation + credit, balance caching, and credit deduction. This grew organically because `lit-payments` didn't exist yet.

Now that `lit-payments` is a real backend, the TEE should own **only the two things that actually need to be inside a TEE**, both as internal operations on the debit path:

1. **Read** a customer's Stripe balance (to gate a request before doing the work).
2. **Deduct** credits from a customer's Stripe balance (to bill for work performed inside the TEE).

Everything else — including the *public* balance-read endpoint that today's dashboard hits — moves to `lit-payments`. After the move, lit-api-server exposes **zero Stripe-related HTTP endpoints**; Stripe is purely an internal dependency of the debit path. lit-payments owns customer management, payments, webhooks, reporting, auto top-up, and balance reads.

Additionally: the in-process balance cache becomes **strictly a performance optimization**. It is no longer treated as a source of truth. Insufficient-balance cache hits become cache misses, so external top-ups (manual admin credit, LITKEY → credit, auto top-up, future flows) propagate within milliseconds, not minutes, and horizontal scaling becomes safe.

## Motivation

- **TEE compute is the most expensive compute we have.** Every code path inside the enclave costs more (money, attestation surface, audit burden) than the same code outside. Code that doesn't need TEE protection shouldn't be there.
- **The payment path is getting more complex, not less.** Auto top-up (in flight) adds a writer that lit-api-server doesn't know about. Future features — subscription credits, promo codes, refunds, multi-currency, accounting integrations — will keep adding writers. Each one risks the current "lit-api-server is the only writer to balance" invariant.
- **Horizontal scaling is on the roadmap.** The current design has correctness bugs the moment there's more than one replica: a top-up handled by replica A invalidates A's cache only; replica B keeps serving stale "insufficient funds" until its 10-minute TTL expires. Same problem for any in-process state (`balance_refresh_in_flight`, wallet→customer cache, etc.).
- **Reduce the blast radius of bugs in the TEE.** Less code means fewer places a bug can leak signing material or cause a deploy-blocking incident.

## Non-goals

- Replacing Stripe.
- Changing the customer-facing top-up UX (dashboard, LITKEY flow, admin portal continue to work).
- Re-architecting authentication or the billing-auth guard.
- Subscriptions / recurring billing — out of scope.

## Target architecture

```
┌──────────────────────────────────────────┐
│  TEE: lit-api-server                     │
│  ─────────────────────────────────────── │
│  Stripe responsibilities (internal only, │
│  ZERO public Stripe HTTP endpoints):     │
│   • Read balance (Rust fn, used by the   │
│     debit gate — cache w/ fall-through   │
│     on miss OR on insufficient cache hit)│
│   • Write balance transaction (debit)    │
│  ─────────────────────────────────────── │
│  NO PaymentIntent, NO confirm,           │
│  NO customer creation, NO email mgmt,    │
│  NO public balance endpoint,             │
│  NO webhooks, NO reporting jobs.         │
└──────────────────────────────────────────┘
                  │
                  │ shares: lit-billing-core (types, format helpers)
                  │
┌──────────────────────────────────────────┐
│  lit-payments (normal cloud service)     │
│  ─────────────────────────────────────── │
│   • Customer create + email register     │
│   • Wallet → customer lookup             │
│   • PaymentIntent create + confirm       │
│   • Public GET /billing/balance          │
│   • LITKEY → credit                      │
│   • Admin credit portal                  │
│   • Auto top-up                          │
│   • Stripe webhooks (single destination) │
│   • Reporting / `stripe_report` binary   │
└──────────────────────────────────────────┘
```

## Scope of code moves

Concrete `stripe.rs` items and where each one goes:

| Function | Today (lit-api-server/src/stripe.rs) | Destination |
|---|---|---|
| `get_credit_balance` | line 262 | **stays** |
| `charge` (private) | line 337 | **stays** |
| `charge_management` | line 453 | **stays** |
| `charge_lit_action_time` | line 465 | **stays** |
| `should_update_balance_cache` | line 319 | **stays** |
| `resolve_wallet_address` | line 211 | **stays** (used by debit path) |
| `get_customer_by_wallet` | line 238 | **stays** (used by debit path) |
| `record_billing_event` | line 76 | **stays** (debit-side audit) |
| `create_payment_intent` | line 483 | **move to lit-payments** |
| `confirm_payment_and_credit` | line 538 | **move to lit-payments** |
| `set_customer_email` | line 619 | **move to lit-payments** |
| `register_customer_email` | line 624 | **move to lit-payments** |
| `list_all_customers` | line 641 | **move to lit-payments** (reporting) |
| `list_balance_transactions_since` | line 647 | **move to lit-payments** (reporting) |
| `bin/stripe_report.rs` | binary | **move to lit-payments** |

Endpoints to remove from `core/v1/endpoints/billing.rs`:

- `POST /billing/payment-intent` (`billing_create_payment_intent`, line 105)
- `POST /billing/confirm-payment` (`billing_confirm_payment`, line 139)
- `GET /billing/stripe-config` (`billing_stripe_config`, line 48) — move to lit-payments; the dashboard fetches it from there instead.

- `GET /billing/balance` (`billing_balance`, line 65) — also moves. Dashboard/clients read balance from lit-payments. The TEE keeps `get_credit_balance` as an internal Rust function used by the debit-path gate, but no longer exposes it as an HTTP endpoint.

After Phase 4, lit-api-server exposes **zero Stripe-related HTTP endpoints.**

Endpoints to audit (out-of-scope items that also touch stripe state):

- `new_account`, `convert_to_chain_secured_account`, `add_usage_api_key`, `remove_usage_api_key` — these currently call `invalidate_wallet_cache` on the local cache. After the move, they keep doing that (still correct as a local-cache hint), but the durable wallet→customer mapping lives in lit-payments.

## Cache redesign

The cache becomes a pure performance optimization with these rules:

1. **Positive balances**: serve from cache for up to 10 min (keep current TTL, or tighten to 60–120 s — TBD; longer = cheaper, shorter = fresher).
2. **Insufficient-for-this-request balance is treated as a cache miss.** Refetch from Stripe before denying. This makes external top-ups (auto top-up, admin credit, LITKEY) visible within one request, not within `TTL`.
3. **Anti-abuse**: gate insufficient-balance refetches with a short-lived "I just refetched and they're still broke" negative cache (~10–30 s) so a spam loop can't be used as a Stripe-rate-limit DoS amplifier. Same shape as the permission denial cache from #489.
4. **No cross-replica coherence required.** Each replica caches independently; the source of truth is always Stripe. With rule 2, the worst stale-positive case is `TTL` seconds of over-serving for one customer on one replica, bounded by their actual remaining balance (debits decrement the local cache, so balance can't go more than `TTL`-worth-of-charges below cached value).
5. **Remove `balance_refresh_in_flight`'s role as a correctness mechanism.** It stays as a singleflight dedupe but isn't required for invariants.
6. **`lit-api-server` no longer needs an invalidate-on-credit hook.** Rule 2 makes external credits self-healing on the read side, and lit-api-server isn't doing the crediting any more anyway.

## Wallet → customer lookup

Today `get_customer_by_wallet` queries Stripe by metadata each time (with a cache). After the move:

- **Option A (simpler):** lit-api-server keeps querying Stripe directly by `metadata['wallet']`. lit-payments writes that metadata at customer-creation time. No new dependency.
- **Option B (faster, more coupled):** lit-api-server calls a `GET /customer-by-wallet/:addr` endpoint on lit-payments.

**Recommendation: A.** Keeps the TEE's external dependency footprint to "Stripe only." lit-payments crashing should not prevent the TEE from charging for work it already did.

## Sequencing

Each phase is independently shippable and safe to halt between.

1. **Phase 1 — Cache fix (small, urgent for auto top-up):**
   - Treat insufficient cached balance as a miss; refetch from Stripe.
   - Add the short-lived negative cache.
   - This unblocks auto top-up from working correctly today, even before any code moves.

2. **Phase 2 — Stand up the move targets in lit-payments:**
   - Port `create_payment_intent`, `confirm_payment_and_credit`, `set_customer_email`, `register_customer_email`.
   - Port the **read-balance** path so lit-payments can serve `GET /billing/balance`.
   - Add endpoints in lit-payments mirroring `POST /billing/payment-intent`, `POST /billing/confirm-payment`, `GET /billing/stripe-config`, and `GET /billing/balance`.
   - Stand up the Stripe webhook receiver in lit-payments (single destination going forward).
   - Ship behind a feature flag in the dashboard so it can flip between TEE-served and lit-payments-served endpoints.

3. **Phase 3 — Flip clients:**
   - Dashboard top-up + balance display → lit-payments.
   - Any SDK or example that hits the TEE for payment intents or balance reads → lit-payments.
   - Monitor for regression for ~1 week.

4. **Phase 4 — Delete from TEE:**
   - Remove all five Stripe HTTP endpoints (`payment-intent`, `confirm-payment`, `stripe-config`, `balance`, and any reporting endpoints) and their handlers from `lit-api-server`.
   - Delete the moved functions from `stripe.rs`. Keep `get_credit_balance` as an internal function used by the debit path.
   - Re-attest / redeploy enclave with the smaller surface.

5. **Phase 5 — Reporting:**
   - Move reporting helpers and the `stripe_report` binary to lit-payments.
   - Decommission TEE-side report runners.

## Resolved decisions

- **lit-payments is the single Stripe-webhook destination.** lit-api-server does not receive webhooks. lit-api-server does not need a webhook-driven invalidation hook either — cache rule 2 (insufficient = miss) means external credits self-heal on the next request that would have been denied.
- **`GET /billing/balance` moves out of the TEE to lit-payments.** It's a public read endpoint the dashboard hits to show "you have $X.YZ left"; there's no reason it needs to run in the TEE. lit-payments reads directly from Stripe (its own short cache is fine).
  - The TEE retains its **internal** `get_credit_balance` function — it's used by the debit path's gate check. That's a Rust function call, not an HTTP endpoint.
  - Consequence: after Phase 4, lit-api-server's Stripe-related HTTP surface is **zero endpoints**. Stripe is purely an internal dependency of the debit path.

## Deferred

- **Stripe key split.** Today one secret key is in the TEE. Post-move it only needs `read balance` + `create balance_transaction`, while lit-payments needs full permissions. Splitting reduces blast radius if either side is compromised. Worth doing eventually; not blocking this plan.

## Risks

- **Behavioral parity gap during the flip.** Two replicas of the payment-confirm endpoint (TEE + lit-payments) running concurrently could double-credit. Mitigation: idempotency keys (already part of `confirm_payment_and_credit`) and a hard cutover per client rather than concurrent traffic.
- **lit-payments availability becomes part of the funding path.** Today the TEE can take a payment even if lit-payments is down. After the move, lit-payments must be up to top up. Charging still works (no dependency on lit-payments for the debit path), so workloads in flight aren't affected. Acceptable.
- **Reporting binaries assume direct stripe key access.** Moving them is mostly mechanical but they may have hidden assumptions about running co-located with the TEE config. Audit during Phase 5.
