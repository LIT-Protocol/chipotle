-- Hardening for enterprise billing (codex adversarial review follow-up).
--
-- Additive ALTERs ONLY. The original 20260623000001 migration is already applied
-- in deployed environments; sqlx checksums each migration file and refuses to
-- boot if an applied one changes, so the hardening that was briefly (and wrongly)
-- folded into 20260623000001 lives here instead. See PR history for #541.
--
-- Idempotent: ADD COLUMN IF NOT EXISTS, and each ADD CONSTRAINT is wrapped to
-- swallow duplicate_object, so this applies cleanly whether a given environment
-- previously applied the original 20260623000001 (no constraints) or the briefly
-- modified one (constraints already present).
--
-- See plans/enterprise-committed-billing.md ("Post-review hardening").

-- enterprise_accounts -------------------------------------------------------

-- Baseline retry-window guard: stamped before the baseline Stripe write so a lost
-- success-record can't double-credit the buffer past Stripe's idempotency TTL.
ALTER TABLE enterprise_accounts
    ADD COLUMN IF NOT EXISTS baseline_attempted_at TIMESTAMPTZ;

-- Payer and invoice customers are deliberately DIFFERENT Stripe customers.
DO $$ BEGIN
    ALTER TABLE enterprise_accounts
        ADD CONSTRAINT enterprise_accounts_distinct_customers
            CHECK (payer_customer_id <> invoice_customer_id);
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- A term cannot end before it starts.
DO $$ BEGIN
    ALTER TABLE enterprise_accounts
        ADD CONSTRAINT enterprise_accounts_term_order
            CHECK (term_start IS NULL OR term_end IS NULL OR term_start <= term_end);
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- enterprise_invoices -------------------------------------------------------

-- Frozen amount/usage snapshots are never negative.
DO $$ BEGIN
    ALTER TABLE enterprise_invoices
        ADD CONSTRAINT enterprise_invoices_nonneg
            CHECK (
                consumed_units      >= 0 AND
                included_units      >= 0 AND
                overage_units       >= 0 AND
                committed_fee_cents >= 0 AND
                overage_cents       >= 0 AND
                total_cents         >= 0
            );
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- Status is a fixed lifecycle set.
DO $$ BEGIN
    ALTER TABLE enterprise_invoices
        ADD CONSTRAINT enterprise_invoices_status_valid
            CHECK (status IN ('pending', 'draft', 'sent', 'paid', 'error', 'manual'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;
