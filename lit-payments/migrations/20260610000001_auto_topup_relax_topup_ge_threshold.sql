-- Relax the `enabled_requires_config` CHECK constraint by dropping the
-- `topup_amount_cents >= threshold_cents` clause. The original intent
-- (prevent a sub-threshold top-up from causing back-to-back charges)
-- is over-cautious: a single huge deduction is the only realistic way
-- to land far enough below threshold that one top-up can't recover,
-- and in that case any back-to-back charges are bounded by the monthly
-- cap. The OpenAI / Anthropic auto-recharge UIs both permit
-- "restore-to" < "threshold + topup" (e.g. drops below $20, restore to
-- $30 → topup = $10) and we should match that UX.

ALTER TABLE auto_topup_config DROP CONSTRAINT enabled_requires_config;

ALTER TABLE auto_topup_config ADD CONSTRAINT enabled_requires_config CHECK (
    enabled = false OR (
        threshold_cents IS NOT NULL AND threshold_cents > 0 AND
        topup_amount_cents IS NOT NULL AND topup_amount_cents >= 500 AND
        topup_amount_cents <= 20000 AND
        monthly_cap_cents IS NOT NULL AND monthly_cap_cents >= topup_amount_cents AND
        payment_method_id IS NOT NULL AND
        consent_version IS NOT NULL AND consent_signed_at IS NOT NULL
    )
);
