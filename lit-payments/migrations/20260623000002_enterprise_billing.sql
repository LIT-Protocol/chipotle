-- Enterprise committed-use billing (prepaid allotment + arrears overage).
--
-- See plans/enterprise-committed-billing.md. One row per committed-use customer
-- in `enterprise_accounts`; one row per generated invoice in `enterprise_invoices`
-- (also the per-period idempotency gate for the billing job).
--
-- Metering model: the payer Stripe customer is debited at the standard $0.01/unit
-- rate by lit-api-server (1 cent == 1 unit). The billing job keeps the payer's
-- credit topped to `target_credit_cents` with exactly ONE regrant per cycle, so
-- consumption for the cycle == target_credit_cents + balance_at_billing_time
-- (balance is negative when credit is available). No usage ledger, no Stripe
-- transaction listing. This identity only holds if the monthly regrant (and the
-- one-time baseline grant) are the ONLY credits on the payer account — so the
-- payer account must NOT have auto-topup enabled and must not receive manual
-- portal grants.

CREATE TABLE enterprise_accounts (
    id                                    BIGSERIAL   PRIMARY KEY,
    name                                  TEXT        NOT NULL,
    -- Stripe customer that consumes service on Chipotle and receives the
    -- credit buffer (regrants). Unique: one enterprise profile per payer.
    payer_customer_id                     TEXT        NOT NULL UNIQUE,
    -- Stripe customer that receives the invoice — deliberately a DIFFERENT
    -- customer from the payer.
    invoice_customer_id                   TEXT        NOT NULL,
    -- Flat committed fee billed monthly in advance (e.g. 900000 = $9,000).
    committed_fee_cents                   BIGINT      NOT NULL CHECK (committed_fee_cents >= 0),
    -- Included allotment per cycle, in units (1 unit == 1 cent of $0.01-rate
    -- charges == ~1 compute second). e.g. 3_000_000.
    included_units                        BIGINT      NOT NULL CHECK (included_units >= 0),
    -- Overage price per unit, in hundredths of a cent, to keep integer math
    -- exact. $0.0025/unit == 25 hundredths-of-a-cent.
    overage_rate_hundredths_cent_per_unit BIGINT      NOT NULL CHECK (overage_rate_hundredths_cent_per_unit >= 0),
    -- Credit buffer target the payer account is kept topped up to (e.g.
    -- 50_000_000 = $500k). Must dwarf a cycle's burn so they never run dry.
    target_credit_cents                   BIGINT      NOT NULL CHECK (target_credit_cents > 0),
    -- Day of month the invoice is issued / the cycle rolls. 1..=28 to dodge
    -- short-month edge cases.
    billing_anchor_day                    INT         NOT NULL CHECK (billing_anchor_day BETWEEN 1 AND 28),
    -- Where the human-review breakdown email is sent.
    notify_email                          TEXT        NOT NULL,
    -- v1 = false: create a DRAFT invoice + email for manual send. Flip to true
    -- to finalize+send automatically (future).
    auto_send                             BOOLEAN     NOT NULL DEFAULT false,
    active                                BOOLEAN     NOT NULL DEFAULT true,
    term_start                            DATE,
    term_end                              DATE,
    -- One-time buffer establishment. Set once the initial top-to-target credit
    -- has been written; gates the onboarding step so it never double-grants.
    baseline_balance_txn_id               TEXT,
    baseline_granted_at                   TIMESTAMPTZ,
    created_at                            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at                            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE enterprise_invoices (
    id                       BIGSERIAL   PRIMARY KEY,
    enterprise_account_id    BIGINT      NOT NULL REFERENCES enterprise_accounts(id),
    -- Anchor month the invoice is issued in, 'YYYY-MM'. One invoice per account
    -- per period — the idempotency gate for the billing job.
    period_key               TEXT        NOT NULL,
    -- Arrears window the overage is measured over (previous anchor → this anchor).
    period_start             DATE        NOT NULL,
    period_end               DATE        NOT NULL,
    -- Human label for the advance cycle the committed fee covers.
    committed_period         TEXT        NOT NULL,
    -- Snapshot, frozen at first attempt so resume/retry never recomputes from a
    -- balance that the regrant has since changed.
    consumed_units           BIGINT      NOT NULL,
    included_units           BIGINT      NOT NULL,
    overage_units            BIGINT      NOT NULL,
    committed_fee_cents      BIGINT      NOT NULL,
    overage_cents            BIGINT      NOT NULL,
    total_cents              BIGINT      NOT NULL,
    stripe_invoice_id        TEXT,
    regrant_balance_txn_id   TEXT,
    -- pending | draft | sent | paid | error | manual
    status                   TEXT        NOT NULL DEFAULT 'pending',
    notified_at              TIMESTAMPTZ,
    created_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (enterprise_account_id, period_key)
);

CREATE INDEX enterprise_invoices_account_period_idx
    ON enterprise_invoices (enterprise_account_id, period_key);

-- Seed the first committed-use customer: Uneven Labs, Inc.
-- (contract signed 2026-06-17; see plans/enterprise-committed-billing.md).
INSERT INTO enterprise_accounts (
    name, payer_customer_id, invoice_customer_id,
    committed_fee_cents, included_units, overage_rate_hundredths_cent_per_unit,
    target_credit_cents, billing_anchor_day, notify_email,
    auto_send, active, term_start, term_end
) VALUES (
    'Uneven Labs, Inc.',
    'cus_UXvHcFlfhR6rc5',   -- payer (engineering@unevenlabs.com)
    'cus_UiVuFoABiqcMs5',   -- invoice (accounting@unevenlabs.com)
    900000,                 -- $9,000/mo committed
    3000000,                -- 3,000,000 unit allotment
    25,                     -- $0.0025/unit overage
    50000000,               -- $500k credit buffer target
    17,                     -- anchor on the 17th (effective date)
    'chris@litprotocol.com',
    false,                  -- draft + email for manual send (v1)
    true,
    '2026-06-17',
    '2027-06-17'
);

-- June 2026 committed fee was invoiced MANUALLY. Record it so the billing job's
-- first generated invoice is the July anchor (July advance + June arrears
-- overage) rather than re-billing June's committed fee.
INSERT INTO enterprise_invoices (
    enterprise_account_id, period_key, period_start, period_end, committed_period,
    consumed_units, included_units, overage_units,
    committed_fee_cents, overage_cents, total_cents, status
)
SELECT
    id, '2026-06', DATE '2026-06-17', DATE '2026-07-17', '2026-06-17 → 2026-07-17',
    0, included_units, 0,
    committed_fee_cents, 0, committed_fee_cents, 'manual'
FROM enterprise_accounts
WHERE payer_customer_id = 'cus_UXvHcFlfhR6rc5';
