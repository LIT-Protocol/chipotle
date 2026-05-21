-- Audit-grade log of every credit granted via the admin portal.
-- Each row = one Stripe balance_transaction the portal wrote on behalf of an operator.

CREATE TABLE grants (
    id                            BIGSERIAL PRIMARY KEY,
    operator_id                   BIGINT      NOT NULL REFERENCES operators(id),
    stripe_customer_id            TEXT        NOT NULL,
    wallet_address                TEXT        NOT NULL,
    -- Snapshot of the customer's email at grant time. Stripe records may
    -- change after the fact; this column is the "as written" record.
    email                         TEXT,
    cents                         BIGINT      NOT NULL CHECK (cents > 0),
    note                          TEXT        NOT NULL DEFAULT '',
    stripe_balance_transaction_id TEXT        NOT NULL,
    -- UUID generated client-side per submission. Prevents double-credit on
    -- network retries: if the same key shows up again, we skip the Stripe
    -- write and return the existing row.
    idempotency_key               TEXT        NOT NULL UNIQUE,
    created_at                    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX grants_operator_id_created_at_idx
    ON grants (operator_id, created_at DESC);
CREATE INDEX grants_created_at_idx ON grants (created_at DESC);
