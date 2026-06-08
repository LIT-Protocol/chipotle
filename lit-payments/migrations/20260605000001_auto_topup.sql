-- Auto top-up: per-customer config + per-PI credit dedup ledger.
--
-- Scope: Stripe-paying users only. The Stripe customer is the source of truth
-- for the saved PaymentMethod and the credit balance; these two tables hold
-- the per-user *rule* (enable/threshold/amount/cap/card), the SCA recovery
-- handoff (when an off-session charge returns authentication_required), and a
-- dedup row for each successful auto top-up PaymentIntent we credit.

CREATE TABLE auto_topup_config (
    customer_id                TEXT        PRIMARY KEY,
    wallet_address             TEXT        NOT NULL UNIQUE,
    enabled                    BOOLEAN     NOT NULL DEFAULT false,
    threshold_cents            BIGINT,
    topup_amount_cents         BIGINT,
    monthly_cap_cents          BIGINT,
    payment_method_id          TEXT,
    consent_version            TEXT,
    consent_signed_at          TIMESTAMPTZ,
    -- NULL | 'manual' | 'failures' | 'card_invalid' | 'requires_action'.
    -- Set when the trigger handler auto-disables (3 consecutive failures) or
    -- when off-session confirm returns authentication_required.
    disabled_reason            TEXT,
    -- SCA handoff: when paymentIntents.create returns authentication_required,
    -- we stash the pending PI id here so the recovery page can resume it.
    pending_action_pi_id       TEXT,
    pending_action_at          TIMESTAMPTZ,
    -- One-time, single-use token included in the SCA recovery email link.
    -- 24h expiry; cleared on successful credit or expiry.
    recovery_token             TEXT,
    recovery_token_expires_at  TIMESTAMPTZ,
    updated_at                 TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- enabled=true ⇒ every field needed to actually charge a card must be set,
    -- and the cap must cover at least one top-up. Min top-up $5 matches the
    -- existing one-shot floor (MIN_TOPUP_CENTS in lit-api-server).
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

CREATE INDEX auto_topup_config_wallet_address_idx ON auto_topup_config (wallet_address);

-- Partial index for the SCA recovery token lookup: GET /billing/auto_topup_resume?token=...
-- resolves a presented token to a customer_id. Partial keeps the index small
-- since recovery_token is non-NULL only while an SCA handoff is in flight.
CREATE INDEX auto_topup_config_recovery_token_idx
    ON auto_topup_config (recovery_token)
    WHERE recovery_token IS NOT NULL;

-- One row per successful auto top-up PaymentIntent we have credited.
-- Primary key on payment_intent_id is the permanent dedup against the
-- synchronous credit path (sync handler vs reconciler retry).
CREATE TABLE auto_topup_credits (
    payment_intent_id              TEXT        PRIMARY KEY,
    customer_id                    TEXT        NOT NULL,
    amount_cents                   BIGINT      NOT NULL CHECK (amount_cents > 0),
    stripe_balance_transaction_id  TEXT,
    credited_at                    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX auto_topup_credits_customer_credited_at_idx
    ON auto_topup_credits (customer_id, credited_at DESC);

-- Partial index used by the reconciler (§9) to find rows where the PI
-- succeeded and we inserted the dedup row but the balance_transactions
-- write didn't finish (NULL stripe_balance_transaction_id). Partial keeps
-- this index trivially small under normal operation.
CREATE INDEX auto_topup_credits_pending_balance_tx_idx
    ON auto_topup_credits (stripe_balance_transaction_id)
    WHERE stripe_balance_transaction_id IS NULL;
