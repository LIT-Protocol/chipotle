-- Glitch's PR review #5: extend the `enabled_requires_config` CHECK
-- with an upper bound on `topup_amount_cents` ($200) and require that
-- `topup_amount_cents >= threshold_cents` so a single charge brings
-- the balance back above threshold. Handler validation also enforces
-- these but the DB CHECK is the durable guard.

ALTER TABLE auto_topup_config DROP CONSTRAINT enabled_requires_config;

ALTER TABLE auto_topup_config ADD CONSTRAINT enabled_requires_config CHECK (
    enabled = false OR (
        threshold_cents IS NOT NULL AND threshold_cents > 0 AND
        topup_amount_cents IS NOT NULL AND topup_amount_cents >= 500 AND
        topup_amount_cents <= 20000 AND
        topup_amount_cents >= threshold_cents AND
        monthly_cap_cents IS NOT NULL AND monthly_cap_cents >= topup_amount_cents AND
        payment_method_id IS NOT NULL AND
        consent_version IS NOT NULL AND consent_signed_at IS NOT NULL
    )
);
