-- Make `monthly_cap_cents` truly optional. The dashboard's "Monthly
-- recharge limit" toggle was always a UX-level toggle, but the DB
-- CHECK still required it. Now: when enabled=true, the cap is
-- optional; when present, must be >= top-up amount. Per-charge
-- MAX_TOPUP_CENTS still bounds individual charges; "no monthly cap"
-- doesn't mean "unlimited spend" — just "no monthly ceiling."

ALTER TABLE auto_topup_config DROP CONSTRAINT enabled_requires_config;

ALTER TABLE auto_topup_config ADD CONSTRAINT enabled_requires_config CHECK (
    enabled = false OR (
        threshold_cents IS NOT NULL AND threshold_cents > 0 AND
        topup_amount_cents IS NOT NULL AND topup_amount_cents >= 500 AND
        topup_amount_cents <= 20000 AND
        (monthly_cap_cents IS NULL OR monthly_cap_cents >= topup_amount_cents) AND
        payment_method_id IS NOT NULL AND
        consent_version IS NOT NULL AND consent_signed_at IS NOT NULL
    )
);
