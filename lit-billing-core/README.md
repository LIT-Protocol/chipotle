# lit-billing-core

Shared Stripe primitives used by **lit-api-server** (TEE, charges and balance
checks) and **lit-payments** (ops portal, credit grants). One library so both
services agree on the identity model.

**The invariant this crate owns:** every Stripe customer is keyed by
`metadata.wallet_address` — the billing wallet derived from the account's API
key at creation. Look up customers only through `customer::find_or_create_by_wallet`
so the two services can never drift into duplicate customers.

| Module | Purpose |
|--------|---------|
| `client` | Thin `reqwest` Stripe client (`get`/`post`/`post_with_idempotency`, 30s timeout) |
| `customer` | Wallet ↔ customer lookup, idempotent creation, email updates |
| `balance` | Credit balance reads (negative balance = credits available) |
| `reporting` | Pagination + per-day aggregation for `stripe_report` |
| `format` | `cents_to_display`, date helpers |

Library only — no binary, no env vars. Callers pass the Stripe secret key to
`StripeClient::new()`.
