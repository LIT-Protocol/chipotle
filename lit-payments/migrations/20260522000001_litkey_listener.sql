-- LITKEY on-chain payment records. Successful credits are idempotent by
-- chain + tx hash + log index so WSS and reconciliation poller races cannot
-- double-credit the same Payment event.
CREATE TABLE litkey_payments (
    id BIGSERIAL PRIMARY KEY,
    chain_id BIGINT NOT NULL,
    gateway_address TEXT NOT NULL CHECK (gateway_address ~ '^0x[0-9a-f]{40}$'),
    tx_hash TEXT NOT NULL CHECK (tx_hash ~ '^0x[0-9a-f]{64}$'),
    log_index BIGINT NOT NULL CHECK (log_index >= 0),
    block_number BIGINT NOT NULL CHECK (block_number >= 0),
    wallet_address TEXT NOT NULL CHECK (wallet_address ~ '^0x[0-9a-f]{40}$'),
    payer_address TEXT NOT NULL CHECK (payer_address ~ '^0x[0-9a-f]{40}$'),
    litkey_amount_wei NUMERIC(78, 0) NOT NULL CHECK (litkey_amount_wei > 0),
    usd_wei_per_litkey NUMERIC(78, 0),
    discount_basis_points BIGINT NOT NULL CHECK (discount_basis_points BETWEEN 0 AND 9000),
    cents_credited BIGINT NOT NULL CHECK (cents_credited >= 0),
    status TEXT NOT NULL CHECK (status IN ('credited', 'dust', 'paused', 'no_customer')),
    stripe_customer_id TEXT,
    stripe_balance_transaction_id TEXT,
    credited_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (chain_id, tx_hash, log_index),
    CONSTRAINT litkey_payments_status_fields_check CHECK (
        (status = 'credited' AND usd_wei_per_litkey IS NOT NULL AND cents_credited > 0 AND stripe_customer_id IS NOT NULL AND stripe_balance_transaction_id IS NOT NULL)
        OR (status <> 'credited' AND stripe_balance_transaction_id IS NULL)
    )
);

CREATE INDEX litkey_payments_wallet_credited_at_idx
    ON litkey_payments (wallet_address, credited_at DESC);

CREATE INDEX litkey_payments_credited_at_idx
    ON litkey_payments (credited_at DESC);

-- Reconciliation checkpoint. Advanced only by the HTTPS poll path after a
-- whole confirmed range has been processed successfully.
CREATE TABLE chain_checkpoint (
    chain_id BIGINT NOT NULL,
    gateway_address TEXT NOT NULL CHECK (gateway_address ~ '^0x[0-9a-f]{40}$'),
    last_processed_block BIGINT NOT NULL CHECK (last_processed_block >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (chain_id, gateway_address)
);
