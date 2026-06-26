# Enterprise committed-use billing (prepaid allotment + arrears overage)

Status: implemented (branch glitch003/enterprise-net30-billing) — pending review + deploy
Owner: chris
First customer: Uneven Labs, Inc. (contract signed 2026-06-17, `.context/attachments/RGXuyR/`)

## Goal

Automate the monthly enterprise billing flow we currently do by hand:

- Customer prepays a **committed monthly fee** for an **included allotment**, and pays
  **overage in arrears** at a discounted rate.
- The Stripe customer that **receives the invoice** ("invoice account") is **not** the
  Stripe customer that **consumes service on Chipotle** ("payer account").
- The payer account must **never run out of credits** — they settle by invoice (net 30).

### Uneven Labs terms (signed contract §8)

| Term | Value |
|------|-------|
| Committed fee | **$9,000 / month**, billed **monthly in advance** |
| Included allotment | **3,000,000 compute seconds / calendar month**, no rollover |
| Overage rate | **$0.0025 / compute second**, billed **in arrears** |
| Standard internal rate | **$0.01 per unit** (`COST_LIT_ACTION_PER_SECOND_CENTS = 1`) |
| Payment terms | Net 30 |
| ACV | $108,000 (ex-overage, ex-tax) |
| Term | 12 months from 2026-06-17 |

> June 2026 was invoiced manually. Automation begins from the **next** cycle so we don't
> double-bill the committed fee — see "First-cycle handling."

---

## Chosen approach (per chris)

1. **No changes to `lit-api-server`.** The hot charge path is untouched.
2. **Don't track real compute usage.** They're buying generic "Chipotle credits." We meter
   purely on **dollars charged in Stripe at the standard $0.01 rate** and convert to their
   contract rate. A management call ($0.01/request) and a compute-second ($0.01/sec) both
   count as one unit — accepted and intended.
3. **No polling job.** Grant a buffer that dwarfs monthly burn ($500k), and regrant it once a
   month at billing time. They realistically can't burn $500k in a month, so they never run
   out mid-cycle; if usage ever climbed, we just raise the buffer.

### The metering trick

Internal rate is **exactly 1 cent per unit**, so *dollars charged = unit count*:

- 3,000,000 included compute-seconds ≡ **3,000,000 units** ≡ **$30,000** of internal charges.
- They pay the flat **$9,000** for that allotment; each unit beyond 3,000,000 is overage at $0.0025.

With a single monthly regrant up to a fixed **target** (e.g. $500k), usage is a **one-line
balance read** — no Stripe transaction listing (there are hundreds of thousands/month):

```
consumed_units = target_credit_cents - credit_remaining_now   // credit_remaining = -balance
```

Then we regrant `consumed_units` cents back to restore the buffer to `target`, and bill on it.

### Overage math (integer cents)

```
included_units = 3_000_000                          // = $30,000 of charges
overage_units  = max(0, consumed_units - included_units)
overage_cents  = round(overage_units * 25 / 100)    // $0.0025 = 25 hundredths-of-a-cent/unit
total_cents    = 900_000 + overage_cents             // $9,000 committed + overage
```

---

## The one job: monthly billing + regrant

A single task in `lit-payments`, following the existing reconciler pattern
(`auto_topup/reconciler.rs`, spawned in `main.rs`). Runs daily; acts only when today reaches
an account's `billing_anchor_day` and no `enterprise_invoices` row exists for the period
(idempotency guard). Per active `enterprise_accounts` row:

1. Read the payer account's Stripe balance → `credit_remaining_now`.
2. `consumed_units = target_credit_cents - credit_remaining_now`.
3. Compute `overage_cents` and `total_cents` per the formula above.
4. On the **invoice account**, create Stripe **invoice items**:
   - committed fee `$9,000` (advance, next month)
   - overage `$X` (arrears, month just closed) — only if > 0
5. Create the invoice (`collection_method = send_invoice`, `days_until_due = 30`). For v1
   leave it **draft + notify a human**; flip to auto-finalize+send after a couple clean cycles.
6. **Regrant** `consumed_units` cents (negative balance transaction) to restore the payer
   buffer back to `target`; log it.
7. Persist the `enterprise_invoices` row (amounts, `stripe_invoice_id`, status).

### Invariant this depends on
The monthly regrant must be the **only** thing that credits the payer account between billing
runs, or `consumed = target − remaining` is wrong. On the payer account we therefore:
- **Disable auto-topup** (no `auto_topup_config` row / `enabled=false`).
- **Never issue manual portal grants** to it.
- Record the **initial onboarding grant** ($500k baseline) so we know the starting target.

Since the buffer ($500k) is ~12× expected monthly burn (~$30–40k), one regrant per month is
plenty and there's no mid-month run-out risk.

---

## Data model (new, in `lit-payments`)

`enterprise_accounts` — one row per committed-use customer:

| column | notes |
|--------|-------|
| `id` | uuid pk |
| `name` | "Uneven Labs, Inc." |
| `payer_stripe_customer_id` | consumes on Chipotle; receives regrants |
| `invoice_stripe_customer_id` | the **different** customer that receives invoices |
| `committed_fee_cents` | 900000 |
| `included_units` | 3000000 |
| `overage_rate_hundredths_cent_per_unit` | 25 (i.e. $0.0025; integer math) |
| `target_credit_cents` | 50000000 ($500k buffer) |
| `billing_anchor_day` | e.g. 17 |
| `active` / `term_start` / `term_end` | lifecycle |

`enterprise_invoices` — idempotency + audit (also records the regrant):

| column | notes |
|--------|-------|
| `enterprise_account_id` | fk |
| `period_start` / `period_end` | calendar month metered (arrears) |
| `committed_period` | the advance month the $9k covers |
| `consumed_units` / `included_units` / `overage_units` | snapshot |
| `committed_fee_cents` / `overage_cents` / `total_cents` | snapshot |
| `stripe_invoice_id` | set once created |
| `regrant_balance_txn_id` | the Stripe credit that restored the buffer |
| `status` | `draft` / `sent` / `paid` / `error` |
| `idempotency_key` | `entinv:{account}:{period}` — one invoice + one regrant per period |

> The onboarding $500k baseline grant can be recorded as a simple seeded row / migration note;
> it isn't tied to an invoice.

---

## Stripe surface to add (none exists today)

In `lit-billing-core` (mirroring `balance::write_transaction`'s idempotency style):
`POST /v1/invoiceitems`, `POST /v1/invoices`, `POST /v1/invoices/{id}/finalize`,
`POST /v1/invoices/{id}/send`. Credits reuse the existing `balance::write_transaction`.

---

## Phasing

1. **Schema + onboarding.** `enterprise_accounts`, `enterprise_invoices` migrations. Seed
   Uneven Labs row; disable auto-topup on the payer account; write the initial $500k baseline
   grant.
2. **Stripe invoice primitives** in `lit-billing-core`.
3. **Monthly billing + regrant job** (draft + notify). Seed June as `sent` (manual) so the
   first generated invoice is July's $9k advance + June's overage arrears.
4. **Operate one cycle as draft-review**, reconcile the amounts, then flip to auto-send.

## First-cycle handling
June 2026 committed fee was invoiced manually. Mark June `sent` (manual) in
`enterprise_invoices` and treat the onboarding grant as the starting target, so the job's
first invoice = July $9k advance + June overage arrears — no double committed fee.

## Honest caveats (accepted)
- Management calls count toward the allotment as units, so a management-heavy month bills
  slightly differently than a strict compute-second reading — accepted; they're buying credits.
- `consumed = target − remaining` is only correct if nothing else credits the payer account
  between regrants (hence the invariant above).

## Decisions (locked 2026-06-23)
1. **Stripe customers** — payer (gets grants): `cus_UXvHcFlfhR6rc5` (engineering@unevenlabs.com);
   invoice (gets invoices): `cus_UiVuFoABiqcMs5` (accounting@unevenlabs.com).
2. **Billing anchor day = 17** (contract Effective Date = date of last signature, 2026-06-17,
   per §1). Period = **anchor-to-anchor** (17th→16th); the contract's "calendar month" wording
   can't coexist with a 17th anchor, so each anchor-to-anchor cycle *is* "the month" for the
   $9k, the 3M allotment, and overage. No rollover. The manual-review email is the safety net
   if this interpretation ever needs adjusting.
3. **Buffer target = $500k.** Single regrant per cycle (no polling); $500k ≫ monthly burn so
   no mid-cycle run-out.
4. **Human-in-the-loop send (v1):** the job creates a **draft** invoice and **emails
   chris@litprotocol.com** the full breakdown (consumed units, included, overage units, overage
   $, committed $, total) plus a link to the draft. Chris sanity-checks and clicks **Send** in
   Stripe. Once proven over a few cycles, flip a flag to auto-finalize+send and drop the draft
   stage.
